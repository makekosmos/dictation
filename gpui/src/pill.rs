//! HEX-style dot-scope overlay, driven by Engine audio-level events.
//! Its transparent window keeps the established bottom position while the
//! visible 112×26 plate matches HEX's dot-matrix HUD.
use ::gpui::{prelude::*, *};

use crate::dot_scope::{Frame, Scope as DotScope, Stage, Tuning};
use crate::gpu_scope::{SharedGpuScope, HEIGHT, WIDTH};

const PILL_W: f32 = WIDTH as f32;
const PILL_H: f32 = HEIGHT as f32;
/// Preserve the HEX face's established vertical center independently of its shadow canvas.
const CENTER_FROM_BOTTOM: f32 = 163.;

pub(crate) fn diagnostic(message: &str) {
    use std::io::Write as _;

    let Ok(dir) = mundus_gpui_kit::engine::data_dir() else {
        return;
    };
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let Ok(mut log) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(dir.join("dictation-gpui-diagnostics.log"))
    else {
        return;
    };
    let elapsed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    let _ = writeln!(log, "{elapsed} {message}");
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
    phase: PillPhase,
    delivery: Option<PillDelivery>,
    levels: Vec<f32>,
) -> Option<WindowHandle<DictationPill>> {
    let display = cx.primary_display()?;
    let area = display.visible_bounds();
    let (width, height) = (PILL_W, PILL_H);
    let origin = point(
        area.center().x - px(width / 2.),
        area.origin.y + area.size.height - px(CENTER_FROM_BOTTOM + height / 2.),
    );
    let bounds = Bounds {
        origin,
        size: size(px(width), px(height)),
    };
    let handle = cx
        .open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: None,
                focus: false,
                show: !cfg!(windows),
                kind: WindowKind::PopUp,
                is_movable: false,
                is_resizable: false,
                is_minimizable: false,
                window_background: WindowBackgroundAppearance::Transparent,
                ..Default::default()
            },
            move |_window, cx| {
                let hwnd = {
                    #[cfg(windows)]
                    {
                        configure_pill_window(_window, bounds);
                        native_hwnd(_window).map(|hwnd| hwnd as usize)
                    }
                    #[cfg(not(windows))]
                    {
                        None
                    }
                };
                cx.new(|cx| DictationPill::new(phase, delivery, levels, gpu_scope, hwnd, cx))
            },
        )
        .ok()?;
    Some(handle)
}

pub(crate) fn close(handle: WindowHandle<DictationPill>, cx: &mut App) {
    if handle
        .update(cx, |_, window, _| remove_window(window))
        .is_err()
    {
        cx.defer(move |cx| {
            handle.update(cx, |_, window, _| remove_window(window)).ok();
        });
    }
}

fn remove_window(window: &mut Window) {
    #[cfg(windows)]
    if let Some(hwnd) = native_hwnd(window) {
        unsafe {
            windows_sys::Win32::UI::WindowsAndMessaging::ShowWindow(
                hwnd,
                windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE,
            );
        }
    }
    window.remove_window();
}

#[cfg(windows)]
fn native_hwnd(window: &Window) -> Option<windows_sys::Win32::Foundation::HWND> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get() as _),
        _ => None,
    }
}

fn request_frame(window: &mut Window) {
    window.refresh();
    #[cfg(windows)]
    if let Some(hwnd) = native_hwnd(window) {
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_PAINT};
            PostMessageW(hwnd, WM_PAINT, 0, 0);
        }
    }
}

#[cfg(windows)]
fn reveal_pill(hwnd: usize) -> bool {
    unsafe {
        use windows_sys::Win32::Graphics::Dwm::{DwmFlush, DwmSetWindowAttribute, DWMWA_CLOAK};
        use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_SHOWNOACTIVATE};
        let hwnd = hwnd as _;
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        DwmFlush();
        let cloak: i32 = 0;
        let result = DwmSetWindowAttribute(hwnd, DWMWA_CLOAK as u32, &cloak as *const _ as _, 4);
        if result < 0 {
            diagnostic(&format!(
                "pill reveal failed: DwmSetWindowAttribute={result}"
            ));
            false
        } else {
            diagnostic("pill revealed");
            true
        }
    }
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
fn configure_pill_window(window: &mut Window, bounds: Bounds<Pixels>) {
    let Some(hwnd) = native_hwnd(window) else {
        return;
    };
    unsafe {
        use windows_sys::Win32::Graphics::Dwm::{
            DwmSetWindowAttribute, DWMNCRP_DISABLED, DWMWA_BORDER_COLOR, DWMWA_CLOAK,
            DWMWA_COLOR_NONE, DWMWA_NCRENDERING_POLICY, DWMWA_WINDOW_CORNER_PREFERENCE,
            DWMWCP_DONOTROUND,
        };
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            SetWindowPos, SWP_NOACTIVATE, SWP_NOZORDER,
        };
        let border = DWMWA_COLOR_NONE;
        DwmSetWindowAttribute(hwnd, DWMWA_BORDER_COLOR as u32, &border as *const _ as _, 4);
        let corner = DWMWCP_DONOTROUND;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE as u32,
            &corner as *const _ as _,
            4,
        );
        let ncrp = DWMNCRP_DISABLED;
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_NCRENDERING_POLICY as u32,
            &ncrp as *const _ as _,
            4,
        );
        let cloak: i32 = 1;
        DwmSetWindowAttribute(hwnd, DWMWA_CLOAK as u32, &cloak as *const _ as _, 4);
        clear_accent_backdrop(hwnd);

        // Apply the intended bounds before GPUI shows the first frame.
        let bounds = bounds.to_device_pixels(window.scale_factor());
        let ok = SetWindowPos(
            hwnd,
            std::ptr::null_mut(),
            bounds.origin.x.0,
            bounds.origin.y.0,
            bounds.size.width.0,
            bounds.size.height.0,
            SWP_NOACTIVATE | SWP_NOZORDER,
        );
        if ok == 0 {
            eprintln!("dictation-gpui: failed to position hidden pill window");
        }
    }
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
    last_render: Option<std::time::Instant>,
    gpu_error_logged: bool,
    hwnd: Option<usize>,
    #[cfg(windows)]
    revealed: bool,
    _ticker: Task<()>,
}

impl DictationPill {
    pub(crate) fn new(
        phase: PillPhase,
        delivery: Option<PillDelivery>,
        levels: Vec<f32>,
        gpu_scope: Option<SharedGpuScope>,
        hwnd: Option<usize>,
        cx: &mut Context<Self>,
    ) -> Self {
        diagnostic(&format!(
            "pill opened phase={phase:?} delivery={delivery:?}"
        ));
        let ticker = cx.spawn(async move |this, cx| loop {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(16))
                .await;
            let alive = this.update(cx, |this, cx| {
                let now = std::time::Instant::now();
                let dt = now.duration_since(this.last_tick).as_secs_f32();
                this.last_tick = now;
                let raw = this.levels.last().copied().unwrap_or(0.0);
                // Engine sends a -55..0 dBFS meter mapped to 0..1, not RMS.
                let loudness = if raw < 0.12 {
                    -120.0
                } else {
                    -55.0 + raw * 40.0
                };
                this.dot_scope.advance(
                    dt,
                    Frame {
                        stage: this.stage,
                        visible: true,
                        loudness,
                        reduce_motion: false,
                    },
                );
                #[cfg(windows)]
                if !this.revealed && this.rendered_scope.is_some() {
                    this.revealed = this.hwnd.is_some_and(reveal_pill);
                }
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
            delivery,
            levels,
            dot_scope: DotScope::new(Tuning {
                noise_floor: -42.0,
                ..Tuning::default()
            }),
            stage: stage_for(phase, delivery),
            last_tick: std::time::Instant::now(),
            gpu_scope,
            rendered_scope: None,
            gpu_error_logged: false,
            last_render: None,
            hwnd,
            #[cfg(windows)]
            revealed: false,
            _ticker: ticker,
        }
    }

    pub(crate) fn set_gpu_scope(&mut self, gpu_scope: SharedGpuScope, window: &mut Window) {
        if self.gpu_scope.is_none() {
            self.gpu_scope = Some(gpu_scope);
            request_frame(window);
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
        if self.phase != phase || self.delivery != delivery {
            diagnostic(&format!(
                "pill state phase={:?}->{phase:?} stage={:?} delivery={:?}->{delivery:?}",
                self.phase, self.stage, self.delivery
            ));
        }
        self.stage = stage_for(phase, delivery);
        self.phase = phase;
        self.delivery = delivery;
        self.levels = levels;
        cx.notify();
    }
}

impl DictationPill {
    fn render_hex(&mut self, window: &mut Window) -> Div {
        if self
            .last_render
            .is_some_and(|last| last.elapsed() < std::time::Duration::from_millis(33))
        {
            if let Some(image) = self.rendered_scope.clone() {
                return div()
                    .w(px(PILL_W))
                    .h(px(PILL_H))
                    .child(img(ImageSource::Render(image)).size_full());
            }
        }
        self.last_render = Some(std::time::Instant::now());
        if let Some(renderer) = &self.gpu_scope {
            match renderer.lock() {
                Ok(mut renderer) => match renderer.render(&self.dot_scope) {
                    Ok(image) => {
                        if self.rendered_scope.is_none() {
                            let pose = self.dot_scope.pose();
                            diagnostic(&format!(
                                "pill first GPU frame phase={:?} stage={:?} cool={:.3}",
                                self.phase, self.stage, pose.cool
                            ));
                        }
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
                        diagnostic(&format!("pill GPU frame error: {error:#}"));
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

fn stage_for(phase: PillPhase, delivery: Option<PillDelivery>) -> Stage {
    match delivery {
        Some(PillDelivery::Failed) => Stage::Failed,
        Some(_) => Stage::Done,
        None => match phase {
            PillPhase::Starting | PillPhase::Recording => Stage::Recording,
            PillPhase::Processing => Stage::Processing,
        },
    }
}

impl Render for DictationPill {
    fn render(&mut self, window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        self.render_hex(window)
    }
}
