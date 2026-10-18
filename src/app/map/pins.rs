//! The cities' dots and labels are divs rather than canvas paint, so they
//! hover, click and drag like the list's rows do.

use gpui::{
    AnyElement, Context, Font, Pixels, Point, SharedString, Size, TextRun, Window, div, point,
    prelude::*, px, rems, rgb,
};
use jiff::Zoned;

use super::labels;
use crate::app::Elsewhere;
use crate::app::drag::DragRow;
use crate::app::tooltip::Tooltip;
use crate::clock;
use crate::geo::{self, LonLat};
use crate::theme;

/// The radius is what the label placement keeps clear.
const DOT: Pixels = px(7.);
const RADIUS: f32 = 3.5;

const LABEL_HEIGHT: f32 = 16.0;
const LABEL_GAP: f32 = 3.0;

const PART_GAP: Pixels = px(4.);

struct Marker {
    geonameid: u32,
    /// In the map rect's own coordinates.
    centre: Point<f32>,
    /// The full name, for the tooltip.
    label: String,
    name: String,
    time: String,
    day: Option<String>,
    /// Only a resolved zone can anchor a pin, as in the list.
    known: bool,
    /// The machine's own zone.
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
        let Some(rect) = self.map_rect() else {
            return Vec::new();
        };
        let markers = self.markers(saved, now, rect.size, window);
        let dots: Vec<(f32, f32)> = markers.iter().map(|m| (m.centre.x, m.centre.y)).collect();
        let sizes: Vec<(f32, f32)> = markers.iter().map(|m| (m.width, LABEL_HEIGHT)).collect();
        let slots = labels::place(
            &dots,
            &sizes,
            RADIUS,
            LABEL_GAP,
            (f32::from(rect.size.width), f32::from(rect.size.height)),
        );

        let origin = rect.origin;
        let mut elements: Vec<AnyElement> = Vec::with_capacity(markers.len() * 2);
        for (marker, slot) in markers.iter().zip(&slots) {
            if let Some(slot) = slot {
                elements.push(self.render_label(
                    marker,
                    origin + point(px(slot.x), px(slot.y)),
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
    /// the one the user arranged.
    fn markers(
        &self,
        saved: &[u32],
        now: &Zoned,
        size: Size<Pixels>,
        window: &Window,
    ) -> Vec<Marker> {
        let home = Zoned::now().time_zone().iana_name().map(str::to_string);
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

        saved
            .iter()
            .filter_map(|geonameid| {
                let city = self.index.city(*geonameid)?;
                let reading = self
                    .zones
                    .get(&city.timezone)
                    .map(|zone| clock::reading(now, zone, self.format));
                let known = reading.is_some();
                let (time, day) = match reading {
                    Some(reading) => (reading.time, reading.day),
                    None => (clock::UNKNOWN.to_string(), None),
                };
                let (x, y) = geo::project(LonLat {
                    lon: city.longitude,
                    lat: city.latitude,
                });
                let mut width = measure(window, &city.name, &text, text_size)
                    + f32::from(PART_GAP)
                    + measure(window, &time, &mono, text_size);
                if let Some(day) = &day {
                    width += f32::from(PART_GAP) + measure(window, day, &text, text_size);
                }
                Some(Marker {
                    geonameid: *geonameid,
                    centre: point(
                        f32::from(size.width) * x as f32,
                        f32::from(size.height) * y as f32,
                    ),
                    label: self.index.label(city),
                    name: city.name.clone(),
                    time,
                    day,
                    known,
                    home: home.as_deref() == Some(city.timezone.as_str()),
                    width,
                })
            })
            .collect()
    }

    /// Hollow for the machine's own zone, since that is the reader's place
    /// rather than an elsewhere.
    fn render_dot(
        &self,
        marker: &Marker,
        centre: Point<Pixels>,
        unlabelled: bool,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let geonameid = marker.geonameid;
        // Without a label, the tooltip is the only place to read the time.
        let tip = if unlabelled {
            format!("{} · {}", marker.label, marker.time)
        } else {
            marker.label.clone()
        };
        let label = marker.label.clone();
        div()
            .id(("dot", geonameid as usize))
            .absolute()
            .left(centre.x - px(RADIUS))
            .top(centre.y - px(RADIUS))
            .w(DOT)
            .h(DOT)
            .rounded_full()
            .bg(rgb(theme::BLUE))
            .when(marker.home, |dot| {
                dot.bg(rgb(theme::BASE))
                    .border(px(1.5))
                    .border_color(rgb(theme::BLUE))
            })
            .hover(|style| style.bg(rgb(theme::TEXT)))
            .tooltip(Tooltip::text(tip))
            // The dot is the whole handle, as the row is in the list.
            .cursor_grab()
            .on_drag(DragRow { geonameid, label }, Self::drag_preview(cx))
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
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let geonameid = marker.geonameid;
        let time = marker.time.clone();
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
                    .text_xs()
                    .text_color(rgb(theme::SUBTEXT0))
                    .child(marker.name.clone()),
            )
            .child(match &self.editing {
                // Taller than the line, so it overhangs rather than moving
                // the label.
                Some((editing, input)) if *editing == geonameid => {
                    self.render_editor(input, cx).flex_none().into_any_element()
                }
                _ => div()
                    .id(("map-time", geonameid as usize))
                    .text_xs()
                    .when_some(self.mono.clone(), Styled::font_family)
                    .when_some(
                        self.pinned.as_ref().map(|pin| pin.accent().text),
                        |cell, text| cell.text_color(rgb(text)),
                    )
                    .when(marker.known, |cell| {
                        cell.cursor_pointer()
                            .rounded_sm()
                            .hover(|style| style.bg(rgb(theme::SURFACE0)))
                            .on_click(cx.listener(move |this, _event, window, cx| {
                                this.edit(geonameid, time.clone(), window, cx)
                            }))
                    })
                    .child(marker.time.clone())
                    .into_any_element(),
            })
            .children(
                marker
                    .day
                    .clone()
                    .map(|day| div().text_xs().text_color(rgb(theme::OVERLAY0)).child(day)),
            )
            .into_any_element()
    }
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
