//! The world is painted in one canvas; the saved cities are divs laid over it.
mod labels;
mod pins;

use std::cell::Cell;
use std::rc::Rc;

use gpui::{
    Axis, Bounds, Context, CursorStyle, DispatchPhase, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Path, PathBuilder, Pixels, Point, ScrollDelta, ScrollEvent, ScrollHandle, Size,
    TextAlign, TextRun, canvas, div, fill, point, prelude::*, px, rgb, rgba, size,
};
use jiff::Zoned;

use super::footer::{BANNER_HEIGHT, FOOTER_HEIGHT};
use super::here::hint_over;
use super::scrollbar::scrollbar;
use super::{Elsewhere, GAP, PAD_BOTTOM};
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
/// The least clear space between two offset labels.
const OFFSET_ROOM: Pixels = px(3.);
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
/// gpui reports a wheel's click as three lines, as desktops scroll text.
const NOTCH_LINES: f32 = 3.0;

/// Where the map looks, kept between fits so that neither a resize nor the
/// banner reframes it. The height sets the scale, so a fit only turns the
/// map to its cities.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Frame {
    /// The x to put mid-viewport, as a share of the world's width.
    middle: f32,
}

/// A drag on the map's body: the pointer and the scroll offset at the press.
#[derive(Clone, Copy, Debug)]
pub(super) struct Pan {
    from: Point<Pixels>,
    offset: Point<Pixels>,
}

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
    pub(super) fn render_map(
        &self,
        saved: &[u32],
        now: &Zoned,
        empty: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let layout = self.map_layout.get();
        let panning = self.map_pan.get().is_some();
        div()
            .relative()
            .size_full()
            .child(
                div()
                    .id("map")
                    .size_full()
                    .overflow_x_scroll()
                    .track_scroll(&self.map_scroll)
                    // Otherwise gpui turns a plain wheel sideways here, which
                    // would leave the hint about scrubbing describing a pan;
                    // shift+wheel and a sideways wheel already scroll.
                    .map(|mut scroller| {
                        scroller.style().restrict_scroll_to_axis = Some(true);
                        scroller
                    })
                    .cursor(if panning {
                        CursorStyle::ClosedHand
                    } else {
                        CursorStyle::OpenHand
                    })
                    // Dots, times and the scrollbar stop the press first, so
                    // only the body pans.
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _window, cx| {
                            this.map_pan.set(Some(Pan {
                                from: event.position,
                                offset: this.map_scroll.offset(),
                            }));
                            cx.notify();
                        }),
                    )
                    .on_scroll(
                        cx.listener(|this, event: &ScrollEvent, _window, cx| this.wheel(event, cx)),
                    )
                    .child(
                        div()
                            .relative()
                            .h_full()
                            .min_w_full()
                            .when_some(layout, |content, layout| content.w(layout.rect.size.width))
                            .child(self.render_world(now))
                            .children(self.render_pins(saved, now, cx)),
                    ),
            )
            .child(pan(self.map_pan.clone(), self.map_scroll.clone()))
            // Along the map's own bottom edge rather than the region's, which
            // lies below it while the banner's room is empty.
            .when_some(layout.filter(Layout::overflows), |map, layout| {
                map.child(
                    div()
                        .absolute()
                        .left_0()
                        .right_0()
                        .top(layout.rect.origin.y)
                        .h(layout.rect.size.height)
                        .child(scrollbar(
                            "map-bar",
                            &self.map_scroll,
                            Axis::Horizontal,
                            px(2.),
                        )),
                )
            })
            .when(empty, |map| map.child(hint_over(div().inset_0())))
    }

    /// What a fit frames, as shares of the world's width: the saved cities and
    /// here's marker. Nothing with no city saved, which frames the world.
    fn framed(&self) -> Vec<f32> {
        if self.saved.is_empty() {
            return Vec::new();
        }
        self.here
            .place()
            .into_iter()
            .chain(self.saved.iter().copied())
            .filter_map(|geonameid| self.index.city(geonameid))
            .map(|city| {
                geo::project(LonLat {
                    lon: city.longitude,
                    lat: 0.0,
                })
                .0 as f32
            })
            .collect()
    }

    /// Ctrl scrubs. A plain wheel is too easy to turn by accident, reaching
    /// for a pan, so it does nothing; the scroller takes shift and sideways
    /// wheels as pans.
    fn wheel(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        if event.modifiers.control {
            self.scrub(event, cx);
        }
    }

    /// A notch down is a quarter hour later, the way the night moves. It is an
    /// ordinary pin, so everything follows it and escape undoes it.
    fn scrub(&mut self, event: &ScrollEvent, cx: &mut Context<Self>) {
        let notches = match event.delta {
            ScrollDelta::Lines(lines) => lines.y / NOTCH_LINES,
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
        let framed = self.framed();
        let frame = self.map_frame.clone();
        let sun = geo::subsolar(now);

        canvas(
            move |bounds, window, _cx| {
                // Fit to the region, not the canvas, which may already be
                // wider. The scroller has measured it by now.
                let region = scroll.bounds();
                let viewport = region.size;
                // The room under the search and over the banner or, unpinned,
                // the hint in its place. Taken from the window, so a notice
                // leaves the map alone too.
                let bottom = window.viewport_size().height - (FOOTER_HEIGHT + GAP + PAD_BOTTOM);
                let tallest = bottom - region.top() - (BANNER_HEIGHT + GAP);
                let level = tallest / 2.;
                let refit = frame.get().is_none() && viewport.width > px(0.);
                let framing = match frame.get() {
                    Some(framing) => framing,
                    None => frame_on(&framed),
                };
                if refit {
                    frame.set(Some(framing));
                }
                let rect = fit(viewport, tallest, level);
                let layout = Layout { viewport, rect };
                let before = remembered.get();
                if refit || before != Some(layout) {
                    remembered.set(Some(layout));
                    if layout.overflows() {
                        let middle = match before.filter(|before| !refit && before.overflows()) {
                            // Keep the middle longitude through the resize.
                            Some(before) => {
                                (before.viewport.width / 2. - scroll.offset().x)
                                    / before.rect.size.width
                            }
                            None => framing.middle,
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
                let lines: Vec<_> = offset_marks()
                    .map(|(lon, hours)| {
                        let text = clock::offset_label(jiff::tz::offset(hours));
                        let run = TextRun {
                            len: text.len(),
                            font: font.clone(),
                            color: rgb(theme::OVERLAY2).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        };
                        let line =
                            window
                                .text_system()
                                .shape_line(text.into(), OFFSET_SIZE, &[run], None);
                        (lon, hours, line)
                    })
                    .collect();
                // A short map is narrow too, with too little room for every
                // hour, so it marks every other one rather than run them
                // together.
                let widest = lines
                    .iter()
                    .map(|(_, _, line)| line.width)
                    .max()
                    .unwrap_or_default();
                let every = if rect.size.width / 24. < widest + OFFSET_ROOM {
                    2
                } else {
                    1
                };
                for (lon, hours, line) in lines {
                    if hours % every != 0 {
                        continue;
                    }
                    // ±12 straddle the map's ends, so they are kept inside.
                    let left = (at(LonLat { lon, lat: 0.0 }).x - line.width / 2.)
                        .clamp(rect.left(), (rect.right() - line.width).max(rect.left()));
                    let origin = point(left, rect.bottom() - TICK - OFFSET_GAP - OFFSET_LINE);
                    // A label that fails to paint is only a label missing.
                    line.paint(origin, OFFSET_LINE, TextAlign::Left, None, window, cx)
                        .ok();
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

/// Centres on `xs`, shares of the world's width: midway between the
/// outermost. Nothing to frame centres the world. The map is one unwrapped
/// world, so cities either side of the date line centre the arc between them
/// the long way round, as the map draws it.
fn frame_on(xs: &[f32]) -> Frame {
    let west = xs.iter().copied().reduce(f32::min);
    let east = xs.iter().copied().reduce(f32::max);
    let middle = match (west, east) {
        (Some(west), Some(east)) => (west + east) / 2.0,
        _ => 0.5,
    };
    Frame { middle }
}

/// The map is always `tallest`, the most height it may have, and as wide as
/// that makes it, so no window leaves it a strip; a narrow one scrolls it
/// sideways instead. `tallest` is the same with the banner or a notice up,
/// so neither rescales it. Vertically it centres on `level`, kept inside the
/// viewport; across, it is centred when narrower than the viewport and flush
/// left for the scroller when wider.
pub(super) fn fit(viewport: Size<Pixels>, tallest: Pixels, level: Pixels) -> Bounds<Pixels> {
    let height = tallest.max(px(1.));
    let width = height * geo::aspect() as f32;
    let top = (level - height / 2.).clamp(px(0.), (viewport.height - height).max(px(0.)));
    Bounds::new(
        point(((viewport.width - width) / 2.).max(px(0.)), top),
        size(width, height),
    )
}

/// Follows a pan anywhere in the window, so the map keeps up when the pointer
/// leaves it. Listens only while one is in progress.
fn pan(grab: Rc<Cell<Option<Pan>>>, scroll: ScrollHandle) -> impl IntoElement {
    canvas(
        |_bounds, _window, _cx| {},
        move |_bounds, (), window, _cx| {
            if grab.get().is_none() {
                return;
            }
            window.set_window_cursor_style(CursorStyle::ClosedHand);
            window.on_mouse_event({
                let grab = grab.clone();
                move |event: &MouseMoveEvent, phase, window, _cx| {
                    let Some(pan) = grab.get() else {
                        return;
                    };
                    if phase != DispatchPhase::Bubble {
                        return;
                    }
                    // The release happened where no listener saw it.
                    if event.pressed_button != Some(MouseButton::Left) {
                        grab.set(None);
                        window.refresh();
                        return;
                    }
                    let most = scroll.max_offset().x;
                    let x = (pan.offset.x + event.position.x - pan.from.x).clamp(-most, px(0.));
                    scroll.set_offset(point(x, pan.offset.y));
                    window.refresh();
                }
            });
            window.on_mouse_event(move |event: &MouseUpEvent, _phase, window, _cx| {
                if event.button == MouseButton::Left && grab.take().is_some() {
                    window.refresh();
                }
            });
        },
    )
    .absolute()
    .inset_0()
}

/// `None` for too few points or a path lyon cannot tessellate.
fn polygon(points: impl Iterator<Item = Point<Pixels>>) -> Option<Path> {
    let points: Vec<_> = points.collect();
    if points.len() < 3 {
        return None;
    }
    let mut builder = PathBuilder::fill();
    builder.add_polygon(&points, true);
    builder.build().ok()
}

/// `None` for too few points or a path lyon cannot tessellate.
fn polyline(points: impl Iterator<Item = Point<Pixels>>) -> Option<Path> {
    let points: Vec<_> = points.collect();
    if points.len() < 2 {
        return None;
    }
    let mut builder = PathBuilder::stroke(ZONE_LINE_WIDTH);
    builder.add_polygon(&points, false);
    builder.build().ok()
}

fn rays(centre: Point<Pixels>) -> Option<Path> {
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
    fn centres_midway_between_the_outermost_cities() {
        assert!((frame_on(&[0.25, 0.4, 0.75]).middle - 0.5).abs() < 1e-6);
        assert!((frame_on(&[0.4]).middle - 0.4).abs() < 1e-6);
    }

    #[test]
    fn centres_the_world_with_no_cities() {
        assert_eq!(frame_on(&[]).middle, 0.5);
    }

    #[test]
    fn fills_the_height_in_a_narrow_window() {
        let rect = fit(size(px(400.), px(440.)), px(400.), px(200.));
        assert!(close(rect.size.height, 400.), "{rect:?}");
        assert!(close(rect.size.width, 400. * (360. / 140.)), "{rect:?}");
        // Flush left, for the scroller.
        assert_eq!(rect.origin, point(px(0.), px(0.)));
    }

    #[test]
    fn a_shorter_viewport_does_not_rescale_it() {
        let tall = fit(size(px(400.), px(600.)), px(500.), px(250.));
        let short = fit(size(px(400.), px(560.)), px(500.), px(250.));
        assert_eq!(tall, short);
    }

    #[test]
    fn centres_across_a_window_wider_than_the_tallest_map() {
        let rect = fit(size(px(800.), px(200.)), px(100.), px(100.));
        assert!(close(rect.size.height, 100.), "{rect:?}");
        assert!(
            close(rect.origin.x + rect.size.width / 2., 400.),
            "{rect:?} is not centred"
        );
        assert!(close(rect.origin.y, 50.), "{rect:?}");
    }

    #[test]
    fn stays_inside_the_viewport_when_its_level_is_off_centre() {
        let rect = fit(size(px(400.), px(300.)), px(200.), px(20.));
        assert_eq!(rect.origin.y, px(0.));
        let rect = fit(size(px(400.), px(300.)), px(200.), px(290.));
        assert_eq!(rect.origin.y, px(100.));
    }
}
