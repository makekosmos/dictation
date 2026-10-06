//! Blocking Engine calls live on a worker thread; the UI drains replies on a
//! 100ms poll (same shape as manager-gpui's worker).
use serde_json::{json, Value};
use std::sync::mpsc::{Receiver, Sender};

use mundus_gpui_kit::engine::Engine;
use mundus_gpui_kit::engine_error::{EngineError, ErrorKind};

#[derive(Debug)]
pub enum Command {
    /// POST /v1/rpc — `slot` routes the reply into `DictationApp::slots`.
    Rpc {
        slot: String,
        op: &'static str,
        params: Value,
    },
    /// Pill start: `dictation.capture_foreground_window` (inject target HWND,
    /// captured before the pill window exists) then engine-owned mic capture
    /// via `dictation.capture.start` (WASAPI 16kHz mono WAV on Windows).
    DictationStart { slot: String },
    /// Pill stop: `dictation.capture.stop {captureId}` → `{audioB64,
    /// durationMs}` payload for the follow-up transcribe call.
    DictationStop { slot: String, capture_id: String },
    /// Chained while the pill shows its processing wave: re-capture the
    /// foreground HWND (capture.stop resets `prev_hwnd`) then
    /// `dictation.speech.transcribe`, which transcribes and injects per the
    /// configured `injectMode`.
    DictationTranscribe {
        slot: String,
        audio_b64: String,
        duration_sec: f64,
    },
    /// Pill cancel: terminate the capture session (audio discarded) and reset
    /// the backend state machine via `dictation.cancel`.
    DictationCancel {
        slot: String,
        capture_id: Option<String>,
    },
}

pub struct Reply {
    pub slot: String,
    pub result: Result<Value, EngineError>,
}

pub struct Worker {
    pub commands: Sender<Command>,
    pub replies: Receiver<Reply>,
    /// Engine broadcast events (`{"event": ...}`) from the WS subscription —
    /// drained on the same UI poll as `replies`.
    pub events: Receiver<Value>,
}

impl Worker {
    pub fn start(data_dir: Option<std::path::PathBuf>) -> Self {
        let (commands, requests) = std::sync::mpsc::channel::<Command>();
        let (results, replies) = std::sync::mpsc::channel::<Reply>();
        let (event_sink, events) = std::sync::mpsc::channel::<Value>();
        let events_dir = data_dir.clone();
        std::thread::spawn(move || {
            let engine = Engine { data_dir };
            for request in requests {
                let reply = match request {
                    Command::Rpc { slot, op, params } => Reply {
                        slot,
                        result: engine.rpc(op, params),
                    },
                    Command::DictationStart { slot } => Reply {
                        slot,
                        result: dictation_start(&engine),
                    },
                    Command::DictationStop { slot, capture_id } => Reply {
                        slot,
                        result: engine
                            .rpc("dictation.capture.stop", json!({ "captureId": capture_id })),
                    },
                    Command::DictationTranscribe {
                        slot,
                        audio_b64,
                        duration_sec,
                    } => Reply {
                        slot,
                        result: dictation_transcribe(&engine, &audio_b64, duration_sec),
                    },
                    Command::DictationCancel { slot, capture_id } => Reply {
                        slot,
                        result: dictation_cancel(&engine, capture_id.as_deref()),
                    },
                };
                if results.send(reply).is_err() {
                    break;
                }
            }
        });
        // Engine broadcast events (dictation hotkey triggers, audio levels,
        // state and download progress) — ws_server pushes them to every
        // hello'd client. Blocking socket reads stay off the UI thread;
        // reconnect with backoff so an Engine restart resubscribes on its own.
        std::thread::spawn(move || {
            let engine = Engine {
                data_dir: events_dir,
            };
            let mut backoff = std::time::Duration::from_millis(500);
            loop {
                if let Ok(mut stream) = engine.subscribe() {
                    backoff = std::time::Duration::from_millis(500);
                    while let Some(event) = stream.next_event() {
                        if event_sink.send(event).is_err() {
                            return;
                        }
                    }
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(std::time::Duration::from_secs(10));
            }
        });
        Self {
            commands,
            replies,
            events,
        }
    }
}

// --- Dictation pill session (Electron dictation-pill.ts parity) --------------

/// Foreground HWND is captured before the pill window exists — on Windows the
/// inject path restores focus to it and sends the paste shortcut. Failure is
/// non-fatal (clipboard-only fallback in the runtime).
fn dictation_start(engine: &Engine) -> Result<Value, EngineError> {
    let _ = engine.rpc("dictation.capture_foreground_window", json!({}));
    engine.rpc("dictation.capture.start", json!({}))
}

/// `dictation.capture.stop` clears `prev_hwnd` together with the rest of the
/// session state, so the foreground HWND is re-captured here — the no-activate
/// pill window never stole it and the target app owns the foreground again.
/// `speech.transcribe` then runs the queued transcribe + inject attempt inline.
fn dictation_transcribe(
    engine: &Engine,
    audio_b64: &str,
    duration_sec: f64,
) -> Result<Value, EngineError> {
    if audio_b64.is_empty() {
        return Err(EngineError::local(
            ErrorKind::Malformed,
            "capture.stop reply missing audioB64",
        ));
    }
    let _ = engine.rpc("dictation.capture_foreground_window", json!({}));
    engine.rpc(
        "dictation.speech.transcribe",
        json!({ "audioB64": audio_b64, "durationSec": duration_sec }),
    )
}

/// `dictation.cancel` resets the state machine but leaves a live capture
/// session marked busy, so the session is stopped (audio discarded) first.
fn dictation_cancel(engine: &Engine, capture_id: Option<&str>) -> Result<Value, EngineError> {
    if let Some(id) = capture_id {
        let _ = engine.rpc("dictation.capture.stop", json!({ "captureId": id }));
    }
    engine.rpc("dictation.cancel", json!({}))
}
