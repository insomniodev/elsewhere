use gpui::{
    Context, MouseButton, MouseDownEvent, Pixels, Point, Window, WindowControlArea, div, point,
    prelude::*, px, rgb,
};

use super::Elsewhere;
use super::button::Button;
use crate::theme;

/// The title bar looks the same everywhere, but what moves the window and
/// works its controls differs by platform.
#[derive(Clone, Copy)]
enum Chrome {
    /// Windows works the controls and caption itself, from the areas the title
    /// bar marks out.
    Windows,
    /// gpui cannot hide the traffic lights, so the title bar makes room for
    /// them and draws no controls. AppKit does the dragging.
    /// TODO: untested, no Mac to run it on.
    Mac,
    /// The app works the controls and asks the compositor to move the window;
    /// the other Unixes share the backends.
    Linux,
}

impl Chrome {
    const CURRENT: Chrome = if cfg!(target_os = "windows") {
        Chrome::Windows
    } else if cfg!(target_os = "macos") {
        Chrome::Mac
    } else {
        Chrome::Linux
    };
}

/// In line with the padding, centered on the title bar.
pub(crate) const MAC_TRAFFIC_LIGHTS: Point<Pixels> = point(px(16.), px(9.));
/// The lights' width plus a gap before the name.
const MAC_TRAFFIC_LIGHTS_ROOM: Pixels = px(62.);

struct Control {
    id: &'static str,
    icon: &'static str,
    size: Pixels,
    /// The icon's color on hover.
    hover: u32,
    /// For Windows, which works the button itself.
    area: WindowControlArea,
    /// Where the app works the button. A plain fn, so the element can outlive
    /// the borrow that built it.
    click: fn(&mut Window, &mut Context<Elsewhere>),
}

const MINIMIZE: Control = Control {
    id: "minimize",
    icon: "icons/minus.svg",
    size: px(14.),
    hover: theme::TEXT,
    area: WindowControlArea::Min,
    click: |window, _| window.minimize_window(),
};

const MAXIMIZE: Control = Control {
    id: "maximize",
    icon: "icons/maximize-2.svg",
    size: px(12.),
    hover: theme::TEXT,
    area: WindowControlArea::Max,
    click: |window, _| window.zoom_window(),
};

const RESTORE: Control = Control {
    icon: "icons/minimize-2.svg",
    ..MAXIMIZE
};

const CLOSE: Control = Control {
    id: "close",
    icon: "icons/x.svg",
    size: px(14.),
    hover: theme::RED,
    area: WindowControlArea::Close,
    // The one window is the whole app. Windows quits by itself once the last
    // window closes.
    click: |_, cx| cx.quit(),
};

impl Elsewhere {
    fn render_control(&self, control: &Control, cx: &mut Context<Self>) -> impl IntoElement {
        let Control {
            id,
            icon,
            size,
            hover,
            area,
            click,
        } = *control;
        let button = Button::new(id, icon, size).hover(hover);
        match Chrome::CURRENT {
            // Windows acts on the press as its own caption button, so a click
            // handler would act twice. This also gives maximize its snap
            // layouts.
            Chrome::Windows => button.area(area),
            Chrome::Mac | Chrome::Linux => {
                button.on_click(cx.listener(move |_, _event, window, cx| click(window, cx)))
            }
        }
        .build()
    }

    /// Stands in for the title bar the window goes without.
    pub(super) fn render_titlebar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let maximize = if window.is_maximized() {
            &RESTORE
        } else {
            &MAXIMIZE
        };
        // Full screen hides the lights.
        let lights = !window.is_fullscreen();
        div()
            .flex()
            .flex_row()
            .items_center()
            .h_6()
            // The name sits in the drag strip, so it moves the window too.
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(rgb(theme::OVERLAY2))
                    .child("elsewhere")
                    .map(|strip| match Chrome::CURRENT {
                        // Windows has no move on request; the strip is its
                        // caption, which drags, snaps and maximizes by itself.
                        Chrome::Windows => strip.window_control_area(WindowControlArea::Drag),
                        // A double click does what the system settings say.
                        Chrome::Mac => strip
                            .when(lights, |strip| strip.pl(MAC_TRAFFIC_LIGHTS_ROOM))
                            .on_mouse_down(
                                MouseButton::Left,
                                |event: &MouseDownEvent, window, _| {
                                    if event.click_count == 2 {
                                        window.titlebar_double_click();
                                    }
                                },
                            ),
                        Chrome::Linux => strip.on_mouse_down(
                            MouseButton::Left,
                            |event: &MouseDownEvent, window, _| {
                                if event.click_count == 2 {
                                    window.zoom_window();
                                } else {
                                    window.start_window_move();
                                }
                            },
                        ),
                    }),
            )
            .when(!matches!(Chrome::CURRENT, Chrome::Mac), |bar| {
                bar.child(
                    div()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(self.render_control(&MINIMIZE, cx))
                        .child(self.render_control(maximize, cx))
                        .child(self.render_control(&CLOSE, cx)),
                )
            })
    }
}
