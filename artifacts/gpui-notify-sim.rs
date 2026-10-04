// KOS-256 artifact — logic simulation of DictationApp::handle_reply's
// missing cx.notify() calls (gpui/src/app.rs).
//
// gpui does NOT repaint on entity update: `Context::notify()` is the only
// signal that invalidates a window tracking the entity (verified against
// gpui-pre 0.3.6 app.rs — `update_entity_erased` only leases the entity;
// `notify()` pushes Effect::Notify). The drain loop runs handle_reply per
// worker reply, but several reply arms mutate status-window state (error
// banner, phase-driven record button, the "Последняя расшифровка" slot)
// without ever notifying. The status window keeps painting the old frame
// until an unrelated event calls cx.notify().
//
// PRE-FIX: "@action" Err and fail_pill mutate without notify — repaints=0.
//   "@action" Err is the worst: a failed update_config/use/delete/download
//   click produces NO visible feedback until the next arbitrary interaction
//   or Engine event — the user can't tell the op was rejected.
//   fail_pill is masked by schedule_pill_close's notify only ~4.5s later —
//   until then the status window still reads "Распознаю…" after a failure.
// POST-FIX: one funnel notify at the end of handle_reply repaints every
//   arm that mutated state.
//
// Build+run: rustc --edition 2021 gpui-notify-sim.rs -o /tmp/sim && /tmp/sim

#[derive(Default)]
struct Sim {
    error: Option<String>,
    notice: Option<String>,
    phase: Option<&'static str>,
    delivery: Option<&'static str>,
    repaints: u32,
}

impl Sim {
    fn notify(&mut self) {
        self.repaints += 1;
    }

    // --- PRE-FIX: verbatim shapes from handle_reply -------------------------

    /// "@action" arm — Err stores the banner text but never repaints.
    fn action_reply_pre(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.error = None;
                self.notice = Some("Выполнено.".into());
                self.notify(); // via refresh()
            }
            Err(e) => self.error = Some(e), // no notify — banner invisible
        }
    }

    /// fail_pill — delivery=Failed + error set, no notify: the status window
    /// keeps showing "Распознаю…" for the whole 4.5s linger.
    fn fail_pill_pre(&mut self, error: String) {
        self.phase = Some("Processing");
        self.delivery = Some("Failed");
        self.error = Some(error); // no notify
    }

    /// "dictation.pill.result" Ok arm — writes the dictation.result slot;
    /// the card only appears when schedule_pill_close fires later.
    fn result_reply_pre(&mut self) {
        self.delivery = Some("Pasted");
        // slots.insert("dictation.result", Ready(v)) — no notify
    }

    // --- POST-FIX: funnel notify covers every mutating arm ------------------

    fn end_of_handle_reply_post(&mut self) {
        self.notify(); // single repaint per reply
    }
}

fn main() {
    // "@action" Err: click Удалить → Engine rejects → silent.
    let mut pre = Sim::default();
    pre.action_reply_pre(Err("engine rejected delete".into()));
    assert_eq!(pre.error.as_deref(), Some("engine rejected delete"));
    println!(
        "PRE-FIX  @action Err: error set, repaints={} (banner invisible)",
        pre.repaints
    );
    assert_eq!(pre.repaints, 0);

    let mut post = Sim::default();
    post.action_reply_pre(Err("engine rejected delete".into()));
    post.end_of_handle_reply_post();
    println!(
        "POST-FIX @action Err: error set, repaints={} (banner paints now)",
        post.repaints
    );
    assert_eq!(post.repaints, 1);

    // fail_pill: capture.stop rejected → status window stale 4.5s.
    let mut pre = Sim::default();
    pre.phase = Some("Recording");
    pre.fail_pill_pre("Engine отклонил операцию".into());
    println!(
        "PRE-FIX  fail_pill: phase={:?} repaints={} (label still shows старое до таймера)",
        pre.phase, pre.repaints
    );
    assert_eq!(pre.repaints, 0);

    let mut post = Sim::default();
    post.phase = Some("Recording");
    post.fail_pill_pre("Engine отклонил операцию".into());
    post.end_of_handle_reply_post();
    println!(
        "POST-FIX fail_pill: phase={:?} repaints={} (repaint сразу)",
        post.phase, post.repaints
    );
    assert_eq!(post.repaints, 1);

    // pill.result Ok: the "Последняя расшифровка" card waits for the
    // scheduled-close notify (~4.5s) instead of painting on arrival.
    let mut pre = Sim::default();
    pre.result_reply_pre();
    println!(
        "PRE-FIX  result Ok: delivery={:?} repaints={} (card deferred)",
        pre.delivery, pre.repaints
    );
    assert_eq!(pre.repaints, 0);
    let mut post = Sim::default();
    post.result_reply_pre();
    post.end_of_handle_reply_post();
    println!("POST-FIX result Ok: repaints={}", post.repaints);
    assert_eq!(post.repaints, 1);
}
