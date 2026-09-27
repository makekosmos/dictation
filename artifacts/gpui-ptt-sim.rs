// KOS-193 artifact — logic simulation of DictationApp session routing.
// Models gpui/src/app.rs `handle_engine_event` (dictation_ptt_trigger) and the
// `dictation.pill.start` reply path, PRE-FIX vs POST-FIX, plus the
// `hotkey_capturing` flag lifecycle on a failed begin_hotkey_capture op.
// Build+run: rustc --edition 2021 gpui-ptt-sim.rs -o /tmp/ptt-sim && /tmp/ptt-sim

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Starting,
    Recording,
    Processing,
}

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    CaptureStart,
    CaptureStop,
    Cancel,
}

#[derive(Default)]
struct Sim {
    phase: Option<Phase>,
    finish_after_start: bool,
    cmds: Vec<Cmd>,
}

impl Sim {
    // dictation_toggle — identical pre/post fix.
    fn toggle(&mut self) {
        match self.phase {
            None => self.begin(),
            Some(Phase::Recording) => self.finish(),
            Some(Phase::Starting) => {}
            Some(Phase::Processing) => {
                // preempt: close the lingering pill, start a new session
                self.phase = None;
                self.begin();
            }
        }
    }
    fn begin(&mut self) {
        self.phase = Some(Phase::Starting);
        self.finish_after_start = false;
        self.cmds.push(Cmd::CaptureStart);
    }
    fn finish(&mut self) {
        self.phase = Some(Phase::Processing);
        self.cmds.push(Cmd::CaptureStop);
    }
    fn cancel(&mut self) {
        self.phase = None;
        self.finish_after_start = false;
        self.cmds.push(Cmd::Cancel);
    }

    // dictation_ptt_trigger { phase: "up"|"down" }
    fn ptt_prefix(&mut self, _up: bool) {
        // PRE-FIX: both phases routed into dictation_toggle — phase ignored.
        self.toggle();
    }
    fn ptt_fixed(&mut self, up: bool) {
        match (up, self.phase) {
            (false, None) => self.begin(),
            (true, Some(Phase::Starting)) => self.finish_after_start = true,
            (true, Some(Phase::Recording)) => self.finish(),
            (false, Some(Phase::Processing)) => self.toggle(),
            _ => {}
        }
    }

    // `dictation.pill.start` Ok({captureId}) reply lands while phase != None.
    fn start_reply_prefix(&mut self) {
        // PRE-FIX: adopts the capture, phase=Recording — no pending-release
        // notion exists, a swallowed `up` is forgotten.
        self.phase = Some(Phase::Recording);
    }
    fn start_reply_fixed(&mut self) {
        if self.phase.is_none() {
            self.cmds.push(Cmd::Cancel); // cancelled mid-flight → orphan cleanup
            return;
        }
        self.phase = Some(Phase::Recording);
        if self.finish_after_start {
            self.finish_after_start = false;
            self.finish();
        }
    }
}

fn scenario(name: &str, script: &dyn Fn(&mut Sim, bool)) {
    let mut pre = Sim::default();
    script(&mut pre, false);
    let mut post = Sim::default();
    script(&mut post, true);
    println!("{name}");
    println!("  pre-fix : phase={:?} cmds={:?}", pre.phase, pre.cmds);
    println!("  post-fix: phase={:?} cmds={:?}", post.phase, post.cmds);
}

fn main() {
    println!("=== dictation_ptt_trigger routing ===");

    scenario("bare release while idle (key held at launch / release after pill Отмена)", &|s, fixed| {
        if fixed {
            s.ptt_fixed(true)
        } else {
            s.ptt_prefix(true)
        }
    });

    scenario("quick tap: down then up while capture.start in flight", &|s, fixed| {
        if fixed {
            s.ptt_fixed(false);
            s.ptt_fixed(true);
            s.start_reply_fixed();
        } else {
            s.ptt_prefix(false);
            s.ptt_prefix(true);
            s.start_reply_prefix();
        }
    });

    scenario("key-repeat down while recording", &|s, fixed| {
        if fixed {
            s.ptt_fixed(false);
            s.start_reply_fixed();
            s.ptt_fixed(false); // repeat keydown
        } else {
            s.ptt_prefix(false);
            s.start_reply_prefix();
            s.ptt_prefix(false);
        }
    });

    scenario("hold: down, capture lands, up finishes", &|s, fixed| {
        if fixed {
            s.ptt_fixed(false);
            s.start_reply_fixed();
            s.ptt_fixed(true);
        } else {
            s.ptt_prefix(false);
            s.start_reply_prefix();
            s.ptt_prefix(true);
        }
    });

    scenario("up while Processing (transcribe running)", &|s, fixed| {
        if fixed {
            s.ptt_fixed(false);
            s.start_reply_fixed();
            s.ptt_fixed(true); // finish → Processing
            s.ptt_fixed(true); // stray up during transcribe
        } else {
            s.ptt_prefix(false);
            s.start_reply_prefix();
            s.ptt_prefix(true);
            s.ptt_prefix(true);
        }
    });

    println!();
    println!("=== hotkey_capturing flag on failed begin_hotkey_capture ===");
    // PRE-FIX: flag set eagerly; only capture_key / capture_cancelled WS
    // events clear it — a rejected arm means neither ever arrives.
    let capturing = true; // set before the rpc
    let rpc_ok = false;
    if rpc_ok {
        // would wait for events…
    } else {
        // Err arm: `self.error = Some(e)` — flag untouched
    }
    println!("  pre-fix : hotkey_capturing={capturing} (stuck — window shows 'Нажмите комбинацию…' forever)");
    // POST-FIX: dedicated slot Err clears the flag.
    let mut capturing = true;
    if !rpc_ok {
        capturing = false;
    }
    println!("  post-fix: hotkey_capturing={capturing} (cleared on the Err reply)");

    let mut s = Sim::default();
    s.cancel();
    let _ = s;
}
