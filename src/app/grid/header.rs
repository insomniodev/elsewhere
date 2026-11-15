//! Set flush left over the labels, under the date. A saved city's head
//! shows its remove button while hovered, on the offset's line, where there
//! is room for it.

use gpui::{Context, ElementId, Pixels, Styled, div, prelude::*, px, rgb};
use jiff::Zoned;

use super::{COLUMN, Column, INSET, LABEL};
use crate::app::Elsewhere;
use crate::app::drag::{DragRow, Edge, drop_target};
use crate::app::tooltip::Tooltip;
use crate::clock;
use crate::theme;

/// The date line over the heads: out of the scroller so it stays in the
/// corner and never widens the content.
pub(super) const DATE: Pixels = px(18.);

/// Fixed, so a column with no offset line keeps the rules level, and the
/// hours' scrollbar knows where to start. The heads sit at the bottom, under
/// the date line, with a gap between so the date reads as the grid's title
/// rather than a third line of the first head.
pub(super) const HEAD: Pixels = px(66.);

/// What a head's remove button shows on the hover of.
const HEAD_GROUP: &str = "head";

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
    /// label and what the zone is called today. The line under the name is
    /// hours from here, or "here" on here's own column.
    ///
    /// A saved city's head is its column's drag handle, in the hand-arranged
    /// order; the hours are not, as a click on them pins.
    fn render_head(
        &self,
        column: &Column,
        now: &Zoned,
        dragging: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let here = column.geonameid.is_none() || column.geonameid == self.here.city;
        let offset = if here {
            Some(
                self.here_tag("head-here-tag", now)
                    .text_size(LABEL)
                    .into_any_element(),
            )
        } else {
            column.zone.as_ref().map(|zone| {
                div()
                    .text_color(rgb(theme::palette().text_faint))
                    .child(clock::shift_label(zone, now))
                    .into_any_element()
            })
        }
        .map(|line| shortened(div()).child(line));
        let open = !self.by_clock;
        let id = match column.geonameid {
            Some(geonameid) => ElementId::from(("head-column", geonameid as usize)),
            None => ElementId::from("head-column-here"),
        };
        let head = drop_target(div(), Edge::Left, open)
            .id(id)
            .group(HEAD_GROUP)
            .flex_1()
            .min_w(COLUMN)
            .pl(INSET)
            .flex()
            .flex_col()
            .justify_end()
            .pb_2()
            .when(open, |head| match column.before {
                Some(before) => {
                    head.on_drop(cx.listener(move |this, row: &DragRow, _window, cx| {
                        this.drop(row.geonameid, Some(before), cx)
                    }))
                }
                None => head,
            });
        let Some(geonameid) = column.geonameid else {
            return head
                .child(
                    shortened(div().id("head-here"))
                        .text_color(rgb(theme::palette().text_muted))
                        .tooltip(Tooltip::text(self.here_detail(now)))
                        .child(self.here.name.clone()),
                )
                .children(offset);
        };
        let Some(city) = self.index.city(geonameid) else {
            return head;
        };
        let label = self.index.label(city);
        let tip = clock::city_detail(&label, column.zone.as_ref(), now);
        let label_for_drag = label.clone();
        head.when(open, |head| {
            head.cursor_grab().on_drag(
                DragRow {
                    geonameid,
                    label: label_for_drag,
                },
                Self::drag_preview(cx),
            )
        })
        .when(dragging && self.drag == Some(geonameid), |head| {
            head.opacity(0.4)
        })
        .child(
            shortened(div().id(("head", geonameid as usize)))
                .text_color(rgb(theme::palette().text_muted))
                .tooltip(Tooltip::text(tip))
                // As the list's row is washed, for a city just picked or
                // restored.
                .when(self.revealed == Some(geonameid), |name| {
                    name.text_color(rgb(theme::palette().text))
                })
                .child(city.name.clone()),
        )
        .child(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_1()
                .children(offset)
                .child(self.render_remove_small(geonameid, HEAD_GROUP, cx)),
        )
    }
}

/// `truncate` with the text left wrappable. gpui keeps a nowrap line's first
/// shaping for the frame, from a taffy sizing pass at zero or full width, so
/// the ellipsis would land in the wrong place.
fn shortened<T: Styled>(line: T) -> T {
    line.text_ellipsis().line_clamp(1).text_size(LABEL)
}
