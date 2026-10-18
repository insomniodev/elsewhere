//! The world is painted in one canvas; the saved cities are divs laid over it.
mod labels;
mod pins;

use gpui::{
    Axis, Bounds, Context, Path, PathBuilder, Pixels, Point, ScrollDelta, ScrollWheelEvent, Size,
    TextRun, Window, canvas, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use jiff::Zoned;

use super::Elsewhere;
use super::scrollbar::scrollbar;
use crate::clock;
use crate::geo::{self, LonLat};
use crate::theme;

const MERIDIAN_STEP: f64 = 15.0;
/// Degrees of longitude. The terminator is gentle enough that this is well
/// under a pixel of error at any window size.
const NIGHT_STEP: f64 = 2.0;

const ZONE_LINE_WIDTH: Pixels = px(1.);
const OFFSET_SIZE: Pixels = px(10.);
const OFFSET_LINE: Pixels = px(12.);
const OFFSET_GAP: Pixels = px(2.);
const TICK: Pixels = px(3.);

/// The sun has rays so it never reads as one of the cities' plain dots.
const SUN_RADIUS: Pixels = px(2.5);
const SUN_RAYS: usize = 8;
const RAY_FROM: f32 = 4.0;
const RAY_TO: f32 = 6.5;
const RAY_WIDTH: Pixels = px(1.);

/// A notch for wheels that report pixels, as a trackpad does. Roughly what a
/// mouse wheel's click carries.
const NOTCH_PIXELS: f32 = 40.0;

const SCROLLBAR_GAP: Pixels = px(4.);

/// `rect` is relative to the scrolled content, and wider than `viewport` when
/// the map scrolls sideways.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Layout {
    pub viewport: Size<Pixels>,
    pub rect: Bounds<Pixels>,
}

impl Layout {
    fn overflows(&self) -> bool {
        self.rect.size.width > self.viewport.width
    }
}

impl Elsewhere {
    /// `None` until the first frame has been laid out.
    pub(super) fn map_rect(&self) -> Option<Bounds<Pixels>> {
        self.map_layout.get().map(|layout| layout.rect)
    }

    pub(super) fn render_map(
        &self,
        saved: &[u32],
        now: &Zoned,
        dragging: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let layout = self.map_layout.get();
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("map")
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.map_scroll)
                    // Otherwise gpui turns a plain wheel sideways here, but
                    // that is the time scrub; shift+wheel already scrolls.
                    .map(|mut scroller| {
                        scroller.style().restrict_scroll_to_axis = Some(true);
                        scroller
                    })
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _window, cx| {
                        this.scrub(event, cx)
                    }))
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .min_w_full()
                            .when_some(layout, |content, layout| content.w(layout.rect.size.width))
                            .child(self.render_world(now))
                            .children(self.render_pins(saved, now, dragging, window, cx)),
                    ),
            )
            .when(layout.is_some_and(|layout| layout.overflows()), |map| {
                map.child(scrollbar(
                    &self.map_scroll,
                    Axis::Horizontal,
                    -SCROLLBAR_GAP,
                ))
            })
    }

    /// Where the map centres when it first overflows: the reader's own city if
    /// saved, else the first saved city, else Greenwich.
    fn focus(&self) -> f64 {
        let now = Zoned::now();
        let home = now.time_zone().iana_name();
        let mut cities = self
            .saved
            .iter()
            .filter_map(|geonameid| self.index.city(*geonameid));
        cities
            .clone()
            .find(|city| Some(city.timezone.as_str()) == home)
            .or_else(|| cities.next())
            .map_or(0.0, |city| city.longitude)
    }

    /// A notch down is a quarter hour later, the way the night moves. It is an
    /// ordinary pin, so everything follows it and escape undoes it.
    fn scrub(&mut self, event: &ScrollWheelEvent, cx: &mut Context<Self>) {
        let notches = match event.delta {
            ScrollDelta::Lines(lines) => lines.y,
            ScrollDelta::Pixels(pixels) => f32::from(pixels.y) / NOTCH_PIXELS,
        };
        // A trackpad sends small deltas; carry the fraction to the next one.
        let carried = self.scrub_carry + notches;
        self.scrub_carry = carried.fract();
        let notches = carried.trunc();
        if notches == 0.0 {
            return;
        }

        // Scrolling down reports a negative delta; it should move time forward.
        self.nudge(-(notches as i64), cx);
    }

    fn render_world(&self, now: &Zoned) -> impl IntoElement + use<> {
        let land = self.land.clone();
        let zones = self.zone_borders.clone();
        let remembered = self.map_layout.clone();
        let scroll = self.map_scroll.clone();
        let focus = self.focus();
        let sun = geo::subsolar(now);

        canvas(
            move |bounds, window, _cx| {
                // Fit to the region, not the canvas, which may already be
                // wider. The scroller has measured it by now.
                let viewport = scroll.bounds().size;
                let rect = fit(viewport);
                let layout = Layout { viewport, rect };
                let before = remembered.get();
                if before != Some(layout) {
                    remembered.set(Some(layout));
                    if layout.overflows() {
                        let middle = match before.filter(Layout::overflows) {
                            // Keep the middle longitude through the resize.
                            Some(before) => {
                                (before.viewport.width / 2. - scroll.offset().x)
                                    / before.rect.size.width
                            }
                            // Just overflowed: start on the reader's own
                            // longitude rather than the Pacific.
                            None => {
                                geo::project(LonLat {
                                    lon: focus,
                                    lat: 0.0,
                                })
                                .0 as f32
                            }
                        };
                        let most = rect.size.width - viewport.width;
                        let left =
                            (rect.size.width * middle - viewport.width / 2.).clamp(px(0.), most);
                        scroll.set_offset(point(-left, px(0.)));
                    }
                    // The overlay was placed from the old rect, so this frame
                    // is stale. A plain refresh is ignored mid-draw; this one
                    // lands.
                    window.request_animation_frame();
                }
                Bounds {
                    origin: bounds.origin + rect.origin,
                    size: rect.size,
                }
            },
            move |_bounds, rect: Bounds<Pixels>, window, cx| {
                let at = |place: LonLat| {
                    let (x, y) = geo::project(place);
                    point(
                        rect.origin.x + rect.size.width * x as f32,
                        rect.origin.y + rect.size.height * y as f32,
                    )
                };

                for ring in land.iter() {
                    if let Some(path) = polygon(ring.iter().copied().map(at)) {
                        window.paint_path(path, rgb(theme::SURFACE0));
                    }
                }

                for border in zones.iter() {
                    if let Some(path) = polyline(border.iter().copied().map(at)) {
                        window.paint_path(path, rgb(theme::ZONE_LINE));
                    }
                }

                if let Some(path) = polygon(geo::night(sun, NIGHT_STEP).into_iter().map(at)) {
                    window.paint_path(path, rgba(theme::NIGHT));
                }

                let mut lon = -180.0;
                while lon < 180.0 {
                    let x = at(LonLat { lon, lat: 0.0 }).x;
                    window.paint_quad(fill(
                        Bounds::new(point(x - px(0.5), rect.bottom() - TICK), size(px(1.), TICK)),
                        rgb(theme::SURFACE1),
                    ));
                    lon += MERIDIAN_STEP;
                }

                // The bottom edge runs through open ocean, where the zones are
                // the plain nautical ones.
                let font = window.text_style().font();
                for (lon, hours) in offset_marks() {
                    let text = clock::offset_label(jiff::tz::offset(hours));
                    let run = TextRun {
                        len: text.len(),
                        font: font.clone(),
                        color: rgb(theme::OVERLAY0).into(),
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    let line =
                        window
                            .text_system()
                            .shape_line(text.into(), OFFSET_SIZE, &[run], None);
                    let origin = point(
                        at(LonLat { lon, lat: 0.0 }).x - line.width / 2.,
                        rect.bottom() - TICK - OFFSET_GAP - OFFSET_LINE,
                    );
                    // A label that fails to paint is only a label missing.
                    line.paint(origin, OFFSET_LINE, window, cx).ok();
                }

                // The crop is an editable constant, and clamping would park
                // the sun on an edge it never reaches.
                if (geo::SOUTH..=geo::NORTH).contains(&sun.lat) {
                    let centre = at(sun);
                    window.paint_quad(
                        fill(
                            Bounds::new(
                                point(centre.x - SUN_RADIUS, centre.y - SUN_RADIUS),
                                size(SUN_RADIUS * 2., SUN_RADIUS * 2.),
                            ),
                            rgb(theme::YELLOW),
                        )
                        .corner_radii(SUN_RADIUS),
                    );
                    if let Some(path) = rays(centre) {
                        window.paint_path(path, rgb(theme::YELLOW));
                    }
                }
            },
        )
        .size_full()
    }
}

/// The window is tall first, so the map takes the full height: centred when
/// that leaves it narrower than `area`, flush left for the scroller when wider.
pub(super) fn fit(area: Size<Pixels>) -> Bounds<Pixels> {
    let width = area.height * geo::aspect() as f32;
    Bounds::new(
        point(((area.width - width) / 2.).max(px(0.)), px(0.)),
        size(width, area.height),
    )
}

/// `None` for too few points or a path lyon cannot tessellate.
fn polygon(points: impl Iterator<Item = Point<Pixels>>) -> Option<Path<Pixels>> {
    let points: Vec<_> = points.collect();
    if points.len() < 3 {
        return None;
    }
    let mut builder = PathBuilder::fill();
    builder.add_polygon(&points, true);
    builder.build().ok()
}

/// `None` for too few points or a path lyon cannot tessellate.
fn polyline(points: impl Iterator<Item = Point<Pixels>>) -> Option<Path<Pixels>> {
    let points: Vec<_> = points.collect();
    if points.len() < 2 {
        return None;
    }
    let mut builder = PathBuilder::stroke(ZONE_LINE_WIDTH);
    builder.add_polygon(&points, false);
    builder.build().ok()
}

fn rays(centre: Point<Pixels>) -> Option<Path<Pixels>> {
    let mut builder = PathBuilder::stroke(RAY_WIDTH);
    for ray in 0..SUN_RAYS {
        let angle = std::f32::consts::TAU * ray as f32 / SUN_RAYS as f32;
        let (sin, cos) = angle.sin_cos();
        builder.move_to(centre + point(px(cos * RAY_FROM), px(sin * RAY_FROM)));
        builder.line_to(centre + point(px(cos * RAY_TO), px(sin * RAY_TO)));
    }
    builder.build().ok()
}

/// Each label sits mid nautical zone, on a meridian tick, except ±12, which
/// share the zone split by the date line.
fn offset_marks() -> impl Iterator<Item = (f64, i8)> {
    let half = MERIDIAN_STEP / 4.0;
    (-11..=11)
        .map(|hours: i8| (f64::from(hours) * MERIDIAN_STEP, hours))
        .chain([(-180.0 + half, -12), (180.0 - half, 12)])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(got: Pixels, want: f32) -> bool {
        (f32::from(got) - want).abs() < 0.01
    }

    #[test]
    fn marks_each_offset_inside_its_nautical_zone() {
        let marks: Vec<(f64, i8)> = offset_marks().collect();
        assert_eq!(marks.len(), 25);
        for (lon, hours) in marks {
            let centre = f64::from(hours) * MERIDIAN_STEP;
            assert!(
                (lon - centre).abs() <= MERIDIAN_STEP / 2.0 && (-180.0..=180.0).contains(&lon),
                "{hours} is marked at {lon}"
            );
        }
    }

    #[test]
    fn takes_the_height_and_overflows_a_taller_area() {
        let rect = fit(size(px(400.), px(600.)));
        assert_eq!(rect.origin, point(px(0.), px(0.)));
        assert_eq!(rect.size.height, px(600.));
        assert!(
            close(rect.size.width, 600. * (360. / 140.)),
            "{rect:?} is not the map's aspect"
        );
    }

    #[test]
    fn fills_the_width_exactly_at_the_maps_own_shape() {
        let rect = fit(size(px(360.), px(140.)));
        assert_eq!(rect.origin, point(px(0.), px(0.)));
        assert!(
            close(rect.size.width, 360.),
            "{rect:?} is not the full width"
        );
        assert_eq!(rect.size.height, px(140.));
    }

    #[test]
    fn centres_across_the_width_when_the_height_is_the_limit() {
        let rect = fit(size(px(400.), px(100.)));
        assert_eq!(rect.origin.y, px(0.));
        assert_eq!(rect.size.height, px(100.));
        assert!(
            close(rect.size.width, 100. * (360. / 140.)),
            "{rect:?} is not the map's aspect"
        );
        assert!(
            close(rect.origin.x + rect.size.width / 2., 200.),
            "{rect:?} is not centred"
        );
    }
}
