//! dictation-gpui — standalone Kosmos Dictation overlay app. All native work
//! (global hotkey hook, WASAPI capture, transcription, injection) is owned by
//! Kosmos Engine; this process is the UI client over /v1/rpc + the Engine
//! broadcast WebSocket (dictation_toggle_trigger → pill session).
#![windows_subsystem = "windows"]

mod app;
mod assets;
mod pill;
mod pill_wave;
mod view;
mod worker;

use std::sync::atomic::AtomicBool;

use gpui::{
    px, size, App, AppContext, Bounds, Context, SharedString, Window, WindowBounds, WindowOptions,
};

use app::DictationApp;

/// `DICTATION_GPUI_OFFSCREEN=1` parks the window far outside the desktop for
/// automated runs (same convention as manager-gpui's MANAGER_GPUI_OFFSCREEN).
fn window_bounds(cx: &mut App) -> Bounds<gpui::Pixels> {
    if std::env::var("DICTATION_GPUI_OFFSCREEN").is_ok() {
        gpui::bounds(
            gpui::point(px(-20000.), px(-20000.)),
            size(px(420.), px(620.)),
        )
    } else {
        Bounds::centered(None, size(px(420.), px(620.)), cx)
    }
}

/// Set when a second process launch asks the running instance to resurface
/// its status window (drain loop consumes the flag and activates it).
pub(crate) static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Single-instance guard: every process subscribes to Engine hotkey events,
/// so a duplicate instance would spawn a second pill for each press. The
/// mutex handle intentionally leaks for the process lifetime. A second launch
/// signals `Local\KosmosDictationGpuiShow` so the running instance reopens
/// its (possibly minimized) status window, then exits quietly.
#[cfg(windows)]
fn claim_single_instance() -> bool {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
        System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject},
    };
    let mutex_name: Vec<u16> = "Local\\KosmosDictationGpui\0".encode_utf16().collect();
    let event_name: Vec<u16> = "Local\\KosmosDictationGpuiShow\0".encode_utf16().collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if handle.is_null() {
        return true; // fail-open: running beats not running
    }
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr()) };
    if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
        if !event.is_null() {
            unsafe { SetEvent(event) };
        }
        return false;
    }
    if !event.is_null() {
        // HANDLE as usize — the waiter thread owns the handle for life
        // (raw pointers aren't Send; usize is).
        let event = event as usize;
        std::thread::spawn(move || unsafe {
            loop {
                WaitForSingleObject(event as _, u32::MAX);
                SHOW_REQUESTED.store(true, Ordering::SeqCst);
            }
        });
    }
    true
}

fn main() {
    #[cfg(windows)]
    if !claim_single_instance() {
        return;
    }
    // Explicit: the pill is a transient overlay and the status window may be
    // minimized/closed-adjacent — dictation keeps working without any window.
    gpui::application()
        .with_quit_mode(gpui::QuitMode::Explicit)
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            gpui_component::init(cx);
            cx.text_system()
                .add_fonts(imago_gpui::assets::font_bytes())
                .expect("load Imago fonts");
            imago_gpui::theme::apply(cx);
            let bounds = window_bounds(cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some(SharedString::from("Kosmos Dictation")),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(px(12.), px(14.))),
                    }),
                    ..Default::default()
                },
                |window, cx| {
                    let app = cx.new(|cx| DictationApp::new(window, cx));
                    cx.new(|cx| gpui_component::Root::new(app, window, cx))
                },
            )
            .unwrap();
            if std::env::var("DICTATION_GPUI_OFFSCREEN").is_err() {
                cx.activate(true);
            }
        });
}

#[allow(dead_code)]
fn _sig(_: &mut Window, _: &mut Context<DictationApp>) {}
