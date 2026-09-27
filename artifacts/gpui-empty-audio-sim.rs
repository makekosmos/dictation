// KOS-220 artifact — logic simulation of the gpui `dictation.pill.stop`
// reply arm (gpui/src/app.rs handle_reply) on a zero-length / missing
// audioB64 payload, PRE-FIX vs POST-FIX, against worker.ts semantics
// (finishCapture: empty string → silent idle; missing field → "audio-missing").
// Build+run: rustc --edition 2021 gpui-empty-audio-sim.rs -o /tmp/sim && /tmp/sim

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Phase {
    Recording,
    Processing,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Delivery {
    Failed,
}

#[derive(Debug, PartialEq, Eq)]
enum Cmd {
    Transcribe,
}

#[derive(Default)]
struct Sim {
    phase: Option<Phase>,
    delivery: Option<Delivery>,
    pill_open: bool,
    error: Option<&'static str>,
    cmds: Vec<Cmd>,
}

impl Sim {
    // dictation_finish → capture.stop issued; pill stays open (Processing).
    fn finish(&mut self) {
        self.phase = Some(Phase::Processing);
        self.pill_open = true;
    }

    // PRE-FIX: `unwrap_or_default()` collapses missing and empty audioB64
    // into "" — both are handed to dictation_transcribe, whose
    // `audio_b64.is_empty()` check returns Err → fail_pill:
    // "Не доставлено" lingers 4.5s and a sticky error banner is set.
    fn stop_reply_prefix(&mut self, audio_b64: Option<&str>) {
        let audio_b64 = audio_b64.unwrap_or_default();
        if audio_b64.is_empty() {
            // dictation_transcribe Err → fail_pill
            self.phase = Some(Phase::Processing);
            self.delivery = Some(Delivery::Failed);
            self.error = Some("Engine не вернул аудио записи");
        } else {
            self.cmds.push(Cmd::Transcribe);
        }
    }

    // POST-FIX: empty string → silent close (worker.ts `!audioB64` → idle).
    // Missing/malformed field → the error pill (worker.ts "audio-missing").
    fn stop_reply_fixed(&mut self, audio_b64: Option<&str>) {
        match audio_b64 {
            Some("") => {
                self.phase = None;
                self.delivery = None;
                self.pill_open = false;
            }
            Some(_) => self.cmds.push(Cmd::Transcribe),
            None => {
                self.phase = Some(Phase::Processing);
                self.delivery = Some(Delivery::Failed);
                self.error = Some("Engine не вернул аудио записи");
            }
        }
    }
}

fn scenario(name: &str, audio_b64: Option<&str>) {
    let mut pre = Sim::default();
    pre.finish();
    pre.stop_reply_prefix(audio_b64);
    let mut post = Sim::default();
    post.finish();
    post.stop_reply_fixed(audio_b64);
    println!("{name}");
    println!(
        "  pre-fix : phase={:?} delivery={:?} error={:?} cmds={:?}",
        pre.phase, pre.delivery, pre.error, pre.cmds
    );
    println!(
        "  post-fix: phase={:?} delivery={:?} error={:?} cmds={:?} pill_open={}",
        post.phase, post.delivery, post.error, post.cmds, post.pill_open
    );
}

fn main() {
    println!("=== dictation.pill.stop reply: audioB64 payload ===");
    scenario("zero-length capture (quick PTT tap) — audioB64: \"\"", Some(""));
    scenario("normal capture — audioB64: \"UklGRg==\"", Some("UklGRg=="));
    scenario("malformed reply — audioB64 field absent", None);
}
