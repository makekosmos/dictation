//! Root entity for the standalone dictation app: the session state machine
//! (idle → starting → recording → processing → delivery), Engine event
//! handling and the pill overlay handle. Every Engine answer lands in
//! `slots` keyed by a string the issuing call chose — the view renders
//! whatever arrived.
use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::TryRecvError;

use ::gpui::{prelude::*, *};
use serde_json::{json, Value};

use crate::pill::{DictationPill, PillDelivery, PillPhase};
use crate::worker::{Command, Worker};

pub use mundus_gpui_kit::fields::Slot;

pub struct DictationApp {
    worker: Worker,
    pub slots: HashMap<String, Slot>,
    /// slot → (op, params) for every `call`/`action` — lets `drain` retry
    /// slots that failed while Engine was down (they'd otherwise stay
    /// Failed forever: nothing else reissues them).
    ops: HashMap<String, (&'static str, Value)>,
    drain_ticks: u32,
    /// Persistent Engine error (banner until a retry succeeds).
    pub error: Option<String>,
    /// Transient success line; cleared on the next action.
    pub notice: Option<String>,
    worker_dead: bool,

    /// Dictation pill overlay window while a recording session is active.
    pub pill: Option<WindowHandle<DictationPill>>,
    /// Status window handle — `None` while closed. The window is disposable:
    /// the entity (worker + Engine subscription) is held by an app global,
    /// so closing only drops the HWND and `SHOW_REQUESTED` reopens it.
    pub(crate) status: Option<AnyWindowHandle>,
    /// Session phase shared by the pill and the status window.
    pub phase: Option<PillPhase>,
    /// `dictation.begin_hotkey_capture` armed — hook intercepts the next
    /// keystroke and emits `dictation_capture_key` / `_cancelled`.
    pub hotkey_capturing: bool,
    /// `captureId` of the live `dictation.capture.start` session — ours or
    /// adopted from `dictation_audio_level` events when another client
    /// (e.g. the Cortex Manager record button) started the capture.
    pub(crate) capture: Option<String>,
    /// `capture` came from an `dictation_audio_level` broadcast, not our own
    /// `capture.start` reply — adopted sessions get no reply to reconcile
    /// against, so a non-recording `dictation_state_changed` broadcast is
    /// their only "ended" signal and must retire the pill.
    pub(crate) capture_adopted: bool,
    /// A `dictation_ptt_trigger` `up` that landed while `capture.start` was
    /// in flight — the session is finished the moment its reply arrives
    /// (worker.ts `stopAfterStart` parity).
    pub(crate) finish_after_start: bool,
    /// The last captureId WE ended (cancel/finish/orphan stop). Its
    /// `dictation_audio_level` frames still drain after our stop command
    /// lands — without this tombstone a straggler re-adopts the dead
    /// capture and pops the pill back open on "Идёт запись".
    pub(crate) ended_capture: Option<String>,
    pub(crate) last_duration_ms: f64,
    /// Live mic RMS levels (`dictation_audio_level` WS events) — the pill's
    /// waveform ring buffer, last 120 samples like the Vue pill history.
    pub levels: VecDeque<f32>,
    /// Delivery state after `speech.transcribe` (Vue pill-footer parity):
    /// the pill stays open showing Вставлено/Буфер обмена/Ошибка, then closes.
    pub delivery: Option<PillDelivery>,
    /// Monotonic session counter — a delayed pill close scheduled by session N
    /// must not close session N+1's pill.
    pub(crate) session: u64,
    /// Destructive action awaiting an inline confirmation in the status
    /// window (model delete, stats reset, queue purge).
    pub confirm: Option<Confirm>,
    /// Queue items with a retry/discard op in flight — their row buttons
    /// stay disabled until the refreshed `dictation.pending` list lands
    /// (or the action errors out), so a double-click can't fire the op
    /// twice.
    pub pending_inflight: std::collections::HashSet<String>,
}

/// Destructive Engine op the status window asks to confirm inline — cheaper
/// than a modal for this utility window.
pub enum Confirm {
    /// `dictation.delete_local_model` for this model id.
    DeleteModel(String),
    /// `dictation.reset_stats` — counters are zeroed.
    ResetStats,
    /// `dictation.discard_all` — queued audio is deleted unrecognised.
    DiscardAll,
}

impl DictationApp {
    /// Windowless by design — `--background` (Engine-managed) starts the
    /// entity with no window at all; `open_status_window` binds one later.
    pub fn new(cx: &mut Context<Self>) -> Self {
        // `None` data_dir = the kit's Engine client re-runs discovery on
        // every call (MUNDUS_DATA_DIR → legacy env → config/Mundus →
        // config/Kosmos). Dictation can outlive an Engine restart or start
        // before it, and the lock may sit in either dir while an install
        // moves over — a startup snapshot would pin the wrong one.
        Self::with_worker(Worker::start(None), cx)
    }

    /// `new` with a caller-provided worker — tests pass channel ends they
    /// hold themselves instead of spawning Engine-bound threads.
    fn with_worker(worker: Worker, cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            worker,
            slots: HashMap::new(),
            ops: HashMap::new(),
            drain_ticks: 0,
            error: None,
            notice: None,
            worker_dead: false,
            pill: None,
            status: None,
            phase: None,
            capture: None,
            capture_adopted: false,
            finish_after_start: false,
            ended_capture: None,
            last_duration_ms: 0.0,
            levels: VecDeque::new(),
            hotkey_capturing: false,
            delivery: None,
            session: 0,
            confirm: None,
            pending_inflight: std::collections::HashSet::new(),
        };
        this.refresh(cx);
        cx.spawn(async move |this, cx| loop {
            // 30ms ≈ the Engine level-event rate — draining slower batches
            // several samples per push and the waveform visibly steps.
            cx.background_executor()
                .timer(std::time::Duration::from_millis(30))
                .await;
            if this.update(cx, |this, cx| this.drain(cx)).is_err() {
                break;
            }
        })
        .detach();
        this
    }

    /// Initial data loads for the status window.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        self.call("dictation.state", "dictation.get_state", json!({}));
        self.call("dictation.local", "dictation.local_status", json!({}));
        self.call("dictation.models", "dictation.list_local_models", json!({}));
        self.call("dictation.pending", "dictation.list_pending", json!({}));
        self.call("dictation.stats", "dictation.get_stats", json!({}));
        cx.notify();
    }

    /// Mutation op: on success refreshes state/models (same refresh-after-write
    /// as the Vue settings tab), on failure shows the Engine error.
    pub fn action(&mut self, op: &'static str, params: Value) {
        self.notice = None;
        self.call("@action", op, params);
    }

    /// Two-step inline confirm for a destructive action (model delete,
    /// stats reset, queue purge).
    pub fn ask_confirm(&mut self, confirm: Confirm) {
        self.confirm = Some(confirm);
    }

    /// Per-item queue actions. The uuid goes into `pending_inflight` so the
    /// view disables the row's buttons until `dictation.pending` refreshes —
    /// a second click before the reply would fire a duplicate op, and a
    /// discard of an already-removed uuid would surface a bogus error banner.
    pub fn queue_retry(&mut self, uuid: String) {
        self.queue_op("dictation.retry", uuid);
    }

    /// See `queue_retry`.
    pub fn queue_discard(&mut self, uuid: String) {
        self.queue_op("dictation.discard", uuid);
    }

    fn queue_op(&mut self, op: &'static str, uuid: String) {
        self.pending_inflight.insert(uuid.clone());
        self.action(op, json!({ "uuid": uuid }));
    }

    /// Queue an Engine op into a named slot; the reply overwrites it.
    pub fn call(&mut self, slot: impl Into<String>, op: &'static str, params: Value) {
        let slot = slot.into();
        // "@action" replies are one-shot writes — never auto-retry them.
        if slot != "@action" {
            self.ops.insert(slot.clone(), (op, params.clone()));
        }
        self.slots.insert(slot.clone(), Slot::Loading);
        if !self.send_command(Command::Rpc {
            slot: slot.clone(),
            op,
            params,
        }) {
            // The worker thread is gone — no reply can ever land, so the
            // slot would paint "Загрузка…" forever next to the dead-
            // connection banner.
            self.slots.insert(
                slot,
                Slot::Failed("Соединение с Engine завершено. Перезапустите приложение.".into()),
            );
        }
    }

    /// Worker command that doesn't map to a data slot (session control ops
    /// are intercepted by name in `drain`). False when the worker channel is
    /// already dead — the command never left.
    pub(crate) fn send_command(&mut self, command: Command) -> bool {
        if self.worker.commands.send(command).is_err() {
            self.worker_dead = true;
            self.error = Some("Соединение с Engine завершено. Перезапустите приложение.".into());
            return false;
        }
        true
    }

    /// Ready slot payload or Null — the view stays total over missing data.
    pub fn data(&self, key: &str) -> Value {
        match self.slots.get(key) {
            Some(Slot::Ready(v)) => v.clone(),
            _ => Value::Null,
        }
    }

    /// Idle-unload selector for the local STT model: minutes → ms, `None` =
    /// never unload (`localIdleUnloadMs: null` in the Engine config patch).
    pub fn set_idle_unload_min(&mut self, minutes: Option<u64>) {
        let ms = minutes.map_or(Value::Null, |m| json!(m * 60_000));
        self.action(
            "dictation.update_config",
            json!({ "localIdleUnloadMs": ms }),
        );
    }

    // --- Worker drain --------------------------------------------------------

    fn drain(&mut self, cx: &mut Context<Self>) {
        // Second-launch wake: resurface the live status window, or open a
        // fresh one when the last close destroyed it.
        if crate::SHOW_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            let resurfaced = self
                .status
                .and_then(|h| {
                    h.update(cx, |_, window, _| {
                        crate::status_window::show_and_activate(window)
                    })
                    .ok()
                })
                .is_some();
            if !resurfaced {
                let entity = cx.entity();
                crate::status_window::open_status_window(&entity, cx);
            }
        }
        // ~2s: retry slots whose op FAILED (Engine offline at launch,
        // restart swap). A Loading slot already has its RPC queued or in
        // flight — the Engine client times out at 15s, so re-sending it
        // every 2s piles up duplicates that run ahead of real session
        // commands once the Engine is back.
        self.drain_ticks += 1;
        if self.drain_ticks.is_multiple_of(66) {
            for (slot, (op, params)) in self.ops.clone() {
                if matches!(self.slots.get(&slot), Some(Slot::Failed(_)) | None) {
                    self.slots.insert(slot.clone(), Slot::Loading);
                    self.send_command(Command::Rpc { slot, op, params });
                }
            }
        }

        loop {
            let reply = match self.worker.replies.try_recv() {
                Ok(reply) => reply,
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.worker_dead = true;
                    self.error =
                        Some("Соединение с Engine завершено. Перезапустите приложение.".into());
                    cx.notify();
                    break;
                }
            };
            self.handle_reply(reply, cx);
        }
        // Engine broadcast events from the WS subscription (worker thread →
        // this UI-thread drain). A dead events thread degrades the hotkey
        // silently — the RPC surface keeps working, so no error banner.
        loop {
            let event = match self.worker.events.try_recv() {
                Ok(event) => event,
                Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
            };
            self.handle_engine_event(event, cx);
            cx.notify();
        }
        // Single funnel for async session mutations (RPC replies + WS events):
        // refresh the pill's snapshot once per drain tick.
        self.push_pill(cx);
    }

    fn handle_reply(&mut self, reply: crate::worker::Reply, cx: &mut Context<Self>) {
        // Sessioned ops carry `name@session`; replies from a preempted session
        // must not mutate the new session's state. Only dictation.pill slots
        // are tagged — "@action" is a slot name itself.
        let (slot, tag) = match reply
            .slot
            .strip_prefix("dictation.pill.")
            .and_then(|_| reply.slot.split_once('@'))
        {
            Some((name, n)) => (name.to_string(), n.parse::<u64>().ok()),
            None => (reply.slot.clone(), None),
        };
        if slot.starts_with("dictation.pill.") && tag.is_some() && tag != Some(self.session) {
            // Stale start reply leaves an orphaned Engine capture — stop it by
            // id. Not `dictation.cancel`: that op is an Engine-global reset —
            // it clears another client's one-shot inject token
            // (contract_window_id), drops prev_hwnd and cancels the local STT
            // sidecar, and an errored stale start created nothing to clean.
            // The orphan's own capture.stop already returns Engine to idle.
            if slot == "dictation.pill.start" {
                if let Some(capture_id) = reply.result.ok().and_then(|v| {
                    v.get("captureId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                }) {
                    self.ended_capture = Some(capture_id.clone());
                    self.send_command(Command::DictationStop {
                        slot: "dictation.pill.orphan_stop".into(),
                        capture_id,
                    });
                }
            }
            return;
        }
        match slot.as_str() {
            "dictation.pill.start" => match reply.result {
                Ok(v) => {
                    let capture_id = v
                        .get("captureId")
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    // Cancel during Starting: the pill is already gone —
                    // stop the just-started session instead of resurrecting.
                    if self.phase.is_none() {
                        if capture_id.is_some() {
                            self.ended_capture = capture_id.clone();
                        }
                        self.send_command(Command::DictationCancel {
                            slot: "dictation.pill.cancel".into(),
                            capture_id,
                        });
                    } else if let Some(id) = capture_id {
                        self.capture = Some(id);
                        self.capture_adopted = false;
                        self.phase = Some(PillPhase::Recording);
                        if self.finish_after_start {
                            // The PTT key was released while capture.start
                            // was in flight — finish the fresh session now.
                            self.finish_after_start = false;
                            self.dictation_finish(cx);
                        }
                    } else {
                        self.fail_pill("Engine не вернул captureId".into(), cx);
                    }
                }
                Err(e) => self.fail_pill(e, cx),
            },
            "dictation.pill.stop" => match reply.result {
                Ok(v) => {
                    // speech.transcribe's reply has no durationMs — carry it
                    // over so the result card shows the record length.
                    self.last_duration_ms =
                        v.get("durationMs").and_then(Value::as_f64).unwrap_or(0.0);
                    match v.get("audioB64").and_then(Value::as_str) {
                        // Zero-length capture (e.g. a PTT tap released before
                        // audio buffered): worker.ts parity — finish silently
                        // instead of surfacing a bogus Не доставлено.
                        Some("") => {
                            self.phase = None;
                            self.delivery = None;
                            self.capture_adopted = false;
                            self.close_pill(cx);
                            cx.notify();
                        }
                        Some(audio_b64) => {
                            let sent = self.send_command(Command::DictationTranscribe {
                                slot: self.pill_slot("result"),
                                audio_b64: audio_b64.to_string(),
                                duration_sec: self.last_duration_ms / 1000.0,
                            });
                            if !sent {
                                self.fail_pill(
                                    "Соединение с Engine завершено. Перезапустите приложение."
                                        .into(),
                                    cx,
                                );
                            }
                        }
                        // capture.stop answered without audioB64 at all —
                        // malformed reply (worker.ts "audio-missing" parity).
                        None => self.fail_pill("Engine не вернул аудио записи".into(), cx),
                    }
                }
                Err(e) => self.fail_pill(e, cx),
            },
            "dictation.pill.result" => match reply.result {
                Ok(mut v) => {
                    if v.get("cancelled").and_then(Value::as_bool) == Some(true) {
                        self.phase = None;
                        self.delivery = None;
                        self.capture_adopted = false;
                        self.close_pill(cx);
                    } else {
                        self.delivery = Some(Self::pill_delivery_of(&v));
                        self.schedule_pill_close(cx);
                        // A cancelled session produced no transcript — only a
                        // real result replaces the "Последняя расшифровка" card.
                        if let Some(obj) = v.as_object_mut() {
                            obj.insert("durationMs".into(), Value::from(self.last_duration_ms));
                        }
                        self.slots.insert("dictation.result".into(), Slot::Ready(v));
                    }
                    if self.error.is_some() {
                        self.error = None;
                    }
                    // Post-session refresh (same refresh-after-write as Vue).
                    self.call("dictation.state", "dictation.get_state", json!({}));
                    self.call("dictation.stats", "dictation.get_stats", json!({}));
                    self.call("dictation.pending", "dictation.list_pending", json!({}));
                }
                Err(e) => self.fail_pill(e, cx),
            },
            "dictation.pill.cancel" => {
                if let Err(e) = reply.result {
                    self.error = Some(e);
                    cx.notify();
                }
            }
            "dictation.hotkey_capture" => {
                // Armed: capture_key / capture_cancelled events drive the
                // rest. A failed arm must clear the flag or the status
                // window is stuck showing "Нажмите комбинацию…" forever.
                if let Err(e) = reply.result {
                    self.hotkey_capturing = false;
                    self.error = Some(e);
                    cx.notify();
                }
            }
            "@action" => match reply.result {
                Ok(_) => {
                    // A succeeded write IS the retry succeeding — clear the
                    // persistent banner like the pill.result arm does, or a
                    // stale error sits next to "Выполнено." forever.
                    self.error = None;
                    self.notice = Some("Выполнено.".into());
                    self.refresh(cx);
                }
                Err(e) => {
                    // The op failed and no refresh will follow — re-enable
                    // in-flight queue rows now, or their buttons stay dead.
                    self.pending_inflight.clear();
                    self.error = Some(e);
                }
            },
            slot => {
                if slot == "dictation.pending" {
                    // Fresh queue snapshot — every in-flight op was answered.
                    self.pending_inflight.clear();
                }
                let slot = slot.to_string();
                self.slots.insert(
                    slot.clone(),
                    match reply.result {
                        Ok(v) => Slot::Ready(v),
                        Err(e) => Slot::Failed(e),
                    },
                );
                cx.notify();
            }
        }
        // A reply always mutates something the status window renders (slot
        // data, the error banner, phase). gpui repaints only on notify() —
        // arms that skipped it (notably "@action" Err and fail_pill) left a
        // failed click painting the old frame until an unrelated event.
        cx.notify();
    }
}

/// Exposes the named data slots to `mundus_gpui_kit::fields::slot_or`.
impl mundus_gpui_kit::fields::Slots for DictationApp {
    fn slot(&self, key: &str) -> Option<&Slot> {
        self.slots.get(key)
    }
}

#[cfg(test)]
#[path = "app_tests.rs"]
mod tests;
