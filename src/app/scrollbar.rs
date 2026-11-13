use std::{
    cell::Cell,
    time::{Duration, Instant},
};

use gpui::{
    App, Axis, Bounds, DispatchPhase, ElementId, HitboxBehavior, Hsla, MouseButton, MouseDownEvent,
    MouseExitEvent, MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderOnce, ScrollHandle, Window,
    canvas, fill, point, prelude::*, px, rgb, size,
};

use crate::theme;

/// The drawn thumb; the hit area around it is `HIT` wide.
const THUMB: Pixels = px(3.);
/// The thumb while the pointer is on it or it is dragged.
const THUMB_ACTIVE: Pixels = px(5.);
const HIT: Pixels = px(10.);
const MIN_THUMB: Pixels = px(20.);

/// How long the bar stays after the last scroll, or after the pointer leaves
/// it, and how long it then takes to fade.
const LINGER: Duration = Duration::from_millis(1000);
const FADE: Duration = Duration::from_millis(200);
/// The thumb lies over content, so it is a light tint rather than a surface.
const REST: f32 = 0.45;
const ACTIVE: f32 = 0.75;

#[derive(Default)]
struct State {
    /// Where on the thumb, along the axis, the drag took hold.
    grab: Cell<Option<Pixels>>,
    over_thumb: Cell<bool>,
    /// Whether the pointer is on the bar's strip. Over what it scrolls is not
    /// enough: a view is mostly that, so the bar would never leave.
    over_area: Cell<bool>,
    /// The offset along the axis at the last paint; `None` before the first.
    last: Cell<Option<Pixels>>,
    /// When the bar last had a reason to show.
    active: Cell<Option<Instant>>,
}

/// A thumb to drag and a track to page by, drawn over the content it scrolls
/// and only while the bar is hovered, dragged or lately scrolled. `inset` is the
/// offset from the container's right edge, or bottom edge for a horizontal
/// bar, of the drawn thumb. The hit area is centred on the thumb, so it never
/// swallows the wheel: that goes to the content beneath, as before.
pub(super) fn scrollbar(
    id: impl Into<ElementId>,
    handle: &ScrollHandle,
    axis: Axis,
    inset: Pixels,
) -> Scrollbar {
    Scrollbar {
        id: id.into(),
        handle: handle.clone(),
        axis,
        inset,
    }
}

#[derive(IntoElement)]
pub(super) struct Scrollbar {
    id: ElementId,
    handle: ScrollHandle,
    axis: Axis,
    inset: Pixels,
}

/// The thumb's length and start along the track, or `None` without overflow.
fn thumb(handle: &ScrollHandle, axis: Axis, track: Pixels) -> Option<(Pixels, Pixels)> {
    let (overflow, viewport, scrolled) = match axis {
        Axis::Vertical => (
            handle.max_offset().y,
            handle.bounds().size.height,
            -handle.offset().y,
        ),
        Axis::Horizontal => (
            handle.max_offset().x,
            handle.bounds().size.width,
            -handle.offset().x,
        ),
    };
    if overflow <= px(0.) {
        return None;
    }
    let length = (track * (viewport / (viewport + overflow))).max(MIN_THUMB);
    let start = (track - length) * (scrolled / overflow).clamp(0., 1.);
    Some((length, start))
}

fn scroll_to(handle: &ScrollHandle, axis: Axis, scrolled: Pixels, window: &mut Window) {
    let max = handle.max_offset();
    let offset = handle.offset();
    handle.set_offset(match axis {
        Axis::Vertical => point(offset.x, -scrolled.clamp(px(0.), max.y)),
        Axis::Horizontal => point(-scrolled.clamp(px(0.), max.x), offset.y),
    });
    window.refresh();
}

impl RenderOnce for Scrollbar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let Scrollbar {
            id,
            handle,
            axis,
            inset,
        } = self;
        let state = window.use_keyed_state(id, cx, |_, _| State::default());
        // Centres the drawn thumb in the hit area.
        let inset = inset - (HIT - THUMB) / 2.;
        canvas(
            move |bounds, window, _cx| {
                (bounds, window.insert_hitbox(bounds, HitboxBehavior::Normal))
            },
            move |_bounds, (bounds, hitbox), window, cx| {
                let along = move |p: Point<Pixels>| match axis {
                    Axis::Vertical => p.y,
                    Axis::Horizontal => p.x,
                };
                let (origin, track) = match axis {
                    Axis::Vertical => (bounds.origin.y, bounds.size.height),
                    Axis::Horizontal => (bounds.origin.x, bounds.size.width),
                };
                let Some((length, start)) = thumb(&handle, axis, track) else {
                    return;
                };
                let scrolled = along(handle.offset());
                let now = Instant::now();
                let (dragging, over_thumb, opacity) = {
                    let state = state.read(cx);
                    // The first paint counts as a scroll, so a view opening
                    // with overflow shows its bar once.
                    if state.last.replace(Some(scrolled)) != Some(scrolled) {
                        if state.active.get().is_none() {
                            let over = bounds.contains(&window.mouse_position());
                            state.over_area.set(over);
                        }
                        state.active.set(Some(now));
                    }
                    let dragging = state.grab.get().is_some();
                    let opacity = if dragging || state.over_area.get() {
                        1.
                    } else {
                        let idle = state
                            .active
                            .get()
                            .map_or(FADE, |at| now.duration_since(at).saturating_sub(LINGER));
                        let opacity = 1. - (idle.as_secs_f32() / FADE.as_secs_f32()).min(1.);
                        // Nothing else repaints an idle window through the
                        // linger and the fade.
                        if opacity > 0. {
                            window.request_animation_frame();
                        }
                        opacity
                    };
                    (dragging, state.over_thumb.get(), opacity)
                };
                let shown = opacity > 0.;
                let thickness = if dragging || over_thumb {
                    THUMB_ACTIVE
                } else {
                    THUMB
                };
                let thumb_bounds = |thickness: Pixels| match axis {
                    Axis::Vertical => Bounds::new(
                        point(bounds.origin.x + (HIT - thickness) / 2., origin + start),
                        size(thickness, length),
                    ),
                    Axis::Horizontal => Bounds::new(
                        point(origin + start, bounds.origin.y + (HIT - thickness) / 2.),
                        size(length, thickness),
                    ),
                };
                if shown {
                    let strength = if dragging || over_thumb { ACTIVE } else { REST };
                    let color = Hsla::from(rgb(theme::OVERLAY2)).opacity(strength * opacity);
                    window.paint_quad(
                        fill(thumb_bounds(thickness), color).corner_radii(thickness / 2.),
                    );
                }
                // The whole width of the hit area, not just what is drawn.
                let grip = match axis {
                    Axis::Vertical => {
                        Bounds::new(point(bounds.origin.x, origin + start), size(HIT, length))
                    }
                    Axis::Horizontal => {
                        Bounds::new(point(origin + start, bounds.origin.y), size(length, HIT))
                    }
                };

                window.on_mouse_event({
                    let (state, handle, hitbox) = (state.clone(), handle.clone(), hitbox.clone());
                    move |event: &MouseDownEvent, phase, window, cx| {
                        // Out of sight, it is not there to press.
                        if !shown
                            || phase != DispatchPhase::Bubble
                            || event.button != MouseButton::Left
                            || !hitbox.is_hovered(window)
                        {
                            return;
                        }
                        cx.stop_propagation();
                        let state = state.read(cx);
                        if grip.contains(&event.position) {
                            state.grab.set(Some(along(event.position) - origin - start));
                            window.refresh();
                            return;
                        }
                        // A page is one viewport, toward the click.
                        let (viewport, scrolled) = match axis {
                            Axis::Vertical => (handle.bounds().size.height, -handle.offset().y),
                            Axis::Horizontal => (handle.bounds().size.width, -handle.offset().x),
                        };
                        let page = if along(event.position) < origin + start {
                            -viewport
                        } else {
                            viewport
                        };
                        scroll_to(&handle, axis, scrolled + page, window);
                    }
                });
                window.on_mouse_event({
                    let (state, handle) = (state.clone(), handle.clone());
                    move |event: &MouseMoveEvent, _phase, window, cx| {
                        let state = state.read(cx);
                        if let Some(grab) = state.grab.get() {
                            // The thumb follows the pointer; the content
                            // follows the thumb, by the overflow it spans.
                            let (overflow, free) = match axis {
                                Axis::Vertical => (handle.max_offset().y, track - length),
                                Axis::Horizontal => (handle.max_offset().x, track - length),
                            };
                            if free > px(0.) {
                                let to = (along(event.position) - origin - grab) / free;
                                scroll_to(&handle, axis, overflow * to.clamp(0., 1.), window);
                            }
                            return;
                        }
                        let over_area = hitbox.is_hovered(window);
                        let over = over_area && grip.contains(&event.position);
                        let left = state.over_area.replace(over_area) && !over_area;
                        if left {
                            state.active.set(Some(Instant::now()));
                        }
                        if state.over_thumb.replace(over) != over || left || over_area != shown {
                            window.refresh();
                        }
                    }
                });
                window.on_mouse_event({
                    let state = state.clone();
                    move |_: &MouseExitEvent, _phase, window, cx| {
                        let state = state.read(cx);
                        if state.over_area.replace(false) {
                            state.over_thumb.set(false);
                            state.active.set(Some(Instant::now()));
                            window.refresh();
                        }
                    }
                });
                window.on_mouse_event({
                    let state = state.clone();
                    move |event: &MouseUpEvent, _phase, window, cx| {
                        let state = state.read(cx);
                        if event.button == MouseButton::Left && state.grab.take().is_some() {
                            state.over_thumb.set(grip.contains(&event.position));
                            window.refresh();
                        }
                    }
                });
            },
        )
        .absolute()
        .map(|bar| match axis {
            Axis::Vertical => bar.top_1().bottom_1().right(inset).w(HIT),
            Axis::Horizontal => bar.left_1().right_1().bottom(inset).h(HIT),
        })
    }
}
