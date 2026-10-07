//! «Очередь распознавания» card — history of every transcription attempt
//! (`list_pending`/`retry`/`discard`/`retry_all`/`discard_all`). Stats are
//! collected by the Engine passively; the card is hidden for now.
use ::gpui::{prelude::*, *};
use chrono::{Datelike, Timelike};
use serde_json::{json, Value};

use crate::app::{Confirm, DictationApp, Feed};
use crate::button::{btn, btn_id};
use mundus_gpui_kit::fields::*;
use mundus_gpui_kit::theme::*;

const MONTHS_SHORT: [&str; 12] = [
    "янв.",
    "фев.",
    "мар.",
    "апр.",
    "мая",
    "июн.",
    "июл.",
    "авг.",
    "сен.",
    "окт.",
    "нояб.",
    "дек.",
];

/// Row title for a queue item: the recording's local «день месяц, чч:мм».
/// `createdAt` arrives as RFC 3339 (Engine writes `to_rfc3339`); a missing
/// or malformed value falls back to its date part, or «—» — never panics.
fn pending_title(created_at: &str) -> String {
    match chrono::DateTime::parse_from_rfc3339(created_at) {
        Ok(dt) => {
            let local = dt.with_timezone(&chrono::Local);
            format_day_time(local.day(), local.month(), local.hour(), local.minute())
        }
        Err(_) => {
            let fallback = created_at.split('T').next().unwrap_or("").trim();
            if fallback.is_empty() {
                "—".into()
            } else {
                fallback.to_string()
            }
        }
    }
}

fn format_day_time(day: u32, month: u32, hour: u32, minute: u32) -> String {
    let month = MONTHS_SHORT
        .get(month.saturating_sub(1) as usize)
        .copied()
        .unwrap_or("?");
    format!("{day} {month}, {hour:02}:{minute:02}")
}

/// Inline confirm row shared by the stats, queue and model cards — shows the action's consequences
/// and the confirming button issues the op.
pub(crate) fn confirm_row(
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

/// Recording length — m:ss (h:mm:ss past an hour).
fn fmt_duration(ms: f64) -> String {
    let secs = (ms / 1000.0).round().max(0.0) as u64;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

/// Pending/failed recordings kept on disk after a network or key failure —
/// per-item retry/discard plus bulk actions
/// (`dictation.retry_all`/`discard_all`). Every item renders: the bulk
/// actions cover the whole queue, so silently truncating the list would lie
/// about what they affect.
pub(crate) fn pending_card(
    app: &mut DictationApp,
    latest_result: &Value,
    cx: &mut Context<DictationApp>,
) -> AnyElement {
    slot_or(app, Feed::Pending.slot(), |v| {
        let items = varr(v, "items");
        let open = app.queue_open;
        let has_result = !latest_result.is_null();
        let header = div()
            .id("dict-queue-toggle")
            .w_full()
            .rounded_t(px(12.))
            .when(!open, |el| el.rounded_b(px(12.)))
            .flex()
            .items_center()
            .gap_3()
            .mx(px(imago_gpui::settings::INSET))
            .py(px(12.))
            .min_h(px(60.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.))
                    .child("История распознаваний"),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(c(MUTED_FG()))
                    .child(format!("{}", items.len())),
            )
            .child(
                div()
                    .rounded(px(4.))
                    .p(px(2.))
                    .hover(|s| s.bg(fade(FG(), 0.08)))
                    .child(
                        gpui_component::Icon::default()
                            .path(if open {
                                "icons/chevron-up.svg"
                            } else {
                                "icons/chevron-down.svg"
                            })
                            .size(px(16.))
                            .text_color(c(MUTED_FG())),
                    ),
            )
            .cursor_pointer()
            .on_click(cx.listener(|this, _, _, cx| {
                this.queue_open = !this.queue_open;
                cx.notify();
            }));
        let mut el = imago_gpui::settings::UiStyle::default()
            .settings_card()
            .child(header);
        if !open {
            return el.into_any_element();
        }
        el = el.child(
            div()
                .w_full()
                .border_t_1()
                .border_color(fade(BORDER(), 0.6)),
        );
        let mut first_row = true;
        if has_result {
            let text = vopt(latest_result, "text").unwrap_or_default();
            let delivery = vstr(latest_result, "delivery");
            let mut meta = Vec::new();
            if !delivery.is_empty() {
                meta.push(format!("delivery: {delivery}"));
            }
            let ms = vnum(latest_result, "durationMs");
            if ms > 0.0 {
                meta.push(format!("запись {}", fmt_duration(ms)));
            }
            let sub = if text.is_empty() {
                meta.join(" · ")
            } else {
                let preview: String = text.chars().take(80).collect();
                let preview = if text.chars().count() > 80 {
                    format!("{preview}…")
                } else {
                    preview
                };
                if meta.is_empty() {
                    preview
                } else {
                    format!("{} · {preview}", meta.join(" · "))
                }
            };
            let mut result_row = imago_gpui::settings::UiStyle::default()
                .card_row(true)
                .child(row("Последняя", sub));
            if let Some(err) = vopt(latest_result, "error") {
                result_row = result_row.child(
                    div()
                        .text_size(px(11.))
                        .text_color(c(MUTED_FG()))
                        .child(format!("Ошибка: {err}")),
                );
            }
            result_row = result_row.child(btn(
                "dict-dismiss-result",
                "Скрыть",
                false,
                cx,
                |this, cx| this.dismiss_result(cx),
            ));
            el = el.child(result_row);
            first_row = false;
        }
        if items.is_empty() && !has_result {
            el = el.child(
                imago_gpui::settings::UiStyle::default()
                    .card_row(first_row)
                    .child(empty("Очередь пуста")),
            );
        }
        for item in items.iter() {
            let uuid = vstr(item, "uuid");
            let delivered = vstr(item, "status") == "delivered";
            if delivered {
                // История доставленной расшифровки: дата + текст, без кнопок.
                let text = vstr(item, "transcript");
                let preview: String = text.chars().take(80).collect();
                let preview = if text.chars().count() > 80 {
                    format!("{preview}…")
                } else {
                    preview
                };
                let title = pending_title(&vstr(item, "createdAt"));
                let sub = format!("{:.0} сек. · {preview}", vnum(item, "durationSec"));
                el = el.child(
                    imago_gpui::settings::UiStyle::default()
                        .card_row(first_row)
                        .child(row(title, sub)),
                );
                first_row = false;
                continue;
            }
            let in_flight = app.pending_inflight.contains(&uuid);
            let retry_uuid = uuid.clone();
            let discard_uuid = uuid.clone();
            el = el.child(
                imago_gpui::settings::UiStyle::default()
                    .card_row(first_row)
                    .child(
                        row(
                            pending_title(&vstr(item, "createdAt")),
                            // lastError arrives as the Engine's own message — the app
                            // shows Engine errors verbatim elsewhere too.
                            format!(
                                "{:.0} сек. · попыток: {:.0}{}",
                                vnum(item, "durationSec"),
                                vnum(item, "attempts"),
                                vopt(item, "lastError")
                                    .map(|e| format!(" · {e}"))
                                    .unwrap_or_default()
                            ),
                        )
                        .child(
                            btn_id(&format!("dict-retry-{uuid}"), "Повторить", {
                                cx.listener(move |this, _, _, cx| {
                                    this.queue_retry(retry_uuid.clone());
                                    cx.notify();
                                })
                            })
                            .disabled(in_flight),
                        )
                        .child(
                            btn_id(&format!("dict-discard-{uuid}"), "Удалить", {
                                cx.listener(move |this, _, _, cx| {
                                    this.queue_discard(discard_uuid.clone());
                                    cx.notify();
                                })
                            })
                            .disabled(in_flight),
                        ),
                    ),
            );
            first_row = false;
        }
        let any_pending = items.iter().any(|i| vstr(i, "status") != "delivered");
        if !items.is_empty() {
            let mut bulk = imago_gpui::settings::UiStyle::default()
                .card_row(false)
                .gap(px(8.));
            if any_pending {
                bulk = bulk.child(btn_id("dict-retry-all", "Повторить все", {
                    cx.listener(|this, _, _, cx| {
                        this.action("dictation.retry_all", json!({}));
                        cx.notify();
                    })
                }));
            }
            bulk = bulk.child(btn_id(
                "dict-discard-all",
                "Очистить список",
                {
                    cx.listener(|this, _, _, cx| {
                        this.ask_confirm(Confirm::DiscardAll);
                        cx.notify();
                    })
                },
            ));
            el = el.child(bulk);
        }
        if matches!(&app.confirm, Some(Confirm::DiscardAll)) {
            el = el.child(
                imago_gpui::settings::UiStyle::default()
                    .card_row(false)
                    .child(confirm_row(
                        "Очистить список?",
                        "Все записи будут удалены без восстановления",
                        "Очистить",
                        |this| this.action("dictation.discard_all", json!({})),
                        cx,
                    )),
            );
        }
        el.into_any_element()
    })
}

#[cfg(test)]
mod tests {
    use super::{fmt_duration, format_day_time, pending_title};

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
    use chrono::{DateTime, Datelike, Timelike};

    /// Queue rows render a local «день месяц, чч:мм» — parse must convert to
    /// the machine's timezone and never panic on garbage.
    #[test]
    fn pending_title_formats_rfc3339_in_local_time() {
        let title = pending_title("2026-10-01T12:32:00+00:00");
        // Not asserting the rendered hour (host timezone varies): check the
        // shape «d мон., HH:MM» and that the UTC→local shift happened
        // consistently with chrono's own conversion.
        let local = DateTime::parse_from_rfc3339("2026-10-01T12:32:00+00:00")
            .unwrap()
            .with_timezone(&chrono::Local);
        assert_eq!(
            title,
            format_day_time(local.day(), local.month(), local.hour(), local.minute())
        );
        assert_eq!(format_day_time(1, 10, 14, 32), "1 окт., 14:32");
    }

    /// Malformed or missing `createdAt` must not panic or produce an empty
    /// title — the date part of the raw value is a readable fallback.
    #[test]
    fn pending_title_falls_back_on_bad_input() {
        assert_eq!(pending_title(""), "—");
        assert_eq!(pending_title("not a date"), "not a date");
        assert_eq!(pending_title("2026-13-45T99:99"), "2026-13-45");
        assert_eq!(format_day_time(1, 13, 0, 0), "1 ?, 00:00");
    }
}
