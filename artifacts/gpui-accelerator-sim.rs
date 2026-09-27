// KOS-247 artifact — logic simulation of build_accelerator /
// vk_to_key_name (gpui/src/app.rs, ported from
// src/settings/composables/useDictationConfig.shared.ts).
//
// VK_OEM_PLUS (0xBB, the =/+ key) maps to "+" — so Ctrl+= produces the
// accelerator string "Ctrl++". The accelerator grammar uses '+' as the
// modifier/key SEPARATOR, so "Ctrl++" has an empty key part: Engine-side
// registration gets "Ctrl" + "" and the pill footer (which renders
// hotkey.split('+')) shows just "Ctrl". Electron accelerators spell this
// key "Plus" ("Ctrl+Plus").
//
// PRE-FIX: vk 0xBB + ctrl → "Ctrl++" — round-trips to nothing.
// POST-FIX: vk 0xBB + ctrl → "Ctrl+Plus".
//
// Build+run: rustc --edition 2021 gpui-accelerator-sim.rs -o /tmp/s && /tmp/s

fn vk_to_key_name_prefix(vk: u32) -> Option<String> {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(vk).map(|c| c.to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x6f)),
        0xBA => Some(";".into()),
        0xBB => Some("+".into()), // VK_OEM_PLUS — '+' is the accelerator delimiter
        0xBC => Some(",".into()),
        0xBD => Some("-".into()),
        0xBE => Some(".".into()),
        0xBF => Some("/".into()),
        0xC0 => Some("`".into()),
        0xDB => Some("[".into()),
        0xDC => Some("\\".into()),
        0xDD => Some("]".into()),
        0xDE => Some("'".into()),
        _ => None,
    }
}

fn vk_to_key_name_postfix(vk: u32) -> Option<String> {
    if vk == 0xBB {
        return Some("Plus".into());
    }
    vk_to_key_name_prefix(vk)
}

fn build_accelerator(vk: u32, ctrl: bool, to_key: fn(u32) -> Option<String>) -> Option<String> {
    let key = to_key(vk)?;
    let mut parts = Vec::new();
    if ctrl {
        parts.push("Ctrl".to_string());
    }
    parts.push(key);
    Some(parts.join("+"))
}

/// The app's own parser/display path: pill.rs / view.rs split on '+'.
fn parse_parts(accel: &str) -> Vec<String> {
    accel
        .split('+')
        .map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .collect()
}

fn main() {
    let pre = build_accelerator(0xBB, true, vk_to_key_name_prefix).unwrap();
    let post = build_accelerator(0xBB, true, vk_to_key_name_postfix).unwrap();
    println!("PRE-FIX  Ctrl + VK_OEM_PLUS => {pre:?} -> parts {:?}", parse_parts(&pre));
    println!("POST-FIX Ctrl + VK_OEM_PLUS => {post:?} -> parts {:?}", parse_parts(&post));

    assert_eq!(pre, "Ctrl++");
    // The key part is empty — the app itself cannot read the accelerator back.
    assert_eq!(parse_parts(&pre), vec!["Ctrl".to_string()]);
    assert_eq!(post, "Ctrl+Plus");
    assert_eq!(parse_parts(&post), vec!["Ctrl".to_string(), "Plus".to_string()]);

    // Sanity: unchanged keys still round-trip.
    for vk in [0xBAu32, 0xBC, 0xBD, 0xBE, 0xBF, 0xC0, 0xDB, 0xDC, 0xDD, 0xDE] {
        let a = build_accelerator(vk, true, vk_to_key_name_postfix).unwrap();
        assert_eq!(parse_parts(&a).len(), 2, "{a}");
    }
    println!("all other OEM keys still produce two-part accelerators");
}
