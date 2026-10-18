use gpui::{
    Context, MouseButton, MouseDownEvent, Pixels, Point, Window, WindowControlArea, div, point,
    prelude::*, px, rgb, svg, transparent_black,
};

use super::Elsewhere;
use crate::theme;

/// Who works the window's frame: the title bar is drawn the same everywhere,
/// but what makes it move the window and its controls act differs by platform.
#[derive(Clone, Copy)]
enum Chrome {
    /// Windows works the controls and the caption itself, from the areas the
    /// title bar marks out as them.
    Windows,
    /// macOS keeps its traffic lights, which gpui cannot hide, so the title
    /// bar makes room for them and draws no controls. Dragging is AppKit's.
    /// TODO: untested, no Mac to run it on.
    Mac,
    /// The app works the controls and asks the compositor to move the window.
    /// The other Unixes go by the same backends.
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

/// Where macOS puts its traffic lights: in line with the padding, centered on
/// the title bar.
pub(crate) const MAC_TRAFFIC_LIGHTS: Point<Pixels> = point(px(16.), px(11.));
/// The lights' width and a gap, kept clear of the name.
const MAC_TRAFFIC_LIGHTS_ROOM: Pixels = px(62.);

/// A window control, as a footer-sized icon button.
struct Control {
    id: &'static str,
    icon: &'static str,
    size: Pixels,
    /// The icon's color under the pointer.
    hover: u32,
    /// What the button is to Windows, which works it itself.
    area: WindowControlArea,
    /// What a click does where the app works it. A plain fn, so the element can outlive
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
    // The one window is the whole app. On Windows the app quits on its own
    // once its last window closes.
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
        div()
            .id(id)
            .group(id)
            .w_7()
            .h_5()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            // The same invisible border as the footer's buttons, so the icons
            // sit in the same box.
            .border_1()
            .border_color(transparent_black())
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme::SURFACE0)))
            .map(|button| match Chrome::CURRENT {
                // Windows sees the press as on its own caption button and acts
                // on it, so a click handler too would act twice. This way the
                // maximize button also brings up snap layouts.
                Chrome::Windows => button.window_control_area(area),
                Chrome::Mac | Chrome::Linux => {
                    button.on_click(cx.listener(move |_, _event, window, cx| click(window, cx)))
                }
            })
            .child(
                svg()
                    .path(icon)
                    .size(size)
                    // An svg paints only in a color set on itself; the
                    // parent's does not reach it.
                    .text_color(rgb(theme::OVERLAY0))
                    .group_hover(id, |style| style.text_color(rgb(hover))),
            )
    }

    /// The strip along the top that stands in for the title bar the window
    /// goes without: the app's name and empty space that move the window, a
    /// double click on them maximizing, and the window's controls on the right.
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
            .h_5()
            // The name rides inside the empty space, so it moves the window too.
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_xs()
                    .text_color(rgb(theme::OVERLAY0))
                    .child("elsewhere")
                    .map(|strip| match Chrome::CURRENT {
                        // Windows has no moving on request: the strip is its
                        // caption instead, which drags, snaps, and maximizes
                        // on a double click by itself.
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
