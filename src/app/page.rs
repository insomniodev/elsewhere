//! Pages that take the window over from the views. The search, the view and
//! the footer give way to the page, which leads with a way back.

use gpui::{
    AnyElement, Axis, Context, Div, FocusHandle, Focusable, ScrollHandle, SharedString, Stateful,
    Window, WindowDecorations, div, prelude::*, px, rgb, svg, transparent_black,
};

use super::Elsewhere;
use super::focus::keyboard_only;
use super::scrollbar::scrollbar;
use super::titlebar;
use crate::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Settings,
}

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");
const LICENSE: &str = include_str!("../../license.md");

impl Elsewhere {
    pub(super) fn open_page(&mut self, page: Page, window: &mut Window, cx: &mut Context<Self>) {
        // The editor's box goes with the view.
        self.close_editor(window, cx);
        self.page = Some(page);
        self.page_scroll = ScrollHandle::new();
        // The page itself, so nothing wears a ring after a click.
        window.focus(&self.focus.page.clone(), cx);
        cx.notify();
    }

    pub(super) fn close_page(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.page.take().is_some() {
            window.focus(&self.picker.focus_handle(cx), cx);
            cx.notify();
        }
    }

    /// The page's stops, in the order they show.
    pub(super) fn page_stops(&self) -> Vec<FocusHandle> {
        let mut stops = vec![self.focus.page.clone(), self.focus.back.clone()];
        if titlebar::FRAME_CHOICE {
            stops.push(self.focus.decorations.clone());
        }
        stops.extend([
            self.focus.repository.clone(),
            self.focus.third_party.clone(),
        ]);
        stops
    }

    fn toggle_frame(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.system_frame = !self.system_frame;
        window.request_decorations(if self.system_frame {
            WindowDecorations::Server
        } else {
            WindowDecorations::Client
        });
        self.persist_settings(window, cx);
        cx.notify();
    }

    pub(super) fn render_page(&self, page: Page, cx: &mut Context<Self>) -> impl IntoElement {
        let body = match page {
            Page::Settings => self.render_settings(cx),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_3()
            .track_focus(&self.focus.page)
            // Framed like the page's other controls, so the arrow lines up
            // with the text below it.
            .child(
                div().flex().child(
                    framed(div().id("back").track_focus(&self.focus.back))
                        .h(px(24.))
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .on_click(
                            cx.listener(|this, _event, window, cx| this.close_page(window, cx)),
                        )
                        .child(
                            svg()
                                .path("icons/arrow-left.svg")
                                .size(px(16.))
                                // The glyph starts a few pixels into its box.
                                .ml(px(-3.))
                                .text_color(rgb(theme::OVERLAY2)),
                        )
                        .child(div().text_sm().child("go back")),
                ),
            )
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        div()
                            .id("page")
                            .size_full()
                            .flex()
                            .flex_col()
                            .gap_6()
                            .overflow_y_scroll()
                            .track_scroll(&self.page_scroll)
                            .child(body),
                    )
                    .child(scrollbar(
                        "page-bar",
                        &self.page_scroll,
                        Axis::Vertical,
                        px(2.),
                    )),
            )
    }

    fn render_settings(&self, cx: &mut Context<Self>) -> AnyElement {
        let version = env!("CARGO_PKG_VERSION");
        // One group so far, so it goes without a heading.
        let window = titlebar::FRAME_CHOICE.then(|| {
            self.render_switch_row(
                "decorations",
                "system title bar",
                "use the desktop's title bar and borders",
                self.system_frame,
                &self.focus.decorations,
                Self::toggle_frame,
                cx,
            )
            .into_any_element()
        });
        let groups = window.into_iter().chain([
            section("about")
                .child(
                    div()
                        .px_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(
                            div()
                                .flex()
                                .flex_row()
                                .items_baseline()
                                .gap_2()
                                .child("elsewhere")
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(theme::OVERLAY2))
                                        .child(format!("version {version}")),
                                ),
                        )
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme::SUBTEXT0))
                                .child(env!("CARGO_PKG_DESCRIPTION").to_lowercase()),
                        ),
                )
                .child(link(
                    "repository",
                    "source code",
                    REPOSITORY.into(),
                    &self.focus.repository,
                ))
                .into_any_element(),
            section("license")
                .child(
                    div()
                        .px_2()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .text_xs()
                        .text_color(rgb(theme::SUBTEXT0))
                        .children(paragraphs(LICENSE).map(|p| div().child(p))),
                )
                .child(link(
                    "third-party",
                    "third-party licenses",
                    format!("{REPOSITORY}/blob/main/third_party_licenses.md").into(),
                    &self.focus.third_party,
                ))
                .into_any_element(),
        ]);
        let mut page = div().flex().flex_col().gap_4().pb_2();
        for (i, group) in groups.enumerate() {
            if i > 0 {
                page = page.child(div().h(px(1.)).bg(rgb(theme::SURFACE0)));
            }
            page = page.child(group);
        }
        page.into_any_element()
    }

    /// A setting that is on or off: the whole row flips it.
    #[allow(clippy::too_many_arguments)]
    fn render_switch_row(
        &self,
        id: &'static str,
        label: &'static str,
        detail: &'static str,
        on: bool,
        focus: &FocusHandle,
        flip: fn(&mut Self, &mut Window, &mut Context<Self>),
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        framed(div().id(id).track_focus(focus))
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .py_1p5()
            .on_click(cx.listener(move |this, _event, window, cx| flip(this, window, cx)))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child(label))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::OVERLAY2))
                            .child(detail),
                    ),
            )
            .child(switch(on))
    }
}

/// What can be clicked or focused on a page: its text inset like the rest of
/// the page's, and its frame drawn always, so the focus ring takes no room.
fn framed(element: Stateful<Div>) -> Stateful<Div> {
    keyboard_only(
        element
            // With the border, the page's `px_2` inset.
            .px(px(7.))
            .rounded_md()
            .border_1()
            .border_color(transparent_black())
            .focus(|style| style.border_color(rgb(theme::BLUE)))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme::SURFACE0))),
    )
}

/// A heading over a group of settings.
fn section(title: &'static str) -> Div {
    div().flex().flex_col().gap_2().child(
        div()
            .px_2()
            .text_xs()
            .text_color(rgb(theme::OVERLAY2))
            .child(title),
    )
}

/// An on/off pill, drawn from the row's state.
fn switch(on: bool) -> impl IntoElement {
    div()
        .flex_none()
        .flex()
        .items_center()
        .w(px(28.))
        .h(px(16.))
        .p(px(2.))
        .rounded_full()
        .bg(rgb(if on { theme::BLUE } else { theme::SURFACE1 }))
        .when(on, |track| track.justify_end())
        .child(div().size(px(12.)).rounded_full().bg(rgb(if on {
            theme::BASE
        } else {
            theme::OVERLAY2
        })))
}

/// Opens `url` in the browser.
fn link(
    id: &'static str,
    label: &'static str,
    url: SharedString,
    focus: &FocusHandle,
) -> impl IntoElement {
    div().flex().child(
        framed(div().id(id).track_focus(focus))
            .py_0p5()
            .text_sm()
            .text_color(rgb(theme::BLUE))
            .on_click(move |_event, _window, cx| cx.open_url(&url))
            .child(format!("{label} ↗")),
    )
}

/// The license as written is wrapped for a terminal; each paragraph rejoined
/// so it wraps to the window instead.
fn paragraphs(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_license_rejoins_into_paragraphs() {
        let text = "MIT License\n\nline one\nline two\n\n\nlast\n";
        assert_eq!(
            paragraphs(text).collect::<Vec<_>>(),
            ["MIT License", "line one line two", "last"]
        );
    }
}
