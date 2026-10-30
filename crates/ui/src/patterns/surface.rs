//! Chrome for the shell's floating surfaces.

use gpui::{App, Styled};

use crate::{ActiveTheme, Radius, Spacing};

/// The inset every panel a bar module opens paints its content in.
pub const PANEL_PADDING: Spacing = Spacing::Large;
/// The gap between a panel's blocks.
pub const PANEL_GAP: Spacing = Spacing::Medium;

/// The two surface levels the shell paints with.
pub trait PanelSurface: Styled + Sized {
    /// A floating panel: the control center, a bar popup, a notification.
    fn panel_surface(self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        self.bg(colors.background)
            .border_1()
            .border_color(colors.border)
            .rounded(Radius::Large.pixels())
    }

    /// The panel a bar module opens: sysinfo, mpris, the notification centre,
    /// the control center and its settings all wear this, so they read as one
    /// surface - same inset, same gap between blocks, no title bar.
    ///
    /// `background` is the config's: turned off, the panel keeps its layout
    /// and drops its own fill, leaving whatever the compositor paints behind.
    fn bar_panel(self, background: bool, cx: &App) -> Self {
        let panel = self.p(PANEL_PADDING.pixels()).gap(PANEL_GAP.pixels());

        match background {
            true => panel.panel_surface(cx),
            false => panel.rounded(Radius::Large.pixels()),
        }
    }

    /// A card *inside* a panel - one slider, one expanded section.
    fn panel_card(self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        self.bg(colors.surface_background)
            .border_1()
            .border_color(colors.border_variant)
            .rounded(Radius::Medium.pixels())
    }
}

impl<T: Styled> PanelSurface for T {}
