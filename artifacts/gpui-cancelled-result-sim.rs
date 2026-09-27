// KOS-247 artifact — logic simulation of DictationApp::handle_reply's
// "dictation.pill.result" Ok arm (gpui/src/app.rs).
//
// A `cancelled: true` transcribe reply (Engine-side cancel while
// speech.transcribe was in flight) closed the pill correctly, but the arm
// still wrote the stub into `slots["dictation.result"]` — overwriting the
// "Последняя расшифровка" card's real transcript with `{cancelled: true,
// durationMs}` (no text, no delivery).
//
// PRE-FIX: a cancelled result clobbers the last real transcript card.
// POST-FIX: only non-cancelled results land in the dictation.result slot.
//
// Build+run: rustc --edition 2021 gpui-cancelled-result-sim.rs -o /tmp/s && /tmp/s

use std::collections::HashMap;

#[derive(Default)]
struct Sim {
    slots: HashMap<String, String>,
    pill_closed: bool,
    delivered: bool,
}

impl Sim {
    fn result_reply_prefix(&mut self, v: &str, cancelled: bool) {
        if cancelled {
            self.pill_closed = true;
        } else {
            self.delivered = true; // delivery + timed close
        }
        // Slot write ran unconditionally — stub overwrites the real result.
        self.slots.insert("dictation.result".into(), v.into());
    }

    fn result_reply_postfix(&mut self, v: &str, cancelled: bool) {
        if cancelled {
            self.pill_closed = true;
        } else {
            self.delivered = true;
            self.slots.insert("dictation.result".into(), v.into());
        }
    }
}

fn main() {
    for (label, cancelled_clobbers) in [
        ("PRE-FIX", true),
        ("POST-FIX", false),
    ] {
        let mut sim = Sim::default();
        // A real transcription landed earlier and is shown in the card.
        sim.slots
            .insert("dictation.result".into(), "text=Hello world".into());
        // Engine-side cancel mid-transcribe: {cancelled: true}.
        if cancelled_clobbers {
            sim.result_reply_prefix("{cancelled:true}", true);
        } else {
            sim.result_reply_postfix("{cancelled:true}", true);
        }
        let card = sim.slots.get("dictation.result").map(String::as_str);
        println!("{label} after cancelled reply: result slot = {card:?} (pill_closed={})", sim.pill_closed);
        assert!(sim.pill_closed);
        assert!(!sim.delivered);
        if cancelled_clobbers {
            assert_eq!(card, Some("{cancelled:true}"), "pre-fix: stub replaced the transcript");
        } else {
            assert_eq!(card, Some("text=Hello world"), "post-fix: real transcript kept");
        }
    }
}
