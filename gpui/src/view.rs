//! Status window for the standalone dictation app: record control, last
//! transcript and the local-model manager. Settings live in `settings.rs`,
//! the recognition queue and stats in `queue.rs`. The pill overlay itself
//! is `pill.rs` — a separate always-on-top window.
use ::gpui::{prelude::*, *};
use gpui_component::scroll::ScrollableElement;

use crate::app::{DictationApp, Feed};
use crate::button::btn;
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
        // Theme mirrors Mundus — re-apply when the Engine snapshot changed.
        let appearance = self.data(Feed::Appearance.slot());
        crate::theme::sync_theme(&appearance, window, cx);

        let state = self.data(Feed::State.slot());
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
        let label = match phase {
            Some(PillPhase::Starting) => "Запуск записи…",
            Some(PillPhase::Recording) => "Остановить запись",
            Some(PillPhase::Processing) => "Распознаю…",
            None => "Начать запись",
        };
        let busy = phase.is_some();
        let record = plate().child(
            label_row("Диктовка")
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
        col = col.child(record);

        // --- Хоткей ---
        col = col.child(crate::settings::hotkey_card(self, cx));

        // --- Модель ---
        col = col.child(crate::settings::models_card(self, window, cx));

        // --- Выгрузка модели ---
        col = col.child(crate::settings::unload_card(self, cx));

        // --- Настройки (Engine config mirror + update_config controls) ---
        col = col.child(crate::settings::config_card(self, window, cx));

        // --- Последняя расшифровка ---
        let result = self.data("dictation.result");
        if !result.is_null() {
            let text = vopt(&result, "text").unwrap_or_default();
            let delivery = vstr(&result, "delivery");
            let mut card_el = plate().child(
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
        // Статистика собирается пассивно — карточку пока не показываем.

        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(c(BG()))
            .child(titlebar(window))
            .child(col)
    }
}

/// Imago settings plaque (r12 soft fg-wash card, no border) — the shared
/// card chrome for the status window, same component manager uses.
pub(crate) fn plate() -> Div {
    imago_gpui::settings::UiStyle::default().card()
}

/// Label-only row: the kit's `row` minus the muted subtitle line.
pub(crate) fn label_row(label: impl Into<String>) -> Div {
    div()
        .w_full()
        .min_h_10()
        .flex()
        .items_center()
        .gap_3()
        .child(
            div()
                .flex_1()
                .min_w_0()
                .whitespace_nowrap()
                .text_size(px(13.))
                .child(label.into()),
        )
}

/// Shared imago chrome titlebar (Agenda/Manager pattern): a
/// WindowControlArea::Drag region with the left inset clearing the native
/// macOS traffic lights (reclaimed in fullscreen). Windows gets the kit's
/// caption hitboxes; macOS draws its own — do not duplicate min/close.
/// Close destroys the window while the worker keeps dictation running.
fn titlebar(window: &Window) -> Div {
    let drag = div()
        .id("titlebar-drag")
        .flex_1()
        .h_full()
        .window_control_area(WindowControlArea::Drag);
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
