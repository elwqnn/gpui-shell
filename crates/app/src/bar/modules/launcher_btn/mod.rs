//! Launcher button widget for opening the application launcher.

mod config;
pub use config::LauncherBtnConfig;

use crate::launcher;
use gpui::{AnyElement, Context, Window, prelude::*};
use ui::{Color, Icon, IconName, IconSource};

use super::{BarClick, BarWidget, style};
use crate::config::ActiveConfig;

/// A button widget that opens the launcher when clicked.
pub struct LauncherBtn;

const LAUNCHER_ICON: IconName = IconName::Layers;

impl LauncherBtn {
    /// Create a new launcher button.
    pub fn new(_cx: &mut Context<Self>) -> Self {
        LauncherBtn
    }

    fn render_button_content(icon: IconSource, is_vertical: bool) -> AnyElement {
        // The chip owns the hover, the padding and the click, so the button
        // is just its icon.
        Icon::new(icon)
            .size(style::icon(is_vertical))
            .color(Color::Default)
            .into_any_element()
    }
}

impl BarWidget for LauncherBtn {
    fn on_click(&self, _cx: &mut Context<Self>) -> Option<BarClick> {
        Some(Box::new(|_, _, cx| launcher::toggle(None, cx)))
    }

    fn render_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let config = &cx.config().bar.modules.launcher_btn;
        let icon = crate::icons::source_or(config.icon.as_ref(), LAUNCHER_ICON);
        Self::render_button_content(icon, true)
    }

    fn render_horizontal(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let config = &cx.config().bar.modules.launcher_btn;
        let icon = crate::icons::source_or(config.icon.as_ref(), LAUNCHER_ICON);
        Self::render_button_content(icon, false)
    }
}

impl Render for LauncherBtn {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_bar_widget(window, cx)
    }
}
