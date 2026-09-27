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

pub use kosmos_gpui_kit::fields::Slot;

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
    /// Status window handle — minimized instead of closed (the worker/Engine
    /// subscription lives on this entity, so the window must not die). `None`
    /// in `--background` (autostart) mode until the window is requested.
    status: Option<AnyWindowHandle>,
    /// HKCU Run entry `KosmosDictation` present → launch at Windows sign-in.
    autostart: bool,
    /// Session phase shared by the pill and the status window.
    pub phase: Option<PillPhase>,
    /// `dictation.begin_hotkey_capture` armed — hook intercepts the next
    /// keystroke and emits `dictation_capture_key` / `_cancelled`.
    pub hotkey_capturing: bool,
    /// `captureId` of the live `dictation.capture.start` session — ours or
    /// adopted from `dictation_audio_level` events when another client
    /// (e.g. the Cortex Manager record button) started the capture.
    capture: Option<String>,
    /// A `dictation_ptt_trigger` `up` that landed while `capture.start` was
    /// in flight — the session is finished the moment its reply arrives
    /// (worker.ts `stopAfterStart` parity).
    finish_after_start: bool,
    last_duration_ms: f64,
    /// Live mic RMS levels (`dictation_audio_level` WS events) — the pill's
    /// waveform ring buffer, last 120 samples like the Vue pill history.
    pub levels: VecDeque<f32>,
    /// Delivery state after `speech.transcribe` (Vue pill-footer parity):
    /// the pill stays open showing Вставлено/Буфер обмена/Ошибка, then closes.
    pub delivery: Option<PillDelivery>,
    /// Monotonic session counter — a delayed pill close scheduled by session N
    /// must not close session N+1's pill.
    session: u64,
    /// Local model awaiting a delete confirmation in the status window.
    pub delete_confirm: Option<String>,
}

impl DictationApp {
    /// In `--background` (autostart) mode the window is created hidden
    /// (`show: false`) — same entity, nothing on screen.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let data_dir = kosmos_gpui_kit::engine::data_dir().ok();
        // Closing the only window would drop this entity — the worker, the
        // Engine subscription and the hotkey flow with it. Intercept close:
        // minimize instead (same affordance as tray apps — dictation keeps
        // working, the window is gone from view).
        install_should_close(window, cx);
        let mut this = Self {
            worker: Worker::start(data_dir),
            slots: HashMap::new(),
            ops: HashMap::new(),
            drain_ticks: 0,
            error: None,
            notice: None,
            worker_dead: false,
            pill: None,
            status: Some(window.window_handle()),
            autostart: crate::autostart_enabled(),
            phase: None,
            capture: None,
            finish_after_start: false,
            last_duration_ms: 0.0,
            levels: VecDeque::new(),
            hotkey_capturing: false,
            delivery: None,
            session: 0,
            delete_confirm: None,
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

    /// Two-step inline delete confirm for a local model row.
    pub fn ask_delete(&mut self, model_id: String) {
        self.delete_confirm = Some(model_id);
    }

    /// Queue an Engine op into a named slot; the reply overwrites it.
    pub fn call(&mut self, slot: impl Into<String>, op: &'static str, params: Value) {
        let slot = slot.into();
        // "@action" replies are one-shot writes — never auto-retry them.
        if slot != "@action" {
            self.ops.insert(slot.clone(), (op, params.clone()));
        }
        self.slots.insert(slot.clone(), Slot::Loading);
        self.send_command(Command::Rpc { slot, op, params });
    }

    /// Worker command that doesn't map to a data slot (session control ops
    /// are intercepted by name in `drain`). False when the worker channel is
    /// already dead — the command never left.
    fn send_command(&mut self, command: Command) -> bool {
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

    /// Arm the Engine hotkey-capture mode (the next modifier+key press
    /// becomes the new hotkey; Esc cancels).
    pub fn hotkey_capture_start(&mut self, cx: &mut Context<Self>) {
        self.hotkey_capturing = true;
        self.notice = None;
        // Dedicated slot instead of `action()`: the "@action" reply is
        // anonymous, so a failed arm couldn't reset the flag — the window
        // would show "Нажмите комбинацию…" forever (only the capture_key /
        // capture_cancelled events clear it, and they never come when the
        // op failed). Not sent via `call()` either: the ops retry map would
        // re-arm capture every ~2s after a transient failure.
        let sent = self.send_command(Command::Rpc {
            slot: "dictation.hotkey_capture".into(),
            op: "dictation.begin_hotkey_capture",
            params: json!({}),
        });
        if !sent {
            self.hotkey_capturing = false;
        }
        cx.notify();
    }

    // --- Session state machine (Electron dictation-pill.ts parity) ----------

    /// Pill toggle — the status-window button, the pill's own Стоп button and
    /// the Engine hotkey triggers all funnel here. A new session preempts a
    /// lingering Processing/delivery pill (Vue parity: `pillFinished` hides
    /// instantly and the hotkey always works) — stale replies are dropped by
    /// the session tag in `handle_reply`.
    pub fn dictation_toggle(&mut self, cx: &mut Context<Self>) {
        match self.phase {
            None => self.dictation_begin(cx),
            Some(PillPhase::Recording) => self.dictation_finish(cx),
            Some(PillPhase::Starting) => {}
            Some(PillPhase::Processing) => {
                self.phase = None;
                self.delivery = None;
                self.levels.clear();
                self.close_pill(cx);
                self.dictation_begin(cx);
            }
        }
    }

    /// Cancel the session from any phase: close the pill, terminate the
    /// capture session and reset the backend state machine.
    pub fn dictation_cancel(&mut self, cx: &mut Context<Self>) {
        self.close_pill(cx);
        self.phase = None;
        self.delivery = None;
        self.levels.clear();
        self.finish_after_start = false;
        self.session += 1;
        let capture_id = self.capture.take();
        self.send_command(Command::DictationCancel {
            slot: "dictation.pill.cancel".into(),
            capture_id,
        });
        cx.notify();
    }

    /// Idle → Starting: open the overlay immediately (it renders "Запуск
    /// записи…"), then ask the Engine to capture the foreground HWND and start
    /// WASAPI capture.
    fn dictation_begin(&mut self, cx: &mut Context<Self>) {
        self.session += 1;
        self.delivery = None;
        self.finish_after_start = false;
        // Fixed-width history from frame one: without it the first samples
        // remap every bar slot as the buffer grows ("bars squeeze in").
        self.levels = vec![0.0; 120].into();
        if self.pill.is_none() {
            self.pill = crate::pill::open(cx.entity(), self.dictation_hotkey(), cx);
        }
        if !matches!(self.slots.get("dictation.state"), Some(Slot::Ready(_))) {
            self.call("dictation.state", "dictation.get_state", json!({}));
        }
        self.phase = Some(PillPhase::Starting);
        self.push_pill(cx);
        self.send_command(Command::DictationStart {
            slot: self.pill_slot("start"),
        });
        cx.notify();
    }

    /// Replies carry the issuing session in the slot (`name@N`) — a session
    /// preempted mid-flight must not clobber the new session's state.
    fn pill_slot(&self, name: &str) -> String {
        format!("dictation.pill.{name}@{}", self.session)
    }

    /// Recording → Processing: the pill STAYS open showing the processing
    /// waveform (Vue pill parity — its no-activate PopUp never takes focus,
    /// so the target app keeps foreground for the auto_paste re-capture).
    fn dictation_finish(&mut self, cx: &mut Context<Self>) {
        self.phase = Some(PillPhase::Processing);
        self.push_pill(cx);
        match self.capture.take() {
            Some(capture_id) => self.send_command(Command::DictationStop {
                slot: self.pill_slot("stop"),
                capture_id,
            }),
            None => self.send_command(Command::DictationCancel {
                slot: "dictation.pill.cancel".into(),
                capture_id: None,
            }),
        }
        cx.notify();
    }

    fn close_pill(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.pill.take() {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        }
    }

    /// The pill footer renders the configured hotkey next to Отправить.
    fn dictation_hotkey(&self) -> String {
        self.data("dictation.state")
            .get("config")
            .map(|c| kosmos_gpui_kit::fields::vstr(c, "hotkey"))
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "Ctrl+Shift+;".into())
    }

    /// Push the session snapshot into the pill entity. The pill renders ONLY
    /// its own fields — `cx.open_window` draws synchronously, and that first
    /// draw would re-enter the `DictationApp` update that opened it.
    fn push_pill(&self, cx: &mut Context<Self>) {
        if let Some(handle) = &self.pill {
            let phase = self.phase.unwrap_or(PillPhase::Starting);
            let delivery = self.delivery;
            let levels = self.levels.iter().copied().collect::<Vec<f32>>();
            let hotkey = self.dictation_hotkey();
            handle
                .update(cx, |pill, _, cx| {
                    pill.set_state(phase, delivery, levels, hotkey, cx)
                })
                .ok();
        }
    }

    /// Vue `deliveryFinishDelay` parity — the delivery outcome stays visible
    /// long enough to read, then the overlay closes itself. The session
    /// counter guards against a stale timer closing a NEW session's pill.
    fn schedule_pill_close(&mut self, cx: &mut Context<Self>) {
        let delay = match self.delivery {
            Some(PillDelivery::Pasted) => std::time::Duration::from_millis(80),
            Some(PillDelivery::ClipboardOnly) => std::time::Duration::from_millis(2500),
            Some(PillDelivery::ClipboardFallback | PillDelivery::Failed) | None => {
                std::time::Duration::from_millis(4500)
            }
        };
        let session = self.session;
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                if this.session == session && this.phase == Some(PillPhase::Processing) {
                    this.phase = None;
                    this.delivery = None;
                    this.close_pill(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    /// Map a `speech.transcribe` reply onto the Vue pill delivery semantics.
    fn pill_delivery_of(v: &Value) -> PillDelivery {
        match v.get("delivery").and_then(Value::as_str) {
            Some("pasted") => PillDelivery::Pasted,
            Some("clipboard_only") => PillDelivery::ClipboardOnly,
            Some("clipboard_fallback") => PillDelivery::ClipboardFallback,
            Some("failed") => PillDelivery::Failed,
            _ if v.get("state").and_then(Value::as_str) == Some("error") => PillDelivery::Failed,
            _ if v.get("injected").and_then(Value::as_bool) == Some(false) => {
                PillDelivery::ClipboardFallback
            }
            _ => PillDelivery::Pasted,
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

    pub fn autostart(&self) -> bool {
        self.autostart
    }

    /// Toggle the HKCU Run entry — `--background` starts the app silently
    /// (no window) at Windows sign-in. Enabling also registers Engine
    /// (`engine.autostart.set` → `kepler-backend --start`): dictation at
    /// sign-in is useless without Engine up.
    pub fn set_autostart(&mut self, on: bool, cx: &mut Context<Self>) {
        if crate::set_autostart(on) {
            self.autostart = on;
            if on {
                self.action("engine.autostart.set", json!({ "enabled": true }));
            }
            cx.notify();
        }
    }

    // --- Worker drain --------------------------------------------------------

    fn drain(&mut self, cx: &mut Context<Self>) {
        // Second-launch wake: resurface the (possibly minimized/hidden) window.
        if crate::SHOW_REQUESTED.swap(false, std::sync::atomic::Ordering::SeqCst) {
            if let Some(h) = self.status {
                let _ = h.update(cx, |_, window, _| show_and_activate(window));
            }
        }
        // ~2s: retry slots whose op failed or never landed (Engine offline at
        // launch, restart swap). Without this they stay empty forever.
        self.drain_ticks += 1;
        if self.drain_ticks.is_multiple_of(66) {
            for (slot, (op, params)) in self.ops.clone() {
                if !matches!(self.slots.get(&slot), Some(Slot::Ready(_))) {
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
            // Stale start reply leaves an orphaned Engine capture — cancel it.
            if slot == "dictation.pill.start" {
                let capture_id = reply.result.ok().and_then(|v| {
                    v.get("captureId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
                self.send_command(Command::DictationCancel {
                    slot: "dictation.pill.cancel".into(),
                    capture_id,
                });
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
                        self.send_command(Command::DictationCancel {
                            slot: "dictation.pill.cancel".into(),
                            capture_id,
                        });
                    } else if let Some(id) = capture_id {
                        self.capture = Some(id);
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
                    let audio_b64 = v
                        .get("audioB64")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_string();
                    // speech.transcribe's reply has no durationMs — carry it
                    // over so the result card shows the record length.
                    self.last_duration_ms =
                        v.get("durationMs").and_then(Value::as_f64).unwrap_or(0.0);
                    self.send_command(Command::DictationTranscribe {
                        slot: self.pill_slot("result"),
                        audio_b64,
                        duration_sec: self.last_duration_ms / 1000.0,
                    });
                }
                Err(e) => self.fail_pill(e, cx),
            },
            "dictation.pill.result" => match reply.result {
                Ok(mut v) => {
                    if v.get("cancelled").and_then(Value::as_bool) == Some(true) {
                        self.phase = None;
                        self.delivery = None;
                        self.close_pill(cx);
                    } else {
                        self.delivery = Some(Self::pill_delivery_of(&v));
                        self.schedule_pill_close(cx);
                    }
                    if let Some(obj) = v.as_object_mut() {
                        obj.insert("durationMs".into(), Value::from(self.last_duration_ms));
                    }
                    self.slots.insert("dictation.result".into(), Slot::Ready(v));
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
                    self.notice = Some("Выполнено.".into());
                    self.refresh(cx);
                }
                Err(e) => self.error = Some(e),
            },
            slot => {
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
    }

    /// Vue pill parity: показать ошибку в overlay, а не мгновенно закрыть
    /// (delivery=failed → 4.5s linger).
    fn fail_pill(&mut self, error: String, cx: &mut Context<Self>) {
        self.phase = Some(PillPhase::Processing);
        self.capture = None;
        self.delivery = Some(PillDelivery::Failed);
        self.schedule_pill_close(cx);
        self.error = Some(error);
    }

    /// desktop/electron/dictation-pill.ts parity: the Rust WH_KEYBOARD_LL
    /// hook emits `dictation_toggle_trigger` (toggle mode) and
    /// `dictation_ptt_trigger {phase}` (PTT mode — `down` arms, `up` finishes,
    /// an `up` during Starting sets `finish_after_start`). State/progress
    /// events refresh the matching slots.
    fn handle_engine_event(&mut self, event: Value, cx: &mut Context<Self>) {
        let name = event
            .get("event")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match name {
            "dictation_toggle_trigger" => {
                self.dictation_toggle(cx);
            }
            // PTT is phase-aware: only `down` arms a session. A bare `up`
            // (key held at launch, releasing after the pill's Отмена) must
            // not start capture, a repeat `down` while recording must not
            // finish it early, and an `up` during Starting is remembered so
            // the session finishes the moment capture.start lands — dropping
            // it would leave the mic recording with the key released.
            "dictation_ptt_trigger" => {
                let released = event.get("phase").and_then(Value::as_str) == Some("up");
                match (released, self.phase) {
                    (false, None) => self.dictation_begin(cx),
                    (true, Some(PillPhase::Starting)) => self.finish_after_start = true,
                    (true, Some(PillPhase::Recording)) => self.dictation_finish(cx),
                    // A fresh hold while the previous session still
                    // transcribes preempts it, like a toggle press.
                    (false, Some(PillPhase::Processing)) => self.dictation_toggle(cx),
                    _ => {}
                }
            }
            // Live mic RMS from the Engine capture thread — ring buffer of the
            // last 120 samples for the pill waveform (Vue WAVEFORM_HISTORY_SIZE).
            // `captureId` in the payload lets us ADOPT a session started by
            // another client (the Manager record button) — the pill shows and
            // the next stop trigger lands on this captureId.
            "dictation_audio_level" => {
                let level = event.get("level").and_then(Value::as_f64).unwrap_or(0.0) as f32;
                if self.phase.is_none() {
                    if let Some(capture_id) = event
                        .get("captureId")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                    {
                        self.session += 1;
                        self.delivery = None;
                        self.finish_after_start = false;
                        self.levels = vec![0.0; 120].into();
                        self.capture = Some(capture_id);
                        if self.pill.is_none() {
                            self.pill = crate::pill::open(cx.entity(), self.dictation_hotkey(), cx);
                        }
                        self.phase = Some(PillPhase::Recording);
                    }
                }
                if self.phase == Some(PillPhase::Recording) {
                    // AnalyserNode smoothingTimeConstant=0.85 parity —
                    // bidirectional EMA so bars don't jitter packet to packet.
                    let level = self
                        .levels
                        .back()
                        .map(|prev| prev * 0.75 + level * 0.25)
                        .unwrap_or(level)
                        .clamp(0.0, 1.0);
                    if self.levels.len() >= 120 {
                        self.levels.pop_front();
                    }
                    self.levels.push_back(level);
                }
            }
            "dictation_capture_key" if self.hotkey_capturing => {
                self.hotkey_capturing = false;
                if let Some(accel) = build_accelerator(&event) {
                    self.action("dictation.update_config", json!({ "hotkey": accel }));
                }
            }
            "dictation_capture_cancelled" => {
                self.hotkey_capturing = false;
            }
            "dictation_state_changed" | "dictation.state_changed" | "dictation_config_changed" => {
                self.call("dictation.state", "dictation.get_state", json!({}));
            }
            "dictation_stats_changed" => {
                self.call("dictation.stats", "dictation.get_stats", json!({}));
            }
            "dictation_pending_changed" => {
                self.call("dictation.pending", "dictation.list_pending", json!({}));
            }
            // Progress ticks stream per chunk — stash the payload for the
            // view rather than re-issuing RPCs; started resets the slot and
            // complete/failed clear it while refreshing the model list.
            "dictation_local_model_download_progress"
            | "dictation_local_model_download_started" => {
                self.slots
                    .insert("dictation.download".into(), Slot::Ready(event));
            }
            "dictation_local_model_download_complete" | "dictation_local_model_download_failed" => {
                self.slots.remove("dictation.download");
                self.call("dictation.local", "dictation.local_status", json!({}));
                self.call("dictation.models", "dictation.list_local_models", json!({}));
            }
            _ => {}
        }
    }
}

/// Unhide a `show: false` / minimized window, then activate it. GPUI's
/// `activate_window` only handles IsIconic→SW_RESTORE; a window that was
/// never shown needs an explicit SW_SHOW first.
fn show_and_activate(window: &mut Window) {
    #[cfg(windows)]
    if let Ok(wh) = raw_window_handle::HasWindowHandle::window_handle(window) {
        use raw_window_handle::RawWindowHandle;
        if let RawWindowHandle::Win32(w32) = wh.as_raw() {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                ShowWindow(w32.hwnd.get() as _, SW_SHOW);
            }
        }
    }
    window.activate_window();
}

/// Close-intercept for the status window: minimize instead of close so the
/// entity
/// (worker + Engine subscription) keeps the dictation flow alive.
fn install_should_close(window: &mut Window, cx: &App) {
    window.on_window_should_close(cx, |window, _cx| {
        window.minimize_window();
        false
    });
}

/// Exposes the named data slots to `kosmos_gpui_kit::fields::slot_or`.
impl kosmos_gpui_kit::fields::Slots for DictationApp {
    fn slot(&self, key: &str) -> Option<&Slot> {
        self.slots.get(key)
    }
}

/// vk + modifier flags → Electron-style accelerator ("Ctrl+Shift+;").
/// Ported from `useDictationConfig.shared.ts` (vkToKeyName/buildAccelerator).
fn build_accelerator(event: &Value) -> Option<String> {
    let vk = event.get("vk").and_then(Value::as_u64)? as u32;
    let key = vk_to_key_name(vk)?;
    let mut parts = Vec::new();
    for (flag, name) in [
        ("ctrl", "Ctrl"),
        ("alt", "Alt"),
        ("shift", "Shift"),
        ("win", "Super"),
    ] {
        if event.get(flag).and_then(Value::as_bool) == Some(true) {
            parts.push(name.to_string());
        }
    }
    parts.push(key);
    Some(parts.join("+"))
}

fn vk_to_key_name(vk: u32) -> Option<String> {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(vk).map(|c| c.to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x6f)),
        0xBA => Some(";".into()),
        0xBB => Some("+".into()),
        0xBC => Some(",".into()),
        0xBD => Some("-".into()),
        0xBE => Some(".".into()),
        0xBF => Some("/".into()),
        0xC0 => Some("`".into()),
        0xDB => Some("[".into()),
        0xDC => Some("\\".into()),
        0xDD => Some("]".into()),
        0xDE => Some("'".into()),
        0x08 => Some("Backspace".into()),
        0x09 => Some("Tab".into()),
        0x0D => Some("Enter".into()),
        0x20 => Some("Space".into()),
        0x21 => Some("PageUp".into()),
        0x22 => Some("PageDown".into()),
        0x23 => Some("End".into()),
        0x24 => Some("Home".into()),
        0x25 => Some("Left".into()),
        0x26 => Some("Up".into()),
        0x27 => Some("Right".into()),
        0x28 => Some("Down".into()),
        0x2D => Some("Insert".into()),
        0x2E => Some("Delete".into()),
        _ => None,
    }
}
