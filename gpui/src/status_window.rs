//! Status-window helpers: opening/reopening the disposable window and
//! surfacing a `show: false` or minimized one (native titlebar is in
//! view.rs). Split out of app.rs.
use ::gpui::{prelude::*, *};

use crate::app::DictationApp;

/// Unhide a `show: false` / minimized window, then activate it. GPUI's
/// `activate_window` only handles IsIconic→SW_RESTORE; a window that was
/// never shown needs an explicit SW_SHOW first.
pub(crate) fn show_and_activate(window: &mut Window) {
    #[cfg(windows)]
    if let Ok(wh) = raw_window_handle::HasWindowHandle::window_handle(window) {
        use raw_window_handle::RawWindowHandle;
        if let RawWindowHandle::Win32(w32) = wh.as_raw() {
            unsafe {
                use windows_sys::Win32::UI::WindowsAndMessaging::*;
                ShowWindow(w32.hwnd.get() as _, SW_SHOW);
            }
        }
    }
    window.activate_window();
}

/// Open (or reopen) the status window on the existing entity. The window is
/// disposable — the entity is held by the `StatusApp` global, so the close
/// button really closes the HWND while the worker, Engine subscription and
/// pill flow keep running. A second launch (`SHOW_REQUESTED`) calls this
/// again for a fresh window.
pub(crate) fn open_status_window(app: &Entity<DictationApp>, cx: &mut App) {
    let app = app.clone();
    let weak = app.downgrade();
    let root_app = app.clone();
    let bounds = crate::window_bounds(cx);
    let result = cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some(SharedString::from("Mundus Dictation")),
                appears_transparent: true,
                traffic_light_position: Some(gpui::point(px(12.), px(14.))),
            }),
            ..Default::default()
        },
        move |window, cx| {
            // Mark the handle dead the moment close is requested — the drain
            // loop then treats `SHOW_REQUESTED` as "reopen", not "resurface".
            window.on_window_should_close(cx, move |_, cx| {
                let _ = weak.update(cx, |this, cx| {
                    this.status = None;
                    // A capture armed behind a closed window would keep
                    // eating keystrokes with no UI left to cancel it.
                    this.hotkey_capture_cancel(cx);
                });
                true
            });
            cx.new(|cx| gpui_component::Root::new(root_app, window, cx))
        },
    );
    match result {
        Ok(handle) => {
            app.update(cx, |this, _| this.status = Some(handle.into()));
        }
        Err(error) => eprintln!("dictation-gpui: status window open failed: {error}"),
    }
}
