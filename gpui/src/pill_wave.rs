//! Waveform rendering for the dictation pill — ported from
//! `desktop/src/views/dictation-pill-waveform.ts` (canvas bars, idle dashed
//! line, synthetic transcribing wave).
use ::gpui::{prelude::*, *};

use crate::app::DictationApp;
use crate::pill::{DictationPill, BOTTOM_MARGIN, PILL_H, PILL_W};
use kosmos_gpui_kit::theme::*;

/// dictation-pill-waveform.ts constants (history 120 lives on `DictationApp`).
const WAVE_BAR_W: f32 = 3.;
const WAVE_BAR_GAP: f32 = 2.;
const WAVE_BAR_MIN_H: f32 = 4.;
const WAVE_SENSITIVITY: f32 = 0.8;
const WAVE_FADE_PX: f32 = 48.;

// Fixed Vue palette (dictation-pill-waveform.ts / DictationPillView.vue CSS).
pub const WAVE_RECORDING: u32 = 0x71717a; // zinc-500
pub const WAVE_WAITING: u32 = 0xf5a524; // amber
pub const WAVE_ERROR: u32 = 0xff453a; // red
pub const DELIVERY_PASTED: u32 = 0x2dd4bf; // teal

/// What the waveform area paints this frame.
#[derive(Clone)]
pub enum Wave {
    /// Dashed center line (idle/starting — nothing captured yet).
    Idle,
    /// Live RMS history pushed by `dictation_audio_level` events.
    Live(Vec<f32>),
    /// Vue `nextProcessingBars` synthetic wave — animates over time.
    Processing {
        time: f32,
        last_active: Vec<f32>,
        blend: f32,
    },
}

/// `sampleWaveformValue` parity — nearest-sample the 120-value history down
/// to the number of bars that fit the canvas width.
fn sample_value(values: &[f32], index: usize, count: usize) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    let src =
        ((index as f32 / (count.max(2) - 1) as f32) * (values.len() - 1) as f32).round() as usize;
    values.get(src).copied().unwrap_or(0.0).clamp(0.0, 1.0)
}

/// `nextProcessingBars` parity — three sine/cosine voices over a center-weight
/// envelope, blended out of the last live waveform.
pub fn processing_bars(time: f32, last_active: &[f32], blend: f32, count: usize) -> Vec<f32> {
    let half = (count / 2).max(1) as f32;
    (0..count)
        .map(|i| {
            let pos = (i as f32 - half) / half;
            let center_weight = 1.0 - pos.abs() * 0.4;
            let w1 = (time * 1.5 + pos * 3.0).sin() * 0.25;
            let w2 = (time * 0.8 - pos * 2.0).sin() * 0.2;
            let w3 = (time * 2.0 + pos).cos() * 0.15;
            let synthetic = (0.2 + w1 + w2 + w3) * center_weight;
            let last = sample_value(last_active, i, count);
            (last * (1.0 - blend) + synthetic * blend).clamp(0.05, 1.0)
        })
        .collect()
}

/// `drawWaveformCanvas` parity — rounded 3px bars, alpha by level, edge fade,
/// dashed center line when idle.
pub fn paint_wave(bounds: Bounds<Pixels>, wave: &Wave, color: u32, window: &mut Window) {
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    if w <= 0.0 || h <= 0.0 {
        return;
    }
    let center_y = bounds.origin.y + px(h / 2.);
    let base = rgb(color);
    match wave {
        Wave::Idle => {
            // setLineDash([2,4]) at 0.22 alpha — dashes of 2px every 6px.
            let mut x = 0.0f32;
            while x < w {
                window.paint_quad(
                    fill(
                        Bounds::from_corners(
                            point(bounds.origin.x + px(x), center_y - px(1.)),
                            point(bounds.origin.x + px((x + 2.).min(w)), center_y + px(1.)),
                        ),
                        base.opacity(0.22),
                    )
                    .corner_radii(px(1.)),
                );
                x += 6.0;
            }
        }
        Wave::Live(_) | Wave::Processing { .. } => {
            let step = WAVE_BAR_W + WAVE_BAR_GAP;
            let bar_count = (w / step).floor().max(1.0) as usize;
            let total =
                bar_count as f32 * WAVE_BAR_W + (bar_count.saturating_sub(1)) as f32 * WAVE_BAR_GAP;
            let start_x = bounds.origin.x + px((w - total) / 2.);
            let bars: Vec<f32> = match wave {
                Wave::Live(values) => (0..bar_count)
                    .map(|i| sample_value(values, i, bar_count))
                    .collect(),
                Wave::Processing {
                    time,
                    last_active,
                    blend,
                } => processing_bars(*time, last_active, *blend, bar_count),
                Wave::Idle => unreachable!(),
            };
            for (i, value) in bars.iter().enumerate() {
                let x = start_x + px(i as f32 * step);
                let bar_h = (value * h * WAVE_SENSITIVITY).max(WAVE_BAR_MIN_H).min(h);
                // destination-out edge fade ≈ per-bar alpha ramp near edges.
                let left_f = ((x - bounds.origin.x) / px(WAVE_FADE_PX)).clamp(0.0, 1.0);
                let right_f = ((bounds.origin.x + px(w) - x) / px(WAVE_FADE_PX)).clamp(0.0, 1.0);
                let alpha = (0.4 + value * 0.6) * left_f.min(right_f);
                window.paint_quad(
                    fill(
                        Bounds::from_corners(
                            point(x, center_y - px(bar_h / 2.)),
                            point(x + px(WAVE_BAR_W), center_y + px(bar_h / 2.)),
                        ),
                        base.opacity(alpha.clamp(0.0, 1.0)),
                    )
                    .corner_radii(px((8.0f32).min(WAVE_BAR_W / 2.).min(bar_h / 2.))),
                );
            }
        }
    }
}

/// Kbd chip for one hotkey part (Vue `KbdKey` parity — bordered mini-key).
pub fn kbd(label: impl Into<String>) -> Div {
    div()
        .px(px(3.))
        .rounded(px(3.))
        .border_1()
        .border_color(fade(FG(), 0.16))
        .bg(fade(FG(), 0.06))
        .text_size(px(9.))
        .text_color(fade(FG(), 0.8))
        .child(label.into())
}

/// Footer hint button — 11px plain text, same hit area semantics as the Vue
/// `pill-footer__hint` buttons.
pub fn hint_button(
    id: &'static str,
    label: &'static str,
    primary: bool,
    cx: &mut Context<DictationPill>,
    on: impl Fn(&mut DictationPill, &ClickEvent, &mut Window, &mut Context<DictationPill>) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .flex()
        .items_center()
        .gap_1()
        .text_size(px(11.))
        .text_color(fade(FG(), if primary { 0.92 } else { 0.8 }))
        .hover(|el| el.text_color(fade(FG(), 0.94)))
        .on_click(cx.listener(on))
        .child(label)
}

/// Recording session phase mirrored from `DictationApp` so the pill and the
/// Диктовка view render the same machine.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillPhase {
    /// `dictation.capture.start` in flight.
    Starting,
    /// Engine-owned WASAPI capture is live; pill window is visible.
    Recording,
    /// `capture.stop` → `speech.transcribe` chain in flight (pill stays open
    /// showing the processing wave, like Vue's transcribing status).
    Processing,
}

/// Footer delivery outcome (Vue `TranscriptDelivery` parity) shown after the
/// transcribe reply until `DictationApp::schedule_pill_close` fires.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PillDelivery {
    Pasted,
    ClipboardOnly,
    ClipboardFallback,
    Failed,
}

/// Open the pill bottom-center of the primary display's work area. Returns
/// `None` when no display is available or window creation failed — callers
/// fall back to the in-view status instead of crashing the app.
pub fn open(
    manager: Entity<DictationApp>,
    hotkey: String,
    cx: &mut App,
) -> Option<WindowHandle<DictationPill>> {
    let display = cx.primary_display()?;
    let area = display.visible_bounds();
    let origin = point(
        area.center().x - px(PILL_W / 2.),
        area.origin.y + area.size.height - px(PILL_H + BOTTOM_MARGIN),
    );
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin,
                size: size(px(PILL_W), px(PILL_H)),
            })),
            titlebar: None,
            focus: false,
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|cx| DictationPill::new(manager, crate::pill::PillPhase::Starting, hotkey, cx))
        },
    )
    .ok()
}
