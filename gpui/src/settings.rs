//! «Конфигурация» card for the status window: Engine config mirror plus the
//! editable settings (`dictation.update_config` — hotkey, idle-unload,
//! providerEnabled, injectMode, duckAudioDuringRecording, language). Split
//! out of view.rs (source-size gate).
use ::gpui::{prelude::*, *};
use serde_json::json;

use crate::app::DictationApp;
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

fn trigger_label(mode: &str) -> &'static str {
    match mode {
        "push_to_talk" => "Удержание (push-to-talk)",
        _ => "Переключение (toggle)",
    }
}

pub(crate) fn config_card(app: &mut DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    let state = app.data("dictation.state");
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
    el = el.child(
        row(
            "Диктовка включена",
            "Выключает распознавание, запись по клавише останется",
        )
        .child(
            toggle(
                "dict-provider-enabled",
                vbool(cfg, "providerEnabled"),
                cx,
                |this, checked, _| {
                    this.action(
                        "dictation.update_config",
                        json!({ "providerEnabled": checked }),
                    );
                },
            )
            .accessibility_label("Диктовка включена"),
        ),
    );
    el = el.child(
        row(
            "Вставлять текст автоматически",
            "Иначе результат только копируется в буфер обмена",
        )
        .child(
            toggle(
                "dict-autopaste",
                vstr(cfg, "injectMode") != "clipboard_only",
                cx,
                |this, checked, _| {
                    this.action(
                        "dictation.update_config",
                        json!({ "injectMode": if checked { "auto_paste" } else { "clipboard_only" } }),
                    );
                },
            )
            .accessibility_label("Вставлять текст автоматически"),
        ),
    );
    el = el.child(
        row(
            "Приглушать звук при записи",
            "Понижает системную громкость, пока идёт диктовка",
        )
        .child(
            toggle(
                "dict-duck",
                vbool(cfg, "duckAudioDuringRecording"),
                cx,
                |this, checked, _| {
                    this.action(
                        "dictation.update_config",
                        json!({ "duckAudioDuringRecording": checked }),
                    );
                },
            )
            .accessibility_label("Приглушать звук при записи"),
        ),
    );
    {
        let language = vstr(cfg, "language");
        let mut opts = div().flex().items_center().gap_1();
        for lang in ["ru", "en", "auto"] {
            opts = opts.child(
                seg_opt(&format!("dict-lang-{lang}"), lang, language == lang).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.action("dictation.update_config", json!({ "language": lang }));
                        cx.notify();
                    }),
                ),
            );
        }
        el = el.child(row("Язык распознавания", "«auto» — определить автоматически").child(opts));
    }

    // --- Read-only mirror + hotkey/idle-unload (already wired) ------------
    el = el.child(kv("Режим", trigger_label(&vstr(cfg, "triggerMode"))));
    el = el.child(kv(
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
    el = el.child(kv("Модель", vstr(cfg, "model")));
    el = el.child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .child(kv("Хоткей", vstr(cfg, "hotkey")))
            .child(if app.hotkey_capturing {
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .text_size(px(11.))
                            .text_color(c(WARN()))
                            .child("Нажмите комбинацию… (Esc — отмена)"),
                    )
                    .child(
                        btn_id("dict-hotkey-cancel", "Отмена", {
                            cx.listener(|this, _, _, cx| {
                                this.hotkey_capture_cancel(cx);
                            })
                        })
                        .into_any_element(),
                    )
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
    el = el.child(unload_row);

    if let Some(err) = vopt(&state, "lastError") {
        el = el.child(kv("Последняя ошибка", err));
    }
    el.into_any_element()
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
