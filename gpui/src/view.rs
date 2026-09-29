//! Status window for the standalone dictation app: record control, Engine
//! config/state mirror and the local-model manager. The pill overlay itself
//! is `pill.rs` — a separate always-on-top window.
use ::gpui::{prelude::*, *};
use gpui_component::scroll::ScrollableElement;
use serde_json::json;

use crate::app::DictationApp;
use crate::pill::PillPhase;
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

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
            .flex_1()
            .w_full()
            .min_h_0()
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
                        .text_size(px(12.))
                        .text_color(c(MUTED_FG()))
                        .child("Статус движка"),
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
            card_el = card_el.child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(kv("Хоткей", vstr(cfg, "hotkey")))
                    .child(if self.hotkey_capturing {
                        div()
                            .text_size(px(11.))
                            .text_color(c(WARN()))
                            .child("Нажмите комбинацию… (Esc — отмена)")
                            .into_any_element()
                    } else {
                        btn(
                            "dict-hotkey-capture",
                            "Изменить",
                            false,
                            cx,
                            |this, cx| this.hotkey_capture_start(cx),
                        )
                        .into_any_element()
                    }),
            );
            // Idle unload: number → minutes, null/0 → "Не выгружать".
            let unload_ms = vnum(cfg, "localIdleUnloadMs");
            let current_min = if unload_ms <= 0.0 {
                None
            } else {
                Some((unload_ms / 60_000.0).round() as u64)
            };
            let mut unload_row = div()
                .flex()
                .items_center()
                .justify_between()
                .child(kv("Выгрузка модели", "после простоя"));
            let mut opts = div().flex().items_center().gap_1();
            for (label, mins) in [
                ("5 мин", Some(5u64)),
                ("10 мин", Some(10)),
                ("30 мин", Some(30)),
                ("∞", None),
            ] {
                let selected = mins == current_min;
                opts = opts.child(
                    seg_opt(&format!("unload-{label}"), label, selected).on_click(cx.listener(
                        move |this, _, _, cx| {
                            this.set_idle_unload_min(mins);
                            cx.notify();
                        },
                    )),
                );
            }
            unload_row = unload_row.child(opts);
            card_el = card_el.child(unload_row);

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

        // Native-feel titlebar (Agenda pattern): the strip is a
        // WindowControlArea::Drag region (HTCAPTION → native move/snap), the
        // trailing controls are platform hitboxes — Windows handles press,
        // snap flyout and the close button; close destroys the window while
        // the worker keeps dictation running.
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(c(BG()))
            .child(
                div()
                    .h(px(30.))
                    .w_full()
                    .flex_none()
                    .flex()
                    .border_b_1()
                    .border_color(fade(FG(), 0.10))
                    .child(
                        div()
                            .id("titlebar-drag")
                            .flex_1()
                            .h_full()
                            .flex()
                            .items_center()
                            .px_3()
                            .window_control_area(WindowControlArea::Drag)
                            .child(
                                div()
                                    .text_size(px(12.))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(fade(FG(), 0.9))
                                    .child("Mundus Dictation"),
                            ),
                    )
                    .child(caption_btn("–", WindowControlArea::Min, false))
                    .child(caption_btn("×", WindowControlArea::Close, true)),
            )
            .child(col)
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
    }
    for model in varr(&models, "models").iter().take(10) {
        let id = vstr(model, "id");
        let use_id = id.clone();
        let download_id = id.clone();
        let ask_id = id.clone();
        let mut r = row(
            vstr(model, "name"),
            format!(
                "{:.0} МБ{}",
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

/// Native caption button: the platform hit-tests the WindowControlArea, we
/// only draw the glyph + hover state (Windows convention 46px wide).
fn caption_btn(label: &'static str, area: WindowControlArea, danger: bool) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("cap-{area:?}")))
        .w(px(46.))
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .text_color(fade(FG(), 0.75))
        .window_control_area(area)
        .hover(move |el| {
            if danger {
                el.bg(fade(0xe81123, 1.0)).text_color(fade(0xffffff, 1.0))
            } else {
                el.bg(fade(FG(), 0.10))
            }
        })
        .child(label)
}

/// Segmented-option chip (idle-unload selector) — small clickable token,
/// highlighted when `selected`.
fn seg_opt(id: &str, label: &'static str, selected: bool) -> Stateful<Div> {
    let el = div()
        .id(SharedString::from(id.to_string()))
        .px(px(7.))
        .py(px(2.))
        .rounded(px(4.))
        .text_size(px(11.))
        .cursor_pointer()
        .child(label);
    if selected {
        el.bg(c(ACCENT())).text_color(c(BG()))
    } else {
        el.bg(fade(FG(), 0.08))
            .text_color(fade(FG(), 0.75))
            .hover(|s| s.bg(fade(FG(), 0.14)))
    }
}
