//! Engine hotkey-capture flow: arming/disarming the Engine-side keyboard
//! hook (`dictation.begin/end_hotkey_capture`) and translating its
//! `dictation_capture_key` event into an Electron-style accelerator string.
//! Split out of app.rs.
use ::gpui::prelude::*;
use serde_json::{json, Value};

use crate::app::DictationApp;
use crate::worker::Command;

impl DictationApp {
    /// Arm the Engine hotkey-capture mode (the next modifier+key press
    /// becomes the new hotkey; Esc cancels).
    pub fn hotkey_capture_start(&mut self, cx: &mut Context<Self>) {
        self.hotkey_capturing = true;
        self.notice = None;
        // Dedicated slot instead of `action()`: the "@action" reply is
        // anonymous, so a failed arm couldn't reset the flag — the window
        // would show "Нажмите комбинацию…" forever (only the capture_key /
        // capture_cancelled events clear it, and they never come when the
        // op failed). Not sent via `call()` either: the ops retry map would
        // re-arm capture every ~2s after a transient failure.
        let sent = self.send_command(Command::Rpc {
            slot: "dictation.hotkey_capture".into(),
            op: "dictation.begin_hotkey_capture",
            params: json!({}),
        });
        if !sent {
            self.hotkey_capturing = false;
        }
        cx.notify();
    }

    /// Disarm the Engine hotkey-capture mode (status-window Отмена while
    /// capturing, and window close): without `end_hotkey_capture` the Engine
    /// hook stays armed and consumes the next keystroke forever. Same
    /// dedicated slot as `begin` — a failed end surfaces via the banner and
    /// clears nothing user-visible (the flag is already off).
    pub fn hotkey_capture_cancel(&mut self, cx: &mut Context<Self>) {
        if !self.hotkey_capturing {
            return;
        }
        self.hotkey_capturing = false;
        self.send_command(Command::Rpc {
            slot: "dictation.hotkey_capture".into(),
            op: "dictation.end_hotkey_capture",
            params: json!({}),
        });
        cx.notify();
    }
}

/// vk + modifier flags → Electron-style accelerator ("Ctrl+Shift+;").
/// Ported from `useDictationConfig.shared.ts` (vkToKeyName/buildAccelerator).
/// macOS Engine emits a ready `accelerator` string instead of `vk` — the
/// keyCode→name mapping lives in cortex `macos_native.rs`.
pub(crate) fn build_accelerator(event: &Value) -> Option<String> {
    if let Some(accel) = event.get("accelerator").and_then(Value::as_str) {
        let accel = accel.trim();
        if !accel.is_empty() {
            return Some(accel.to_string());
        }
    }
    let vk = event.get("vk").and_then(Value::as_u64)? as u32;
    let key = vk_to_key_name(vk)?;
    let mut parts = Vec::new();
    for (flag, name) in [
        ("ctrl", "Ctrl"),
        ("alt", "Alt"),
        ("shift", "Shift"),
        ("win", "Super"),
    ] {
        if event.get(flag).and_then(Value::as_bool) == Some(true) {
            parts.push(name.to_string());
        }
    }
    parts.push(key);
    Some(parts.join("+"))
}

pub(crate) fn vk_to_key_name(vk: u32) -> Option<String> {
    match vk {
        0x41..=0x5A | 0x30..=0x39 => char::from_u32(vk).map(|c| c.to_string()),
        0x70..=0x87 => Some(format!("F{}", vk - 0x6f)),
        0xBA => Some(";".into()),
        // VK_OEM_PLUS must NOT map to the literal '+': '+' is the accelerator
        // delimiter, so "Ctrl++" parses back as Ctrl alone and the saved
        // hotkey loses its key. Electron's name for this key is "Plus".
        0xBB => Some("Plus".into()),
        0xBC => Some(",".into()),
        0xBD => Some("-".into()),
        0xBE => Some(".".into()),
        0xBF => Some("/".into()),
        0xC0 => Some("`".into()),
        0xDB => Some("[".into()),
        0xDC => Some("\\".into()),
        0xDD => Some("]".into()),
        0xDE => Some("'".into()),
        0x08 => Some("Backspace".into()),
        0x09 => Some("Tab".into()),
        0x0D => Some("Enter".into()),
        0x20 => Some("Space".into()),
        0x21 => Some("PageUp".into()),
        0x22 => Some("PageDown".into()),
        0x23 => Some("End".into()),
        0x24 => Some("Home".into()),
        0x25 => Some("Left".into()),
        0x26 => Some("Up".into()),
        0x27 => Some("Right".into()),
        0x28 => Some("Down".into()),
        0x2D => Some("Insert".into()),
        0x2E => Some("Delete".into()),
        _ => None,
    }
}
