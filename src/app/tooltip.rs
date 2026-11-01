//! gpui shows whatever view a tooltip builder returns; this is the one all of
//! ours use.

use gpui::{AnyView, App, Entity, Render, SharedString, Window, div, prelude::*, rgb};

use crate::theme;

pub(super) struct Tooltip {
    text: Box<dyn Fn(&App) -> SharedString>,
}

impl Tooltip {
    pub(super) fn text(
        text: impl Into<SharedString>,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        let text = text.into();
        move |_window, cx| {
            let text = text.clone();
            cx.new(|_| Tooltip {
                text: Box::new(move |_| text.clone()),
            })
            .into()
        }
    }

    /// Reads its text from `owner` on every frame, since gpui builds the view
    /// once per hover and a click can change what the control would say.
    pub(super) fn live<T: 'static>(
        owner: Entity<T>,
        text: impl Fn(&T) -> SharedString + Clone + 'static,
    ) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
        move |_window, cx| {
            let (owner, text) = (owner.clone(), text.clone());
            cx.new(|_| Tooltip {
                text: Box::new(move |cx| text(owner.read(cx))),
            })
            .into()
        }
    }
}

impl Render for Tooltip {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .px_1p5()
            .py_0p5()
            .rounded_md()
            .border_1()
            .border_color(rgb(theme::SURFACE1))
            .bg(rgb(theme::MANTLE))
            .text_xs()
            .text_color(rgb(theme::SUBTEXT0))
            .child((self.text)(cx))
    }
}
