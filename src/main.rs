// No console window on Windows; debug builds keep it for panics and logs.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use gpui::{
    App, Bounds, KeyBinding, Pixels, Size, TitlebarOptions, WindowBounds, WindowDecorations,
    WindowOptions, prelude::*, px, size,
};

mod app;
mod assets;
mod cities;
mod clock;
mod geo;
mod here;
mod hours;
mod land;
mod saved;
mod search;
mod settings;
mod theme;
mod vendor;
mod zones;

/// Wayland app id and X11 class; `assets/app/<APP_ID>.desktop` is named after it.
const APP_ID: &str = "io.github.insomniodev.elsewhere";

/// Tall, to suit the list; the map scrolls sideways instead. Under 600 so it
/// fits a 1200-pixel display at 2x.
const SIZE: Size<Pixels> = size(px(400.), px(560.));

/// Narrower and a row cannot hold a name and a time; shorter and the search
/// has no room for results.
const MIN_SIZE: Size<Pixels> = size(px(320.), px(360.));

/// Where the last session left the window. A saved place that no display shows
/// any part of (one since unplugged) starts over centred at the default size.
fn bounds(settings: Option<&settings::Settings>, cx: &App) -> Bounds<Pixels> {
    let Some(saved) = settings.and_then(|settings| settings.size) else {
        return Bounds::centered(None, SIZE, cx);
    };
    let size = saved.max(&MIN_SIZE);
    let Some(origin) = settings.and_then(|settings| settings.origin) else {
        return Bounds::centered(None, size, cx);
    };
    let bounds = Bounds { origin, size };
    let displays = cx.displays();
    // No displays known means nothing to check against.
    if displays.is_empty()
        || displays
            .iter()
            .any(|display| display.bounds().intersects(&bounds))
    {
        bounds
    } else {
        Bounds::centered(None, SIZE, cx)
    }
}

fn main() {
    gpui_platform::application()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            vendor::text_input::register_key_bindings(cx);
            // cmd on macOS and ctrl elsewhere. gpui's own "secondary-" needs
            // the host OS, which `KeyBinding::new` no longer has.
            let secondary = |mac: &'static str, other: &'static str| {
                if cfg!(target_os = "macos") {
                    mac
                } else {
                    other
                }
            };
            cx.bind_keys([
                KeyBinding::new("enter", app::Commit, Some("PinEditor")),
                KeyBinding::new("escape", app::Cancel, Some("PinEditor")),
                KeyBinding::new("enter", app::search::Confirm, Some("SearchPicker")),
                KeyBinding::new("escape", app::search::Clear, Some("SearchPicker")),
                KeyBinding::new("up", app::search::MoveUp, Some("SearchPicker")),
                KeyBinding::new("down", app::search::MoveDown, Some("SearchPicker")),
                KeyBinding::new("escape", app::Unpin, Some("Elsewhere")),
                KeyBinding::new("alt-down", app::Later, Some("Elsewhere")),
                KeyBinding::new("alt-up", app::Earlier, Some("Elsewhere")),
                KeyBinding::new("tab", app::FocusNext, Some("Elsewhere")),
                KeyBinding::new("shift-tab", app::FocusPrev, Some("Elsewhere")),
                KeyBinding::new(
                    secondary("cmd-1", "ctrl-1"),
                    app::ShowList,
                    Some("Elsewhere"),
                ),
                KeyBinding::new(
                    secondary("cmd-2", "ctrl-2"),
                    app::ShowMap,
                    Some("Elsewhere"),
                ),
                KeyBinding::new(
                    secondary("cmd-3", "ctrl-3"),
                    app::ShowGrid,
                    Some("Elsewhere"),
                ),
                KeyBinding::new(secondary("cmd-q", "ctrl-q"), app::Close, Some("Elsewhere")),
                KeyBinding::new(secondary("cmd-w", "ctrl-w"), app::Close, Some("Elsewhere")),
                KeyBinding::new("enter", app::Edit, Some("Row")),
                KeyBinding::new("delete", app::Remove, Some("Row")),
                KeyBinding::new("backspace", app::Remove, Some("Row")),
                // Not alt, which steps the time from anywhere, a row included.
                KeyBinding::new("ctrl-up", app::MoveUp, Some("Row")),
                KeyBinding::new("ctrl-down", app::MoveDown, Some("Row")),
                KeyBinding::new("left", app::CellLeft, Some("Grid")),
                KeyBinding::new("right", app::CellRight, Some("Grid")),
                KeyBinding::new("up", app::CellUp, Some("Grid")),
                KeyBinding::new("down", app::CellDown, Some("Grid")),
                KeyBinding::new("enter", app::PinCell, Some("Grid")),
            ]);
            // Whatever closes the one window, the compositor included, ends
            // the app with it.
            cx.on_window_closed(|cx, _| {
                if cx.windows().is_empty() {
                    cx.quit();
                }
            })
            .detach();

            // Read before the window opens, so it opens where it was left.
            let settings = settings::load();
            let bounds = bounds(settings.as_ref().ok(), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(MIN_SIZE),
                    titlebar: Some(TitlebarOptions {
                        title: Some("elsewhere".into()),
                        // Drops the caption but keeps the system frame on
                        // Windows (shadow, resizing, snapping) and the traffic
                        // lights on macOS. Linux goes by the decorations below.
                        appears_transparent: true,
                        traffic_light_position: Some(app::titlebar::MAC_TRAFFIC_LIGHTS),
                    }),
                    // Linux: the app draws its own title bar and frame, if
                    // the compositor allows.
                    window_decorations: Some(WindowDecorations::Client),
                    // Matches the desktop entry, so docks show its icon.
                    app_id: Some(APP_ID.into()),
                    ..Default::default()
                },
                |window, cx| cx.new(|cx| app::Elsewhere::new(settings, window, cx)),
            )
            .expect("failed to open window");
            cx.activate(true);
        });
}
