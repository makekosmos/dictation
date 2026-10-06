//! dictation-gpui — standalone Mundus Dictation overlay app. All native work
//! (global hotkey hook, WASAPI capture, transcription, injection) is owned by
//! Mundus Engine; this process is the UI client over /v1/rpc + the Engine
//! broadcast WebSocket (dictation_toggle_trigger → pill session).
#![windows_subsystem = "windows"]

mod app;
mod assets;
mod button;
mod hotkey;
mod languages;
mod pill;
mod pill_wave;
mod queue;
mod session;
mod settings;
mod status_window;
mod view;
mod worker;

use std::sync::atomic::AtomicBool;

use gpui::{px, size, App, AppContext, Bounds};

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
/// The mutex/event names stay `Kosmos*` across the Mundus rename — renamed
/// names would let a legacy and a Mundus instance run side by side.
#[cfg(windows)]
fn claim_single_instance() -> bool {
    use std::sync::atomic::Ordering;
    use windows_sys::Win32::{
        Foundation::{GetLastError, SetLastError, ERROR_ALREADY_EXISTS},
        System::Threading::{CreateEventW, CreateMutexW, SetEvent, WaitForSingleObject},
    };
    let mutex_name: Vec<u16> = "Local\\KosmosDictationGpui\0".encode_utf16().collect();
    let event_name: Vec<u16> = "Local\\KosmosDictationGpuiShow\0".encode_utf16().collect();
    // Clear last-error first: CreateMutexW only sets it when the mutex
    // already exists, so a stale ERROR_ALREADY_EXISTS left by an earlier
    // unrelated call would wrongly exit the first instance.
    unsafe { SetLastError(0) };
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    if handle.is_null() {
        return true; // fail-open: running beats not running
    }
    // GetLastError must be read before the next Win32 call — CreateEventW
    // would overwrite it.
    let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    let event = unsafe { CreateEventW(std::ptr::null(), 0, 0, event_name.as_ptr()) };
    if already_exists {
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
    // Headless app with no console — without this a panic dies silently.
    // Writes to <data dir>\dictation-gpui-panic.log — %APPDATA%\Mundus by
    // default, the legacy %APPDATA%\Kosmos dir when discovery falls back.
    std::panic::set_hook(Box::new(|info| {
        if let Ok(dir) = mundus_gpui_kit::engine::data_dir() {
            let _ = std::fs::write(
                dir.join("dictation-gpui-panic.log"),
                format!("{info}\n\n{:?}", std::backtrace::Backtrace::capture()),
            );
        }
    }));

    #[cfg(windows)]
    if !claim_single_instance() {
        return;
    }
    // `--background` = the Engine-managed entry point: the entity runs
    // windowless (hotkey + pill keep working); a second launch resurfaces
    // the status window via the SHOW_REQUESTED event.
    let background = std::env::args().any(|arg| arg == "--background");
    // Explicit: the pill is a transient overlay and the status window closes
    // for real — dictation keeps working with no window at all.
    gpui::application()
        .with_quit_mode(gpui::QuitMode::Explicit)
        .with_assets(assets::Assets)
        .run(move |cx: &mut App| {
            gpui_component::init(cx);
            cx.text_system()
                .add_fonts(imago_gpui::assets::font_bytes())
                .expect("load Imago fonts");
            imago_gpui::theme::apply(cx);
            // The entity owns the worker + Engine subscription, so it must
            // outlive the status window: the window's root view drops its
            // ref on close, this global keeps the entity alive for the
            // process lifetime.
            let entity = cx.new(DictationApp::new);
            cx.set_global(StatusApp(entity));
            let app = cx.global::<StatusApp>().0.clone();
            if !background {
                status_window::open_status_window(&app, cx);
                if std::env::var("DICTATION_GPUI_OFFSCREEN").is_err() {
                    cx.activate(true);
                }
            }
        });
}

/// Holds the session entity for the process lifetime — see `main`.
struct StatusApp(gpui::Entity<DictationApp>);

impl gpui::Global for StatusApp {}
