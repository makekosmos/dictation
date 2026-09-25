//! Status window for the standalone dictation app: record control, Engine
//! config/state mirror and the local-model manager. The pill overlay itself
//! is `pill.rs` — a separate always-on-top window.
use ::gpui::{prelude::*, *};
use gpui_component::scroll::ScrollableElement;
use serde_json::json;

use crate::app::DictationApp;
use crate::pill::PillPhase;
use kosmos_gpui_kit::fields::*;
use kosmos_gpui_kit::theme::*;

fn trigger_label(mode: &str) -> &'static str {
    match mode {
        "push_to_talk" => "Удержание (push-to-talk)",
        _ => "Переключение (toggle)",
    }
}

fn state_label(state: &str) -> &'static str {
    match state {
        "recording" => "Запись",
        "transcribing" => "Распознаю",
        "waiting" => "Жду сеть",
        "error" => "Ошибка",
        _ => "Готов",
    }
}

impl Render for DictationApp {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.data("dictation.state");
        let cfg = vget(&state, "config");
        let hotkey = vstr(cfg, "hotkey");

        let mut col = div()
            .size_full()
            .bg(c(BG()))
            .text_color(c(FG()))
            .font_family("Inter")
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .overflow_y_scrollbar();

        // --- Header ---
        col = col.child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .text_size(px(15.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("Kosmos Dictation"),
                )
                .child({
                    let s = vstr(&state, "state");
                    let color = match s.as_str() {
                        "recording" => DESTRUCTIVE(),
                        "transcribing" | "waiting" => WARN(),
                        "error" => DESTRUCTIVE(),
                        _ => SUCCESS(),
                    };
                    badge(state_label(&s), color)
                }),
        );

        if let Some(error) = &self.error {
            col = col.child(
                div()
                    .text_size(px(12.))
                    .text_color(c(DESTRUCTIVE()))
                    .child(error.clone()),
            );
        }
        if let Some(notice) = &self.notice {
            col = col.child(
                div()
                    .text_size(px(12.))
                    .text_color(c(SUCCESS()))
                    .child(notice.clone()),
            );
        }

        // --- Запись ---
        let phase = self.phase;
        let (label, hint): (&str, &str) = match phase {
            Some(PillPhase::Starting) => ("Запуск записи…", "Engine открывает захват микрофона"),
            Some(PillPhase::Recording) => (
                "Остановить запись",
                "Идёт запись — pill-окно у нижнего края экрана",
            ),
            Some(PillPhase::Processing) => ("Распознаю…", "capture.stop → speech.transcribe"),
            None => (
                "Начать запись",
                "WASAPI-захват на стороне Engine, результат вставляется/копируется по injectMode",
            ),
        };
        let busy = phase.is_some();
        let mut record = card().child(
            row("Диктовка", hint)
                .child(btn("dictation-toggle", label, true, cx, |this, cx| {
                    this.dictation_toggle(cx);
                }))
                .when(busy, |el| {
                    el.child(btn(
                        "dictation-cancel",
                        "Отмена",
                        false,
                        cx,
                        |this, cx| {
                            this.dictation_cancel(cx);
                        },
                    ))
                }),
        );
        if !hotkey.is_empty() {
            record = record.child(
                div()
                    .flex()
                    .items_center()
                    .gap_1()
                    .text_size(px(11.))
                    .text_color(c(MUTED_FG()))
                    .child("Горячая клавиша:")
                    .children(
                        hotkey
                            .split('+')
                            .map(|p| crate::pill_wave::kbd(p.trim().to_string())),
                    ),
            );
        }
        col = col.child(record);

        // --- Конфиг ---
        if !state.is_null() {
            let mut card_el = card().child(
                div()
                    .text_size(px(12.))
                    .text_color(c(MUTED_FG()))
                    .child("Конфигурация (Engine)"),
            );
            card_el = card_el.child(kv("Режим", trigger_label(&vstr(cfg, "triggerMode"))));
            card_el = card_el.child(kv("Язык", vstr(cfg, "language")));
            card_el = card_el.child(kv(
                "Провайдер",
                format!(
                    "{}{}",
                    vstr(cfg, "provider"),
                    if vbool(cfg, "providerEnabled") {
                        ""
                    } else {
                        " (выключен)"
                    }
                ),
            ));
            card_el = card_el.child(kv("Модель", vstr(cfg, "model")));
            card_el = card_el.child(kv("Вставка", vstr(cfg, "injectMode")));
            if let Some(err) = vopt(&state, "lastError") {
                card_el = card_el.child(kv("Последняя ошибка", err));
            }
            col = col.child(card_el);
        } else {
            col = col.child(
                card().child(
                    div()
                        .text_size(px(12.))
                        .text_color(c(MUTED_FG()))
                        .child("Подключение к Engine…"),
                ),
            );
        }

        // --- Последняя расшифровка ---
        let result = self.data("dictation.result");
        if !result.is_null() {
            let text = vopt(&result, "text").unwrap_or_default();
            let delivery = vstr(&result, "delivery");
            let mut card_el = card().child(
                div()
                    .text_size(px(12.))
                    .text_color(c(MUTED_FG()))
                    .child("Последняя расшифровка"),
            );
            if let Some(err) = vopt(&result, "error") {
                card_el = card_el.child(kv("Ошибка", err));
            }
            if !text.is_empty() {
                card_el = card_el.child(div().text_size(px(13.)).child(text));
            }
            let mut meta = Vec::new();
            if !delivery.is_empty() {
                meta.push(format!("delivery: {delivery}"));
            }
            let ms = vnum(&result, "durationMs");
            if ms > 0.0 {
                meta.push(format!("запись {}", fmt_ms(ms)));
            }
            if !meta.is_empty() {
                card_el = card_el.child(
                    div()
                        .text_size(px(11.))
                        .text_color(c(MUTED_FG()))
                        .child(meta.join(" · ")),
                );
            }
            col = col.child(card_el);
        }

        // --- Локальные модели ---
        col = col.child(models_card(self, cx));

        col
    }
}

fn models_card(app: &mut DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    let local = app.data("dictation.local");
    let models = app.data("dictation.models");
    if local.is_null() && models.is_null() {
        return div().into_any_element();
    }
    let mut el = card().child(
        div()
            .text_size(px(12.))
            .text_color(c(MUTED_FG()))
            .child("Локальная модель (on-device STT)"),
    );
    if !local.is_null() {
        el = el.child(kv(
            "Движок",
            format!(
                "{} · {}",
                if vbool(&local, "warm") {
                    "прогрет"
                } else {
                    "холодный"
                },
                vopt(&local, "backend").unwrap_or_else(|| "—".into())
            ),
        ));
        if let Some(model) = vopt(&local, "loadedModel") {
            el = el.child(kv("Загружена", model));
        }
        if let Some(err) = vopt(&local, "error") {
            el = el.child(kv("Ошибка движка", err));
        }
    }
    for model in varr(&models, "models").iter().take(10) {
        let id = vstr(model, "id");
        let use_id = id.clone();
        let download_id = id.clone();
        let ask_id = id.clone();
        let mut r = row(
            vstr(model, "name"),
            format!(
                "{} · {:.0} МБ{}",
                vstr(model, "description"),
                vnum(model, "sizeMb"),
                if vbool(model, "recommended") {
                    " · рекомендуется"
                } else {
                    ""
                }
            ),
        );
        if vbool(model, "selected") {
            r = r.child(badge("Выбрана", SUCCESS()));
        } else if vbool(model, "downloaded") {
            r = r
                .child(badge("Скачана", MUTED_FG()))
                .child(btn_id(
                    &format!("dict-use-{id}"),
                    "Использовать",
                    {
                        cx.listener(move |this, _, _, cx| {
                            this.action("dictation.use_local_model", json!({"modelId": use_id}));
                            cx.notify();
                        })
                    },
                ))
                .child(btn_id(&format!("dict-del-{id}"), "Удалить", {
                    cx.listener(move |this, _, _, cx| {
                        this.ask_delete(ask_id.clone());
                        cx.notify();
                    })
                }));
        } else {
            r = r.child(btn_id(&format!("dict-dl-{id}"), "Скачать", {
                cx.listener(move |this, _, _, cx| {
                    this.action(
                        "dictation.download_local_model",
                        json!({"modelId": download_id, "select": true}),
                    );
                    cx.notify();
                })
            }));
        }
        el = el.child(r);
    }
    // Inline delete confirm — cheaper than a modal for this utility window.
    if let Some(delete_id) = app.delete_confirm.clone() {
        let yes = delete_id.clone();
        el = el.child(
            row(
                format!("Удалить {delete_id}?"),
                "Файлы модели будут удалены с диска",
            )
            .child(btn(
                "dict-del-yes",
                "Удалить",
                false,
                cx,
                move |this, _cx| {
                    this.action("dictation.delete_local_model", json!({"modelId": yes}));
                    this.delete_confirm = None;
                },
            ))
            .child(btn("dict-del-no", "Отмена", false, cx, |this, _| {
                this.delete_confirm = None;
            })),
        );
    }
    // Live download progress from the WS event slot
    // (`dictation_local_model_download_progress`, app.rs::handle_engine_event).
    let download = app.data("dictation.download");
    if !download.is_null() {
        let model = vopt(&download, "modelId").unwrap_or_else(|| "модель".into());
        let percent = vnum(&download, "percent");
        let text = if percent > 0.0 {
            format!("{model} — {percent:.0}%")
        } else {
            format!("{model}…")
        };
        el = el.child(kv("Скачивание", text));
    }
    el.into_any_element()
}
