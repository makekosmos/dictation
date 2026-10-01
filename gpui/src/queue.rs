//! «Статистика» and «Очередь распознавания» cards for the status window —
//! `dictation.get_stats`/`reset_stats` and the pending queue
//! (`list_pending`/`retry`/`discard`/`retry_all`/`discard_all`). Split out
//! of view.rs (source-size gate).
use ::gpui::{prelude::*, *};
use serde_json::json;

use crate::app::{Confirm, DictationApp};
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

/// Inline confirm row shared by both cards — shows the action's consequences
/// and the confirming button issues the op.
fn confirm_row(
    question: &str,
    hint: &str,
    yes_label: &'static str,
    yes: impl Fn(&mut DictationApp) + 'static,
    cx: &mut Context<DictationApp>,
) -> Div {
    row(question, hint)
        .child(btn(
            "dict-confirm-yes",
            yes_label,
            false,
            cx,
            move |this, _| {
                this.confirm = None;
                yes(this);
            },
        ))
        .child(btn(
            "dict-confirm-no",
            "Отмена",
            false,
            cx,
            |this, _| {
                this.confirm = None;
            },
        ))
}

/// Aggregate dictation metrics (`dictation.get_stats`) with a reset action.
pub(crate) fn stats_card(app: &mut DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    slot_or(app, "dictation.stats", |v| {
        let mut el = card().child(
            row("Статистика", "Считается по всем сессиям диктовки").child(btn(
                "dict-stats-reset",
                "Сбросить",
                false,
                cx,
                |this, _| this.ask_confirm(Confirm::ResetStats),
            )),
        );
        if matches!(&app.confirm, Some(Confirm::ResetStats)) {
            el = el.child(confirm_row(
                "Сбросить статистику?",
                "Счётчики слов и сессий обнулятся",
                "Сбросить",
                |this| this.action("dictation.reset_stats", json!({})),
                cx,
            ));
        }
        el = el.child(kv("Сессий", vstr(v, "totalSessions")));
        el = el.child(kv("Слов", vstr(v, "totalWords")));
        el = el.child(kv(
            "Записано",
            format!("{:.0} сек.", vnum(v, "totalRecordSeconds")),
        ));
        el = el.child(kv("Скорость", format!("{:.0} слов/мин", vnum(v, "wpm"))));
        el = el.child(kv(
            "Сэкономлено",
            format!("{:.0} сек. печати", vnum(v, "timeSavedSeconds")),
        ));
        el.into_any_element()
    })
}

/// Pending/failed recordings kept on disk after a network or key failure —
/// per-item retry/discard plus bulk actions
/// (`dictation.retry_all`/`discard_all`).
pub(crate) fn pending_card(app: &mut DictationApp, cx: &mut Context<DictationApp>) -> AnyElement {
    slot_or(app, "dictation.pending", |v| {
        let items = varr(v, "items");
        let mut el = card().child(
            row(
                "Очередь распознавания",
                "Записи, не дошедшие до распознавания (сбой сети или ключа)",
            )
            .child(badge(format!("{}", items.len()), MUTED_FG())),
        );
        if items.is_empty() {
            el = el.child(empty("Очередь пуста"));
        }
        for item in items.iter().take(20) {
            let uuid = vstr(item, "uuid");
            let retry_uuid = uuid.clone();
            let discard_uuid = uuid.clone();
            el = el.child(
                row(
                    uuid.chars().take(8).collect::<String>(),
                    format!(
                        "{} · {:.0} сек. · попыток: {:.0}{}",
                        vstr(item, "createdAt"),
                        vnum(item, "durationSec"),
                        vnum(item, "attempts"),
                        vopt(item, "lastError")
                            .map(|e| format!(" · {e}"))
                            .unwrap_or_default()
                    ),
                )
                .child(btn_id(
                    &format!("dict-retry-{uuid}"),
                    "Повторить",
                    {
                        cx.listener(move |this, _, _, cx| {
                            this.action("dictation.retry", json!({"uuid": retry_uuid}));
                            cx.notify();
                        })
                    },
                ))
                .child(btn_id(&format!("dict-discard-{uuid}"), "Удалить", {
                    cx.listener(move |this, _, _, cx| {
                        this.action("dictation.discard", json!({"uuid": discard_uuid}));
                        cx.notify();
                    })
                })),
            );
        }
        if !items.is_empty() {
            el = el.child(
                div()
                    .flex()
                    .gap_2()
                    .child(btn_id("dict-retry-all", "Повторить все", {
                        cx.listener(|this, _, _, cx| {
                            this.action("dictation.retry_all", json!({}));
                            cx.notify();
                        })
                    }))
                    .child(btn_id("dict-discard-all", "Удалить все", {
                        cx.listener(|this, _, _, cx| {
                            this.ask_confirm(Confirm::DiscardAll);
                            cx.notify();
                        })
                    })),
            );
        }
        if matches!(&app.confirm, Some(Confirm::DiscardAll)) {
            el = el.child(confirm_row(
                "Удалить всю очередь?",
                "Записи будут удалены без распознавания",
                "Удалить все",
                |this| this.action("dictation.discard_all", json!({})),
                cx,
            ));
        }
        el.into_any_element()
    })
}
