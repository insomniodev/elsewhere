use gpui::{
    App, Context, Div, Entity, Pixels, Point, Window, div, prelude::*, rgb, transparent_black,
};

use super::Elsewhere;
use crate::saved;
use crate::theme;

/// The row that follows the cursor during a drag.
pub(super) struct DragRow {
    pub(super) geonameid: u32,
    pub(super) label: String,
}

impl Render for DragRow {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(rgb(theme::MANTLE))
            .text_color(rgb(theme::SUBTEXT0))
            .opacity(0.9)
            .shadow_md()
            .child(self.label.clone())
    }
}

/// A transparent top border that lights up as the insertion line while
/// `open`, and is worn even closed so the layout does not change with the
/// view. The caller attaches the `on_drop`, and may color the border at rest.
pub(super) fn drop_target(element: Div, open: bool) -> Div {
    element
        .border_t_1()
        .border_color(transparent_black())
        .when(open, |element| {
            element.drag_over::<DragRow>(|style, _, _, _| style.border_color(rgb(theme::BLUE)))
        })
}

impl Elsewhere {
    /// Runs once a drag actually starts, noting the row in flight so the next
    /// frame shows the bin. Shared by the list's rows, the map's dots and the
    /// grid's heads.
    pub(super) fn drag_preview(
        cx: &mut Context<Self>,
    ) -> impl Fn(&DragRow, Point<Pixels>, &mut Window, &mut App) -> Entity<DragRow> + 'static {
        let this = cx.weak_entity();
        move |row, _position, _window, cx| {
            this.update(cx, |this, cx| {
                this.drag = Some(row.geonameid);
                cx.notify();
            })
            .ok();
            cx.new(|_cx| DragRow {
                geonameid: row.geonameid,
                label: row.label.clone(),
            })
        }
    }

    /// `None` lands the row at the end.
    pub(super) fn drop(&mut self, dragged: u32, before: Option<u32>, cx: &mut Context<Self>) {
        self.drag = None;
        if saved::reorder(&mut self.saved, dragged, before) {
            self.persist();
        }
        cx.notify();
    }

    /// The space below the list catches a drop meant for the end.
    pub(super) fn render_drop_tail(&self, cx: &mut Context<Self>) -> impl IntoElement {
        drop_target(div().flex_1().min_h_4(), true).on_drop(
            cx.listener(|this, row: &DragRow, _window, cx| this.drop(row.geonameid, None, cx)),
        )
    }
}
