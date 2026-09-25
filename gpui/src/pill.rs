//! Dictation pill — always-on-top overlay shown while a recording session is
//! active. Visual parity with the Electron/Vue pill
//! (desktop/electron/dictation-pill.ts + DictationPillView.vue): 380×126,
//! waveform canvas over a 34px footer, live RMS levels from Engine
//! `dictation_audio_level` WS events, `WindowKind::PopUp` + `focus: false`
//! (never steals foreground → auto_paste inject target stays put), delivery
//! states linger for the Vue `deliveryFinishDelay` durations.
use ::gpui::{prelude::*, *};

use crate::app::DictationApp;
use crate::pill_wave::{
    hint_button, kbd, paint_wave, Wave, DELIVERY_PASTED, WAVE_ERROR, WAVE_RECORDING, WAVE_WAITING,
};
pub use crate::pill_wave::{open, PillDelivery, PillPhase};
use kosmos_gpui_kit::theme::*;

/// Vue pill body is 380x126 (`dictation-pill.ts` PILL_WIDTH/PILL_HEIGHT).
pub(crate) const PILL_W: f32 = 380.;
pub(crate) const PILL_H: f32 = 126.;
/// Bottom-center of the work area, like the Electron pill.
pub(crate) const BOTTOM_MARGIN: f32 = 100.;
const FOOTER_H: f32 = 34.;

impl PillDelivery {
    /// Vue `deliveryLabel()` parity.
    fn label(self) -> &'static str {
        match self {
            PillDelivery::Pasted => "Вставлено",
            PillDelivery::ClipboardOnly => "Буфер обмена — вставьте вручную",
            PillDelivery::ClipboardFallback => "Вставка не удалась — текст в буфере",
            PillDelivery::Failed => "Не доставлено",
        }
    }

    /// Vue `pill-footer__dot--delivery-*` parity.
    fn dot_color(self) -> u32 {
        match self {
            PillDelivery::Pasted => DELIVERY_PASTED,
            PillDelivery::ClipboardOnly => WAVE_WAITING,
            PillDelivery::ClipboardFallback | PillDelivery::Failed => WAVE_ERROR,
        }
    }
}

impl PillPhase {
    /// Vue `footerLabel()` parity (status side).
    pub fn label(self) -> &'static str {
        match self {
            PillPhase::Starting => "Запуск записи…",
            PillPhase::Recording => "Идёт запись",
            PillPhase::Processing => "Распознаю",
        }
    }
}

pub struct DictationPill {
    /// Click handlers only — the window is opened from inside a
    /// `DictationApp` update, so `render` must NEVER touch this entity
    /// (re-entrant read → "already being updated" panic). State is a
    /// snapshot pushed by `DictationApp::push_pill`.
    manager: Entity<DictationApp>,
    phase: PillPhase,
    delivery: Option<PillDelivery>,
    /// Ring buffer of the last 120 RMS samples pushed by the owner.
    levels: Vec<f32>,
    hotkey: String,
    /// Enter-animation clock (`pill-in`, 260ms) — drives repaints through
    /// the ticker instead of `with_animation`, which only queued a frame per
    /// notify and rendered at ~5fps on this no-activate popup.
    enter_at: std::time::Instant,
    /// Synthetic-wave clock for the Processing status (ticks at ~30fps only
    /// while the phase needs animation).
    processing_time: f32,
    /// Waveform snapshot at the moment Recording → Processing flipped —
    /// the processing wave blends out of it (`processingTransitionProgress`).
    last_active: Vec<f32>,
    processing_blend: f32,
    _ticker: Task<()>,
}

impl DictationPill {
    pub(crate) fn new(
        manager: Entity<DictationApp>,
        phase: PillPhase,
        hotkey: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            let alive = this.update(cx, |this, cx| {
                let entering = this.enter_at.elapsed() < std::time::Duration::from_millis(320);
                let processing = this.phase == PillPhase::Processing && this.delivery.is_none();
                if processing {
                    this.processing_time += 0.05;
                    this.processing_blend = (this.processing_blend + 0.033).min(1.0);
                }
                // Enter animation + live waveform both repaint at tick rate;
                // processing additionally advances its synthetic wave clock.
                if entering || processing || this.phase == PillPhase::Recording {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        });
        Self {
            manager,
            phase,
            delivery: None,
            levels: Vec::new(),
            hotkey,
            enter_at: std::time::Instant::now(),
            processing_time: 0.0,
            last_active: Vec::new(),
            processing_blend: 0.0,
            _ticker: ticker,
        }
    }

    /// Owner pushes the session snapshot after every mutation (`DictationApp::push_pill`).
    pub(crate) fn set_state(
        &mut self,
        phase: PillPhase,
        delivery: Option<PillDelivery>,
        levels: Vec<f32>,
        hotkey: String,
        cx: &mut Context<Self>,
    ) {
        if phase == PillPhase::Processing && self.phase != PillPhase::Processing {
            // Vue `processingTransitionProgress`: the synthetic wave blends
            // out of the last live waveform.
            self.last_active = levels.clone();
            self.processing_time = 0.0;
            self.processing_blend = 0.0;
        }
        self.phase = phase;
        self.delivery = delivery;
        self.levels = levels;
        self.hotkey = hotkey;
        cx.notify();
    }
}

impl Render for DictationPill {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (phase, delivery, hotkey) = (self.phase, self.delivery, self.hotkey.clone());
        let recording = phase == PillPhase::Recording;

        let (wave, wave_color) = match delivery {
            Some(d) => (Wave::Idle, d.dot_color()),
            None => match phase {
                PillPhase::Starting => (Wave::Idle, WAVE_RECORDING),
                PillPhase::Recording => (Wave::Live(self.levels.clone()), WAVE_RECORDING),
                PillPhase::Processing => (
                    Wave::Processing {
                        time: self.processing_time,
                        last_active: self.last_active.clone(),
                        blend: self.processing_blend,
                    },
                    WAVE_RECORDING,
                ),
            },
        };

        let status_label = delivery
            .map(PillDelivery::label)
            .unwrap_or_else(|| phase.label());
        // Vue `pill-footer__dot`: fg@44% at idle, red while recording, amber on
        // waiting; delivery overrides with its own palette.
        let dot: Hsla = match delivery {
            Some(d) => rgb(d.dot_color()).into(),
            None => match phase {
                PillPhase::Recording => rgb(WAVE_ERROR).into(),
                _ => fade(FG(), 0.44),
            },
        };
        // Ring ≈ Vue `box-shadow: 0 0 0 2px <dot>@14%`.
        let dot_ring = dot.opacity(0.14);

        let hotkey_parts: Vec<String> = hotkey
            .split('+')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();

        // Vue `pill-in` parity: scale(0.96)+fade → scale(1) over 260ms on
        // cubic-bezier(0.2, 0.7, 0.2, 1.4). The window is exactly pill-sized,
        // so we animate the inner container's size instead of a transform.
        let pill_body = div()
            .w(px(PILL_W))
            .h(px(PILL_H))
            .rounded(px(8.))
            .border_1()
            .border_color(fade(FG(), 0.18))
            .bg(fade(POPOVER(), 0.94))
            .flex()
            .flex_col()
            .overflow_hidden()
            .font_family("Inter")
            .text_color(c(FG()))
            // --- Waveform area (~92px) ---
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .px(px(10.))
                    .pt(px(8.))
                    .pb(px(6.))
                    .child(
                        canvas(
                            move |_, _, _| (wave, wave_color),
                            move |bounds, (wave, color), window, _| {
                                paint_wave(bounds, &wave, color, window);
                            },
                        )
                        .size_full(),
                    ),
            )
            // --- Footer (34px): dot + status … actions ---
            .child(
                div()
                    .w_full()
                    .h(px(FOOTER_H))
                    .border_t_1()
                    .border_color(fade(FG(), 0.14))
                    .bg(fade(BG(), 0.18))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(10.))
                    .px(px(10.))
                    .text_size(px(11.))
                    .text_color(fade(FG(), 0.78))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.))
                            .min_w_0()
                            .whitespace_nowrap()
                            .child(
                                div()
                                    .w(px(7.))
                                    .h(px(7.))
                                    .flex_none()
                                    .rounded_full()
                                    .border_2()
                                    .border_color(dot_ring)
                                    .bg(dot),
                            )
                            .child(status_label),
                    )
                    .child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(px(7.))
                            .child(hint_button(
                                "pill-cancel",
                                "Отмена",
                                false,
                                cx,
                                |this, _, _, cx| {
                                    let manager = this.manager.clone();
                                    manager.update(cx, |app, cx| app.dictation_cancel(cx));
                                },
                            ))
                            .when(recording && delivery.is_none(), |el| {
                                el.child(div().w(px(1.)).h(px(12.)).bg(fade(FG(), 0.16)))
                                    .child(
                                        hint_button(
                                            "pill-stop",
                                            "Отправить",
                                            true,
                                            cx,
                                            |this, _, _, cx| {
                                                let manager = this.manager.clone();
                                                manager
                                                    .update(cx, |app, cx| app.dictation_toggle(cx));
                                            },
                                        )
                                        .children(
                                            hotkey_parts.iter().map(|part| {
                                                kbd(if part.eq_ignore_ascii_case("shift") {
                                                    "⇧".to_string()
                                                } else {
                                                    part.clone()
                                                })
                                            }),
                                        ),
                                    )
                            }),
                    ),
            );

        // Wrapper centers the animated body so the grow/shrink scales from
        // the middle, like CSS `transform: scale()` on the Vue root. The
        // eased progress is clocked by `enter_at` — the ticker notifies at
        // ~60fps during the first 320ms.
        let enter_t = (self.enter_at.elapsed().as_secs_f32() / 0.26).min(1.0);
        let eased = cubic_bezier(enter_t, 0.2, 0.7, 0.2, 1.4);
        let scale = 0.96 + 0.04 * eased;
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                pill_body
                    .w(px(PILL_W * scale))
                    .h(px(PILL_H * scale))
                    .opacity(eased),
            )
    }
}

/// CSS cubic-bezier(x1, y1, x2, y2) — Newton-solve x(u)=t, then y(u).
/// Ported curve for the Vue `pill-in` keyframes.
fn cubic_bezier(t: f32, _x1: f32, y1: f32, _x2: f32, y2: f32) -> f32 {
    fn curve(a1: f32, a2: f32, u: f32) -> f32 {
        let i = 1.0 - u;
        3.0 * i * i * u * a1 + 3.0 * i * u * u * a2 + u * u * u
    }
    fn deriv(a1: f32, a2: f32, u: f32) -> f32 {
        let i = 1.0 - u;
        3.0 * i * i * a1 + 6.0 * i * u * (a2 - a1) + 3.0 * u * u * (1.0 - a2)
    }
    let (x1, x2) = (_x1, _x2);
    let mut u = t;
    for _ in 0..8 {
        let x = curve(x1, x2, u) - t;
        if x.abs() < 1e-4 {
            break;
        }
        let d = deriv(x1, x2, u);
        if d.abs() < 1e-6 {
            break;
        }
        u = (u - x / d).clamp(0.0, 1.0);
    }
    curve(y1, y2, u)
}
