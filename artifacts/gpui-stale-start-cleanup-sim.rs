// KOS-237 artifact — logic simulation of DictationApp::handle_reply's stale
// `dictation.pill.start` arm (gpui/src/app.rs ~line 465) and the worker-side
// Command::DictationCancel executor (gpui/src/worker.rs::dictation_cancel).
//
// Scenario: session 1's capture.start is still in flight when the user cancels
// (session counter bumps). The Ok{captureId:C1} or Err reply lands late, tagged
// @1 — stale. The orphaned Engine capture C1 must be stopped, but PRE-FIX the
// cleanup also runs `dictation.cancel` — an Engine-GLOBAL reset that clears the
// packaged worker's one-shot inject token (contract_window_id), drops
// prev_hwnd and cancels the local STT sidecar, even when the stale reply was
// an ERROR and there is no orphan to clean at all.
//
// Engine truth (runtime/src/dictation/host.rs + host_capture.rs, evidence):
//   dictation.cancel: state=idle, contract_window_id=None, prev_hwnd=None,
//                     local::cancel_sidecar() when Recording/Transcribing.
//   capture.stop {id}: only ends that capture + state=idle.
//
// Build+run: rustc --edition 2021 gpui-stale-start-cleanup-sim.rs -o /tmp/s && /tmp/s

#[derive(Debug, PartialEq, Eq)]
enum EngineOp {
    CaptureStop(&'static str),
    DictationCancel,
}

#[derive(Default)]
struct Engine {
    contract_window_id: Option<&'static str>, // packaged worker's inject token
    prev_hwnd: bool,
    sidecar_job: bool, // local STT sidecar running for a live session
}

impl Engine {
    fn run(&mut self, op: &EngineOp) {
        match op {
            EngineOp::CaptureStop(_) => {}
            EngineOp::DictationCancel => {
                // host.rs::op_cancel — clears the one-shot inject contract of
                // WHOEVER owns it, plus prev_hwnd and the STT sidecar.
                self.contract_window_id = None;
                self.prev_hwnd = false;
                self.sidecar_job = false;
            }
        }
    }
}

fn stale_start_cleanup_prefix(capture_id: Option<&'static str>) -> Vec<EngineOp> {
    // PRE-FIX app.rs: stale dictation.pill.start reply -> DictationCancel{slot,
    // capture_id} -> worker dictation_cancel(): capture.stop(id) + ALWAYS
    // dictation.cancel — the cancel fires even on an errored reply (no orphan).
    let mut ops = Vec::new();
    if let Some(id) = capture_id {
        ops.push(EngineOp::CaptureStop(id));
    }
    ops.push(EngineOp::DictationCancel);
    ops
}

fn stale_start_cleanup_postfix(capture_id: Option<&'static str>) -> Vec<EngineOp> {
    // POST-FIX app.rs: surgical cleanup — only capture.stop for the orphaned
    // id. An errored/malformed stale start reply sends nothing at all.
    capture_id.map(|id| EngineOp::CaptureStop(id)).into_iter().collect()
}

fn run(ops: &[EngineOp], label: &str) {
    let mut engine = Engine {
        contract_window_id: Some("w-77"), // packaged worker holds an inject token
        prev_hwnd: true,
        sidecar_job: true, // its session is Recording — sidecar preloaded
    };
    for op in ops {
        engine.run(op);
    }
    println!(
        "  {label:<14} -> ops={ops:?}  contract_token={:?}  prev_hwnd={}  sidecar={}",
        engine.contract_window_id, engine.prev_hwnd, engine.sidecar_job
    );
}

fn main() {
    println!("=== stale dictation.pill.start cleanup (Ok reply, orphan C1) ===");
    run(&stale_start_cleanup_prefix(Some("cap-C1")), "pre-fix");
    run(&stale_start_cleanup_postfix(Some("cap-C1")), "post-fix");

    println!("\n=== stale dictation.pill.start cleanup (Err reply, NO orphan) ===");
    run(&stale_start_cleanup_prefix(None), "pre-fix");
    run(&stale_start_cleanup_postfix(None), "post-fix");

    // Assertions for the post-fix behaviour.
    assert_eq!(
        stale_start_cleanup_postfix(Some("cap-C1")),
        vec![EngineOp::CaptureStop("cap-C1")]
    );
    assert!(stale_start_cleanup_postfix(None).is_empty());
    println!("\npost-fix: orphan stopped by id; no dictation.cancel side-effects");
}
