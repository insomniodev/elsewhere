//! Set flush left over the labels, under the date. A saved city's head
//! shows its remove button while hovered, on the offset's line, where there
//! is room for it.

use gpui::{Context, Pixels, Styled, div, prelude::*, px, rgb};
use jiff::Zoned;

use super::{COLUMN, Column, INSET, LABEL};
use crate::app::Elsewhere;
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
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        div().flex_none().h(HEAD).flex().flex_row().children(
            columns
                .iter()
                .map(|column| self.render_head(column, now, cx)),
        )
    }

    /// The name is all the room there is, so the tooltip carries the full
    /// label and what the zone is called today. The line under the name is
    /// hours from here, or "here" on here's own column.
    fn render_head(
        &self,
        column: &Column,
        now: &Zoned,
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
                    .text_color(rgb(theme::OVERLAY2))
                    .child(clock::shift_label(zone, now))
                    .into_any_element()
            })
        }
        .map(|line| shortened(div()).child(line));
        let head = div()
            .group(HEAD_GROUP)
            .flex_1()
            .min_w(COLUMN)
            .pl(INSET)
            .flex()
            .flex_col()
            .justify_end()
            .pb_2();
        let Some(geonameid) = column.geonameid else {
            return head
                .child(
                    shortened(div().id("head-here"))
                        .text_color(rgb(theme::SUBTEXT0))
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
        head.child(
            shortened(div().id(("head", geonameid as usize)))
                .text_color(rgb(theme::SUBTEXT0))
                .tooltip(Tooltip::text(tip))
                // As the list's row is washed, for a city just picked or
                // restored.
                .when(self.revealed == Some(geonameid), |name| {
                    name.text_color(rgb(theme::TEXT))
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
