//! Pages that take the window over from the views. The search, the view and
//! the footer give way to the page, which leads with a way back.

use gpui::{
    AnyElement, Axis, Context, Div, FocusHandle, Focusable, ScrollHandle, SharedString, Stateful,
    Window, WindowDecorations, div, prelude::*, px, rgb, rgba, svg, transparent_black,
};

use super::Elsewhere;
use super::button::Button;
use super::focus::keyboard_only;
use super::scrollbar::scrollbar;
use super::titlebar;
use crate::theme::{self, Mode};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Page {
    Settings,
}

/// The theme's segments, left to right.
const THEMES: [(Mode, &str); 3] = [
    (Mode::System, "system"),
    (Mode::Light, "light"),
    (Mode::Dark, "dark"),
];

const REPOSITORY: &str = env!("CARGO_PKG_REPOSITORY");

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
        stops.extend(self.focus.themes.iter().cloned());
        if titlebar::FRAME_CHOICE {
            stops.push(self.focus.decorations.clone());
        }
        stops.extend([
            self.focus.repository.clone(),
            self.focus.third_party.clone(),
        ]);
        stops
    }

    fn set_theme(&mut self, mode: Mode, window: &mut Window, cx: &mut Context<Self>) {
        self.theme = mode;
        cx.set_window_appearance(mode.appearance());
        theme::show(mode, theme::desktop(window.appearance()));
        // The palette is read while rendering, so every view redraws.
        window.refresh();
        self.persist_settings(window, cx);
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
                                .text_color(rgb(theme::palette().text_faint)),
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
        let frame = titlebar::FRAME_CHOICE.then(|| {
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
        let groups = [
            section("appearance")
                .child(self.render_theme_row(cx))
                .children(frame)
                .into_any_element(),
            section("about")
                .child(
                    div()
                        .px_2()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child("elsewhere")
                        .child(
                            div()
                                .text_sm()
                                .text_color(rgb(theme::palette().text_muted))
                                .child(env!("CARGO_PKG_DESCRIPTION").to_lowercase()),
                        )
                        .child(
                            div()
                                .text_xs()
                                .text_color(rgb(theme::palette().text_faint))
                                .child(format!("version {version}")),
                        ),
                )
                .child(link(
                    "repository",
                    "source code",
                    REPOSITORY.into(),
                    &self.focus.repository,
                ))
                .child(link(
                    "third-party",
                    "third-party licenses",
                    format!("{REPOSITORY}/blob/main/third_party_licenses.md").into(),
                    &self.focus.third_party,
                ))
                .into_any_element(),
        ];
        let mut page = div().flex().flex_col().gap_4().pb_2();
        for (i, group) in groups.into_iter().enumerate() {
            if i > 0 {
                page = page.child(div().h(px(1.)).bg(rgb(theme::palette().surface)));
            }
            page = page.child(group);
        }
        page.into_any_element()
    }

    /// Light, dark, or as the desktop is: segments like the view switch's.
    fn render_theme_row(&self, cx: &mut Context<Self>) -> impl IntoElement {
        // The framed rows' inset, frame included, so the text lines up.
        div()
            .px(px(7.))
            .border_1()
            .border_color(transparent_black())
            .flex()
            .flex_row()
            .items_center()
            .justify_between()
            .gap_3()
            .py_1p5()
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(div().text_sm().child("theme"))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(theme::palette().text_faint))
                            .child("light, dark, or as the desktop is"),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .flex()
                    .flex_row()
                    .rounded_md()
                    .bg(rgb(theme::palette().panel))
                    .text_sm()
                    .children(THEMES.iter().zip(&self.focus.themes).enumerate().map(
                        |(i, (&(mode, label), focus))| {
                            Button::label("theme", label)
                                .key(i)
                                .focus(focus.clone())
                                .on(self.theme == mode)
                                .current(self.theme == mode)
                                .wash(rgba(theme::palette().wash))
                                .on_click(cx.listener(move |this, _event, window, cx| {
                                    this.set_theme(mode, window, cx)
                                }))
                                .build()
                        },
                    )),
            )
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
                            .text_color(rgb(theme::palette().text_faint))
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
            .focus(|style| style.border_color(rgb(theme::palette().blue)))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(theme::palette().surface))),
    )
}

/// A heading over a group of settings.
fn section(title: &'static str) -> Div {
    div().flex().flex_col().gap_2().child(
        div()
            .px_2()
            .text_xs()
            .text_color(rgb(theme::palette().text_faint))
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
        .bg(rgb(if on {
            theme::palette().blue
        } else {
            theme::palette().surface_strong
        }))
        .when(on, |track| track.justify_end())
        .child(div().size(px(12.)).rounded_full().bg(rgb(if on {
            theme::palette().bg
        } else {
            theme::palette().text_faint
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
            .text_color(rgb(theme::palette().blue))
            .on_click(move |_event, _window, cx| cx.open_url(&url))
            .child(format!("{label} ↗")),
    )
}
