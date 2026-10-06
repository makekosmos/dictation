//! «Конфигурация» card for the status window: Engine config mirror plus the
//! editable settings (`dictation.update_config` — hotkey, idle-unload,
//! providerEnabled, injectMode, duckAudioDuringRecording, language).
use ::gpui::{prelude::*, *};
use serde_json::json;

use crate::app::{DictationApp, Feed};
use crate::button::btn_id;
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

fn trigger_label(mode: &str) -> &'static str {
    match mode {
        "push_to_talk" => "Удержание (push-to-talk)",
        _ => "Переключение (toggle)",
    }
}

/// Recognition-language chips: (wire code, display label, selected). An
/// unknown `config.language` selects nothing — the selector then just
/// doesn't overwrite it until the user picks a chip.
fn language_options(current: &str) -> [(&'static str, &'static str, bool); 3] {
    [
        ("ru", "Русский", current == "ru"),
        ("en", "English", current == "en"),
        ("auto", "Авто", current == "auto"),
    ]
}

/// One on/off setting: a labelled row whose toggle sends the
/// `dictation.update_config` patch `patch(checked)`.
fn toggle_row(
    id: &'static str,
    label: &'static str,
    checked: bool,
    patch: impl Fn(bool) -> serde_json::Value + 'static,
    cx: &mut Context<DictationApp>,
) -> Div {
    crate::view::label_row(label).child(
        toggle(id, checked, cx, move |this, on, _| {
            this.update_config(patch(on))
        })
        .accessibility_label(label),
    )
}

pub(crate) fn config_card(app: &mut DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    let state = app.data(Feed::State.slot());
    if state.is_null() {
        return card()
            .child(
                div()
                    .text_size(px(12.))
                    .text_color(c(MUTED_FG()))
                    .child("Подключение к Engine…"),
            )
            .into_any_element();
    }
    let cfg = vget(&state, "config");

    let mut el = card().child(
        div()
            .text_size(px(12.))
            .text_color(c(MUTED_FG()))
            .child("Настройки (Engine)"),
    );

    // --- Editable settings (dictation.update_config — applies live) -------
    el = el
        .child(toggle_row(
            "dict-provider-enabled",
            "Диктовка включена",
            vbool(cfg, "providerEnabled"),
            |on| json!({ "providerEnabled": on }),
            cx,
        ))
        .child(toggle_row(
            "dict-autopaste",
            "Вставлять текст автоматически",
            vstr(cfg, "injectMode") != "clipboard_only",
            |on| json!({ "injectMode": if on { "auto_paste" } else { "clipboard_only" } }),
            cx,
        ))
        .child(toggle_row(
            "dict-duck",
            "Приглушать звук при записи",
            vbool(cfg, "duckAudioDuringRecording"),
            |on| json!({ "duckAudioDuringRecording": on }),
            cx,
        ));
    {
        let language = vstr(cfg, "language");
        let mut opts = div().flex().items_center().gap_1();
        for (code, label, selected) in language_options(&language) {
            opts = opts.child(
                seg_opt(&format!("dict-lang-{code}"), label, selected).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.update_config(json!({ "language": code }));
                        cx.notify();
                    },
                )),
            );
        }
        el = el.child(crate::view::label_row("Язык распознавания").child(opts));
    }

    // --- Read-only mirror + hotkey/idle-unload (already wired) ------------
    el = el.child(kv("Режим", trigger_label(&vstr(cfg, "triggerMode"))));
    el = el.child(kv("Провайдер", vstr(cfg, "provider")));
    el = el.child(kv("Модель", vstr(cfg, "model")));
    el = el.child(crate::view::label_row("Хоткей").child(hotkey_picker(app, cfg, cx)));
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
    el = el.child(unload_row);

    if let Some(err) = vopt(&state, "lastError") {
        el = el.child(kv("Последняя ошибка", err));
    }
    el.into_any_element()
}

/// Shortcut picker (zeron's binding_control shape): one clickable combo
/// chip — click arms the Engine capture (`dictation.begin_hotkey_capture`),
/// Esc cancels on the Engine side; the explicit Отмена next to the chip
/// disarms via `end_hotkey_capture`. While capturing the chip inverts to
/// the accent wash and reads «Нажмите клавиши…».
fn hotkey_picker(
    app: &DictationApp,
    cfg: &serde_json::Value,
    cx: &mut Context<DictationApp>,
) -> Div {
    let capturing = app.hotkey_capturing;
    let chip_text: SharedString = if capturing {
        "Нажмите клавиши…".into()
    } else {
        vstr(cfg, "hotkey").into()
    };
    let chip = div()
        .id("dict-hotkey-capture")
        .min_w(px(96.))
        .h(px(28.))
        .px(px(12.))
        .rounded(px(8.))
        .border_1()
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(12.))
        .cursor_pointer()
        .on_click(cx.listener(|this, _, _, cx| this.hotkey_capture_start(cx)))
        .child(chip_text);
    let chip = if capturing {
        chip.bg(c(ACCENT()).opacity(0.16))
            .border_color(c(ACCENT()).opacity(0.55))
            .text_color(c(FG()))
    } else {
        chip.bg(fade(FG(), 0.06))
            .border_color(fade(FG(), 0.16))
            .text_color(fade(FG(), 0.8))
            .hover(|s| s.bg(fade(FG(), 0.12)))
    };
    let mut row = div().flex().items_center().gap_2().child(chip);
    if capturing {
        row = row.child(
            btn_id("dict-hotkey-cancel", "Отмена", {
                cx.listener(|this, _, _, cx| {
                    this.hotkey_capture_cancel(cx);
                })
            })
            .into_any_element(),
        );
    }
    row
}

/// Segmented-option chip (idle-unload and language selectors) — small
/// clickable token, highlighted when `selected`.
pub(crate) fn seg_opt(id: &str, label: &'static str, selected: bool) -> Stateful<Div> {
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

#[cfg(test)]
mod tests {
    use super::language_options;

    /// Chips show human labels but keep wire codes; an unknown config value
    /// selects nothing, so the selector never silently rewrites it.
    #[test]
    fn language_options_label_and_select() {
        let opts = language_options("en");
        assert_eq!(
            opts,
            [
                ("ru", "Русский", false),
                ("en", "English", true),
                ("auto", "Авто", false),
            ]
        );
        assert!(language_options("fr").iter().all(|(_, _, s)| !s));
    }
}
