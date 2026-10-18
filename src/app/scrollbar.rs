use gpui::{Axis, Bounds, Pixels, ScrollHandle, canvas, fill, point, prelude::*, px, rgb, size};

use crate::theme;

/// Display-only; the wheel scrolls. Reads the handle at paint time, after
/// layout has settled, so it keeps no state. `inset` is the offset from the
/// container's right edge, or bottom edge for a horizontal bar; negative hangs
/// it outside, in the window padding, clear of the content.
pub(super) fn scrollbar(handle: &ScrollHandle, axis: Axis, inset: Pixels) -> impl IntoElement {
    let scroll = handle.clone();
    canvas(
        move |_bounds, _window, _cx| scroll,
        move |bounds, scroll, window, _cx| {
            let (overflow, viewport, scrolled, track) = match axis {
                Axis::Vertical => (
                    scroll.max_offset().height,
                    scroll.bounds().size.height,
                    -scroll.offset().y,
                    bounds.size.height,
                ),
                Axis::Horizontal => (
                    scroll.max_offset().width,
                    scroll.bounds().size.width,
                    -scroll.offset().x,
                    bounds.size.width,
                ),
            };
            if overflow <= px(0.) {
                return;
            }
            let mut thumb = track * (viewport / (viewport + overflow));
            if thumb < px(20.) {
                thumb = px(20.);
            }
            let along = (track - thumb) * (scrolled / overflow);
            let thumb = match axis {
                Axis::Vertical => Bounds::new(
                    point(bounds.origin.x, bounds.origin.y + along),
                    size(bounds.size.width, thumb),
                ),
                Axis::Horizontal => Bounds::new(
                    point(bounds.origin.x + along, bounds.origin.y),
                    size(thumb, bounds.size.height),
                ),
            };
            window.paint_quad(fill(thumb, rgb(theme::SURFACE1)).corner_radii(px(1.5)));
        },
    )
    .absolute()
    .map(|bar| match axis {
        Axis::Vertical => bar.top_1().bottom_1().right(inset).w(px(3.)),
        Axis::Horizontal => bar.left_1().right_1().bottom(inset).h(px(3.)),
    })
}
