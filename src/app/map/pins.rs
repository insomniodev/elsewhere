//! The cities' dots and labels are divs rather than canvas paint, so they
//! hover, click and drag like the list's rows do.

use gpui::{
    AnyElement, Context, ElementId, Font, MouseButton, Pixels, Point, SharedString, Size, TextRun,
    Window, div, point, prelude::*, px, rems, rgb,
};
use jiff::Zoned;

use super::labels;
use crate::app::Elsewhere;
use crate::app::drag::DragRow;
use crate::app::here::tag;
use crate::app::tooltip::Tooltip;
use crate::clock;
use crate::geo::{self, LonLat};
use crate::theme;

/// The radius is what the label placement keeps clear.
const DOT: Pixels = px(7.);
const RADIUS: f32 = 3.5;
/// What takes the pointer for a dot: far easier to hit than the dot drawn.
const HIT: Pixels = px(20.);

const LABEL_HEIGHT: f32 = 16.0;
const LABEL_GAP: f32 = 3.0;

const PART_GAP: Pixels = px(4.);

struct Marker {
    /// `None` for here's own marker, which is not a saved city: it neither
    /// drags nor opens the editor.
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
    /// The label's drawn width.
    width: f32,
}

impl Elsewhere {
    /// Labels first, so a dot is never buried under one. Empty until the
    /// canvas has measured itself.
    pub(super) fn render_pins(
        &self,
        saved: &[u32],
        now: &Zoned,
        dragging: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(layout) = self.map_layout.get() else {
            return Vec::new();
        };
        let rect = layout.rect;
        let markers = self.markers(saved, now, rect.size, window);
        let dots: Vec<(f32, f32)> = markers.iter().map(|m| (m.centre.x, m.centre.y)).collect();
        let sizes: Vec<(f32, f32)> = markers.iter().map(|m| (m.width, LABEL_HEIGHT)).collect();
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
        let slots = labels::place(&dots, &sizes, RADIUS, LABEL_GAP, frame);

        let origin = rect.origin;
        let mut elements: Vec<AnyElement> = Vec::with_capacity(markers.len() * 2);
        for (marker, slot) in markers.iter().zip(&slots) {
            if let Some(slot) = slot {
                elements.push(self.render_label(
                    marker,
                    origin + point(px(slot.x), px(slot.y)),
                    now,
                    cx,
                ));
            }
        }
        for (marker, slot) in markers.iter().zip(&slots) {
            let centre = origin + point(px(marker.centre.x), px(marker.centre.y));
            elements.push(self.render_dot(marker, centre, slot.is_none(), dragging, cx));
        }
        elements
    }

    /// Order matters: `place` lets earlier markers win, and the saved order is
    /// the one the user arranged. Here's own marker, when no saved city is in
    /// the zone, goes first: it is the reader's place.
    fn markers(
        &self,
        saved: &[u32],
        now: &Zoned,
        size: Size<Pixels>,
        window: &Window,
    ) -> Vec<Marker> {
        let text = window.text_style().font();
        // All of a label is `text_xs`, so it reads as one line and stays small
        // enough to find room beside its dot.
        let text_size = rems(0.75).to_pixels(window.rem_size());
        let mono = match self.mono.clone() {
            Some(family) => Font {
                family,
                ..text.clone()
            },
            None => text.clone(),
        };
        let gap = f32::from(PART_GAP);
        let here_width = gap + measure(window, "here", &text, text_size);

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
                let width = measure(window, &self.here.name, &text, text_size)
                    + here_width
                    + gap
                    + measure(window, &time, &mono, text_size);
                Marker {
                    geonameid: None,
                    centre: centre(city.longitude, city.latitude, size),
                    tip: self.here_detail(now),
                    name: self.here.name.clone(),
                    time,
                    day: None,
                    known: true,
                    home: true,
                    width,
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
            let mut width = measure(window, &city.name, &text, text_size)
                + gap
                + measure(window, &time, &mono, text_size);
            if home {
                width += here_width;
            }
            if let Some(day) = &day {
                width += gap + measure(window, day, &text, text_size);
            }
            Some(Marker {
                geonameid: Some(*geonameid),
                centre: centre(city.longitude, city.latitude, size),
                tip,
                name: city.name.clone(),
                time,
                day,
                known,
                home,
                width,
            })
        });
        seat.into_iter().chain(cities).collect()
    }

    /// Hollow for here, since that is the reader's place rather than an
    /// elsewhere. The dot is drawn in the middle of a larger, invisible hit
    /// square that hovers, clicks and drags for it.
    fn render_dot(
        &self,
        marker: &Marker,
        centre: Point<Pixels>,
        unlabelled: bool,
        dragging: bool,
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
        // Here's marker neither drags nor edits, so a press on it pans.
        let Some(geonameid) = marker.geonameid else {
            return dot.into_any_element();
        };
        let label = self
            .index
            .city(geonameid)
            .map(|city| self.index.label(city))
            .unwrap_or_default();
        let time = marker.time.clone();
        // The dot is the whole handle, as the row is in the list, and opens
        // the editor its label's time does, where there is a label to hold it.
        // No grab cursor: the bin is its only drop target, which reorders
        // nothing, so the drag is an extra beside the list's remove button.
        dot.on_mouse_down(MouseButton::Left, |_event, _window, cx| {
            cx.stop_propagation()
        })
        .when(marker.known && !unlabelled, |dot| {
            dot.cursor_pointer()
                .on_click(cx.listener(move |this, _event, window, cx| {
                    this.edit(geonameid, time.clone(), window, cx)
                }))
        })
        .on_drag(
            DragRow::new(geonameid, label, false),
            Self::drag_preview(cx),
        )
        .when(dragging && self.drag == Some(geonameid), |dot| {
            dot.opacity(0.4)
        })
        .into_any_element()
    }

    /// The time opens the same what-if editor a row's does.
    fn render_label(
        &self,
        marker: &Marker,
        at: Point<Pixels>,
        now: &Zoned,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let time = marker.time.clone();
        let cell = |id: ElementId| {
            div()
                .id(id)
                .text_xs()
                .when_some(self.mono.clone(), Styled::font_family)
                .when_some(
                    self.pinned.as_ref().map(|pin| pin.accent().text),
                    |cell, text| cell.text_color(rgb(text)),
                )
                .child(marker.time.clone())
        };
        let reading = match marker.geonameid {
            None => cell(ElementId::from("map-time-here")).into_any_element(),
            // Taller than the line, so it overhangs rather than moving the
            // label.
            Some(geonameid) => match &self.editing {
                Some((editing, input)) if *editing == geonameid => self
                    .render_editor(input, cx)
                    .flex_none()
                    .on_mouse_down(MouseButton::Left, |_event, _window, cx| {
                        cx.stop_propagation()
                    })
                    .into_any_element(),
                _ => cell(ElementId::from(("map-time", geonameid as usize)))
                    .when(marker.known, |cell| {
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
            },
        };
        div()
            .absolute()
            .left(at.x)
            .top(at.y)
            .h(px(LABEL_HEIGHT))
            .flex()
            .flex_row()
            .items_center()
            .gap(PART_GAP)
            .child(
                div()
                    .id(match marker.geonameid {
                        Some(geonameid) => ElementId::from(("map-name", geonameid as usize)),
                        None => ElementId::from("map-name-here"),
                    })
                    .text_xs()
                    .text_color(rgb(theme::SUBTEXT0))
                    .tooltip(Tooltip::text(marker.tip.clone()))
                    .child(marker.name.clone()),
            )
            .when(marker.home, |label| {
                label.child(self.here_tag("map-here", now))
            })
            .child(reading)
            .children(marker.day.clone().map(tag))
            .into_any_element()
    }
}

fn centre(lon: f64, lat: f64, size: Size<Pixels>) -> Point<f32> {
    let (x, y) = geo::project(LonLat { lon, lat });
    point(
        f32::from(size.width) * x as f32,
        f32::from(size.height) * y as f32,
    )
}

/// Labels are placed before layout, so the shaper is asked directly; a guess
/// from character counts would leave them overlapping.
fn measure(window: &Window, text: &str, font: &Font, size: Pixels) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    let run = TextRun {
        len: text.len(),
        font: font.clone(),
        color: gpui::black(),
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line =
        window
            .text_system()
            .shape_line(SharedString::from(text.to_string()), size, &[run], None);
    f32::from(line.width)
}
