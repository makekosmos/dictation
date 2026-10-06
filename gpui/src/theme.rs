//! Theme follows Mundus (Manager) settings — dictation mirrors the Engine
//! `appearance` snapshot unconditionally, independent of the "follow apps"
//! global flag: this app IS part of Mundus.
use ::gpui::*;
use imago_gpui::theme::*;
use mundus_gpui_kit::fields::{vnum, vopt, vstr};
use serde_json::Value;

/// Engine `appearance.settings` (schema v1) — только то, что диктовке нужно.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AppearanceSettings {
    pub mode: String,        // "system" | "light" | "dark"
    pub light_theme: String, // palette key
    pub dark_theme: String,
    pub accent_source: String, // "theme" | "custom" | "wallpaper"
    pub accent_color: Option<String>,
    pub font_family: String,
    pub font_size: f32,
}

impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            mode: "dark".into(),
            light_theme: "default".into(),
            dark_theme: "default".into(),
            accent_source: "theme".into(),
            accent_color: None,
            font_family: "Inter".into(),
            font_size: 13.,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Resolved {
    pub dark: bool,
    pub theme_index: usize,
    pub accent: Option<u32>,
    pub font_family: String,
    pub font_size: f32,
    pub mode: String,
}

fn parse_color(value: &str) -> Option<u32> {
    let hex = value.trim().trim_start_matches('#');
    u32::from_str_radix(hex, 16).ok().filter(|_| hex.len() == 6)
}

impl AppearanceSettings {
    pub fn resolve(&self, system_dark: bool) -> Resolved {
        let dark = match self.mode.as_str() {
            "light" => false,
            "dark" => true,
            _ => system_dark,
        };
        let key = if dark {
            &self.dark_theme
        } else {
            &self.light_theme
        };
        let theme_index = imago_gpui::palettes::THEMES
            .iter()
            .position(|theme| theme.key == key)
            .unwrap_or(0);
        let accent = match self.accent_source.as_str() {
            "custom" => self.accent_color.as_deref().and_then(parse_color),
            _ => None,
        };
        Resolved {
            dark,
            theme_index,
            accent,
            font_family: self.font_family.clone(),
            font_size: self.font_size,
            mode: self.mode.clone(),
        }
    }
}

thread_local! {
    /// Last applied profile — `sync_theme` skips re-applying the same one
    /// (native chrome calls resize the window on every change).
    static APPLIED: std::cell::RefCell<Option<Resolved>> = const { std::cell::RefCell::new(None) };
    static ACCENT_OVERRIDE: std::cell::Cell<Option<u32>> = const { std::cell::Cell::new(None) };
}

/// Accent used by dictation divs — same override-aware read as manager.
pub(crate) fn accent() -> u32 {
    ACCENT_OVERRIDE
        .with(std::cell::Cell::get)
        .unwrap_or_else(ACCENT)
}

fn accent_foreground(accent: u32) -> u32 {
    let channel = |shift: u32| {
        let value = ((accent >> shift) & 255u32) as f32 / 255.;
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    };
    let luminance = 0.2126 * channel(16) + 0.7152 * channel(8) + 0.0722 * channel(0);
    if luminance > 0.179 {
        0x000000
    } else {
        0xffffff
    }
}

/// `appearance.get` → settings + apply on change. Returns whether it applied.
pub(crate) fn sync_theme(settings_json: &Value, window: &Window, cx: &mut App) -> bool {
    let cfg = &settings_json["settings"];
    if cfg.is_null() {
        return false;
    }
    let settings = AppearanceSettings {
        mode: vstr(cfg, "mode"),
        light_theme: vstr(cfg, "light_theme"),
        dark_theme: vstr(cfg, "dark_theme"),
        accent_source: vstr(cfg, "accent_source"),
        accent_color: vopt(cfg, "accent_color"),
        font_family: vstr(cfg, "font_family"),
        font_size: vnum(cfg, "font_size") as f32,
    };
    if !settings.font_size.is_finite() || !(11. ..=18.).contains(&settings.font_size) {
        return false;
    }
    let settings = if settings.mode.is_empty() {
        AppearanceSettings {
            mode: "dark".into(),
            ..settings
        }
    } else {
        settings
    };
    let system_dark = matches!(
        window.appearance(),
        WindowAppearance::Dark | WindowAppearance::VibrantDark
    );
    let profile = settings.resolve(system_dark);
    let already = APPLIED.with(|slot| slot.borrow().as_ref() == Some(&profile));
    if already {
        return false;
    }

    set_mode(profile.dark);
    set_theme(profile.theme_index);
    apply(cx);
    ACCENT_OVERRIDE.with(|slot| slot.set(profile.accent));

    let appearance = match profile.mode.as_str() {
        "light" => Some(WindowAppearance::Light),
        "dark" => Some(WindowAppearance::Dark),
        _ => None,
    };
    cx.set_window_appearance(appearance);

    if let Some(accent) = profile.accent {
        let color = rgb(accent).into();
        let foreground = rgb(accent_foreground(accent)).into();
        let theme = gpui_component::Theme::global_mut(cx);
        let colors = &mut theme.colors;
        colors.accent = color;
        colors.accent_foreground = foreground;
        colors.primary = color;
        colors.primary_foreground = foreground;
        colors.button_primary = color;
        colors.button_primary_foreground = foreground;
        colors.switch = color;
        colors.selection = ::gpui::rgb(accent).into();
        colors.ring = color;
        theme.tokens = gpui_component::ThemeTokens::from(&theme.colors);
    }
    gpui_component::Theme::global_mut(cx).font_family = profile.font_family.clone().into();

    APPLIED.with(|slot| *slot.borrow_mut() = Some(profile));
    true
}
