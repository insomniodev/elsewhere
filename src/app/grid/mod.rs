//! The grid view, set like a calendar's time gutter: home and then each saved
//! city as a column of hour labels down its own rule, the home day's hours
//! lined up across, and the shown instant highlighted in each.

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

/// The narrowest a column goes, and — a flex item's automatic minimum being
/// its own content — what stops a long city name in a head from holding its
/// column open. Sized to the widest label, "10:30 PM": eight glyphs at
/// roughly 0.6em of the 11px face, plus the gap before the rule. Squeezed past
/// this the grid scrolls sideways instead.
const COLUMN: Pixels = px(68.);

/// How far apart the hour lines stand: spaced as a calendar's are, so the day
/// outgrows the region and scrolls, opening on now.
const ROW: Pixels = px(40.);

/// The label face, a step under `text_xs`.
const LABEL: Pixels = px(11.);

/// The room between a label's end and its column's rule.
const INSET: Pixels = px(8.);

/// The height of the bar behind the shown instant's labels.
const BAR: Pixels = px(22.);

/// One column: home's, or a saved city's.
struct Column {
    /// `None` for home's own column, which stands for no saved city.
    geonameid: Option<u32>,
    /// `None` when this machine does not know the city's zone: the column
    /// keeps its head and rule and leaves its labels out.
    zone: Option<TimeZone>,
}

impl Elsewhere {
    /// The grid region: the heads, which stay put, over the hour columns,
    /// which scroll down the day. Both move together sideways.
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
                    // A plain wheel is the hours' to scroll down; only
                    // shift+wheel, which the platform reports sideways, moves
                    // the columns.
                    .map(|mut scroller| {
                        scroller.style().restrict_scroll_to_axis = Some(true);
                        scroller
                    })
                    .child(
                        // As wide as the columns at their floor, or the region
                        // when that is wider: a scroller's content needs a
                        // width of its own to overflow sideways.
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
                                    // Left to itself gpui would turn a
                                    // sideways swipe into a scroll down here
                                    // as well as the columns' scroll across.
                                    .map(|mut scroller| {
                                        scroller.style().restrict_scroll_to_axis = Some(true);
                                        scroller
                                    })
                                    .child(self.render_body(&day, &columns, now, cx)),
                            ),
                    ),
            )
            // Hung out in the window padding, as the list's is, beside the
            // hours alone, and in the gap over the footer.
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

    /// The columns side by side.
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

    /// One column of the day's hour lines, as they read there, against its own
    /// rule. Each hour is a slot of `ROW` with its label at the middle, so the
    /// hour lines fall half a row in from the top.
    fn render_column(
        &self,
        day: &hours::Day,
        column: &Column,
        shown: Option<usize>,
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        // Keyed by city, as the list's rows are, so a column's hour state
        // stays with its city when the order changes.
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

    /// One hour line as a column reads it, and the click that pins it. A city
    /// with no zone keeps the slot, so its rule runs the whole day. `shown` is
    /// the shown instant, on the one hour that holds it.
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
        // The highlighted hour says the minute it actually is there, and
        // pins that, rather than the round hour.
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
                // Worded as the label is, with the day tag the list would
                // give it.
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
            // The click is the grid's whole form of the what-if time: no
            // editor, just the time this label already shows.
            .on_click(cx.listener(move |this, _event, _window, cx| {
                this.pinned = Some(Pin {
                    anchor,
                    at: instant.clone(),
                });
                cx.notify();
            }))
    }

    /// The shown instant's hour, one bar across every column behind its
    /// labels rather than a line struck through them: a plain wash while
    /// live, the banner's tint while pinned.
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

    /// Paints nothing: laid over where the hours will be, and prepainted
    /// before their scroller is, it scrolls them the first time the grid is
    /// shown so the highlighted hour sits a third of the way down — in the
    /// same frame, since the scroller clamps and applies the offset after.
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
                    // Hour line `i` stands at the middle of slot `i`.
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
