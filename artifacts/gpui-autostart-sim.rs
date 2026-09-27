// KOS-237 artifact — logic simulation of main.rs::set_autostart +
// DictationApp::set_autostart (gpui/src/app.rs ~line 392).
//
// PRE-FIX issues:
//  (a) set_autostart(false) when the Run value is ABSENT returns
//      ERROR_FILE_NOT_FOUND -> false, i.e. "failed" — even though the desired
//      end state (no autostart) is already reality.
//  (b) a genuine registry failure is swallowed silently — `self.autostart`
//      stays stale and the toggle snaps back with zero feedback to the user.
//
// POST-FIX: delete-of-absent-value counts as success (desired state reached),
// and a real failure surfaces `self.error`.
//
// Build+run: rustc --edition 2021 gpui-autostart-sim.rs -o /tmp/s && /tmp/s

const ERROR_SUCCESS: u32 = 0;
const ERROR_FILE_NOT_FOUND: u32 = 2;

struct Reg {
    present: bool,
    writable: bool, // false = simulate a denied/broken registry write
}

impl Reg {
    fn delete_value(&self) -> u32 {
        if self.present {
            ERROR_SUCCESS
        } else {
            ERROR_FILE_NOT_FOUND
        }
    }
    fn set_value(&self) -> u32 {
        if self.writable {
            ERROR_SUCCESS
        } else {
            5 // ERROR_ACCESS_DENIED
        }
    }
}

fn prefix_set_autostart(reg: &Reg, on: bool) -> bool {
    if on {
        reg.set_value() == ERROR_SUCCESS
    } else {
        reg.delete_value() == ERROR_SUCCESS
    }
}

fn postfix_set_autostart(reg: &Reg, on: bool) -> bool {
    if on {
        reg.set_value() == ERROR_SUCCESS
    } else {
        // Absent value IS the desired state — not a failure.
        let code = reg.delete_value();
        code == ERROR_SUCCESS || code == ERROR_FILE_NOT_FOUND
    }
}

#[derive(Default)]
struct App {
    autostart: bool,
    error: Option<&'static str>,
}

impl App {
    // app.rs::set_autostart — PRE-FIX: failures silently snap the toggle back.
    fn toggle_prefix(&mut self, reg: &Reg, on: bool) {
        if prefix_set_autostart(reg, on) {
            self.autostart = on;
        }
    }
    // POST-FIX: real failure -> visible error; absent-delete counts as success.
    fn toggle_postfix(&mut self, reg: &Reg, on: bool) {
        if postfix_set_autostart(reg, on) {
            self.autostart = on;
        } else {
            self.error = Some("Не удалось обновить автозапуск.");
        }
    }
}

fn main() {
    println!("=== disable autostart when the Run value was never set ===");
    let reg = Reg {
        present: false,
        writable: true,
    };
    let mut a = App::default();
    a.toggle_prefix(&reg, false);
    println!(
        "  pre-fix : set_autostart(false)=false -> treated as failure (misleading)"
    );
    assert!(!prefix_set_autostart(&reg, false));
    let mut b = App::default();
    b.toggle_postfix(&reg, false);
    println!(
        "  post-fix: set_autostart(false)=true  -> autostart={} error={:?}",
        b.autostart, b.error
    );
    assert!(postfix_set_autostart(&reg, false));

    println!("\n=== enable autostart with a denied registry write ===");
    let reg = Reg {
        present: false,
        writable: false,
    };
    let mut a = App::default();
    a.toggle_prefix(&reg, true);
    println!(
        "  pre-fix : autostart={} error={:?} — silent snap-back",
        a.autostart, a.error
    );
    assert!(a.error.is_none());
    let mut b = App::default();
    b.toggle_postfix(&reg, true);
    println!(
        "  post-fix: autostart={} error={:?} — surfaced",
        b.autostart, b.error
    );
    assert!(b.error.is_some());
}
