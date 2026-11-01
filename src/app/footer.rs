use gpui::{Context, Pixels, Window, div, prelude::*, px, rgb, rgba};
use jiff::Zoned;
use jiff::tz::TimeZone;

use super::button::Button;
use super::drag::DragRow;
use super::tooltip::Tooltip;
use super::{Elsewhere, View};
use crate::clock::{self, Format};
use crate::theme;

/// The banner's and footer's heights, which the map reads to hold still as
/// the banner comes and goes.
pub(super) const BANNER_HEIGHT: Pixels = px(24.);
pub(super) const FOOTER_HEIGHT: Pixels = px(24.);

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
    on: fn(&Elsewhere) -> bool,
    /// Why it does nothing now, when it doesn't apply. It stays in its slot
    /// either way, so the controls never jump between views.
    unavailable: fn(&Elsewhere) -> Option<&'static str>,
}

const FORMAT: Toggle = Toggle {
    id: "format",
    icon: "icons/clock-12.svg",
    size: px(14.),
    tips: ["show 12-hour time", "showing 12-hour time"],
    click: Elsewhere::toggle_format,
    on: |this| this.format == Format::Twelve,
    // Every view shows here's time, even empty, except a map with no city to
    // mark here at.
    unavailable: |this| {
        (this.saved.is_empty() && this.view == View::Map && this.here.place().is_none())
            .then_some("add a city first")
    },
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
    on: |this| this.westward,
    unavailable: |this| {
        if this.view == View::Map {
            Some("no order on a map")
        } else if this.saved.len() < 2 {
            Some("add a city first")
        } else {
            None
        }
    },
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
        // The map opens framed on the cities, wherever it was left.
        if to == View::Map {
            self.map_frame.set(None);
        }
        // The grid opens on now each time, wherever it was left.
        self.grid_at_now.set(false);
        // Not every view has a box for an open editor, and its focus would be
        // stranded where there is none.
        self.close_editor(window, cx);
        self.persist_settings(window, cx);
    }

    fn toggle_westward(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.westward = !self.westward;
        self.persist_settings(window, cx);
        cx.notify();
    }

    fn toggle_format(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.format = match self.format {
            Format::TwentyFour => Format::Twelve,
            Format::Twelve => Format::TwentyFour,
        };
        self.persist_settings(window, cx);
        cx.notify();
    }

    fn render_toggle(
        &self,
        toggle: &'static Toggle,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let Toggle {
            id,
            icon,
            size,
            tips,
            click,
            on,
            unavailable,
        } = *toggle;
        let reason = unavailable(self);
        Button::new(id, icon, size)
            .on(on(self))
            .disabled(reason.is_some())
            .tooltip(Tooltip::live(cx.entity(), move |this| {
                unavailable(this).unwrap_or(tips[on(this) as usize]).into()
            }))
            .on_click(cx.listener(move |this, _event, window, cx| click(this, window, cx)))
            .build()
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
                Button::new(id, icon, px(16.))
                    .on(self.view == view)
                    .current(self.view == view)
                    .wash(rgba(theme::WASH))
                    .tooltip(Tooltip::live(cx.entity(), move |this| {
                        tips[(this.view == view) as usize].into()
                    }))
                    .on_click(
                        cx.listener(move |this, _event, window, cx| this.switch(view, window, cx)),
                    )
                    .build()
            }))
    }

    /// In every view, so the controls never jump, but live only on the map.
    fn render_fit(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        Button::new("fit", "icons/fit.svg", px(14.))
            .disabled(self.view != View::Map)
            .tooltip(Tooltip::live(cx.entity(), |this| {
                if this.view == View::Map {
                    "fit the map to your cities"
                } else {
                    "only on a map"
                }
                .into()
            }))
            .on_click(cx.listener(|this, _event, _window, cx| {
                this.map_frame.set(None);
                cx.notify();
            }))
            .build()
    }

    /// A pin with no row reads here. Shown only while pinned:
    /// entering a mode on purpose earns the layout shift. Names the pinned
    /// day once it leaves today, since the rows' day tags are relative to it,
    /// and keeps the real time in view.
    pub(super) fn render_banner(
        &self,
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement + use<>> {
        let pin = self.pinned.as_ref()?;
        let real = Zoned::now();
        let read = |zone: &TimeZone| {
            let time = clock::reading(now, zone, self.format).time;
            match clock::pinned_day(now, zone, &real) {
                Some(day) => format!("{time} {day}"),
                None => time,
            }
        };
        let label = match pin.anchor {
            Some(anchor) => {
                let city = self.index.city(anchor)?;
                let reading = self
                    .zones
                    .get(&city.timezone)
                    .map(read)
                    .unwrap_or_else(|| clock::UNKNOWN.to_string());
                format!("viewing {reading} in {}", city.name)
            }
            None => format!("viewing {} here", read(&self.here.zone)),
        };
        let live = clock::reading(&real, &self.here.zone, self.format).time;
        let accent = pin.accent();
        Some(
            div()
                .flex()
                .flex_row()
                .justify_between()
                .gap_2()
                .items_center()
                .h(BANNER_HEIGHT)
                .px_2()
                .rounded_md()
                .border_1()
                .border_color(rgba(accent.edge))
                .bg(rgba(accent.tint))
                .text_xs()
                .text_color(rgb(accent.text))
                // In a narrow window the real time gives way before the
                // what-if, and both before the way back.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_row()
                        .gap_1()
                        .child(div().min_w_0().truncate().child(label))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .map(|mut live| {
                                    // Shrink is weighted by size, so a heavy
                                    // weight is what makes it go first.
                                    live.style().flex_shrink = Some(1000.);
                                    live
                                })
                                .text_color(rgb(theme::SUBTEXT0))
                                .child(format!("· now {live}")),
                        ),
                )
                .child(
                    div()
                        .flex_none()
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
                        .child(div().text_color(rgb(theme::OVERLAY2)).child("esc")),
                ),
        )
    }

    fn render_bin(&self, cx: &mut Context<Self>) -> impl IntoElement {
        Button::new("bin", "icons/trash.svg", px(14.))
            .rest(theme::SUBTEXT0)
            .dashed(theme::OVERLAY2)
            .drop_target::<DragRow>()
            .build()
            .on_drop(
                cx.listener(|this, row: &DragRow, window, cx| {
                    this.delete(row.geonameid, window, cx)
                }),
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
            .h(FOOTER_HEIGHT)
            .text_xs()
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(self.render_switch(cx))
                    .child(self.render_fit(cx))
                    .children((dragging && self.drag.is_some()).then(|| self.render_bin(cx))),
            )
            .child(
                div()
                    .flex()
                    .flex_row()
                    .gap_1()
                    .child(self.render_toggle(&FORMAT, cx))
                    .child(self.render_toggle(&WESTWARD, cx)),
            )
    }
}
