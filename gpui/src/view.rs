//! Status window for the standalone dictation app: record control, last
//! transcript and the local-model manager. Settings live in `settings.rs`,
//! the recognition queue and stats in `queue.rs`. The pill overlay itself
//! is `pill.rs` — a separate always-on-top window.
use ::gpui::{prelude::*, *};
use gpui_component::scroll::ScrollableElement;
use gpui_component::InteractiveElementExt;
use serde_json::json;

use crate::app::{DictationApp, Feed};
use crate::pill::PillPhase;
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

fn state_label(state: &str) -> &'static str {
    match state {
        "recording" | "capturing" => "Запись",
        "transcribing" => "Распознаю",
        "waiting" => "Жду сеть",
        "error" => "Ошибка",
        _ => "Готов",
    }
}

impl Render for DictationApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let state = self.data(Feed::State.slot());
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
                        "recording" | "capturing" | "error" => DESTRUCTIVE(),
                        "transcribing" | "waiting" => WARN(),
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
                            .map(str::trim)
                            .filter(|p| !p.is_empty())
                            .map(|p| crate::pill::kbd(p.to_string())),
                    ),
            );
        }
        col = col.child(record);

        // --- Настройки (Engine config mirror + update_config controls) ---
        col = col.child(crate::settings::config_card(self, cx));

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
                meta.push(format!("запись {}", fmt_duration(ms)));
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

        // --- Очередь распознавания + статистика ---
        col = col.child(crate::queue::pending_card(self, cx));
        col = col.child(crate::queue::stats_card(self, cx));

        // --- Локальные модели ---
        col = col.child(models_card(self, cx));

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(c(BG()))
            .child(titlebar(window))
            .child(col)
    }
}

/// Shared imago chrome titlebar (Agenda/Manager pattern): a
/// WindowControlArea::Drag region with the left inset clearing the native
/// macOS traffic lights (reclaimed in fullscreen). Windows gets the kit's
/// caption hitboxes; macOS draws its own — do not duplicate min/close.
/// Close destroys the window while the worker keeps dictation running.
fn titlebar(window: &Window) -> Div {
    #[allow(unused_mut)]
    let mut drag = div()
        .id("titlebar-drag")
        .flex_1()
        .h_full()
        .window_control_area(WindowControlArea::Drag);
    #[cfg(target_os = "macos")]
    {
        drag = drag.on_double_click(|_, window, _| window.titlebar_double_click());
    }
    imago_gpui::chrome::titlebar()
        .p_0()
        .w_full()
        .border_b_1()
        .border_color(fade(FG(), 0.10))
        .child(
            div()
                .w(px(
                    if cfg!(target_os = "macos") && !window.is_fullscreen() {
                        88.0
                    } else {
                        16.0
                    },
                ))
                .h_full()
                .flex_none(),
        )
        .child(drag)
        .when(cfg!(target_os = "macos"), |bar| bar.pr_3())
        .when(!cfg!(target_os = "macos"), |bar| {
            bar.child(imago_gpui::chrome::window_controls())
        })
}

fn models_card(app: &mut DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    let local = app.data(Feed::Local.slot());
    let models = app.data(Feed::Models.slot());
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
                        this.ask_confirm(crate::app::Confirm::DeleteModel(ask_id.clone()));
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
    if let Some(crate::app::Confirm::DeleteModel(delete_id)) = &app.confirm {
        let id = delete_id.clone();
        el = el.child(crate::queue::confirm_row(
            &format!("Удалить {delete_id}?"),
            "Файлы модели будут удалены с диска",
            "Удалить",
            move |this| this.action("dictation.delete_local_model", json!({"modelId": id})),
            cx,
        ));
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

/// Recording length for the "Последняя расшифровка" card — m:ss (h:mm:ss
/// past an hour). `fmt_ms` from the kit formats relative TIMES ("5 мин.
/// назад"), so applying it to a duration printed "запись 2 мин. назад".
fn fmt_duration(ms: f64) -> String {
    let secs = (ms / 1000.0).round().max(0.0) as u64;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::{fmt_duration, state_label};

    /// A duration is not a relative time: fmt_ms would print "запись 2 мин.
    /// назад" for a 90s take — the card must show clock-style length.
    #[test]
    fn duration_formats_as_clock() {
        assert_eq!(fmt_duration(1_250.0), "0:01");
        assert_eq!(fmt_duration(5_000.0), "0:05");
        assert_eq!(fmt_duration(90_000.0), "1:30");
        assert_eq!(fmt_duration(3_725_000.0), "1:02:05");
        assert_eq!(fmt_duration(0.0), "0:00");
    }

    /// The dotted contract spells the live state "capturing" — it must render
    /// "Запись", not fall through to the green "Готов" badge mid-capture.
    #[test]
    fn capturing_is_recording() {
        assert_eq!(state_label("capturing"), "Запись");
        assert_eq!(state_label("recording"), "Запись");
        assert_eq!(state_label("idle"), "Готов");
    }
}
