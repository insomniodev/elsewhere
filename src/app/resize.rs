use gpui::{
    AnyElement, CursorStyle, Decorations, MouseButton, Pixels, ResizeEdge, Tiling, Window, div,
    prelude::*, px,
};

/// Narrower than the window padding, so nothing clickable is covered.
const EDGE: Pixels = px(4.);
/// Wider than an edge, so a corner is easy to hit.
const CORNER: Pixels = px(8.);

/// Stand in for the resize borders of the frame the window goes without.
/// None while maximized, and none along tiled sides.
pub(super) fn handles(window: &Window) -> Vec<AnyElement> {
    let tiling = match window.window_decorations() {
        Decorations::Client { tiling } if !window.is_maximized() => tiling,
        _ => return Vec::new(),
    };
    let Tiling {
        top,
        left,
        right,
        bottom,
    } = tiling;
    let free = |edge| match edge {
        ResizeEdge::Top => !top,
        ResizeEdge::Right => !right,
        ResizeEdge::Bottom => !bottom,
        ResizeEdge::Left => !left,
        ResizeEdge::TopLeft => !top && !left,
        ResizeEdge::TopRight => !top && !right,
        ResizeEdge::BottomRight => !bottom && !right,
        ResizeEdge::BottomLeft => !bottom && !left,
    };
    // The edges first, so the corners lie over their ends.
    [
        ResizeEdge::Top,
        ResizeEdge::Right,
        ResizeEdge::Bottom,
        ResizeEdge::Left,
        ResizeEdge::TopLeft,
        ResizeEdge::TopRight,
        ResizeEdge::BottomRight,
        ResizeEdge::BottomLeft,
    ]
    .into_iter()
    .filter(|edge| free(*edge))
    .map(|edge| handle(edge).into_any_element())
    .collect()
}

fn handle(edge: ResizeEdge) -> impl IntoElement {
    let strip = div().absolute();
    let strip = match edge {
        ResizeEdge::Top => strip.top_0().left(CORNER).right(CORNER).h(EDGE),
        ResizeEdge::Bottom => strip.bottom_0().left(CORNER).right(CORNER).h(EDGE),
        ResizeEdge::Left => strip.left_0().top(CORNER).bottom(CORNER).w(EDGE),
        ResizeEdge::Right => strip.right_0().top(CORNER).bottom(CORNER).w(EDGE),
        ResizeEdge::TopLeft => strip.top_0().left_0().size(CORNER),
        ResizeEdge::TopRight => strip.top_0().right_0().size(CORNER),
        ResizeEdge::BottomRight => strip.bottom_0().right_0().size(CORNER),
        ResizeEdge::BottomLeft => strip.bottom_0().left_0().size(CORNER),
    };
    let cursor = match edge {
        ResizeEdge::Top | ResizeEdge::Bottom => CursorStyle::ResizeUpDown,
        ResizeEdge::Left | ResizeEdge::Right => CursorStyle::ResizeLeftRight,
        ResizeEdge::TopLeft | ResizeEdge::BottomRight => CursorStyle::ResizeUpLeftDownRight,
        ResizeEdge::TopRight | ResizeEdge::BottomLeft => CursorStyle::ResizeUpRightDownLeft,
    };
    strip
        .cursor(cursor)
        .on_mouse_down(MouseButton::Left, move |_, window, _| {
            window.start_window_resize(edge)
        })
}
