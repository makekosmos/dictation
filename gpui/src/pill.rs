//! HEX-style dot-scope overlay, driven by Engine audio-level events.
//! Its transparent window keeps the established bottom position while the
//! visible 112×26 plate matches HEX's dot-matrix HUD.
use ::gpui::{prelude::*, *};

use crate::dot_scope::{Frame, Scope as DotScope, Stage, Tuning};
use crate::gpu_scope::SharedGpuScope;

/// Transparent overlay window bounds; kept fixed to preserve its screen position.
pub(crate) const PILL_W: f32 = 380.;
pub(crate) const PILL_H: f32 = 126.;
/// Bottom-center of the work area, like the Electron pill.
pub(crate) const BOTTOM_MARGIN: f32 = 100.;

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

/// Delivery outcome shown briefly after transcription, as in HEX's HUD.
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
    cx: &mut App,
    gpu_scope: Option<SharedGpuScope>,
) -> Option<WindowHandle<DictationPill>> {
    let display = cx.primary_display()?;
    let area = display.visible_bounds();
    let (width, height, margin) = (PILL_W, PILL_H, BOTTOM_MARGIN);
    let origin = point(
        area.center().x - px(width / 2.),
        area.origin.y + area.size.height - px(height + margin),
    );
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds {
                    origin,
                    size: size(px(width), px(height)),
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
            move |_, cx| cx.new(|cx| DictationPill::new(PillPhase::Starting, gpu_scope, cx)),
        )
        .ok()?;
    strip_dwm_frame(&handle, cx);
    Some(handle)
}

/// Win11 DWM decorates even a transparent borderless PopUp with a 1px frame,
/// corner rounding AND a shape-hugging drop shadow — plus gpui's
/// `WindowBackgroundAppearance::Transparent` sets ACCENT state 2
/// (ENABLE_TRANSPARENTGRADIENT), which paints a faint tint over a wide
/// rectangle. Measured fix (each verified on a live pill HWND over a flat
/// background): border COLOR_NONE + DONOTROUND for the frame, ACCENT_DISABLED
/// for the tint, and DWMWA_NCREDERING_POLICY = DWMNCRP_DISABLED for the
/// shadow ring. Together they leave only the rounded pill silhouette.
#[cfg(windows)]
fn strip_dwm_frame(handle: &WindowHandle<DictationPill>, cx: &mut App) {
    let _ = handle.update(cx, |_, window, _| {
        use raw_window_handle::RawWindowHandle;
        if let Ok(wh) = raw_window_handle::HasWindowHandle::window_handle(window) {
            if let RawWindowHandle::Win32(w32) = wh.as_raw() {
                let hwnd = w32.hwnd.get() as _;
                unsafe {
                    use windows_sys::Win32::Graphics::Dwm::{
                        DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_BORDER_COLOR,
                        DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DWMWA_WINDOW_CORNER_PREFERENCE,
                        DWMWCP_DONOTROUND,
                    };
                    let border = DWMWA_COLOR_NONE;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_BORDER_COLOR as u32,
                        &border as *const _ as _,
                        4,
                    );
                    let corner = DWMWCP_DONOTROUND;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_WINDOW_CORNER_PREFERENCE as u32,
                        &corner as *const _ as _,
                        4,
                    );
                    // Kills the DWM drop shadow that hugs the pill
                    // silhouette and shows through the transparent margin.
                    let ncrp = DWMNCRP_DISABLED;
                    DwmSetWindowAttribute(
                        hwnd,
                        DWMWA_NCRENDERING_POLICY as u32,
                        &ncrp as *const _ as _,
                        4,
                    );
                    clear_accent_backdrop(hwnd);
                }
            }
        }
    });
}

/// ACCENT_DISABLED via the undocumented user32 SetWindowCompositionAttribute
/// — same struct layout gpui-pre-windows uses (window.rs), only state 0
/// instead of 2.
#[cfg(windows)]
unsafe fn clear_accent_backdrop(hwnd: *mut core::ffi::c_void) {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};

    #[repr(C)]
    struct AccentPolicy {
        accent_state: u32,
        accent_flags: u32,
        gradient_color: u32,
        animation_id: u32,
    }
    #[repr(C)]
    struct WindowCompositionAttribData {
        attrib: u32,
        pv_data: *mut core::ffi::c_void,
        cb_data: usize,
    }
    type SetWindowCompositionAttributeFn =
        unsafe extern "system" fn(HWND, *mut WindowCompositionAttribData) -> i32;

    let user32 = unsafe { GetModuleHandleA(c"user32.dll".as_ptr() as *const u8) };
    if user32.is_null() {
        return;
    }
    let Some(raw) = (unsafe {
        GetProcAddress(
            user32,
            c"SetWindowCompositionAttribute".as_ptr() as *const u8,
        )
    }) else {
        return;
    };
    let set: SetWindowCompositionAttributeFn = unsafe { std::mem::transmute(raw) };
    let accent = AccentPolicy {
        accent_state: 0, // ACCENT_DISABLED
        accent_flags: 0,
        gradient_color: 0,
        animation_id: 0,
    };
    let mut data = WindowCompositionAttribData {
        attrib: 0x13, // WCA_ACCENT_POLICY
        pv_data: &accent as *const _ as *mut _,
        cb_data: std::mem::size_of::<AccentPolicy>(),
    };
    let _ = set(hwnd as HWND, &mut data as *mut _);
}

/// Non-Windows platforms draw no frame around a `WindowKind::PopUp`.
#[cfg(not(windows))]
fn strip_dwm_frame(_handle: &WindowHandle<DictationPill>, _cx: &mut App) {}

pub struct DictationPill {
    phase: PillPhase,
    delivery: Option<PillDelivery>,
    /// Ring buffer of the last 120 RMS samples pushed by the owner.
    levels: Vec<f32>,
    /// HEX's fixed-step Dot Scope simulation.
    dot_scope: DotScope,
    stage: Stage,
    last_tick: std::time::Instant,
    gpu_scope: Option<SharedGpuScope>,
    rendered_scope: Option<std::sync::Arc<::gpui::RenderImage>>,
    gpu_error_logged: bool,
    _ticker: Task<()>,
}

impl DictationPill {
    pub(crate) fn new(
        phase: PillPhase,
        gpu_scope: Option<SharedGpuScope>,
        cx: &mut Context<Self>,
    ) -> Self {
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            let alive = this.update(cx, |this, cx| {
                let now = std::time::Instant::now();
                let dt = now.duration_since(this.last_tick).as_secs_f32();
                this.last_tick = now;
                let raw = this.levels.last().copied().unwrap_or(0.0);
                let rms = (raw / 5.0).max(0.0);
                this.dot_scope.advance(
                    dt,
                    Frame {
                        stage: this.stage,
                        visible: true,
                        loudness: crate::dot_scope::loudness(rms),
                        reduce_motion: false,
                    },
                );
                if this.phase == PillPhase::Starting
                    || this.phase == PillPhase::Recording
                    || this.phase == PillPhase::Processing
                    || this.delivery.is_some()
                    || !this.dot_scope.settled()
                {
                    cx.notify();
                }
            });
            if alive.is_err() {
                break;
            }
        });
        Self {
            phase,
            delivery: None,
            levels: Vec::new(),
            dot_scope: DotScope::new(Tuning::default()),
            stage: Stage::Recording,
            last_tick: std::time::Instant::now(),
            gpu_scope,
            rendered_scope: None,
            gpu_error_logged: false,
            _ticker: ticker,
        }
    }

    pub(crate) fn set_gpu_scope(&mut self, gpu_scope: SharedGpuScope) {
        if self.gpu_scope.is_none() {
            self.gpu_scope = Some(gpu_scope);
        }
    }

    /// Owner pushes the session snapshot after every mutation (`DictationApp::push_pill`).
    pub(crate) fn set_state(
        &mut self,
        phase: PillPhase,
        delivery: Option<PillDelivery>,
        levels: Vec<f32>,
        cx: &mut Context<Self>,
    ) {
        if phase == PillPhase::Recording && self.phase != PillPhase::Recording {
            self.dot_scope.restart(false);
            self.stage = Stage::Recording;
        }
        if delivery.is_some() && self.delivery.is_none() {
            self.stage = if delivery == Some(PillDelivery::Failed) {
                Stage::Failed
            } else {
                Stage::Done
            };
        } else if delivery.is_none() {
            self.stage = match phase {
                PillPhase::Starting | PillPhase::Recording => Stage::Recording,
                PillPhase::Processing => Stage::Processing,
            };
        }
        self.phase = phase;
        self.delivery = delivery;
        self.levels = levels;
        cx.notify();
    }
}

impl DictationPill {
    fn render_hex(&mut self, window: &mut Window) -> Div {
        if let Some(renderer) = &self.gpu_scope {
            match renderer.lock() {
                Ok(mut renderer) => match renderer.render(&self.dot_scope) {
                    Ok(image) => {
                        if let Some(previous) = self.rendered_scope.replace(image.clone()) {
                            let _ = window.drop_image(previous);
                        }
                        return div()
                            .w(px(PILL_W))
                            .h(px(PILL_H))
                            .child(img(ImageSource::Render(image)).size_full());
                    }
                    Err(error) if !self.gpu_error_logged => {
                        eprintln!("dictation-gpui: HEX WGSL frame failed: {error:#}");
                        self.gpu_error_logged = true;
                    }
                    Err(_) => {}
                },
                Err(_) if !self.gpu_error_logged => {
                    eprintln!("dictation-gpui: HEX WGSL renderer lock poisoned");
                    self.gpu_error_logged = true;
                }
                Err(_) => {}
            }
        }
        let container = div().w(px(PILL_W)).h(px(PILL_H));
        if let Some(image) = self.rendered_scope.clone() {
            container.child(img(ImageSource::Render(image)).size_full())
        } else {
            container
        }
    }
}

impl Render for DictationPill {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_hex(window)
    }
}
