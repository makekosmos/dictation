// KOS-237 artifact — logic simulation of DictationApp::handle_engine_event for
// an ADOPTED capture session (gpui/src/app.rs dictation_audio_level arm ~654
// + dictation_state_changed arm ~697).
//
// The app adopts a captureId from `dictation_audio_level` broadcasts when
// another client (Manager record button, packaged worker) starts a session —
// the pill mirrors it and a stop trigger can finish it. But an adopted session
// produces NO RPC replies of its own: when the owner stops/cancels the
// capture, the only signal reaching this app is a `dictation_state_changed`
// broadcast with a non-recording state. PRE-FIX that arm only refreshed the
// status slot — phase stayed Recording, capture kept the dead id and the pill
// sat on "Идёт запись" forever; pressing Стоп then sent capture.stop for the
// dead id and got a bogus "Не доставлено".
//
// Engine truth: every session end broadcasts dictation_state_changed
// (idle / transcribing / pending / error); recording emits "recording"
// (the dotted dictation.state_changed contract uses "capturing").
//
// Build+run: rustc --edition 2021 gpui-adopted-session-sim.rs -o /tmp/s && /tmp/s

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Recording,
}

#[derive(Default)]
struct Sim {
    phase: Option<Phase>,
    capture: Option<&'static str>,
    adopted: bool,
    pill_open: bool,
    stop_calls: usize,
}

impl Sim {
    // dictation_audio_level arm — adopt when idle.
    fn audio_level(&mut self, capture_id: Option<&'static str>) {
        if self.phase.is_none() {
            if let Some(id) = capture_id {
                self.capture = Some(id);
                self.adopted = true;
                self.pill_open = true;
                self.phase = Some(Phase::Recording);
            }
        }
    }

    // dictation_state_changed { state } — PRE-FIX: only refreshed the slot.
    fn state_changed_prefix(&mut self, _state: &str) {}

    // POST-FIX: a non-recording broadcast retires an ADOPTED recording session.
    // Own sessions are excluded — dictation.cancel does not stop a live
    // capture, so a stray broadcast must not drop real work.
    fn state_changed_postfix(&mut self, state: &str) {
        let recording = matches!(state, "recording" | "capturing");
        if !recording && self.phase == Some(Phase::Recording) && self.adopted {
            self.phase = None;
            self.capture = None;
            self.adopted = false;
            self.pill_open = false;
        }
    }

    // user presses the pill's Стоп (dictation_toggle -> dictation_finish)
    fn press_stop(&mut self) -> &'static str {
        match self.phase {
            Some(Phase::Recording) => {
                if self.capture.take().is_some() {
                    self.stop_calls += 1;
                    // Engine answers "capture not active" for a dead id ->
                    // fail_pill -> "Не доставлено". (Not modelled further.)
                    "capture.stop sent"
                } else {
                    "dictation.cancel sent"
                }
            }
            None => "starts a NEW session (dictation_begin)",
        }
    }
}

fn scenario_adopted_ends(prefix: bool) {
    let mut sim = Sim::default();
    sim.audio_level(Some("cap-9"));
    if prefix {
        sim.state_changed_prefix("idle"); // owner called capture.stop
    } else {
        sim.state_changed_postfix("idle");
    }
    let stop = sim.press_stop();
    println!(
        "  {}: phase={:?} capture={:?} pill={} -> Стоп: {} (stop_calls={})",
        if prefix { "pre-fix " } else { "post-fix" },
        sim.phase,
        sim.capture,
        sim.pill_open,
        stop,
        sim.stop_calls
    );
}

fn main() {
    println!("=== adopted session: owner stops the capture ===");
    scenario_adopted_ends(true);
    scenario_adopted_ends(false);

    println!("\n=== adopted session: owner still recording ===");
    let mut sim = Sim::default();
    sim.audio_level(Some("cap-9"));
    sim.state_changed_postfix("recording");
    println!(
        "  post-fix: phase={:?} capture={:?} pill={} (kept — still live)",
        sim.phase, sim.capture, sim.pill_open
    );
    assert_eq!(sim.phase, Some(Phase::Recording));

    println!("\n=== own session + stray idle broadcast (cancel doesn't kill capture) ===");
    let mut own = Sim {
        phase: Some(Phase::Recording),
        capture: Some("cap-own"),
        adopted: false, // we sent capture.start ourselves — replies reconcile us
        pill_open: true,
        stop_calls: 0,
    };
    own.state_changed_postfix("idle");
    println!(
        "  post-fix: phase={:?} capture={:?} (kept — capture still streaming)",
        own.phase, own.capture
    );
    assert_eq!(own.phase, Some(Phase::Recording));
    assert_eq!(own.capture, Some("cap-own"));

    println!("\npost-fix: adopted dead session retires; own/live session untouched");
}
