use gpui::{Context, Pixels, Window, div, prelude::*, px, rgb, rgba, svg, transparent_black};
use jiff::Zoned;

use super::drag::DragRow;
use super::tooltip::Tooltip;
use super::{Elsewhere, View};
use crate::clock::{self, Format};
use crate::theme;

struct Toggle {
    id: &'static str,
    icon: &'static str,
    /// Glyphs fill their box unevenly: a circle runs edge to edge where the
    /// signpost leaves a margin, so drawn equal it looks bigger.
    size: Pixels,
    /// Off, then on.
    tips: [&'static str; 2],
    /// A plain fn, so the element can outlive the borrow that built it.
    click: fn(&mut Elsewhere, &mut Window, &mut Context<Elsewhere>),
}

const FORMAT: Toggle = Toggle {
    id: "format",
    icon: "icons/clock-12.svg",
    size: px(14.),
    tips: ["show 12-hour time", "showing 12-hour time"],
    click: Elsewhere::toggle_format,
};

struct Segment {
    view: View,
    id: &'static str,
    icon: &'static str,
    /// Another view shown, then this one.
    tips: [&'static str; 2],
}

const VIEWS: [Segment; 3] = [
    Segment {
        view: View::List,
        id: "show-list",
        icon: "icons/rows.svg",
        tips: ["show as a list", "showing as a list"],
    },
    Segment {
        view: View::Map,
        id: "show-map",
        icon: "icons/map.svg",
        tips: ["show on a map", "showing on a map"],
    },
    Segment {
        view: View::Grid,
        id: "show-grid",
        icon: "icons/columns.svg",
        tips: ["show as a day grid", "showing as a day grid"],
    },
];

const WESTWARD: Toggle = Toggle {
    id: "westward",
    icon: "icons/milestone.svg",
    size: px(16.),
    tips: ["order west to east", "ordered west to east"],
    click: Elsewhere::toggle_westward,
};

impl Elsewhere {
    pub(super) fn unpin(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pinned = None;
        self.close_editor(window, cx);
    }

    fn switch(&mut self, to: View, window: &mut Window, cx: &mut Context<Self>) {
        if self.view == to {
            return;
        }
        self.view = to;
        // The grid opens on now each time, wherever it was left.
        self.grid_at_now.set(false);
        // Not every view has a box for an open editor, and its focus would be
        // stranded where there is none.
        self.close_editor(window, cx);
    }

    fn toggle_westward(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.westward = !self.westward;
        cx.notify();
    }

    fn toggle_format(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.format = match self.format {
            Format::TwentyFour => Format::Twelve,
            Format::Twelve => Format::TwentyFour,
        };
        cx.notify();
    }

    fn render_toggle(
        &self,
        toggle: &Toggle,
        on: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Toggle {
            id,
            icon,
            size,
            tips,
            click,
        } = *toggle;
        div()
            .id(id)
            .w_7()
            .h_5()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            // Invisible, so the icon sits in the same box as the bin's inside
            // its dashed one.
            .border_1()
            .border_color(transparent_black())
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme::SURFACE0)))
            .tooltip(Tooltip::text(tips[on as usize]))
            .on_click(cx.listener(move |this, _event, window, cx| click(this, window, cx)))
            .child(
                svg()
                    .path(icon)
                    .size(size)
                    // An svg does not inherit its parent's color.
                    .text_color(if on {
                        rgb(theme::BLUE)
                    } else {
                        rgb(theme::OVERLAY0)
                    }),
            )
    }

    /// Each segment keeps the toggles' box, so the icons line up with the bin
    /// beside them.
    fn render_switch(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .rounded_md()
            .bg(rgb(theme::MANTLE))
            .children(VIEWS.iter().map(|segment| {
                let Segment {
                    view,
                    id,
                    icon,
                    tips,
                } = *segment;
                let on = self.view == view;
                div()
                    .id(id)
                    .w_7()
                    .h_5()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_md()
                    .border_1()
                    .border_color(transparent_black())
                    .when(on, |segment| segment.bg(rgb(theme::SURFACE0)))
                    .when(!on, |segment| {
                        segment
                            .cursor_pointer()
                            .hover(|style| style.bg(rgba(theme::WASH)))
                    })
                    .tooltip(Tooltip::text(tips[on as usize]))
                    .on_click(
                        cx.listener(move |this, _event, window, cx| this.switch(view, window, cx)),
                    )
                    .child(
                        svg()
                            .path(icon)
                            .size_4()
                            // An svg does not inherit its parent's color.
                            .text_color(if on {
                                rgb(theme::BLUE)
                            } else {
                                rgb(theme::OVERLAY0)
                            }),
                    )
            }))
    }

    /// A pin with no row reads in this machine's zone. Shown only while pinned:
    /// entering a mode on purpose earns the layout shift.
    pub(super) fn render_banner(
        &self,
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let pin = self.pinned.as_ref()?;
        let label = match pin.anchor {
            Some(anchor) => {
                let city = self.index.city(anchor)?;
                let time = self
                    .zones
                    .get(&city.timezone)
                    .map(|zone| clock::reading(now, zone, self.format).time)
                    .unwrap_or_else(|| clock::UNKNOWN.to_string());
                format!("viewing {time} in {}", city.name)
            }
            None => {
                let time = clock::reading(now, now.time_zone(), self.format).time;
                format!("viewing {time} here")
            }
        };
        let accent = pin.accent();
        Some(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .items_center()
                .h_6()
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(rgba(accent.edge))
                .bg(rgba(accent.tint))
                .text_xs()
                .text_color(rgb(accent.text))
                .child(label)
                .child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .id("unpin")
                                .px_1()
                                .rounded_md()
                                .cursor_pointer()
                                .hover(|style| style.bg(rgb(theme::SURFACE0)))
                                .on_click(
                                    cx.listener(|this, _event, window, cx| this.unpin(window, cx)),
                                )
                                .child("back to now"),
                        )
                        .child(div().text_color(rgb(theme::OVERLAY0)).child("esc")),
                ),
        )
    }

    /// Dashed so it reads as a place to drop rather than a button.
    fn render_bin(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("bin")
            .w_7()
            .h_5()
            .flex()
            .items_center()
            .justify_center()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(rgb(theme::OVERLAY0))
            .drag_over::<DragRow>(|style, _, _, _| {
                style
                    .border_color(rgb(theme::BLUE))
                    .bg(rgb(theme::SURFACE0))
            })
            .on_drop(
                cx.listener(|this, row: &DragRow, window, cx| {
                    this.delete(row.geonameid, window, cx)
                }),
            )
            .child(
                svg()
                    .path("icons/trash.svg")
                    .size_3p5()
                    // An svg does not inherit its parent's color.
                    .text_color(rgb(theme::SUBTEXT0))
                    .drag_over::<DragRow>(|style, _, _, _| style.text_color(rgb(theme::BLUE))),
            )
    }

    /// Always there at a fixed height, so the list above never shifts as
    /// controls come and go.
    pub(super) fn render_footer(&self, dragging: bool, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_row()
            .justify_between()
            .items_center()
            .h_5()
            .text_xs()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(self.render_switch(cx))
                    .children((dragging && self.drag.is_some()).then(|| self.render_bin(cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .children(
                        (!self.saved.is_empty()).then(|| {
                            self.render_toggle(&FORMAT, self.format == Format::Twelve, cx)
                        }),
                    )
                    // A map has no order to choose.
                    .children(
                        (self.saved.len() > 1 && self.view != View::Map)
                            .then(|| self.render_toggle(&WESTWARD, self.westward, cx)),
                    ),
            )
    }
}
