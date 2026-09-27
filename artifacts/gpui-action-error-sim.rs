// KOS-247 artifact — logic simulation of DictationApp::handle_reply's
// "@action" arm (gpui/src/app.rs) — a mutation-op reply.
//
// `error` is documented as "Persistent Engine error (banner until a retry
// succeeds)" and the "dictation.pill.result" Ok arm clears it with
// `self.error = None`. But the "@action" Ok arm only sets `notice` — a
// failed action's banner stays up FOREVER after the retry succeeds, so the
// status window shows "Выполнено." and the stale error at the same time.
//
// PRE-FIX: error survives the successful retry.
// POST-FIX: Ok clears error alongside setting notice.
//
// Build+run: rustc --edition 2021 gpui-action-error-sim.rs -o /tmp/s && /tmp/s

#[derive(Default)]
struct Sim {
    error: Option<String>,
    notice: Option<String>,
}

impl Sim {
    // PRE-FIX: verbatim shape of the "@action" arm today.
    fn action_reply_prefix(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.notice = Some("Выполнено.".into());
                // self.refresh(cx) — re-issues read ops; error untouched.
            }
            Err(e) => self.error = Some(e),
        }
    }

    // POST-FIX: a successful write IS a succeeded retry — clear the banner,
    // same as the "dictation.pill.result" Ok arm already does.
    fn action_reply_postfix(&mut self, result: Result<(), String>) {
        match result {
            Ok(()) => {
                self.error = None;
                self.notice = Some("Выполнено.".into());
            }
            Err(e) => self.error = Some(e),
        }
    }
}

fn main() {
    let mut pre = Sim::default();
    // 1) dictation.update_config fails while Engine is restarting.
    pre.action_reply_prefix(Err("engine unavailable".into()));
    assert_eq!(pre.error.as_deref(), Some("engine unavailable"));
    // 2) User retries — the write now succeeds.
    pre.action_reply_prefix(Ok(()));
    println!(
        "PRE-FIX  after successful retry: error={:?} notice={:?}",
        pre.error, pre.notice
    );
    assert_eq!(pre.notice.as_deref(), Some("Выполнено."));
    // The stale failure banner is still up next to the success notice.
    assert_eq!(
        pre.error.as_deref(),
        Some("engine unavailable"),
        "pre-fix: stale error banner survives a successful @action"
    );

    let mut post = Sim::default();
    post.action_reply_postfix(Err("engine unavailable".into()));
    assert_eq!(post.error.as_deref(), Some("engine unavailable"));
    post.action_reply_postfix(Ok(()));
    println!(
        "POST-FIX after successful retry: error={:?} notice={:?}",
        post.error, post.notice
    );
    assert_eq!(post.notice.as_deref(), Some("Выполнено."));
    assert_eq!(post.error, None, "post-fix: banner clears on success");
}
