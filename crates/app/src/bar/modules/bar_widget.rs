use gpui::{AnyElement, Context, IntoElement, Window, prelude::FluentBuilder};

use super::style;
use crate::config::ActiveConfig;
use ui::ActiveTheme;
use ui::patterns::BarChip;
pub(crate) use ui::patterns::BarClick;

#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BarWidgetShell {
    None,
    Standard,
    Group,
}

/// Shared rendering contract for widgets that live in the bar.
///
/// Widgets provide vertical and horizontal content variants. The trait owns
/// the bar-specific shell so compact spacing and chrome stay consistent.
pub(crate) trait BarWidget: Sized {
    fn render_vertical(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement;

    fn render_horizontal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement;

    /// The chip is the hit target: a widget that answers clicks hands its
    /// handler up rather than wrapping its content in a button, so the chip's
    /// padding is not a dead zone around it.
    fn on_click(&self, _cx: &mut Context<Self>) -> Option<BarClick> {
        None
    }

    fn shell(&self) -> BarWidgetShell {
        BarWidgetShell::Standard
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if cx.config().bar.is_vertical() {
            self.render_vertical(window, cx)
        } else {
            self.render_horizontal(window, cx)
        }
    }

    fn wrap_bar_shell(&mut self, cx: &mut Context<Self>, content: AnyElement) -> AnyElement {
        let shell = self.shell();
        if shell == BarWidgetShell::None {
            return content;
        }

        let on_click = self.on_click(cx);
        let theme = cx.theme();
        let bar = &cx.config().bar;
        let is_vertical = bar.is_vertical();

        // One chip per widget, and every widget is its own view, so a fixed id
        // is unique where it has to be.
        BarChip::new("bar-chip", content)
            .vertical(is_vertical)
            .grouped(shell == BarWidgetShell::Group)
            .background(style::widget_background(theme, bar))
            .border(
                bar.widget_border
                    .then(|| style::widget_border(theme, bar, is_vertical)),
            )
            .hover(
                on_click
                    .is_some()
                    .then(|| style::widget_hover_background(theme, bar)),
            )
            .when_some(on_click, |chip, handler| chip.on_click(handler))
            .into_any_element()
    }

    fn render_bar_widget(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let content = self.render_content(window, cx);
        self.wrap_bar_shell(cx, content)
    }
}
