//! Set like a calendar's time gutter: a column of hour labels per city, the
//! home day's hours lined up across.

mod header;

use gpui::{Axis, Context, ElementId, Pixels, canvas, div, point, prelude::*, px, rgb, rgba};
use jiff::Zoned;
use jiff::tz::TimeZone;

use super::scrollbar::scrollbar;
use super::tooltip::Tooltip;
use super::{Elsewhere, Pin};
use crate::clock;
use crate::hours;
use crate::theme;

/// Also stops a long city name holding its column open, as a flex item's
/// automatic minimum is its content. Fits "10:30 PM" at the 11px face plus the
/// inset; narrower, the grid scrolls sideways.
const COLUMN: Pixels = px(68.);

/// Spaced as a calendar's, so the day outgrows the region and scrolls.
const ROW: Pixels = px(40.);

/// A step under `text_xs`.
const LABEL: Pixels = px(11.);

const INSET: Pixels = px(8.);

const BAR: Pixels = px(22.);

struct Column {
    /// `None` for home's column.
    geonameid: Option<u32>,
    /// `None` when the zone is unknown: the column keeps its head and rule but
    /// no labels.
    zone: Option<TimeZone>,
}

impl Elsewhere {
    /// The heads stay put over the hours, which scroll down the day. Both
    /// scroll together sideways.
    pub(super) fn render_grid(
        &self,
        order: &[u32],
        now: &Zoned,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let day = hours::Day::of(now);
        let home = Column {
            geonameid: None,
            zone: Some(now.time_zone().clone()),
        };
        // The list's order, so the westward toggle reorders the columns too.
        let columns: Vec<Column> = std::iter::once(home)
            .chain(order.iter().filter_map(|geonameid| {
                let city = self.index.city(*geonameid)?;
                Some(Column {
                    geonameid: Some(*geonameid),
                    zone: self.zones.get(&city.timezone).cloned(),
                })
            }))
            .collect();

        div()
            .relative()
            .size_full()
            .child(self.render_scroll_to_now(&day, now))
            .child(
                div()
                    .id("grid")
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.grid_scroll)
                    // A plain wheel is the hours'; only shift+wheel moves
                    // the columns.
                    .map(|mut scroller| {
                        scroller.style().restrict_scroll_to_axis = Some(true);
                        scroller
                    })
                    .child(
                        // A scroller's content needs a width of its own to
                        // overflow sideways.
                        div()
                            .h_full()
                            .w(COLUMN * columns.len() as f32)
                            .min_w_full()
                            .flex()
                            .flex_col()
                            .child(self.render_heads(&columns, now, dragging, cx))
                            .child(
                                div()
                                    .id("grid-hours")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_y_scroll()
                                    .track_scroll(&self.grid_hours)
                                    // Otherwise gpui also turns a sideways
                                    // swipe into a scroll down here.
                                    .map(|mut scroller| {
                                        scroller.style().restrict_scroll_to_axis = Some(true);
                                        scroller
                                    })
                                    .child(self.render_body(&day, &columns, now, cx)),
                            ),
                    ),
            )
            // Hung in the window padding, as the list's is.
            .child(
                div()
                    .absolute()
                    .top(header::HEAD)
                    .bottom_0()
                    .left_0()
                    .right_0()
                    .child(scrollbar(&self.grid_hours, Axis::Vertical, px(-6.))),
            )
            .child(scrollbar(&self.grid_scroll, Axis::Horizontal, px(-6.)))
    }

    fn render_body(
        &self,
        day: &hours::Day,
        columns: &[Column],
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let shown = day.index(now);
        div()
            .relative()
            .flex()
            .flex_row()
            // First, so the columns' labels and rules paint over it.
            .children(shown.map(|i| self.render_highlight(i)))
            .children(
                columns
                    .iter()
                    .map(|column| self.render_column(day, column, shown, now, cx)),
            )
    }

    /// Each hour is a `ROW` slot with its label in the middle, so the hour
    /// lines fall half a row in from the top.
    fn render_column(
        &self,
        day: &hours::Day,
        column: &Column,
        shown: Option<usize>,
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Keyed by city so its hour state follows it when the order changes.
        let id = match column.geonameid {
            Some(geonameid) => ElementId::from(("column", geonameid as usize)),
            None => ElementId::from("column-here"),
        };
        div()
            .id(id)
            .flex_1()
            .min_w(COLUMN)
            .pb(ROW / 2.)
            .border_r_1()
            .border_color(rgb(theme::SURFACE0))
            .flex()
            .flex_col()
            .children((0..day.rows()).map(|i| {
                let shown = (shown == Some(i)).then_some(now);
                self.render_hour(&day.row(i), column, i, shown, cx)
            }))
    }

    /// A city with no zone keeps the slot, so its rule runs the whole day.
    /// `shown` is set only on the hour holding the shown instant.
    fn render_hour(
        &self,
        at: &Zoned,
        column: &Column,
        i: usize,
        shown: Option<&Zoned>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let slot = div()
            .id(("hour", i))
            .h(ROW)
            .flex()
            .items_center()
            .justify_end()
            .pr(INSET)
            .text_size(LABEL)
            .text_color(rgb(theme::OVERLAY0));
        let Some(zone) = &column.zone else {
            return slot;
        };
        // The highlighted hour shows and pins the actual minute, not the round
        // hour.
        let instant = shown.unwrap_or(at).clone();
        let local = instant.with_time_zone(zone.clone());
        let label = clock::hour_label(local.hour(), local.minute(), self.format);
        let bright = self
            .pinned
            .as_ref()
            .map_or(theme::TEXT, |pin| pin.accent().text);
        let slot = slot
            .when(shown.is_some(), |slot| slot.text_color(rgb(bright)))
            .child(label.clone());
        // Home's column pins with no row behind it, as a scrub does.
        let (anchor, tip) = match column.geonameid {
            None => (None, format!("{label} here")),
            Some(id) => {
                let Some(city) = self.index.city(id) else {
                    return slot;
                };
                // With the day tag the list would give it.
                let tip = match clock::reading(&instant, zone, self.format).day {
                    Some(day) => format!("{label} in {} · {day}", city.name),
                    None => format!("{label} in {}", city.name),
                };
                (Some(id), tip)
            }
        };
        slot.cursor_pointer()
            .when(shown.is_none(), |slot| {
                slot.hover(|style| style.text_color(rgb(theme::SUBTEXT0)))
            })
            .tooltip(Tooltip::text(tip))
            // The grid's only what-if: no editor, just the label's time.
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.pinned = Some(Pin {
                    anchor,
                    at: instant.clone(),
                });
                cx.notify();
            }))
    }

    /// One bar across every column behind the labels, rather than a line
    /// struck through them.
    fn render_highlight(&self, i: usize) -> impl IntoElement + use<> {
        div()
            .absolute()
            .left_0()
            .right_0()
            .top(ROW * i as f32 + (ROW - BAR) / 2.)
            .h(BAR)
            .rounded_md()
            .map(|bar| match &self.pinned {
                Some(pin) => bar.bg(rgba(pin.accent().tint)),
                None => bar.bg(rgb(theme::SURFACE0)),
            })
    }

    /// Paints nothing. Prepainted before the hours' scroller, so scrolling now
    /// a third of the way down lands in the same frame: the scroller clamps and
    /// applies the offset after.
    fn render_scroll_to_now(&self, day: &hours::Day, now: &Zoned) -> impl IntoElement + use<> {
        let shown = day.index(now);
        let hours = self.grid_hours.clone();
        let at_now = self.grid_at_now.clone();
        canvas(
            move |bounds, _window, _cx| {
                if at_now.get() {
                    return;
                }
                at_now.set(true);
                if let Some(i) = shown {
                    let line = ROW * i as f32 + ROW / 2.;
                    let top = (line - bounds.size.height / 3.).max(px(0.));
                    hours.set_offset(point(px(0.), -top));
                }
            },
            |_bounds, (), _window, _cx| {},
        )
        .absolute()
        .top(header::HEAD)
        .bottom_0()
        .left_0()
        .right_0()
    }
}
