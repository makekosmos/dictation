// KOS-256 artifact — dictation_audio_level re-adopts a just-ended capture.
//
// DictationApp::handle_engine_event adopts foreign captures: a
// `dictation_audio_level` event carrying a captureId while `phase` is None
// opens the pill and marks it Recording. But `capture.stop`/`dictation.cancel`
// reach Engine asynchronously on the worker channel — the events channel can
// still hold level frames for the capture we JUST cancelled/finished (they
// were produced before the stop landed). With phase=None the next drained
// level event re-adopts the dead capture: the pill pops back open on "Идёт
// запись", and a trigger press in that window calls dictation_finish on a
// captureId Engine already dropped — the stop reply fails and the user who
// pressed Отмена gets a bogus "Не доставлено" pill.
//
// PRE-FIX: any captureId in a level event is adopted while phase is None.
// POST-FIX: `ended_capture` remembers the last locally-ended captureId;
//   its stragglers are ignored. (The state-changed broadcast still retires
//   genuinely foreign sessions — the tombstone only covers the gap between
//   our cancel/finish and Engine's broadcast, and makes it deterministic.)
//
// Build+run: rustc --edition 2021 gpui-readopt-sim.rs -o /tmp/sim && /tmp/sim

#[derive(Default)]
struct Sim {
    phase: Option<&'static str>,
    capture: Option<String>,
    capture_adopted: bool,
    pill_open: bool,
    ended_capture: Option<String>,
}

impl Sim {
    // dictation_cancel() / dictation_finish() shape: capture id is taken and
    // the stop/cancel command is queued for the worker thread.
    fn cancel_pre(&mut self) {
        self.phase = None;
        self.capture = None; // id dropped — nothing remembers it
        self.pill_open = false;
    }
    fn cancel_post(&mut self) {
        self.phase = None;
        self.ended_capture = self.capture.take(); // tombstone
        self.pill_open = false;
    }

    // "dictation_audio_level" arm (phase == None path only).
    fn on_level_pre(&mut self, capture_id: &str) {
        if self.phase.is_none() {
            self.capture = Some(capture_id.into());
            self.capture_adopted = true;
            self.pill_open = true; // crate::pill::open
            self.phase = Some("Recording");
        }
    }
    fn on_level_post(&mut self, capture_id: &str) {
        if self.phase.is_none()
            && self.ended_capture.as_deref() != Some(capture_id)
            && !capture_id.is_empty()
        {
            self.capture = Some(capture_id.into());
            self.capture_adopted = true;
            self.pill_open = true;
            self.phase = Some("Recording");
        }
    }
}

fn main() {
    // Own session cancelled: capture.stop is still in flight on the worker
    // thread when a queued level event for cap-1 drains.
    let mut pre = Sim::default();
    pre.phase = Some("Recording");
    pre.capture = Some("cap-1".into());
    pre.cancel_pre();
    pre.on_level_pre("cap-1"); // straggler from the dead capture
    println!(
        "PRE-FIX  straggler level for cancelled cap-1 → phase={:?} pill_open={} (adopted a dead session)",
        pre.phase, pre.pill_open
    );
    assert_eq!(pre.phase, Some("Recording"));
    assert!(pre.capture_adopted);

    let mut post = Sim::default();
    post.phase = Some("Recording");
    post.capture = Some("cap-1".into());
    post.cancel_post();
    post.on_level_post("cap-1");
    println!(
        "POST-FIX straggler level for cancelled cap-1 → phase={:?} pill_open={} (ignored)",
        post.phase, post.pill_open
    );
    assert_eq!(post.phase, None);
    assert!(!post.pill_open);

    // A genuinely foreign capture must still adopt — tombstone must not
    // blanket-block adoption.
    post.on_level_post("cap-foreign");
    println!(
        "POST-FIX foreign capture cap-foreign → phase={:?} pill_open={} (still adopts)",
        post.phase, post.pill_open
    );
    assert_eq!(post.phase, Some("Recording"));
    assert!(post.capture_adopted);
}
