// KOS-237 artifact — logic simulation of DictationApp::push_pill
// (gpui/src/app.rs ~line 319) vs a pill window handle that died behind the
// app's back (compositor/session teardown, external remove_window).
//
// PRE-FIX: `handle.update(...).ok()` swallows the error and `self.pill` keeps
// the stale handle forever. Every later session sees `self.pill.is_none() ==
// false`, skips `pill::open`, and the whole session runs with NO overlay —
// no waveform, no Стоп/Отмена buttons, invisible recording.
//
// POST-FIX: a failed update clears `self.pill`, so the next session reopens
// the overlay.
//
// Build+run: rustc --edition 2021 gpui-pill-handle-sim.rs -o /tmp/s && /tmp/s

#[derive(Default)]
struct Sim {
    pill_handle: Option<u32>, // Some = handle stored; the WINDOW may be dead
    window_alive: bool,       // OS-side truth
    opened: usize,
}

impl Sim {
    fn push_pill_prefix(&mut self) {
        // .update(...).ok() — error swallowed, stale handle kept.
        if self.pill_handle.is_some() {
            let _ok = self.window_alive;
        }
    }

    fn push_pill_postfix(&mut self) {
        // failed update -> drop the dead handle so begin() can reopen.
        if self.pill_handle.is_some() && !self.window_alive {
            self.pill_handle = None;
        }
    }

    fn dictation_begin(&mut self) {
        // `if self.pill.is_none() { self.pill = pill::open(...) }`
        if self.pill_handle.is_none() {
            self.opened += 1;
            self.pill_handle = Some(1);
            self.window_alive = true;
        }
    }
}

fn run(prefix: bool) -> (usize, bool) {
    let mut sim = Sim::default();
    sim.dictation_begin(); // session 1 opens the pill
    sim.window_alive = false; // OS kills the pill window behind our back
    // 30ms drain keeps pushing — pre-fix keeps the dead handle.
    for _ in 0..3 {
        if prefix {
            sim.push_pill_prefix();
        } else {
            sim.push_pill_postfix();
        }
    }
    // session 2 begins — should reopen the overlay.
    sim.dictation_begin();
    (sim.opened, sim.window_alive && sim.pill_handle.is_some())
}

fn main() {
    println!("=== pill window removed externally between sessions ===");
    let (opened_pre, visible_pre) = run(true);
    println!(
        "  pre-fix : pill::open calls={opened_pre} — session 2 visible overlay: {visible_pre}"
    );
    let (opened_post, visible_post) = run(false);
    println!(
        "  post-fix: pill::open calls={opened_post} — session 2 visible overlay: {visible_post}"
    );
    assert_eq!(opened_pre, 1); // stale handle never cleared -> no reopen
    assert_eq!(opened_post, 2); // cleared -> session 2 reopens
}
