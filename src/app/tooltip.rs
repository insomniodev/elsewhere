//! gpui shows whatever view a tooltip builder returns; this is the one all of
//! ours use.

use gpui::{AnyView, App, Render, SharedString, Window, div, prelude::*, rgb};

use crate::theme;

pub(super) struct Tooltip {
    text: SharedString,
}

impl Tooltip {
    pub(super) fn text(
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |_window, cx| {
            let text = text.clone();
            cx.new(|_| Tooltip { text }).into()
        }
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, _cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .px_1p5()
            .py_0p5()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme::SURFACE1))
            .bg(rgb(theme::MANTLE))
            .text_xs()
            .text_color(rgb(theme::SUBTEXT0))
            .child(self.text.clone())
    }
}
