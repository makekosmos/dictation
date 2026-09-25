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

fn main() {
    gpui::application()
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
