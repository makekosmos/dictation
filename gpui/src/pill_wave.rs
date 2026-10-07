//! Waveform rendering for the dictation pill — ported from
//! `desktop/src/views/dictation-pill-waveform.ts` (canvas bars, idle dashed
//! line, synthetic transcribing wave).
use ::gpui::*;

/// dictation-pill-waveform.ts constants (history 120 lives on `DictationApp`).
const WAVE_BAR_W: f32 = 4.;
const WAVE_BAR_GAP: f32 = 3.;
const WAVE_BAR_MIN_H: f32 = 4.;
const WAVE_SENSITIVITY: f32 = 0.8;
const WAVE_FADE_PX: f32 = 48.;

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

/// Instant equalizer shape: every bar follows the LATEST level scaled by a
/// mild center-weighted envelope, plus a per-bar deterministic wobble so the
/// row doesn't move in lockstep. `time` is the pill's animation clock
/// (seconds). At level 0 all bars stay at 0 (paint clamps to min height).
pub fn live_bars(level: f32, time: f32, count: usize) -> Vec<f32> {
    let half = (count / 2) as f32;
    (0..count)
        .map(|i| {
            let pos = ((i as f32) - half) / half.max(1.0);
            let envelope = 1.0 - pos.abs() * 0.35;
            // Per-bar phase/frequency split keeps neighbours out of sync.
            let wobble = (time * 6.0 + i as f32 * 1.7).sin() * 0.5 + 0.5;
            (level * envelope * (0.65 + 0.35 * wobble)).clamp(0.0, 1.0)
        })
        .collect()
}

/// Compact-pill processing indicator: a symmetric pulse travels from the
/// center bars out to both edges, then collapses back to center — one
/// deterministic loop, no history blend. `CONVERGE_PERIOD` is in the pill's
/// `processing_time` units (ticker adds 0.05 per ~16ms frame ⇒ ~0.9s cycle).
const CONVERGE_PERIOD: f32 = 2.8;

pub fn converge_bars(time: f32, count: usize) -> Vec<f32> {
    if count == 0 {
        return Vec::new();
    }
    let phase = (time / CONVERGE_PERIOD).rem_euclid(1.0);
    // Wavefront: 0 = center, 1 = edges, then back to center.
    let front = if phase < 0.5 {
        phase * 2.0
    } else {
        2.0 - phase * 2.0
    };
    let center = (count - 1) as f32 / 2.0;
    let norm = center.max(1.0);
    (0..count)
        .map(|i| {
            let d = ((i as f32) - center).abs() / norm;
            let pulse = (1.0 - (d - front).abs() / 0.45).max(0.0);
            (0.15 + 0.85 * pulse * pulse).clamp(0.0, 1.0)
        })
        .collect()
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
/// dashed center line when idle. `time` drives the live-wave wobble.
pub fn paint_wave(bounds: Bounds<Pixels>, wave: &Wave, color: u32, time: f32, window: &mut Window) {
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
            let mut bars: Vec<f32> = match wave {
                Wave::Live(values) => {
                    let level = values.last().copied().unwrap_or(0.0);
                    live_bars(level, time, bar_count)
                }
                Wave::Processing {
                    time,
                    last_active,
                    blend,
                } => processing_bars(*time, last_active, *blend, bar_count),
                Wave::Idle => unreachable!(),
            };
            // 3-tap spatial smoothing — adjacent bars read as one coherent
            // shape (Vue drew a per-frame spectrum snapshot, not a scrolling
            // history, so neighbour bars were never independent noise).
            if matches!(wave, Wave::Live(_)) && bars.len() > 2 {
                let src = bars.clone();
                for i in 1..bars.len() - 1 {
                    bars[i] = (src[i - 1] + src[i] * 2.0 + src[i + 1]) / 4.0;
                }
            }
            for (i, value) in bars.iter().enumerate() {
                let x = start_x + px(i as f32 * step);
                let amp = value.powf(0.65);
                let bar_h = (amp * h * WAVE_SENSITIVITY).max(WAVE_BAR_MIN_H).min(h);
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

/// Compact pill bars — caller passes the final per-bar heights (0..1):
/// `live_bars` while recording, `converge_bars` while processing.
pub fn paint_compact_wave(bounds: Bounds<Pixels>, bars: &[f32], color: u32, window: &mut Window) {
    const BAR_W: f32 = 3.5;
    const GAP: f32 = 3.;
    let w = f32::from(bounds.size.width);
    let h = f32::from(bounds.size.height);
    if w <= 0.0 || h <= 0.0 || bars.is_empty() {
        return;
    }
    let count = bars.len();
    let total = count as f32 * BAR_W + (count.saturating_sub(1)) as f32 * GAP;
    let start_x = bounds.origin.x + px((w - total) / 2.);
    let center_y = bounds.origin.y + px(h / 2.);
    for (i, value) in bars.iter().enumerate() {
        // dB mapping in session.rs already compresses loudness — linear here.
        let bar_h = (value.clamp(0.0, 1.0) * h).max(2.).min(h);
        let x = start_x + px(i as f32 * (BAR_W + GAP));
        window.paint_quad(
            fill(
                Bounds::from_corners(
                    point(x, center_y - px(bar_h / 2.)),
                    point(x + px(BAR_W), center_y + px(bar_h / 2.)),
                ),
                rgb(color),
            )
            .corner_radii(px((BAR_W / 2.).min(bar_h / 2.))),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{converge_bars, live_bars, CONVERGE_PERIOD};

    #[test]
    fn live_bars_center_weighted() {
        let bars = live_bars(1.0, 0.0, 6);
        assert_eq!(bars.len(), 6);
        assert!(bars.iter().all(|b| *b <= 1.0));
        // Silence stays flat — min-height paint clamp handles the floor.
        assert_eq!(live_bars(0.0, 3.7, 6), vec![0.0; 6]);
        // Center-weighted on average over the wobble period.
        let mut center = 0.0f32;
        let mut edge = 0.0f32;
        for t in [0.0, 0.3, 0.7, 1.1, 1.6, 2.2] {
            let b = live_bars(1.0, t, 6);
            center += b[2] + b[3];
            edge += b[0] + b[5];
        }
        assert!(
            center > edge,
            "center bars must exceed edge bars on average"
        );
    }

    #[test]
    fn converge_bars_is_symmetric() {
        for time in [0.0, 0.4, 0.7, 1.4, 2.1, 5.9] {
            let bars = converge_bars(time, 6);
            for i in 0..3 {
                assert!(
                    (bars[i] - bars[5 - i]).abs() < 1e-6,
                    "bars[{i}]={} must mirror bars[{}]={} at t={time}",
                    bars[i],
                    5 - i,
                    bars[5 - i]
                );
            }
        }
    }

    #[test]
    fn converge_bars_pulses_center_then_edges() {
        // Phase 0: wavefront at center → center bars exceed edge bars.
        let start = converge_bars(0.0, 6);
        assert!(start[2] > start[0] && start[3] > start[5]);
        // Half phase: wavefront at edges → edge bars exceed center bars.
        let half = converge_bars(CONVERGE_PERIOD / 2.0, 6);
        assert!(half[0] > half[2] && half[5] > half[3]);
        // Full period returns to the center-pulse shape.
        let again = converge_bars(CONVERGE_PERIOD, 6);
        assert!(again[2] > again[0]);
    }
}
