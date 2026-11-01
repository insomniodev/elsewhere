//! Set like a calendar's time gutter: a column of hour labels per city, the
//! home day's hours lined up across.

mod header;

use gpui::{
    Axis, Context, Div, ElementId, Pixels, ScrollHandle, canvas, div, point, prelude::*, px, rgb,
    rgba,
};
use jiff::Zoned;
use jiff::civil::Date;
use jiff::tz::TimeZone;

use super::focus::{keyboard_only, ring};
use super::here::{hint_over, tag};
use super::scrollbar::scrollbar;
use super::tooltip::Tooltip;
use super::{CellDown, CellLeft, CellRight, CellUp, Elsewhere, Pin, PinCell};
use crate::clock;
use crate::hours;
use crate::theme;

/// Also stops a long city name holding its column open, as a flex item's
/// automatic minimum is its content. Fits "10:30 pm" at the 11px face plus the
/// inset; narrower, the grid scrolls sideways.
const COLUMN: Pixels = px(68.);

/// Spaced as a calendar's, so the day outgrows the region and scrolls.
const ROW: Pixels = px(40.);

/// A step under `text_xs`.
const LABEL: Pixels = px(11.);

const INSET: Pixels = px(8.);

const BAR: Pixels = px(22.);

/// A midnight mark's rule, and the gap between a time and its day tag.
const RULE: Pixels = px(1.);
const TAG_GAP: Pixels = px(3.);

/// A day name's line box, so it fits the gap over a centred time.
const DAY_LINE: Pixels = px(12.);

/// The now line's hover band, so a 1px line can still be pointed at.
const NOW_REACH: Pixels = px(7.);

struct Column {
    /// `None` for here's own column, drawn only while no saved city is in
    /// the zone.
    geonameid: Option<u32>,
    /// `None` when the zone is unknown: the column keeps its head and rule but
    /// no labels.
    zone: Option<TimeZone>,
    /// The hour the keyboard cursor is on, if it is in this column and the
    /// grid has focus.
    cursor: Option<usize>,
}

/// Scrolls the least that brings `start..start + len` along `axis` into view.
fn reveal(handle: &ScrollHandle, axis: Axis, start: Pixels, len: Pixels) {
    let offset = handle.offset();
    let size = handle.bounds().size;
    let (shown, view) = match axis {
        Axis::Vertical => (-offset.y, size.height),
        Axis::Horizontal => (-offset.x, size.width),
    };
    let to = if start < shown {
        start
    } else if start + len > shown + view {
        start + len - view
    } else {
        return;
    };
    handle.set_offset(match axis {
        Axis::Vertical => point(offset.x, -to),
        Axis::Horizontal => point(-to, offset.y),
    });
}

/// Where the real time falls, `hours` into the day: a label's hour sits
/// mid-slot, so the line runs from there down by its minutes.
fn now_band(hours: f32) -> Div {
    div()
        .absolute()
        .left_0()
        .right_0()
        .top(ROW / 2. + ROW * hours - NOW_REACH / 2.)
        .h(NOW_REACH)
}

impl Elsewhere {
    /// The heads stay put over the hours, which scroll down the day. Both
    /// scroll together sideways.
    pub(super) fn render_grid(
        &self,
        order: &[u32],
        now: &Zoned,
        empty: bool,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let day = hours::Day::of(now);
        // Alone, here's column keeps its own width at the left rather than
        // stretching to set its labels against the far edge.
        let alone = order.is_empty();
        let mut columns = self.grid_columns(order);
        if self.focus.on(&self.focus.grid) {
            let (column, hour) = self.cell(columns.len(), &day, now);
            if let Some(column) = columns.get_mut(column) {
                column.cursor = Some(hour);
            }
        }

        keyboard_only(div().track_focus(&self.focus.grid))
            .key_context("Grid")
            .on_action(cx.listener(|this, _: &CellLeft, _window, cx| this.move_cell(-1, 0, cx)))
            .on_action(cx.listener(|this, _: &CellRight, _window, cx| this.move_cell(1, 0, cx)))
            .on_action(cx.listener(|this, _: &CellUp, _window, cx| this.move_cell(0, -1, cx)))
            .on_action(cx.listener(|this, _: &CellDown, _window, cx| this.move_cell(0, 1, cx)))
            .on_action(cx.listener(|this, _: &PinCell, _window, cx| this.pin_cell(cx)))
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
                            .when(!alone, |content| content.min_w_full())
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
                    .child(scrollbar(
                        "hours-bar",
                        &self.grid_hours,
                        Axis::Vertical,
                        px(-6.),
                    )),
            )
            .child(scrollbar(
                "columns-bar",
                &self.grid_scroll,
                Axis::Horizontal,
                px(-6.),
            ))
            .child(self.render_date(now))
            // Beside here's column, not over its hours.
            .when(empty, |grid| {
                grid.child(hint_over(
                    div().top(header::HEAD).bottom_0().left(COLUMN).right_0(),
                ))
            })
    }

    /// Here's column first, while no saved city is in the zone to be it,
    /// then the list's order, so the westward toggle reorders them too.
    fn grid_columns(&self, order: &[u32]) -> Vec<Column> {
        let here = self.here.city.is_none().then(|| Column {
            geonameid: None,
            zone: Some(self.here.zone.clone()),
            cursor: None,
        });
        here.into_iter()
            .chain(order.iter().filter_map(|geonameid| {
                let city = self.index.city(*geonameid)?;
                Some(Column {
                    geonameid: Some(*geonameid),
                    zone: self.zones.get(&city.timezone).cloned(),
                    cursor: None,
                })
            }))
            .collect()
    }

    /// The keyboard cursor as (column, hour), kept inside a grid that may
    /// have lost columns or hours since it moved. It starts on the first
    /// column at the shown hour.
    fn cell(&self, columns: usize, day: &hours::Day, now: &Zoned) -> (usize, usize) {
        let (column, hour) = self.grid_cursor.unwrap_or((0, day.index(now).unwrap_or(0)));
        (
            column.min(columns.saturating_sub(1)),
            hour.min(day.rows().saturating_sub(1)),
        )
    }

    /// Arrows on the focused grid, scrolling the cell into view.
    fn move_cell(&mut self, across: isize, down: isize, cx: &mut Context<Self>) {
        let now = self.shown();
        let day = hours::Day::of(&now);
        let columns = self.grid_columns(&self.order()).len();
        let (column, hour) = self.cell(columns, &day, &now);
        let step = |at: usize, by: isize, len: usize| {
            at.saturating_add_signed(by).min(len.saturating_sub(1))
        };
        let (column, hour) = (step(column, across, columns), step(hour, down, day.rows()));
        self.grid_cursor = Some((column, hour));
        reveal(&self.grid_hours, Axis::Vertical, ROW * hour as f32, ROW);
        reveal(
            &self.grid_scroll,
            Axis::Horizontal,
            COLUMN * column as f32,
            COLUMN,
        );
        cx.notify();
    }

    /// Enter on the focused grid pins the cell's time, as a click does.
    fn pin_cell(&mut self, cx: &mut Context<Self>) {
        let now = self.shown();
        let day = hours::Day::of(&now);
        let columns = self.grid_columns(&self.order());
        let (column, hour) = self.cell(columns.len(), &day, &now);
        let Some(column) = columns.get(column).filter(|column| column.zone.is_some()) else {
            return;
        };
        // The shown hour pins the actual minute, as its label shows.
        let at = if day.index(&now) == Some(hour) {
            now
        } else {
            day.row(hour)
        };
        self.pinned = Some(Pin {
            anchor: column.geonameid,
            at,
        });
        cx.notify();
    }

    /// The day the hours lay out, in here's zone, over the first column's
    /// corner. "today" is by the real clock, so a pinned day reads as a date.
    fn render_date(&self, now: &Zoned) -> impl IntoElement + use<> {
        let today = Zoned::now().with_time_zone(now.time_zone().clone()).date();
        let color = self
            .pinned
            .as_ref()
            .map_or(theme::SUBTEXT0, |pin| pin.accent().text);
        div()
            .absolute()
            .top_0()
            .left(INSET)
            .h(header::DATE)
            .flex()
            .items_center()
            .text_size(LABEL)
            .text_color(rgb(color))
            .whitespace_nowrap()
            .child(clock::grid_date(now.date(), today))
    }

    fn render_body(
        &self,
        day: &hours::Day,
        columns: &[Column],
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let shown = day.index(now);
        // While pinned the bar follows the pin, so a line keeps the real time.
        let live = self.pinned.as_ref().and_then(|_| {
            let real = Zoned::now();
            Some((day.position(&real)?, real))
        });
        div()
            .relative()
            .flex()
            .flex_row()
            // First, so the columns' labels and rules paint over them.
            .children(shown.map(|i| self.render_highlight(i)))
            .children(live.as_ref().map(|(hours, _)| {
                now_band(*hours)
                    .flex()
                    .items_center()
                    .child(div().w_full().h(px(1.)).bg(rgba(theme::NOW_LINE)))
            }))
            .children(
                columns
                    .iter()
                    .map(|column| self.render_column(day, column, shown, now, cx)),
            )
            // The line's hover band goes last, so it is on top to be pointed at.
            .children(live.map(|(hours, real)| {
                let time = clock::reading(&real, &self.here.zone, self.format).time;
                now_band(hours)
                    .id("now-line")
                    .tooltip(Tooltip::text(format!("now · {time}")))
            }))
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
                let begins = column
                    .zone
                    .as_ref()
                    .and_then(|zone| day.begins_day(i, zone));
                self.render_hour(&day.row(i), column, i, shown, begins, cx)
            }))
    }

    /// A city with no zone keeps the slot, so its rule runs the whole day.
    /// `shown` is set only on the hour holding the shown instant. `begins` is
    /// the date of the local day this hour opens, if it opens one.
    ///
    /// A midnight mark is a rule along the slot's top and the new day's name
    /// in the gap above the time, rather than stacked with it or in place of
    /// the hour: the row keeps its height and its label, and the slot's top is
    /// where the rows of two days meet. The shown hour's day tag goes inline
    /// beside its time for the same reason; it never lengthens the cell.
    fn render_hour(
        &self,
        at: &Zoned,
        column: &Column,
        i: usize,
        shown: Option<&Zoned>,
        begins: Option<Date>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let slot = div()
            .id(("hour", i))
            .relative()
            .h(ROW)
            .flex()
            .items_center()
            .justify_end()
            .gap(TAG_GAP)
            .pr(INSET)
            .text_size(LABEL)
            .text_color(rgb(theme::OVERLAY2))
            .when(column.cursor == Some(i), |slot| slot.child(ring()));
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
            .children(begins.map(|date| {
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .right_0()
                    .h(RULE)
                    .bg(rgb(theme::SURFACE1))
                    .child(
                        div()
                            .absolute()
                            .top(RULE)
                            .right(INSET)
                            .text_color(rgb(theme::SUBTEXT0))
                            .line_height(DAY_LINE)
                            .child(clock::day_label(date)),
                    )
            }))
            .child(label.clone());
        // Only the shown hour, and only where the date differs from here's.
        let slot = match shown {
            Some(shown) => slot.children(
                clock::day_shift(shown.date(), local.date())
                    .map(|shift| tag(shift).text_size(LABEL)),
            ),
            None => slot,
        };
        // Here's column pins with no row behind it, as a scrub does.
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
