//! Session state machine (Electron dictation-pill.ts parity): pill overlay
//! lifecycle, `dictation.capture.*`/`speech.transcribe` replies and the Engine
//! broadcast events that drive them. The fields it touches live on
//! `DictationApp`.
use ::gpui::prelude::*;
use serde_json::{json, Value};

use crate::app::{DictationApp, Feed, Slot, ENGINE_GONE};
use crate::pill::{PillDelivery, PillPhase};
use crate::worker::Command;

impl DictationApp {
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
                self.levels.clear();
                self.end_pill_session(cx);
                self.dictation_begin(cx);
            }
        }
    }

    /// Cancel the session from any phase: close the pill, terminate the
    /// capture session and reset the backend state machine.
    pub fn dictation_cancel(&mut self, cx: &mut Context<Self>) {
        self.levels.clear();
        self.finish_after_start = false;
        self.session += 1;
        let capture_id = self.capture.take();
        if let Some(id) = &capture_id {
            self.tombstone_capture(id.clone());
        }
        self.end_pill_session(cx);
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
        self.capture_adopted = false;
        // Fixed-width history from frame one: without it the first samples
        // remap every bar slot as the buffer grows ("bars squeeze in").
        self.levels = vec![0.0; 120].into();
        if self.pill.is_none() {
            self.pill = crate::pill::open(cx.entity(), self.dictation_hotkey(), cx);
        }
        if !matches!(self.slots.get(Feed::State.slot()), Some(Slot::Ready(_))) {
            self.load(Feed::State);
        }
        self.phase = Some(PillPhase::Starting);
        self.push_pill(cx);
        let start = Command::DictationStart {
            slot: self.pill_slot("start"),
        };
        self.send_pill_command(start, cx);
        cx.notify();
    }

    /// Send a session command; a dead worker channel can never answer, so the
    /// pill would sit on "Запуск записи…"/"Распознаю" forever — fail it like a
    /// refused Engine op (delivery Failed + timed close) instead.
    pub(crate) fn send_pill_command(&mut self, command: Command, cx: &mut Context<Self>) {
        if !self.send_command(command) {
            self.fail_pill(ENGINE_GONE.into(), cx);
        }
    }

    /// Replies carry the issuing session in the slot (`name@N`) — a session
    /// preempted mid-flight must not clobber the new session's state.
    pub(crate) fn pill_slot(&self, name: &str) -> String {
        format!("dictation.pill.{name}@{}", self.session)
    }

    /// Recording → Processing: the pill STAYS open showing the processing
    /// waveform (Vue pill parity — its no-activate PopUp never takes focus,
    /// so the target app keeps foreground for the auto_paste re-capture).
    pub(crate) fn dictation_finish(&mut self, cx: &mut Context<Self>) {
        self.phase = Some(PillPhase::Processing);
        self.push_pill(cx);
        self.capture_adopted = false;
        match self.capture.take() {
            Some(capture_id) => {
                self.tombstone_capture(capture_id.clone());
                let stop = Command::DictationStop {
                    slot: self.pill_slot("stop"),
                    capture_id,
                };
                self.send_pill_command(stop, cx);
            }
            None => {
                let cancel = Command::DictationCancel {
                    slot: "dictation.pill.cancel".into(),
                    capture_id: None,
                };
                self.send_pill_command(cancel, cx);
            }
        }
        cx.notify();
    }

    /// Back to idle: drop the phase and delivery outcome and close the
    /// overlay. Callers retire the capture (`capture.take` + tombstone) first.
    pub(crate) fn end_pill_session(&mut self, cx: &mut Context<Self>) {
        self.phase = None;
        self.delivery = None;
        self.capture_adopted = false;
        self.close_pill(cx);
        cx.notify();
    }

    pub(crate) fn close_pill(&mut self, cx: &mut Context<Self>) {
        if let Some(handle) = self.pill.take() {
            handle
                .update(cx, |_, window, _| window.remove_window())
                .ok();
        }
    }

    /// The pill footer renders the configured hotkey next to Отправить.
    fn dictation_hotkey(&self) -> String {
        self.data(Feed::State.slot())
            .get("config")
            .map(|c| mundus_gpui_kit::fields::vstr(c, "hotkey"))
            .filter(|h| !h.is_empty())
            .unwrap_or_else(|| "Ctrl+Shift+;".into())
    }

    /// Push the session snapshot into the pill entity. The pill renders ONLY
    /// its own fields — `cx.open_window` draws synchronously, and that first
    /// draw would re-enter the `DictationApp` update that opened it. A failed
    /// update means the window died behind our back (compositor/session
    /// teardown) — drop the stale handle or every later session reuses it
    /// and runs with no overlay at all.
    pub(crate) fn push_pill(&mut self, cx: &mut Context<Self>) {
        // Dropping the stale handle below (or a failed open at session
        // start) leaves `self.pill` empty — without this reopen the CURRENT
        // session records with no overlay at all, not just the next one.
        if self.pill.is_none() && self.phase.is_some() {
            self.pill = crate::pill::open(cx.entity(), self.dictation_hotkey(), cx);
        }
        let stale = match &self.pill {
            Some(handle) => {
                let phase = self.phase.unwrap_or(PillPhase::Starting);
                let delivery = self.delivery;
                let levels = self.levels.iter().copied().collect::<Vec<f32>>();
                let hotkey = self.dictation_hotkey();
                handle
                    .update(cx, |pill, _, cx| {
                        pill.set_state(phase, delivery, levels, hotkey, cx)
                    })
                    .is_err()
            }
            None => false,
        };
        if stale {
            self.pill = None;
        }
    }

    /// Vue `deliveryFinishDelay` parity — the delivery outcome stays visible
    /// long enough to read, then the overlay closes itself. The session
    /// counter guards against a stale timer closing a NEW session's pill.
    pub(crate) fn schedule_pill_close(&mut self, cx: &mut Context<Self>) {
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
                    this.end_pill_session(cx);
                }
            });
        })
        .detach();
    }

    /// Map a `speech.transcribe` reply onto the Vue pill delivery semantics.
    pub(crate) fn pill_delivery_of(v: &Value) -> PillDelivery {
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

    /// Vue pill parity: показать ошибку в overlay, а не мгновенно закрыть
    /// (delivery=failed → 4.5s linger).
    pub(crate) fn fail_pill(&mut self, error: String, cx: &mut Context<Self>) {
        self.phase = Some(PillPhase::Processing);
        if let Some(id) = self.capture.take() {
            self.tombstone_capture(id);
        }
        self.capture_adopted = false;
        self.delivery = Some(PillDelivery::Failed);
        self.schedule_pill_close(cx);
        self.error = Some(error);
    }

    /// Remember an ended captureId so its straggler level frames can't
    /// re-adopt the dead session. Bounded — stragglers drain within a
    /// second or two, so a handful of recent ids is plenty and the set
    /// can't grow across a long uptime.
    pub(crate) fn tombstone_capture(&mut self, capture_id: String) {
        if self.ended_capture.len() >= 8 {
            self.ended_capture.clear();
        }
        self.ended_capture.insert(capture_id);
    }

    /// desktop/electron/dictation-pill.ts parity: the Rust WH_KEYBOARD_LL
    /// hook emits `dictation_toggle_trigger` (toggle mode) and
    /// `dictation_ptt_trigger {phase}` (PTT mode — `down` arms, `up` finishes,
    /// an `up` during Starting sets `finish_after_start`). State/progress
    /// events refresh the matching slots.
    pub(crate) fn handle_engine_event(&mut self, event: Value, cx: &mut Context<Self>) {
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
                        .filter(|id| should_adopt_capture(&self.ended_capture, id))
                    {
                        self.session += 1;
                        self.delivery = None;
                        self.finish_after_start = false;
                        self.levels = vec![0.0; 120].into();
                        self.capture = Some(capture_id);
                        self.capture_adopted = true;
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
                if let Some(accel) = crate::hotkey::build_accelerator(&event) {
                    self.update_config(json!({ "hotkey": accel }));
                }
            }
            "dictation_capture_cancelled" => {
                self.hotkey_capturing = false;
            }
            "dictation_state_changed" | "dictation.state_changed" => {
                // An adopted session gets no RPC reply of its own — the owner
                // client's stop/cancel only reaches us as this broadcast. A
                // non-recording state while an adopted session shows
                // Recording means the capture is gone: retire the pill rather
                // than leave a dead waveform whose Стоп later answers with a
                // bogus "Не доставлено". Own sessions skip this — a stray
                // broadcast (dictation.cancel does not stop a live capture)
                // must not drop a capture that is still streaming.
                if state_broadcast_ended(event.get("state").and_then(Value::as_str))
                    && self.phase == Some(PillPhase::Recording)
                    && self.capture_adopted
                {
                    if let Some(id) = self.capture.take() {
                        self.tombstone_capture(id);
                    }
                    self.end_pill_session(cx);
                }
                self.load(Feed::State);
            }
            // Selection/model availability changes land on both mirrors.
            "dictation_config_changed" | "dictation.models_changed" => {
                self.load(Feed::State);
                self.load(Feed::Models);
            }
            "dictation_stats_changed" => {
                self.load(Feed::Stats);
            }
            "dictation_pending_changed" => {
                self.load(Feed::Pending);
            }
            _ => {}
        }
    }
}

/// Does a `dictation_state_changed` / `dictation.state_changed` broadcast mean
/// the Engine session ended? True for every named non-recording state — the
/// underscore variant says "recording", the dotted contract "capturing"; a
/// missing `state` field carries no information.
pub(crate) fn state_broadcast_ended(state: Option<&str>) -> bool {
    matches!(state, Some(s) if s != "recording" && s != "capturing")
}

/// Should an idle-phase `dictation_audio_level` event adopt its captureId?
/// Straggler frames of a capture WE just ended (cancelled/finished — the stop
/// lands on Engine asynchronously) must not reopen the pill, and a malformed
/// event without an id would adopt a session we could never stop.
pub(crate) fn should_adopt_capture(
    ended_capture: &std::collections::HashSet<String>,
    capture_id: &str,
) -> bool {
    !capture_id.is_empty() && !ended_capture.contains(capture_id)
}
