//! The cities' dots and labels are divs rather than canvas paint, so they
//! hover and click like the list's rows do.

use gpui::{
    AnyElement, Context, ElementId, MouseButton, PathBuilder, Pixels, Point, Rems, Size, canvas,
    div, point, prelude::*, px, rems, rgb, rgba,
};
use jiff::Zoned;

use super::labels;
use crate::app::focus::keyboard_only;
use crate::app::here::tag;
use crate::app::row::REMOVE_SMALL;
use crate::app::tooltip::Tooltip;
use crate::app::{Elsewhere, Remove};
use crate::clock;
use crate::geo::{self, LonLat};
use crate::theme;

/// The radius is what the label placement keeps clear.
const DOT: Pixels = px(7.);
const RADIUS: f32 = 3.5;
/// What takes the pointer for a dot: far easier to hit than the dot drawn.
const HIT: Pixels = px(20.);

const LABEL_GAP: f32 = 3.0;
/// How far from its dot a label may be set, nearest first, when a crowd
/// leaves no room beside it. Past the last the label is hidden: a longer
/// leader would be harder to follow than the dot's tooltip is to find.
const LABEL_REACHES: [f32; 4] = [12.0, 24.0, 36.0, 48.0];
const LEADER_WIDTH: Pixels = px(1.);

/// A label is a chip: the time on its first line, the name under it and,
/// for here, the tag under that. The time is the larger, being what is read.
const TIME_LINE: f32 = 16.0;
const NAME_LINE: f32 = 14.0;
const CHIP_PAD_X: f32 = 5.0;
const CHIP_PAD_Y: f32 = 2.0;
const CHIP_EDGE: f32 = 1.0;
/// Every chip's, so they read as one set whatever their names: room for a
/// long name or a twelve-hour time with its day. A longer name is cut with
/// an ellipsis; the tooltip has it whole.
const CHIP_WIDTH: f32 = 90.0;

fn time_size() -> Rems {
    rems(0.8125)
}

fn name_size() -> Rems {
    rems(0.6875)
}

/// Only here's chip has the third line.
fn chip_height(home: bool) -> f32 {
    let lines = TIME_LINE + NAME_LINE + if home { NAME_LINE } else { 0.0 };
    lines + 2.0 * (CHIP_PAD_Y + CHIP_EDGE)
}

const PART_GAP: Pixels = px(4.);

/// What a label's remove button shows on the hover of.
const LABEL_GROUP: &str = "map-label";

struct Marker {
    /// `None` for here's own marker, which is not a saved city: it is neither
    /// removed nor opens the editor.
    geonameid: Option<u32>,
    /// In the map rect's own coordinates.
    centre: Point<f32>,
    tip: String,
    name: String,
    time: String,
    day: Option<String>,
    /// Only a resolved zone can anchor a pin, as in the list.
    known: bool,
    /// Here, whether a saved city or the zone's seat.
    home: bool,
}

impl Elsewhere {
    /// Leaders first, then labels, then dots, so a dot is never buried under
    /// a label nor a label under a line. Empty until the canvas has measured
    /// itself.
    pub(super) fn render_pins(
        &self,
        saved: &[u32],
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(layout) = self.map_layout.get() else {
            return Vec::new();
        };
        let rect = layout.rect;
        let markers = self.markers(saved, now, rect.size);
        let dots: Vec<(f32, f32)> = markers.iter().map(|m| (m.centre.x, m.centre.y)).collect();
        let sizes: Vec<(f32, f32)> = markers
            .iter()
            .map(|m| (CHIP_WIDTH, chip_height(m.home)))
            .collect();
        // What the viewport shows of the map, so a label keeps clear of its
        // edges rather than of the map's, most of which is scrolled away.
        let left = -f32::from(self.map_scroll.offset().x) - f32::from(rect.origin.x);
        let shown = left.max(0.0);
        let frame = labels::Slot {
            x: shown,
            y: 0.0,
            w: (left + f32::from(layout.viewport.width)).min(f32::from(rect.size.width)) - shown,
            h: f32::from(rect.size.height),
        };
        let placed = labels::place(&dots, &sizes, RADIUS, LABEL_GAP, frame, &LABEL_REACHES);

        let origin = rect.origin;
        let mut elements: Vec<AnyElement> = Vec::with_capacity(markers.len() * 2 + 1);
        let leaders: Vec<(Point<Pixels>, Point<Pixels>)> = markers
            .iter()
            .zip(&placed)
            .filter_map(|(marker, placed)| {
                let (x, y) = placed.as_ref()?.leader?;
                Some((
                    origin + point(px(marker.centre.x), px(marker.centre.y)),
                    origin + point(px(x), px(y)),
                ))
            })
            .collect();
        if !leaders.is_empty() {
            elements.push(render_leaders(leaders));
        }
        for (marker, placed) in markers.iter().zip(&placed) {
            if let Some(labels::Placed { slot, .. }) = placed {
                elements.push(self.render_label(
                    marker,
                    origin + point(px(slot.x), px(slot.y)),
                    // Or where the end would leave what the viewport shows.
                    slot.x + slot.w < marker.centre.x
                        || slot.x + slot.w + f32::from(REMOVE_SMALL + PART_GAP) > frame.x + frame.w,
                    now,
                    cx,
                ));
            }
        }
        for (marker, placed) in markers.iter().zip(&placed) {
            let centre = origin + point(px(marker.centre.x), px(marker.centre.y));
            elements.push(self.render_dot(marker, centre, placed.is_none(), cx));
        }
        elements
    }

    /// Order matters: `place` lets earlier markers win, and the saved order is
    /// the one the user arranged. Here's own marker, when no saved city is in
    /// the zone, goes first: it is the reader's place.
    fn markers(&self, saved: &[u32], now: &Zoned, size: Size<Pixels>) -> Vec<Marker> {
        // At the zone's seat, so a zone no saved city is in still gets one;
        // a zone no city keeps has nowhere to go, and the list and grid carry
        // here alone.
        let seat = self
            .here
            .city
            .is_none()
            .then_some(self.here.seat)
            .flatten()
            .and_then(|geonameid| self.index.city(geonameid))
            .map(|city| {
                let time = clock::reading(now, &self.here.zone, self.format).time;
                Marker {
                    geonameid: None,
                    centre: centre(city.longitude, city.latitude, size),
                    tip: self.here_detail(now),
                    name: self.here.name.clone(),
                    time,
                    day: None,
                    known: true,
                    home: true,
                }
            });

        let cities = saved.iter().filter_map(|geonameid| {
            let city = self.index.city(*geonameid)?;
            let zone = self.zones.get(&city.timezone);
            let reading = zone.map(|zone| clock::reading(now, zone, self.format));
            let known = reading.is_some();
            let (time, day) = match reading {
                Some(reading) => (reading.time, reading.day),
                None => (clock::UNKNOWN.to_string(), None),
            };
            let home = self.here.city == Some(*geonameid);
            let tip = clock::city_detail(&self.index.label(city), zone, now);
            Some(Marker {
                geonameid: Some(*geonameid),
                centre: centre(city.longitude, city.latitude, size),
                tip,
                name: city.name.clone(),
                time,
                day,
                known,
                home,
            })
        });
        seat.into_iter().chain(cities).collect()
    }

    /// Hollow for here, since that is the reader's place rather than an
    /// elsewhere. The dot is drawn in the middle of a larger, invisible hit
    /// square that hovers and clicks for it.
    fn render_dot(
        &self,
        marker: &Marker,
        centre: Point<Pixels>,
        unlabelled: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // Without a label, the tooltip is the only place to read the time.
        let tip = if unlabelled {
            format!("{} · {}", marker.tip, marker.time)
        } else {
            marker.tip.clone()
        };
        let id = match marker.geonameid {
            Some(geonameid) => ElementId::from(("dot", geonameid as usize)),
            None => ElementId::from("dot-here"),
        };
        let dot = div()
            .id(id)
            .group("dot")
            .absolute()
            .left(centre.x - HIT / 2.)
            .top(centre.y - HIT / 2.)
            .w(HIT)
            .h(HIT)
            .flex()
            .items_center()
            .justify_center()
            .tooltip(Tooltip::text(tip))
            .child(
                div()
                    .w(DOT)
                    .h(DOT)
                    .rounded_full()
                    .bg(rgb(theme::BLUE))
                    .when(marker.home, |dot| {
                        dot.bg(rgb(theme::BASE))
                            .border(px(1.5))
                            .border_color(rgb(theme::BLUE))
                    })
                    // Lit as the list's row is washed, for a city just picked
                    // or restored.
                    .when(
                        marker.geonameid.is_some() && self.revealed == marker.geonameid,
                        |dot| dot.bg(rgb(theme::TEXT)),
                    )
                    .group_hover("dot", |style| style.bg(rgb(theme::TEXT))),
            );
        // Here's marker does not edit, so a press on it pans.
        let Some(geonameid) = marker.geonameid else {
            return dot.into_any_element();
        };
        let time = marker.time.clone();
        let Some(focus) = self.focus.row(geonameid).cloned() else {
            return dot.into_any_element();
        };
        let focused = self.focus.on(&focus);
        // The dot opens the editor its label's time does, where there is a
        // label to hold it; the label carries the remove button.
        // A stop in the tab order with its row's keys, but for moving: the
        // map has no order to move it in. Ringed rather than lit, so focus
        // and a hover read apart.
        keyboard_only(dot.track_focus(&focus))
            .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                cx.stop_propagation()
            })
            .key_context("Row")
            .when(focused, |dot| {
                dot.rounded_full().border_1().border_color(rgb(theme::BLUE))
            })
            .on_action(
                cx.listener(move |this, _: &Remove, window, cx| this.delete(geonameid, window, cx)),
            )
            // Focused, Enter clicks it, so it needs no Edit of its own.
            .when(marker.known && !unlabelled, |dot| {
                dot.cursor_pointer()
                    .on_click(cx.listener(move |this, _event, window, cx| {
                        this.edit(geonameid, time.clone(), window, cx)
                    }))
            })
            .into_any_element()
    }

    /// A chip, its lines kept to the left: the time, then the name and, for
    /// here, the tag. The time opens the same what-if editor a row's does. A
    /// saved city's label shows its remove button while hovered, beside the
    /// chip on the side away from the dot: before it when `leading`, the
    /// label lying left of its dot or hard against the viewport's right
    /// edge. The button is no part of the label's placement, so it may lie
    /// over a neighbour for as long as the hover lasts.
    fn render_label(
        &self,
        marker: &Marker,
        at: Point<Pixels>,
        leading: bool,
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let input = self
            .editing
            .as_ref()
            .filter(|(editing, _)| marker.geonameid == Some(*editing))
            .map(|(_, input)| input);
        let editing = input.is_some();
        let editable = marker.geonameid.filter(|_| marker.known);
        let time = marker.time.clone();
        let reading = match input {
            // Bigger than the time, so the chip grows to hold it for as
            // long as it is open.
            Some(input) => self
                .render_editor(input, cx)
                .flex_none()
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation()
                })
                .into_any_element(),
            None => div()
                .id(match marker.geonameid {
                    Some(geonameid) => ElementId::from(("map-time", geonameid as usize)),
                    None => ElementId::from("map-time-here"),
                })
                .text_size(time_size())
                .when_some(self.mono.clone(), Styled::font_family)
                .when_some(
                    self.pinned.as_ref().map(|pin| pin.accent().text),
                    |cell, text| cell.text_color(rgb(text)),
                )
                .child(marker.time.clone())
                .when_some(editable, |cell, geonameid| {
                    cell.cursor_pointer()
                        // A press here edits rather than pans.
                        .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                            cx.stop_propagation()
                        })
                        .rounded_sm()
                        .hover(|style| style.bg(rgb(theme::SURFACE0)))
                        .on_click(cx.listener(move |this, _event, window, cx| {
                            this.edit(geonameid, time.clone(), window, cx)
                        }))
                })
                .into_any_element(),
        };
        // A press on it removes rather than pans.
        let remove = marker.geonameid.map(|geonameid| {
            self.render_remove_small(geonameid, LABEL_GROUP, cx)
                .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                    cx.stop_propagation()
                })
        });
        let (before, after) = if leading {
            (remove, None)
        } else {
            (None, remove)
        };
        let inset = if before.is_some() {
            REMOVE_SMALL + PART_GAP
        } else {
            px(0.)
        };
        let top = div()
            .flex()
            .flex_row()
            .items_center()
            .gap(PART_GAP)
            .line_height(px(TIME_LINE))
            .child(reading)
            .children(
                marker
                    .day
                    .clone()
                    .map(|day| tag(day).text_size(name_size())),
            );
        let body = div()
            .flex()
            .flex_col()
            .items_start()
            .px(px(CHIP_PAD_X))
            .py(px(CHIP_PAD_Y))
            .border_1()
            .border_color(rgb(theme::SURFACE1))
            .rounded_sm()
            .bg(rgba(theme::CHIP))
            .whitespace_nowrap()
            // The editor may be wider than the chip, never narrower.
            .min_w(px(CHIP_WIDTH))
            .when(!editing, |body| {
                body.w(px(CHIP_WIDTH)).h(px(chip_height(marker.home)))
            })
            .child(top)
            .child(
                div()
                    .id(match marker.geonameid {
                        Some(geonameid) => ElementId::from(("map-name", geonameid as usize)),
                        None => ElementId::from("map-name-here"),
                    })
                    .max_w_full()
                    .truncate()
                    .text_size(name_size())
                    .line_height(px(NAME_LINE))
                    .text_color(rgb(theme::SUBTEXT0))
                    .tooltip(Tooltip::text(marker.tip.clone()))
                    .child(marker.name.clone()),
            )
            // In the dot's own blue, so the reader's place stands out.
            .when(marker.home, |body| {
                body.child(
                    self.here_tag("map-here", now)
                        .text_size(name_size())
                        .line_height(px(NAME_LINE))
                        .text_color(rgb(theme::BLUE)),
                )
            });
        div()
            .group(LABEL_GROUP)
            .absolute()
            .left(at.x - inset)
            .top(at.y)
            .flex()
            .flex_row()
            .items_start()
            .gap(PART_GAP)
            .children(before)
            .child(body)
            .children(after)
            .into_any_element()
    }
}

/// The lines tying the labels set apart from their dots back to them, each
/// from a dot's centre to its label's edge, in the pins' own coordinates.
/// Painted, since a div cannot slant; nothing hovers or clicks on them.
fn render_leaders(leaders: Vec<(Point<Pixels>, Point<Pixels>)>) -> AnyElement {
    canvas(
        |_bounds, _window, _cx| {},
        move |bounds, (), window, _cx| {
            let mut builder = PathBuilder::stroke(LEADER_WIDTH);
            for (from, to) in &leaders {
                builder.move_to(bounds.origin + *from);
                builder.line_to(bounds.origin + *to);
            }
            // A path lyon cannot tessellate is only the lines missing.
            if let Ok(path) = builder.build() {
                window.paint_path(path, rgb(theme::OVERLAY2));
            }
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}

fn centre(lon: f64, lat: f64, size: Size<Pixels>) -> Point<f32> {
    let (x, y) = geo::project(LonLat { lon, lat });
    point(
        f32::from(size.width) * x as f32,
        f32::from(size.height) * y as f32,
    )
}
