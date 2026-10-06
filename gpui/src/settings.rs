//! «Конфигурация» card for the status window: Engine config mirror plus the
//! editable settings (`dictation.update_config` — hotkey, idle-unload,
//! providerEnabled, injectMode, duckAudioDuringRecording, language).
use ::gpui::{prelude::*, *};
use gpui_component::searchable_list::SearchableVec;
use gpui_component::select::{Select, SelectEvent, SelectState};
use gpui_component::Sizable;
use serde_json::json;

use crate::app::{DictationApp, Feed};
use crate::languages::{language_code, language_items, LangItem};
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

fn trigger_label(mode: &str) -> &'static str {
    match mode {
        "push_to_talk" => "Удержание (push-to-talk)",
        _ => "Переключение (toggle)",
    }
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

pub(crate) fn config_card(
    app: &mut DictationApp,
    window: &mut Window,
    cx: &mut Context<DictationApp>,
) -> AnyElement {
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
        // The Select entity needs `&mut Window` at construction, so it is
        // created lazily on the first render with config loaded, then kept
        // on DictationApp (it must outlive individual renders).
        if app.lang_select.is_none() {
            let entity = cx.new(|cx| {
                SelectState::new(SearchableVec::new(language_items()), None, window, cx)
                    .searchable(true)
            });
            // `detach` keeps the Confirm → update_config wiring alive for
            // the entity's lifetime — the entity itself is held on the app.
            cx.subscribe(
                &entity,
                |this, _entity, event: &SelectEvent<SearchableVec<LangItem>>, _cx| {
                    if let SelectEvent::Confirm(Some(code)) = event {
                        this.update_config(json!({ "language": code }));
                    }
                },
            )
            .detach();
            app.lang_select = Some(entity);
        }
        let entity = app.lang_select.clone().unwrap();
        // Mirror `config.language` (first paint, external edits). Programmatic
        // sets don't emit Confirm, so this never rewrites the Engine value;
        // an unknown language deselects to the placeholder instead.
        let selected: Option<&str> = entity.read(cx).selected_value().copied();
        if selected != Some(language.as_str()) {
            entity.update(cx, |state, cx| match language_code(&language) {
                Some(code) => state.set_selected_value(&code, window, cx),
                None => state.set_selected_index(None, window, cx),
            });
        }
        el = el.child(
            crate::view::label_row("Язык распознавания").child(
                div().flex_none().w(px(180.)).child(
                    Select::new(&entity)
                        .w_full()
                        .small()
                        .placeholder("Выберите язык")
                        .search_placeholder("Поиск…")
                        .accessibility_label("Язык распознавания"),
                ),
            ),
        );
    }

    // --- Read-only mirror + idle-unload (already wired) -------------------
    el = el.child(kv("Режим", trigger_label(&vstr(cfg, "triggerMode"))));
    el = el.child(kv("Провайдер", vstr(cfg, "provider")));
    el = el.child(kv("Модель", vstr(cfg, "model")));
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

/// «Хоткей» card: the hotkey picker lives outside «Настройки (Engine)» —
/// same muted placeholder while the Engine state hasn't landed yet.
pub(crate) fn hotkey_card(app: &DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
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
    card()
        .child(crate::view::label_row("Хоткей").child(hotkey_picker(app, cfg, cx)))
        .into_any_element()
}

/// Shortcut picker (zeron's binding_control shape): one clickable combo
/// chip that toggles the Engine capture — click arms
/// `dictation.begin_hotkey_capture`, a second click (or Esc on the Engine
/// side) disarms via `end_hotkey_capture`. While capturing the chip inverts
/// to the accent wash and reads «Нажмите клавиши…».
fn hotkey_picker(
    app: &DictationApp,
    cfg: &serde_json::Value,
    cx: &mut Context<DictationApp>,
) -> Stateful<Div> {
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
        .on_click(cx.listener(|this, _, _, cx| {
            if this.hotkey_capturing {
                this.hotkey_capture_cancel(cx);
            } else {
                this.hotkey_capture_start(cx);
            }
        }))
        .child(chip_text);
    if capturing {
        chip.bg(c(ACCENT()).opacity(0.16))
            .border_color(c(ACCENT()).opacity(0.55))
            .text_color(c(FG()))
    } else {
        chip.bg(fade(FG(), 0.06))
            .border_color(fade(FG(), 0.16))
            .text_color(fade(FG(), 0.8))
            .hover(|s| s.bg(fade(FG(), 0.12)))
    }
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

/// «Модель» card — выбор локальной модели. Кликабельны только строки,
/// которые Engine реально может запустить (скачаны + поддерживают
/// транскриб + для whisper-семейства стоит whisper.cpp sidecar):
/// `use_local_model` всё равно отклонил бы остальные, показываем причину
/// muted-текстом вместо бесполезного клика.
pub(crate) fn models_card(app: &DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    let data = app.data(Feed::Models.slot());
    let command_installed = vbool(&data, "commandInstalled");
    let models = vget(&data, "models")
        .as_array()
        .cloned()
        .unwrap_or_default();

    let mut el = card().child(
        div()
            .text_size(px(12.))
            .text_color(c(MUTED_FG()))
            .child("Модель"),
    );

    let mut any_usable = false;
    for model in models {
        let id = vstr(&model, "id").to_string();
        let name = vstr(&model, "name").to_string();
        let downloaded = vbool(&model, "downloaded");
        let selected = vbool(&model, "selected");
        let supported = vbool(&model, "transcriptionSupported");
        let usable = downloaded && supported && (id.starts_with("parakeet") || command_installed);
        any_usable |= usable;

        let status: SharedString = if selected {
            "Выбрана".into()
        } else if !downloaded {
            "Не скачана".into()
        } else if !supported {
            "Не для распознавания".into()
        } else if !id.starts_with("parakeet") && !command_installed {
            "Нужен whisper.cpp".into()
        } else {
            "Скачана".into()
        };

        let mut row = div()
            .id(SharedString::from(format!("model-{id}")))
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
                    .child(name),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(12.))
                    .when_else(
                        selected,
                        |el| el.text_color(c(ACCENT())),
                        |el| el.text_color(c(MUTED_FG())),
                    )
                    .child(status),
            );
        if usable && !selected {
            let id2 = id.clone();
            row = row
                .cursor_pointer()
                .on_click(cx.listener(move |this, _, _, _| this.use_local_model(&id2)));
        }
        el = el.child(row);
    }
    if !any_usable {
        el = el.child(
            div()
                .text_size(px(12.))
                .text_color(c(MUTED_FG()))
                .child("Скачай модель в Manager → «Модели»"),
        );
    }
    el.into_any_element()
}
