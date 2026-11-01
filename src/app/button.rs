//! The one icon button the footer, the view switch, the bin and the title bar
//! share, so they keep a box and a state vocabulary.

use gpui::{
    AnyElement, AnyView, App, ClickEvent, Div, ElementId, FocusHandle, Pixels, Rgba, SharedString,
    Stateful, Svg, Window, WindowControlArea, div, prelude::*, px, rgb, svg, transparent_black,
};

use super::focus::keyboard_only;
use crate::theme;

/// At least WCAG 2.5.8's 24 x 24; wider so the icons keep their rhythm.
pub(super) const WIDTH: Pixels = px(28.);
const HEIGHT: Pixels = px(24.);

/// A disabled button stays visible to say what it would do, and why it can't.
const DISABLED_OPACITY: f32 = 0.4;

type Tooltip = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;
type Click = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// What a drag over the button restyles, for the type being dragged.
#[derive(Clone, Copy)]
struct Drop {
    frame: fn(Stateful<Div>) -> Stateful<Div>,
    icon: fn(Svg, &'static str) -> Svg,
}

/// What the button shows.
enum Face {
    Icon {
        path: &'static str,
        /// Glyphs fill their box unevenly: a circle runs edge to edge where
        /// the signpost leaves a margin, so drawn equal it looks bigger.
        size: Pixels,
    },
    /// A word, where an icon would need a tooltip to say what it does. Text
    /// takes its color at layout, so the hover color does not reach it.
    Label(SharedString),
}

pub(super) struct Button {
    id: ElementId,
    /// The id's name, which the icon's hover keys on. Siblings may share it,
    /// as a group resolves to the nearest one up the tree.
    group: &'static str,
    face: Face,
    rest: u32,
    hover: Option<u32>,
    wash: Rgba,
    border: Option<u32>,
    on: bool,
    current: bool,
    disabled: bool,
    drop: Option<Drop>,
    area: Option<WindowControlArea>,
    tooltip: Option<Tooltip>,
    click: Option<Click>,
    focus: Option<FocusHandle>,
}

impl Button {
    pub(super) fn new(id: &'static str, icon: &'static str, size: Pixels) -> Self {
        Self::with(id, Face::Icon { path: icon, size })
    }

    /// As wide as the word, and never narrower than an icon button.
    pub(super) fn label(id: &'static str, label: impl Into<SharedString>) -> Self {
        Self::with(id, Face::Label(label.into()))
    }

    fn with(id: &'static str, face: Face) -> Self {
        Self {
            id: id.into(),
            group: id,
            face,
            rest: theme::OVERLAY2,
            hover: None,
            wash: rgb(theme::SURFACE0),
            border: None,
            on: false,
            current: false,
            disabled: false,
            drop: None,
            area: None,
            tooltip: None,
            click: None,
            focus: None,
        }
    }

    /// One of many, as a button per row is.
    pub(super) fn key(mut self, key: usize) -> Self {
        self.id = (self.group, key).into();
        self
    }

    /// The icon's color at rest.
    pub(super) fn rest(mut self, color: u32) -> Self {
        self.rest = color;
        self
    }

    /// The icon's color on hover.
    pub(super) fn hover(mut self, color: u32) -> Self {
        self.hover = Some(color);
        self
    }

    /// The background on hover, where the surface is not the page's.
    pub(super) fn wash(mut self, wash: Rgba) -> Self {
        self.wash = wash;
        self
    }

    /// A dashed frame, so it reads as a place to drop rather than a button.
    pub(super) fn dashed(mut self, color: u32) -> Self {
        self.border = Some(color);
        self
    }

    /// A switched-on toggle.
    pub(super) fn on(mut self, on: bool) -> Self {
        self.on = on;
        self
    }

    /// The segment already showing: lit, and nothing left to click.
    pub(super) fn current(mut self, current: bool) -> Self {
        self.current = current;
        self
    }

    pub(super) fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    /// Lights up while a `T` is dragged over it.
    pub(super) fn drop_target<T: 'static>(mut self) -> Self {
        self.drop = Some(Drop {
            frame: |frame| {
                frame.drag_over::<T>(|style, _, _, _| {
                    style
                        .border_color(rgb(theme::BLUE))
                        .bg(rgb(theme::SURFACE0))
                })
            },
            icon: |icon, group| {
                icon.group_drag_over::<T>(group, |style| style.text_color(rgb(theme::BLUE)))
            },
        });
        self
    }

    /// For Windows, which works the button itself.
    pub(super) fn area(mut self, area: WindowControlArea) -> Self {
        self.area = Some(area);
        self
    }

    pub(super) fn tooltip(
        mut self,
        tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static,
    ) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    pub(super) fn on_click(
        mut self,
        click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.click = Some(Box::new(click));
        self
    }

    /// A stop in the window's tab order; Enter and space click it.
    pub(super) fn focus(mut self, handle: FocusHandle) -> Self {
        self.focus = Some(handle);
        self
    }

    /// A div, for the caller to hang what is its own (a drop handler) on.
    pub(super) fn build(self) -> Stateful<Div> {
        let Self {
            id,
            group,
            face,
            rest,
            hover,
            wash,
            border,
            on,
            current,
            disabled,
            drop,
            area,
            tooltip,
            click,
            focus,
        } = self;
        let live = !disabled && !current;
        let color = if on { theme::BLUE } else { rest };
        let labelled = matches!(face, Face::Label(_));
        let face: AnyElement = match face {
            Face::Icon { path, size } => {
                let mut icon = svg()
                    .path(path)
                    .size(size)
                    // An svg does not inherit its parent's color.
                    .text_color(rgb(color));
                if let (Some(hover), true) = (hover, live) {
                    icon = icon.group_hover(group, move |style| style.text_color(rgb(hover)));
                }
                if let Some(drop) = drop {
                    icon = (drop.icon)(icon, group);
                }
                icon.into_any_element()
            }
            Face::Label(label) => div().text_color(rgb(color)).child(label).into_any_element(),
        };
        let mut button = div()
            .id(id)
            .group(group)
            .map(|button| {
                if labelled {
                    button.min_w(WIDTH).px_1p5()
                } else {
                    button.w(WIDTH)
                }
            })
            .h(HEIGHT)
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            // Always drawn, so the icon sits in the same box with or without
            // a frame, and a focus ring has room.
            .border_1()
            .border_color(border.map_or_else(transparent_black, |color| rgb(color).into()))
            .when(border.is_some(), |button| button.border_dashed())
            .when(current, |button| button.bg(rgb(theme::SURFACE0)))
            .when(live && click.is_some(), |button| button.cursor_pointer())
            .when(live && (click.is_some() || area.is_some()), |button| {
                button.hover(|style| style.bg(wash))
            })
            .when(disabled, |button| button.opacity(DISABLED_OPACITY));
        if let Some(handle) = focus {
            button = keyboard_only(
                button
                    .track_focus(&handle)
                    .focus(|style| style.border_color(rgb(theme::BLUE))),
            );
        }
        if let Some(area) = area.filter(|_| live) {
            button = button.window_control_area(area);
        }
        if let Some(tooltip) = tooltip {
            button = button.tooltip(tooltip);
        }
        if let (Some(click), true) = (click, live) {
            button = button.on_click(click);
        }
        if let Some(drop) = drop {
            button = (drop.frame)(button);
        }
        button.child(face)
    }
}
