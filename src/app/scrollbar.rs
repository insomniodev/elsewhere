use std::cell::Cell;

use gpui::{
    App, Axis, Bounds, DispatchPhase, ElementId, HitboxBehavior, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, Pixels, Point, RenderOnce, ScrollHandle, Window, canvas, fill,
    point, prelude::*, px, rgb, size,
};

use crate::theme;

/// The drawn thumb; the hit area around it is `HIT` wide.
const THUMB: Pixels = px(3.);
/// The thumb while the pointer is on it or it is dragged.
const THUMB_ACTIVE: Pixels = px(5.);
const HIT: Pixels = px(10.);
const MIN_THUMB: Pixels = px(20.);

/// Only a drag in progress and the hover outlive a frame.
#[derive(Default)]
struct State {
    /// Where on the thumb, along the axis, the drag took hold.
    grab: Cell<Option<Pixels>>,
    over_thumb: Cell<bool>,
}

/// A thumb to drag and a track to page by. `inset` is the offset from the
/// container's right edge, or bottom edge for a horizontal bar, of the drawn
/// thumb; negative hangs it outside, in the window padding, clear of the
/// content. The hit area is centred on the thumb, so it never swallows the
/// wheel: that goes to the content beneath, as before.
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
                let (dragging, over_thumb) = {
                    let state = state.read(cx);
                    (state.grab.get().is_some(), state.over_thumb.get())
                };
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
                window.paint_quad(
                    fill(thumb_bounds(thickness), rgb(theme::SURFACE1))
                        .corner_radii(thickness / 2.),
                );
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
                        if phase != DispatchPhase::Bubble
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
                        let over = hitbox.is_hovered(window) && grip.contains(&event.position);
                        if state.over_thumb.replace(over) != over {
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
