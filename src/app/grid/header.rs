//! A head's name is its drag handle, as the map's dot is. Set flush right over
//! the labels, as a calendar sets its zone names.

use gpui::{Context, Pixels, Styled, div, prelude::*, px, rgb};
use jiff::Zoned;

use super::{COLUMN, Column, INSET, LABEL};
use crate::app::Elsewhere;
use crate::app::drag::DragRow;
use crate::app::tooltip::Tooltip;
use crate::clock;
use crate::theme;

/// Fixed, so a column with no offset line keeps the rules level, and the
/// hours' scrollbar knows where to start.
pub(super) const HEAD: Pixels = px(40.);

impl Elsewhere {
    pub(super) fn render_heads(
        &self,
        columns: &[Column],
        now: &Zoned,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div().flex_none().h(HEAD).flex().flex_row().children(
            columns
                .iter()
                .map(|column| self.render_head(column, now, dragging, cx)),
        )
    }

    /// The name is all the room there is, so the tooltip carries the full
    /// label and what the zone is called today.
    fn render_head(
        &self,
        column: &Column,
        now: &Zoned,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let offset = column.zone.as_ref().map(|zone| {
            shortened(div())
                .text_color(rgb(theme::OVERLAY0))
                .child(clock::offset_label(zone.to_offset(now.timestamp())))
        });
        let head = div()
            .flex_1()
            .min_w(COLUMN)
            .pr(INSET)
            .flex()
            .flex_col()
            .justify_end()
            .pb_2();
        let Some(geonameid) = column.geonameid else {
            let tip = column
                .zone
                .as_ref()
                .map(|zone| clock::zone_detail(zone, now))
                .unwrap_or_default();
            return head
                .child(
                    shortened(div().id("head-here"))
                        .text_color(rgb(theme::OVERLAY0))
                        .tooltip(Tooltip::text(tip))
                        .child("here"),
                )
                .children(offset);
        };
        let Some(city) = self.index.city(geonameid) else {
            return head;
        };
        let label = self.index.label(city);
        let tip = match &column.zone {
            Some(zone) => format!("{label} · {}", clock::zone_detail(zone, now)),
            None => label.clone(),
        };
        head.child(
            shortened(div().id(("head", geonameid as usize)))
                .text_color(rgb(theme::SUBTEXT0))
                .tooltip(Tooltip::text(tip))
                .cursor_grab()
                .on_drag(DragRow { geonameid, label }, Self::drag_preview(cx))
                .when(dragging && self.drag == Some(geonameid), |name| {
                    name.opacity(0.4)
                })
                .child(city.name.clone()),
        )
        .children(offset)
    }
}

/// `truncate` with the text left wrappable. gpui keeps a nowrap line's first
/// shaping for the frame, from a taffy sizing pass at zero or full width, so
/// the ellipsis would land in the wrong place.
fn shortened<T: Styled>(line: T) -> T {
    line.text_ellipsis()
        .line_clamp(1)
        .text_size(LABEL)
        .text_right()
}
