//! Workspaces widget for displaying and switching compositor workspaces.

mod config;
pub use config::WorkspacesConfig;

use gpui::{AnyElement, Context, Hsla, Window, div, prelude::*, px};
use services::{CompositorCommand, CompositorState};
use ui::{ActiveTheme, h_flex};

use super::{BarWidget, BarWidgetShell, style};
use crate::config::ActiveConfig;
use crate::state::AppState;
use crate::state::watch;

/// Workspaces widget that displays workspace indicators and allows switching.
pub struct Workspaces {
    compositor: services::CompositorSubscriber,
    state: CompositorState,
}

impl Workspaces {
    /// Create a new Workspaces widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let compositor = AppState::compositor(cx).clone();
        let state = compositor.get();

        // Subscribe to compositor state changes
        watch(cx, compositor.subscribe(), |this, new_state, cx| {
            this.state = new_state;
            cx.notify();
        });

        Self { compositor, state }
    }

    /// Handle clicking on a workspace to focus it.
    fn focus_workspace(&self, workspace_id: i32) {
        if let Err(e) = self
            .compositor
            .dispatch(CompositorCommand::FocusWorkspace(workspace_id))
        {
            tracing::error!("Failed to focus workspace {}: {}", workspace_id, e);
        }
    }

    /// Resolve the compositor monitor name for the display this widget's
    /// window is on, so we only show that monitor's workspaces.
    ///
    /// Neither `Window::display()` nor display bounds/position nor ordinal
    /// list position are reliable here: gpui_linux derives `Window::display()`
    /// from `primary_output_scale()` (whichever output has the highest scale
    /// factor, unrelated to which output this layer-shell window is actually
    /// anchored to); every display's `bounds().origin` comes back as (0, 0)
    /// regardless of its real position; and `cx.displays()` isn't guaranteed
    /// to enumerate outputs in the same order as the compositor's own monitor
    /// list (confirmed to differ on at least one real setup, which silently
    /// swapped which bar controlled which monitor). Instead we match by
    /// identity: gpui_linux's `PlatformDisplay::uuid()` is a deterministic
    /// hash of the Wayland output's name (`Uuid::new_v5(NAMESPACE_DNS,
    /// name)`), so we look up the `DisplayId` this bar window was opened
    /// with (`crate::state::display_id_for_window`), and compare its uuid
    /// against the same hash computed from each compositor monitor's name.
    fn current_monitor_name(&self, window: &Window, cx: &gpui::App) -> Option<String> {
        let display_id = crate::state::display_id_for_window(window)?;
        crate::state::monitor_for_display(Some(display_id), &self.state, cx).map(|m| m.name.clone())
    }

    /// Handle scrolling to switch workspaces.
    fn scroll_workspace(&self, direction: i32) {
        if let Err(e) = self
            .compositor
            .dispatch(CompositorCommand::ScrollWorkspace(direction))
        {
            tracing::error!("Failed to scroll workspace: {}", e);
        }
    }

    /// The text on a pill. An empty label is not a miss: the pill then draws
    /// itself as a bare indicator, which is exactly what both `show_icons` and
    /// `show_numbers` off asks for.
    fn workspace_label(
        ws: &services::Workspace,
        is_vertical: bool,
        show_numbers: bool,
        show_icons: bool,
    ) -> String {
        let name = ws.name.trim();
        let is_numeric_name = !name.is_empty() && name.chars().all(|ch| ch.is_ascii_digit());
        let short = || name.chars().take(3).collect::<String>().to_uppercase();

        match (show_icons, show_numbers) {
            (false, false) => String::new(),
            // Icons only: a workspace named after its number has nothing to say.
            (true, false) if name.is_empty() || is_numeric_name => String::new(),
            (true, false) => short(),
            (false, true) => ws.id.to_string(),
            // A vertical pill is too narrow for a name.
            (true, true) if is_vertical || name.is_empty() => ws.id.to_string(),
            (true, true) if is_numeric_name => name.to_string(),
            (true, true) => short(),
        }
    }

    /// What an unlabelled pill draws instead: a dash for the active workspace,
    /// a dot for one holding windows, a smaller one when it is empty. Always
    /// along the bar's long axis, so the dash reads as a run of workspace.
    fn indicator(is_vertical: bool, is_active: bool, has_windows: bool, color: Hsla) -> AnyElement {
        let (length, thickness) = if is_active {
            (style::WORKSPACE_DASH, style::WORKSPACE_DOT)
        } else if has_windows {
            (style::WORKSPACE_DOT, style::WORKSPACE_DOT)
        } else {
            (style::WORKSPACE_DOT_EMPTY, style::WORKSPACE_DOT_EMPTY)
        };

        div()
            .rounded_full()
            .bg(color)
            .w(px(if is_vertical { thickness } else { length }))
            .h(px(if is_vertical { length } else { thickness }))
            .into_any_element()
    }

    fn render_workspace_pill(
        &self,
        ws: &services::Workspace,
        active_workspace_id: Option<i32>,
        theme: &ui::Theme,
        is_vertical: bool,
        config: &WorkspacesConfig,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let workspace_id = ws.id;
        let is_active = active_workspace_id == Some(ws.id);
        let has_windows = ws.windows > 0;
        let label = Self::workspace_label(ws, is_vertical, config.show_numbers, config.show_icons);
        let labelled = !label.is_empty();

        // Three weights, loudest first: the focused workspace, one holding
        // windows, an empty one. Not a `ButtonLike`: `ButtonStyle` has no
        // per-state palette to express them.
        let (fill, ink) = if is_active {
            (theme.colors.accent, theme.colors.background)
        } else if has_windows {
            (theme.colors.elevated_surface_background, theme.colors.text)
        } else {
            (
                gpui::transparent_black(),
                theme.colors.text_muted.opacity(0.7),
            )
        };
        // An indicator carries the state in its own colour, so its slot stays
        // empty and only lights up under the pointer.
        let hover_bg = if is_active && labelled {
            theme.colors.accent
        } else {
            theme.colors.element_hover
        };

        h_flex()
            .id(format!("workspace-{}", ws.id))
            .justify_center()
            .min_w(px(style::workspace_pill_width(is_vertical, is_active)))
            .h(px(style::WORKSPACE_PILL_HEIGHT))
            .when(labelled, |this| {
                this.px(px(style::WORKSPACE_PILL_PADDING_X))
            })
            .rounded_full()
            .cursor_pointer()
            .bg(if labelled {
                fill
            } else {
                gpui::transparent_black()
            })
            .hover(move |s| s.bg(hover_bg))
            .on_click(cx.listener(move |this, _, _, _| {
                this.focus_workspace(workspace_id);
            }))
            .child(if labelled {
                style::bar_label(label, is_vertical, ink).into_any_element()
            } else {
                Self::indicator(
                    is_vertical,
                    is_active,
                    has_windows,
                    if is_active { theme.colors.accent } else { ink },
                )
            })
            .into_any_element()
    }

    fn render_workspace_strip(
        &self,
        theme: &ui::Theme,
        is_vertical: bool,
        config: &WorkspacesConfig,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let monitor_name = self.current_monitor_name(window, cx);

        // Prefer this monitor's own active tag over the compositor-wide
        // `active_workspace_id`, which only ever reflects whichever monitor
        // currently has focus - on multi-monitor setups that's a different
        // monitor from this bar's as soon as focus moves elsewhere, which
        // would otherwise make this bar lose (or misreport) its highlight.
        let active_workspace_id = monitor_name
            .as_deref()
            .and_then(|name| self.state.monitors.iter().find(|m| m.name == name))
            .map(|m| m.active_workspace_id)
            .filter(|&id| id >= 0)
            .or(self.state.active_workspace_id);

        style::stack(is_vertical)
            .id("workspaces")
            .justify_center()
            .gap(px(style::group_gap(is_vertical)))
            .on_scroll_wheel(
                cx.listener(|this, event: &gpui::ScrollWheelEvent, _window, _cx| {
                    let delta = event.delta.pixel_delta(px(1.0));
                    if delta.y.abs() > px(0.5) {
                        let direction = if delta.y > px(0.0) { -1 } else { 1 };
                        this.scroll_workspace(direction);
                    }
                }),
            )
            .children(
                self.state
                    .workspaces
                    .iter()
                    .filter(|ws| !ws.is_special)
                    .filter(|ws| {
                        monitor_name
                            .as_deref()
                            .is_none_or(|name| ws.monitor == name)
                    })
                    .map(|ws| {
                        self.render_workspace_pill(
                            ws,
                            active_workspace_id,
                            theme,
                            is_vertical,
                            config,
                            cx,
                        )
                    }),
            )
            .into_any_element()
    }
}

impl BarWidget for Workspaces {
    fn shell(&self) -> BarWidgetShell {
        BarWidgetShell::Group
    }

    fn render_vertical(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let config = cx.config().bar.modules.workspaces.clone();
        self.render_workspace_strip(&theme, true, &config, window, cx)
    }

    fn render_horizontal(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let config = cx.config().bar.modules.workspaces.clone();
        self.render_workspace_strip(&theme, false, &config, window, cx)
    }
}

impl Render for Workspaces {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_bar_widget(window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: i32, name: &str) -> services::Workspace {
        services::Workspace {
            id,
            name: name.to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn label_picks_name_number_or_nothing() {
        // Both off: the pill falls back to its indicator.
        assert_eq!(
            Workspaces::workspace_label(&ws(1, "web"), false, false, false),
            ""
        );
        // Icons only: a name, unless it just repeats the number.
        assert_eq!(
            Workspaces::workspace_label(&ws(1, "web"), false, false, true),
            "WEB"
        );
        assert_eq!(
            Workspaces::workspace_label(&ws(1, "1"), false, false, true),
            ""
        );
        // Numbers only: always the id, name or not.
        assert_eq!(
            Workspaces::workspace_label(&ws(3, "web"), false, true, false),
            "3"
        );
        // Both: name horizontally, id vertically (too narrow for a name).
        assert_eq!(
            Workspaces::workspace_label(&ws(3, "web"), false, true, true),
            "WEB"
        );
        assert_eq!(
            Workspaces::workspace_label(&ws(3, "web"), true, true, true),
            "3"
        );
        assert_eq!(
            Workspaces::workspace_label(&ws(3, ""), false, true, true),
            "3"
        );
    }
}
