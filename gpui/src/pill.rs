//! Dictation pill — always-on-top overlay shown while a recording session is
//! active. Visual parity with the Electron/Vue pill
//! (desktop/electron/dictation-pill.ts + DictationPillView.vue): 380×126,
//! waveform canvas over a 34px footer, live RMS levels from Engine
//! `dictation_audio_level` WS events, `WindowKind::PopUp` + `focus: false`
//! (never steals foreground → auto_paste inject target stays put), delivery
//! states linger for the Vue `deliveryFinishDelay` durations.
use ::gpui::{prelude::*, *};

use crate::app::DictationApp;
use crate::pill_wave::{paint_wave, Wave};
use mundus_gpui_kit::theme::*;

/// Vue pill body is 380x126 (`dictation-pill.ts` PILL_WIDTH/PILL_HEIGHT).
pub(crate) const PILL_W: f32 = 380.;
pub(crate) const PILL_H: f32 = 126.;
/// Bottom-center of the work area, like the Electron pill.
pub(crate) const BOTTOM_MARGIN: f32 = 100.;
const FOOTER_H: f32 = 34.;

// Fixed Vue palette (dictation-pill-waveform.ts / DictationPillView.vue CSS).
pub const WAVE_RECORDING: u32 = 0x71717a; // zinc-500
pub const WAVE_WAITING: u32 = 0xf5a524; // amber
pub const WAVE_ERROR: u32 = 0xff453a; // red
pub const DELIVERY_PASTED: u32 = 0x2dd4bf; // teal

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
        move |_, cx| cx.new(|cx| DictationPill::new(manager, PillPhase::Starting, hotkey, cx)),
    )
    .ok()
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
    /// Per-frame eased copy of `levels` — bars glide toward the latest
    /// snapshot at render fps instead of stepping at the data rate.
    display: Vec<f32>,
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
                if this.phase == PillPhase::Recording {
                    for (d, t) in this.display.iter_mut().zip(this.levels.iter()) {
                        *d += (t - *d) * 0.35;
                    }
                }
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
            display: Vec::new(),
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
        if self.display.len() != self.levels.len() {
            // Session/target width changed — start flat, glide up.
            self.display.resize(self.levels.len(), 0.0);
        }
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
                PillPhase::Recording => (Wave::Live(self.display.clone()), WAVE_RECORDING),
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
            .bg(fade(POPOVER(), 1.0))
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
fn cubic_bezier(t: f32, x1: f32, y1: f32, x2: f32, y2: f32) -> f32 {
    fn curve(a1: f32, a2: f32, u: f32) -> f32 {
        let i = 1.0 - u;
        3.0 * i * i * u * a1 + 3.0 * i * u * u * a2 + u * u * u
    }
    fn deriv(a1: f32, a2: f32, u: f32) -> f32 {
        let i = 1.0 - u;
        3.0 * i * i * a1 + 6.0 * i * u * (a2 - a1) + 3.0 * u * u * (1.0 - a2)
    }
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
