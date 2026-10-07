use super::{Command, DictationApp, Worker};
use crate::hotkey::{build_accelerator, vk_to_key_name};
use crate::session::{level_to_visual, should_adopt_capture, smooth_level, state_broadcast_ended};
use crate::worker::Reply;
use gpui::{AppContext, Entity, TestAppContext};
use mundus_gpui_kit::engine_error::{EngineError, ErrorKind};
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
    // refresh() re-issued the data loads.
    for op in [
        "dictation.get_state",
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
            result: Err(EngineError::engine("not-found")),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert_eq!(
                this.error.as_deref(),
                Some(EngineError::engine("not-found").message().as_str())
            );
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

/// Level frames of a capture we just ended (cancel/finish stop still in
/// flight on the worker channel) must not re-adopt the dead session and
/// pop the pill back open; a foreign capture id still adopts.
#[test]
fn adoption_skips_ended_capture() {
    let mut ended = std::collections::HashSet::new();
    ended.insert("cap-1".to_string());
    assert!(!should_adopt_capture(&ended, "cap-1"));
    assert!(should_adopt_capture(&ended, "cap-foreign"));
    assert!(should_adopt_capture(
        &std::collections::HashSet::new(),
        "cap-1"
    ));
    // A malformed event with an empty captureId would adopt a session we
    // could never stop (stop needs the id) — reject it.
    assert!(!should_adopt_capture(&std::collections::HashSet::new(), ""));
}

/// dB mapping for the pill bars: helper sends rms*5, so raw 0.5 = rms 0.1
/// (−20 dB, loud speech) must read near the top, raw 0.05 (−40 dB, quiet
/// speech) mid-low, silence/0 → 0.
#[test]
fn level_to_visual_db_mapping() {
    assert_eq!(level_to_visual(0.0), 0.0);
    assert!(
        level_to_visual(0.5) >= 0.85,
        "loud speech {}",
        level_to_visual(0.5)
    );
    let quiet = level_to_visual(0.05);
    assert!(
        (0.3..=0.45).contains(&quiet),
        "quiet speech should sit ~0.375, got {quiet}"
    );
    // Top of the meter (raw 1.0 = rms 0.2 = −14 dB) pins at 1.
    assert_eq!(level_to_visual(1.0), 1.0);
}

/// Attack/release envelope: onsets jump most of the gap in one frame,
/// releases decay gently.
#[test]
fn smooth_level_attacks_fast_releases_slow() {
    let attacked = smooth_level(0.0, 1.0);
    assert!((attacked - 0.8).abs() < 1e-6, "attack step {attacked}");
    let released = smooth_level(1.0, 0.0);
    assert!((released - 0.7).abs() < 1e-6, "release step {released}");
    assert_eq!(smooth_level(0.4, 0.4), 0.4);
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

/// macOS Engine emits a prebuilt `accelerator` string (its keyCode→name
/// table lives in cortex `macos_native.rs`) — the event carries no `vk`.
#[test]
fn accelerator_accepts_macos_payload() {
    let event = json!({ "event": "dictation_capture_key", "accelerator": "Super+Shift+K" });
    assert_eq!(build_accelerator(&event).as_deref(), Some("Super+Shift+K"));
    // Empty accelerator still falls back to the vk path.
    assert_eq!(
        build_accelerator(&json!({ "accelerator": "", "vk": 0x4B, "ctrl": true })).as_deref(),
        Some("Ctrl+K")
    );
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
            result: Err(EngineError::engine("not-found")),
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

/// The ~2s ops retry must re-issue only ops whose reply FAILED — a slot
/// still Loading already has its RPC queued or in flight (the Engine
/// client times out at 15s). Re-sending it every 2s would pile duplicates
/// into the command channel that, once the Engine recovers, all execute
/// ahead of real session commands like DictationStart.
#[gpui::test]
fn retry_skips_in_flight_ops(cx: &mut TestAppContext) {
    let (app, rx, replies, _events) = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |this, _| {
            this.call("dictation.stats", "dictation.get_stats", json!({}))
        })
    });
    expect_rpc(&rx, "dictation.get_stats", json!({}));
    // 66+ drain ticks with no reply: the in-flight op must not duplicate.
    // (The ~2s cadence legitimately re-issues the appearance poll — filter
    // it out and assert the failed slot alone isn't re-sent.)
    for _ in 0..66 {
        cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    }
    while let Ok(msg) = rx.try_recv() {
        if let Command::Rpc { op, .. } = msg {
            assert_ne!(op, "dictation.get_stats", "in-flight op was re-sent");
        }
    }
    // Once the op FAILS the retry does re-issue it (Engine back up).
    replies
        .send(Reply {
            slot: "dictation.stats".into(),
            result: Err(EngineError::local(ErrorKind::NotRunning, "Engine offline")),
        })
        .unwrap();
    for _ in 0..67 {
        cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    }
    // The cadence tick may legitimately re-send appearance.get first —
    // pop until the retried stats op shows up.
    loop {
        match rx.try_recv() {
            Ok(Command::Rpc {
                op: "dictation.get_stats",
                params,
                ..
            }) => {
                assert_eq!(params, json!({}));
                break;
            }
            Ok(_) => continue,
            other => panic!("expected retried dictation.get_stats, got {other:?}"),
        }
    }
}

/// `call` on a dead worker channel must not leave the slot in
/// `Slot::Loading` — no reply can ever land, so the view would paint
/// "Загрузка…" forever next to the dead-connection banner.
#[gpui::test]
fn call_marks_slot_failed_when_worker_dead(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    drop(rx); // The worker's command receiver is gone: every send fails.
    cx.update(|cx| {
        app.update(cx, |this, _| {
            this.call("dictation.stats", "dictation.get_stats", json!({}))
        })
    });
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert!(matches!(
                this.slots.get("dictation.stats"),
                Some(crate::app::Slot::Failed(_))
            ));
            assert!(this.error.is_some());
        })
    });
}

/// The ~2s ops retry marks a Failed slot Loading BEFORE re-sending — if
/// that send dies (worker thread gone), the slot must not stay Loading:
/// no reply can ever arrive, so the card would paint "Загрузка…" forever.
#[gpui::test]
fn retry_marks_slot_failed_when_worker_dead(cx: &mut TestAppContext) {
    let (app, rx, replies, _events) = test_app(cx);
    cx.update(|cx| {
        app.update(cx, |this, _| {
            this.call("dictation.stats", "dictation.get_stats", json!({}))
        })
    });
    rx.try_recv().expect("get_stats op");
    replies
        .send(Reply {
            slot: "dictation.stats".into(),
            result: Err(EngineError::local(ErrorKind::NotRunning, "Engine offline")),
        })
        .unwrap();
    cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    drop(rx); // Now the worker dies: the next retry's send fails.
    for _ in 0..67 {
        cx.update(|cx| app.update(cx, |this, cx| this.drain(cx)));
    }
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert!(
                matches!(
                    this.slots.get("dictation.stats"),
                    Some(crate::app::Slot::Failed(_))
                ),
                "dead retry left the slot Loading"
            );
        })
    });
}

/// A queue op whose send fails (dead worker) gets no "@action" reply —
/// without clearing `pending_inflight` there, the row's Повторить/Удалить
/// buttons stay disabled for the rest of the process lifetime.
#[gpui::test]
fn queue_op_releases_inflight_when_worker_dead(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    drop(rx);
    cx.update(|cx| app.update(cx, |this, _| this.queue_retry("u1".into())));
    cx.update(|cx| {
        app.update(cx, |this, _| {
            assert!(
                this.pending_inflight.is_empty(),
                "dead send left the uuid in-flight: row buttons stuck disabled"
            );
        })
    });
}

/// The ended-capture tombstone must cover every recently ended id, not just
/// the last one: cap-1's straggler level frames can still be in flight when
/// a later cap-2 session ends and overwrites a single-slot tombstone —
/// they'd re-adopt the dead cap-1 and reopen the pill on "Идёт запись".
#[test]
fn adoption_skips_all_recently_ended_captures() {
    let mut ended = std::collections::HashSet::new();
    ended.insert("cap-1".to_string());
    ended.insert("cap-2".to_string());
    assert!(!should_adopt_capture(&ended, "cap-1"));
    assert!(!should_adopt_capture(&ended, "cap-2"));
    assert!(should_adopt_capture(&ended, "cap-foreign"));
    assert!(!should_adopt_capture(&ended, ""));
}

/// Pops the next queued worker command, panicking when the queue is empty.
fn next_command(rx: &Receiver<Command>) -> Command {
    rx.try_recv().expect("expected a queued worker command")
}

/// `speech.transcribe` replies map onto the pill's delivery outcome: the
/// explicit `delivery` wins, an `error` state or `injected: false` degrade to
/// failed / clipboard, and a bare success counts as pasted.
#[test]
fn delivery_maps_transcribe_reply() {
    use crate::pill::PillDelivery::*;
    let of = |v: Value| DictationApp::pill_delivery_of(&v);
    assert_eq!(of(json!({ "delivery": "pasted" })), Pasted);
    assert_eq!(of(json!({ "delivery": "clipboard_only" })), ClipboardOnly);
    assert_eq!(
        of(json!({ "delivery": "clipboard_fallback" })),
        ClipboardFallback
    );
    assert_eq!(of(json!({ "delivery": "failed" })), Failed);
    assert_eq!(of(json!({ "state": "error" })), Failed);
    assert_eq!(of(json!({ "injected": false })), ClipboardFallback);
    assert_eq!(of(json!({ "text": "привет" })), Pasted);
}

/// Push-to-talk is phase-aware: `down` arms a session, an `up` that lands
/// while `capture.start` is still in flight is remembered and finishes the
/// session the moment the start reply arrives, and a bare `up` while idle
/// starts nothing.
#[gpui::test]
fn ptt_up_during_starting_finishes_after_start(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    let ptt = |phase: &str| json!({ "event": "dictation_ptt_trigger", "phase": phase });

    cx.update(|cx| app.update(cx, |this, cx| this.handle_engine_event(ptt("up"), cx)));
    assert!(rx.try_recv().is_err(), "a bare up must not start capture");

    cx.update(|cx| app.update(cx, |this, cx| this.handle_engine_event(ptt("down"), cx)));
    assert!(
        matches!(next_command(&rx), Command::Rpc { .. }),
        "state read"
    );
    assert!(matches!(next_command(&rx), Command::DictationStart { .. }));
    cx.update(|cx| app.update(cx, |this, cx| this.handle_engine_event(ptt("up"), cx)));
    assert!(
        rx.try_recv().is_err(),
        "up during Starting only records intent"
    );

    let slot = cx.update(|cx| app.read(cx).pill_slot("start"));
    cx.update(|cx| {
        app.update(cx, |this, cx| {
            this.handle_reply(
                Reply {
                    slot,
                    result: Ok(json!({ "captureId": "cap-1" })),
                },
                cx,
            )
        })
    });
    match next_command(&rx) {
        Command::DictationStop { capture_id, .. } => assert_eq!(capture_id, "cap-1"),
        other => panic!("expected DictationStop, got {other:?}"),
    }
}

/// A toggle press while recording stops the live capture by its id; a start
/// reply that lands after the user cancelled stops the orphaned capture
/// instead of resurrecting the pill.
#[gpui::test]
fn toggle_stops_recording_and_cancel_orphans_start(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    let toggle = json!({ "event": "dictation_toggle_trigger" });

    cx.update(|cx| app.update(cx, |this, cx| this.handle_engine_event(toggle.clone(), cx)));
    while rx.try_recv().is_ok() {}
    let slot = cx.update(|cx| app.read(cx).pill_slot("start"));
    cx.update(|cx| {
        app.update(cx, |this, cx| {
            this.handle_reply(
                Reply {
                    slot,
                    result: Ok(json!({ "captureId": "cap-2" })),
                },
                cx,
            )
        })
    });
    cx.update(|cx| app.update(cx, |this, cx| this.handle_engine_event(toggle.clone(), cx)));
    match next_command(&rx) {
        Command::DictationStop { capture_id, .. } => assert_eq!(capture_id, "cap-2"),
        other => panic!("expected DictationStop, got {other:?}"),
    }

    // Second session: cancel while Starting, then the start reply arrives.
    cx.update(|cx| app.update(cx, |this, cx| this.dictation_cancel(cx)));
    while rx.try_recv().is_ok() {}
    cx.update(|cx| app.update(cx, |this, cx| this.handle_engine_event(toggle.clone(), cx)));
    while rx.try_recv().is_ok() {}
    let slot = cx.update(|cx| app.read(cx).pill_slot("start"));
    cx.update(|cx| app.update(cx, |this, cx| this.dictation_cancel(cx)));
    while rx.try_recv().is_ok() {}
    cx.update(|cx| {
        app.update(cx, |this, cx| {
            this.handle_reply(
                Reply {
                    slot,
                    result: Ok(json!({ "captureId": "cap-3" })),
                },
                cx,
            )
        })
    });
    match next_command(&rx) {
        Command::DictationStop { capture_id, .. } => assert_eq!(capture_id, "cap-3"),
        other => panic!("expected orphan DictationStop, got {other:?}"),
    }
    assert!(cx.update(|cx| app.read(cx).phase.is_none()));
}

/// The models card selects via `dictation.use_local_model` (modelId param),
/// which Engine itself rejects for non-downloaded models.
#[gpui::test]
fn use_local_model_sends_model_id(cx: &mut TestAppContext) {
    let (app, rx, _replies, _events) = test_app(cx);
    cx.update(|cx| app.update(cx, |this, _| this.use_local_model("parakeet-ultra")));
    expect_rpc(
        &rx,
        "dictation.use_local_model",
        json!({ "modelId": "parakeet-ultra" }),
    );
}
