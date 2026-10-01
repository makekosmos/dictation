use super::{Command, DictationApp, Worker};
use crate::hotkey::{build_accelerator, vk_to_key_name};
use crate::session::state_broadcast_ended;
use crate::worker::Reply;
use gpui::{AppContext, Entity, TestAppContext};
use serde_json::{json, Value};
use std::sync::mpsc::{Receiver, Sender};

/// App on test channels: the test holds the command receiver (asserts on
/// the Engine ops the UI would issue) and the reply/event senders
/// (injects Engine answers and broadcasts) — no worker threads, no
/// Engine needed.
fn test_app(
    cx: &mut TestAppContext,
) -> (
    Entity<DictationApp>,
    Receiver<Command>,
    Sender<Reply>,
    Sender<Value>,
) {
    let (commands, commands_rx) = std::sync::mpsc::channel();
    let (replies, replies_rx) = std::sync::mpsc::channel();
    let (events, events_rx) = std::sync::mpsc::channel();
    let app = cx.update(|cx| {
        cx.new(|cx| {
            DictationApp::with_worker(
                Worker {
                    commands,
                    replies: replies_rx,
                    events: events_rx,
                },
                cx,
            )
        })
    });
    // Discard the initial refresh() ops.
    while commands_rx.try_recv().is_ok() {}
    (app, commands_rx, replies, events)
}

/// Pops one queued Engine call and asserts its op name and params.
fn expect_rpc(rx: &Receiver<Command>, expected_op: &str, expected_params: Value) {
    match rx.try_recv() {
        Ok(Command::Rpc { op, params, .. }) => {
            assert_eq!(op, expected_op);
            assert_eq!(params, expected_params);
        }
        other => panic!("expected Command::Rpc {expected_op}, got {other:?}"),
    }
}

/// Queue/settings actions must reach the Engine as the exact op+params
/// the Manager used to send (dictation.retry/discard/retry_all/
/// discard_all/reset_stats/update_config).
#[gpui::test]
fn action_sends_engine_op(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    for (op, params) in [
        ("dictation.retry", json!({ "uuid": "u1" })),
        ("dictation.discard", json!({ "uuid": "u1" })),
        ("dictation.retry_all", json!({})),
        ("dictation.discard_all", json!({})),
        ("dictation.reset_stats", json!({})),
        (
            "dictation.update_config",
            json!({ "providerEnabled": false }),
        ),
    ] {
        cx.update(|cx| app.update(cx, |this, _| this.action(op, params.clone())));
        expect_rpc(&rx, op, params);
    }
}

/// A successful mutation refreshes the data slots (the pending list is
/// re-requested) and shows the transient notice.
#[gpui::test]
fn action_success_sets_notice_and_refreshes(cx: &mut TestAppContext) {
    let (app, rx, replies, _events) = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |this, _| {
            this.action("dictation.reset_stats", json!({}))
        })
    });
    rx.try_recv().expect("reset_stats op");
    replies
        .send(Reply {
            slot: "@action".into(),
            result: Ok(json!({})),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert_eq!(this.notice.as_deref(), Some("Выполнено."));
            assert!(this.error.is_none());
        })
    });
    // refresh() re-issued the five data loads.
    for op in [
        "dictation.get_state",
        "dictation.local_status",
        "dictation.list_local_models",
        "dictation.list_pending",
        "dictation.get_stats",
    ] {
        expect_rpc(&rx, op, json!({}));
    }
}

/// An Engine error on a mutation lands in the persistent banner.
#[gpui::test]
fn action_failure_sets_error(cx: &mut TestAppContext) {
    let (app, rx, replies, _events) = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |this, _| {
            this.action("dictation.discard", json!({"uuid": "u1"}))
        })
    });
    rx.try_recv().expect("discard op");
    replies
        .send(Reply {
            slot: "@action".into(),
            result: Err("discard: uuid 'u1' не найден".into()),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert_eq!(this.error.as_deref(), Some("discard: uuid 'u1' не найден"));
            assert!(this.notice.is_none());
        })
    });
}

/// A discarded queue item disappears from the rendered data once the
/// Engine broadcasts `dictation_pending_changed` and answers the
/// refreshed `list_pending` without it.
#[gpui::test]
fn pending_list_updates_after_discard(cx: &mut TestAppContext) {
    let (app, rx, replies, events) = test_app(cx);
    replies
        .send(Reply {
            slot: "dictation.pending".into(),
            result: Ok(json!({ "items": [{ "uuid": "u1", "attempts": 1 }] })),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert_eq!(this.data("dictation.pending")["items"][0]["uuid"], "u1");
        })
    });
    // The discard op succeeds; Engine then broadcasts pending_changed.
    cx.update(|cx| {
        app.update(cx, |this, _| {
            this.action("dictation.discard", json!({"uuid": "u1"}))
        })
    });
    expect_rpc(&rx, "dictation.discard", json!({"uuid": "u1"}));
    events
        .send(json!({ "event": "dictation_pending_changed" }))
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    expect_rpc(&rx, "dictation.list_pending", json!({}));
    replies
        .send(Reply {
            slot: "dictation.pending".into(),
            result: Ok(json!({ "items": [] })),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert!(this.data("dictation.pending")["items"]
                .as_array()
                .expect("items")
                .is_empty());
        })
    });
}

/// Cancel while the hotkey capture is armed sends
/// `dictation.end_hotkey_capture` and clears the flag — otherwise the
/// Engine hook keeps consuming the next keystroke.
#[gpui::test]
fn hotkey_capture_cancel_disarms_engine(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    cx.update(|cx| app.update(cx, |this, cx| this.hotkey_capture_start(cx)));
    cx.update(|cx| app.update(cx, |this, _| assert!(this.hotkey_capturing)));
    expect_rpc(&rx, "dictation.begin_hotkey_capture", json!({}));
    cx.update(|cx| app.update(cx, |this, cx| this.hotkey_capture_cancel(cx)));
    cx.update(|cx| app.update(cx, |this, _| assert!(!this.hotkey_capturing)));
    expect_rpc(&rx, "dictation.end_hotkey_capture", json!({}));
    // Cancelling an inactive capture must not send anything.
    cx.update(|cx| app.update(cx, |this, cx| this.hotkey_capture_cancel(cx)));
    assert!(rx.try_recv().is_err());
}

/// Adopted-session retirement: only a named non-recording broadcast ends
/// the adopted pill — "recording"/"capturing" (live) and a missing state
/// field (no information) must keep it.
#[test]
fn state_broadcast_ended_classification() {
    for ended in ["idle", "transcribing", "pending", "error"] {
        assert!(state_broadcast_ended(Some(ended)), "{ended}");
    }
    for live in ["recording", "capturing"] {
        assert!(!state_broadcast_ended(Some(live)), "{live}");
    }
    assert!(!state_broadcast_ended(None));
}

/// VK_OEM_PLUS must produce the named accelerator token "Plus" — '+' is
/// the accelerator delimiter, so "Ctrl++" parses back as a bare "Ctrl"
/// (the pill footer's own split('+') drops the empty key part too).
#[test]
fn accelerator_names_oem_plus() {
    let event = json!({ "vk": 0xBB, "ctrl": true });
    let accel = build_accelerator(&event).expect("accelerator");
    assert_eq!(accel, "Ctrl+Plus");
    let parts: Vec<&str> = accel.split('+').filter(|p| !p.is_empty()).collect();
    assert_eq!(parts, ["Ctrl", "Plus"]);
    // The other OEM punctuation keys stay literal — '+' is the only
    // collision with the delimiter.
    for vk in [
        0xBAu32, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE,
    ] {
        let accel = build_accelerator(&json!({ "vk": vk, "ctrl": true })).expect("accelerator");
        assert_eq!(
            accel.split('+').filter(|p| !p.is_empty()).count(),
            2,
            "{accel}"
        );
    }
}

/// A captured key with no usable name must not write a modifier-only
/// accelerator at all.
#[test]
fn accelerator_rejects_unmapped_vk() {
    assert_eq!(vk_to_key_name(0x1B), None); // Esc → capture_cancelled path
    assert_eq!(
        build_accelerator(&json!({ "vk": 0x1B, "ctrl": true })),
        None
    );
    assert_eq!(build_accelerator(&Value::Null), None);
}

/// Per-item queue actions mark the uuid in-flight — the row's buttons
/// stay disabled — until the refreshed `dictation.pending` list lands;
/// an Engine error on the action clears the mark so the row is not stuck.
#[gpui::test]
fn queue_item_in_flight_until_pending_refresh(cx: &mut TestAppContext) {
    let (app, rx, replies, _events) = test_app(cx);
    cx.update(|cx| app.update(cx, |this, _| this.queue_retry("u1".into())));
    expect_rpc(&rx, "dictation.retry", json!({"uuid": "u1"}));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert!(this.pending_inflight.contains("u1"));
        })
    });
    // No second op can fire while in-flight (view disables the buttons;
    // the set is what the view keys off).
    cx.update(|cx| app.update(cx, |this, _| this.queue_discard("u1".into())));
    expect_rpc(&rx, "dictation.discard", json!({"uuid": "u1"}));
    // The refreshed list releases the in-flight marks.
    replies
        .send(Reply {
            slot: "dictation.pending".into(),
            result: Ok(json!({ "items": [] })),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| app.update(cx, |this, _| assert!(this.pending_inflight.is_empty())));
    // An Engine error with no refresh re-enables the rows.
    cx.update(|cx| app.update(cx, |this, _| this.queue_retry("u2".into())));
    rx.try_recv().expect("retry op");
    replies
        .send(Reply {
            slot: "@action".into(),
            result: Err("retry: uuid 'u2' не найден".into()),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert!(this.pending_inflight.is_empty());
            assert!(this.error.is_some());
        })
    });
}
