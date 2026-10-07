//! Status window for the standalone dictation app: record control, last
//! transcript and the local-model manager. Settings live in `settings.rs`,
//! the recognition queue and stats in `queue.rs`. The pill overlay itself
//! is `pill.rs` — a separate always-on-top window.
use ::gpui::{prelude::*, *};
use gpui_component::notification::Notification;
use gpui_component::scroll::ScrollableElement;
use gpui_component::WindowExt;

use crate::app::{DictationApp, Feed};
use crate::button::btn;
use crate::pill::PillPhase;
use mundus_gpui_kit::theme::*;

#[cfg(test)]
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

        // Errors and notices go through the shared toast layer — no
        // persistent banners in the window. Push once per change.
        if let (Some(error), true) = (
            self.error.clone(),
            self.toasted_error.as_ref() != self.error.as_ref(),
        ) {
            self.toasted_error = Some(error.clone());
            window.push_notification(Notification::error(error), cx);
        }
        if self.error.is_none() {
            self.toasted_error = None;
        }
        if let (Some(notice), true) = (
            self.notice.clone(),
            self.toasted_notice.as_ref() != self.notice.as_ref(),
        ) {
            self.toasted_notice = Some(notice.clone());
            window.push_notification(Notification::success(notice), cx);
        }
        if self.notice.is_none() {
            self.toasted_notice = None;
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
        col = col.child(crate::settings::language_model_card(self, window, cx));

        // --- Настройки (Engine config mirror + update_config controls) ---
        col = col.child(crate::settings::config_card(self, window, cx));

        // --- Очередь распознавания + статистика ---
        let latest_result = self.data("dictation.result");
        col = col.child(crate::queue::pending_card(self, &latest_result, cx));
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

#[cfg(test)]
mod tests {
    use super::state_label;

    /// The dotted contract spells the live state "capturing" — it must render
    /// "Запись", not fall through to the green "Готов" badge mid-capture.
    #[test]
    fn capturing_is_recording() {
        assert_eq!(state_label("capturing"), "Запись");
        assert_eq!(state_label("recording"), "Запись");
        assert_eq!(state_label("idle"), "Готов");
    }
}
