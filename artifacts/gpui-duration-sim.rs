// KOS-256 artifact — view.rs renders the last recording's length as
//   meta.push(format!("запись {}", fmt_ms(ms)))
// but kosmos_gpui_kit::fields::fmt_ms is a RELATIVE-TIME formatter built for
// "N мин. назад" labels (used elsewhere for timestamps) — applied to a
// duration it prints nonsense like "запись только что" for a 1.25s take and
// "запись 2 мин. назад" ("2 minutes ago") for a 90s take.
//
// PRE-FIX: fmt_ms(durationMs) — "запись 2 мин. назад".
// POST-FIX: fmt_duration(ms) — "запись 1:30" (mm:ss, h:mm:ss over an hour).
//
// Build+run: rustc --edition 2021 gpui-duration-sim.rs -o /tmp/sim && /tmp/sim

// Verbatim copy of kosmos_gpui_kit::fields::fmt_ms.
fn fmt_ms(ms: f64) -> String {
    let secs = ms / 1000.0;
    let days = (secs / 86400.0).floor();
    let h = (secs % 86400.0) / 3600.0;
    let m = (secs % 3600.0) / 60.0;
    if days >= 1.0 {
        format!("{days:.0} дн. назад")
    } else if h >= 1.0 {
        format!("{h:.0} ч. назад")
    } else if m >= 1.0 {
        format!("{m:.0} мин. назад")
    } else {
        "только что".into()
    }
}

fn fmt_duration(ms: f64) -> String {
    let secs = (ms / 1000.0).round().max(0.0) as u64;
    if secs >= 3600 {
        format!("{}:{:02}:{:02}", secs / 3600, secs % 3600 / 60, secs % 60)
    } else {
        format!("{}:{:02}", secs / 60, secs % 60)
    }
}

fn main() {
    for ms in [1_250.0, 5_000.0, 45_000.0, 90_000.0, 3_725_000.0] {
        println!(
            "PRE-FIX  durationMs={ms:>9} → \"запись {}\"",
            fmt_ms(ms)
        );
    }
    for ms in [1_250.0, 5_000.0, 45_000.0, 90_000.0, 3_725_000.0] {
        println!(
            "POST-FIX durationMs={ms:>9} → \"запись {}\"",
            fmt_duration(ms)
        );
    }
    // A 1.25s recording must not claim "just now"; a 90s take is not "2 min ago".
    assert_eq!(fmt_ms(1_250.0), "только что");
    assert_eq!(fmt_ms(90_000.0), "2 мин. назад");
    assert_eq!(fmt_duration(1_250.0), "0:01");
    assert_eq!(fmt_duration(90_000.0), "1:30");
    assert_eq!(fmt_duration(3_725_000.0), "1:02:05");
}
