// KOS-256 artifact — two status-window rendering gaps in gpui/src/view.rs.
//
// (a) state_label() knows "recording" but NOT "capturing" — the dotted
//     contract spelling the Engine broadcasts use (state_broadcast_ended in
//     app.rs already treats both names as live). A get_state reply of
//     "capturing" hits the `_ => "Готов"` arm and paints a GREEN "ready"
//     badge while a capture is actually running.
// (b) The status-window hotkey chips split on '+' without dropping empty
//     parts (pill.rs's footer filters them). A config hotkey like "Ctrl++"
//     — written by pre-KOS-247 captures of VK_OEM_PLUS, or typed into the
//     Vue settings free-text field — renders ghost empty chips.
//
// Build+run: rustc --edition 2021 gpui-state-label-sim.rs -o /tmp/sim && /tmp/sim

fn state_label_pre(state: &str) -> &'static str {
    match state {
        "recording" => "Запись",
        "transcribing" => "Распознаю",
        "waiting" => "Жду сеть",
        "error" => "Ошибка",
        _ => "Готов",
    }
}

fn state_label_post(state: &str) -> &'static str {
    match state {
        "recording" | "capturing" => "Запись",
        "transcribing" => "Распознаю",
        "waiting" => "Жду сеть",
        "error" => "Ошибка",
        _ => "Готов",
    }
}

fn chips_pre(hotkey: &str) -> Vec<String> {
    hotkey.split('+').map(|p| p.trim().to_string()).collect()
}

fn chips_post(hotkey: &str) -> Vec<String> {
    hotkey
        .split('+')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn main() {
    println!(
        "PRE-FIX  state \"capturing\" → badge \"{}\" (green) — capture is live!",
        state_label_pre("capturing")
    );
    println!(
        "POST-FIX state \"capturing\" → badge \"{}\"",
        state_label_post("capturing")
    );
    assert_eq!(state_label_pre("capturing"), "Готов");
    assert_eq!(state_label_post("capturing"), "Запись");

    println!(
        "PRE-FIX  hotkey \"Ctrl++\" → chips {:?} (empty ghost chips)",
        chips_pre("Ctrl++")
    );
    println!(
        "POST-FIX hotkey \"Ctrl++\" → chips {:?}",
        chips_post("Ctrl++")
    );
    assert_eq!(chips_pre("Ctrl++").len(), 3); // ["Ctrl", "", ""]
    assert_eq!(chips_post("Ctrl++"), vec!["Ctrl".to_string()]);
}
