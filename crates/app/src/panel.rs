//! Panels: what a bar or dock widget opens when it is clicked.
//!
//! A panel is a native anchored popup: the compositor places it against the
//! widget that opened it, holds the keyboard while it is up, and dismisses it
//! when the user clicks another application. Clicks in our own windows still
//! land where they were aimed, so the bar keeps toggling its own panels.

use serde::{Deserialize, Serialize};

use gpui::{
    AnyWindowHandle, App, Bounds, Context, FocusHandle, Pixels, Point, Render, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, point,
    popup::{PopupAnchor, PopupConstraintAdjustment, PopupGravity, PopupOptions},
    prelude::*,
    px, size,
};
use std::sync::Mutex;
use ui::ActiveTheme;

use crate::config::BarPosition;

/// How the panels a bar module opens are painted. The chrome itself is
/// [`ui::patterns::PanelSurface::bar_panel`]; this is the part the user picks.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelsConfig {
    /// Paint the panel's own background and border. Turned off, only the
    /// content shows - for a compositor that already paints behind it.
    pub background: bool,
}

impl Default for PanelsConfig {
    fn default() -> Self {
        Self { background: true }
    }
}

/// Whether panels paint their own background.
pub fn panel_background(cx: &App) -> bool {
    crate::config::Config::global(cx).panels.background
}

/// Where a panel hangs, and how big it opens.
#[derive(Clone)]
pub struct PanelConfig {
    /// The window it belongs to: the bar, or the dock.
    pub parent: AnyWindowHandle,
    /// What it is placed against, in the parent's coordinates.
    pub anchor_rect: Bounds<Pixels>,
    pub anchor: PopupAnchor,
    pub gravity: PopupGravity,
    /// Opening size. A panel that sizes itself to its content resizes from
    /// here and the compositor re-anchors it.
    pub size: Size<Pixels>,
}

/// The cross-section of the opening surface at the click, and the direction
/// the panel grows away in. Taking the whole strip is what clears the bar
/// instead of stopping halfway up it.
pub fn panel_anchor(
    bar_position: BarPosition,
    click: Point<Pixels>,
    parent_size: Size<Pixels>,
) -> (Bounds<Pixels>, PopupAnchor, PopupGravity) {
    let across_bar = Bounds::new(point(click.x, px(0.)), size(px(0.), parent_size.height));
    let along_bar = Bounds::new(point(px(0.), click.y), size(parent_size.width, px(0.)));

    match bar_position {
        BarPosition::Top => (across_bar, PopupAnchor::Bottom, PopupGravity::Bottom),
        BarPosition::Bottom => (across_bar, PopupAnchor::Top, PopupGravity::Top),
        BarPosition::Left => (along_bar, PopupAnchor::Right, PopupGravity::Right),
        BarPosition::Right => (along_bar, PopupAnchor::Left, PopupGravity::Left),
    }
}

/// One panel at a time.
static ACTIVE_PANEL: Mutex<Option<(String, AnyWindowHandle)>> = Mutex::new(None);

/// Open `panel_id`, closing whichever panel is already up. Toggles off, and
/// returns false, when that is this one.
pub fn toggle_panel<V: Render + 'static>(
    panel_id: &str,
    config: PanelConfig,
    cx: &mut App,
    build: impl FnOnce(&mut Context<V>) -> V + 'static,
) -> bool {
    // The lock is never held across a close or an open: both run release
    // handlers that take it themselves.
    if let Some((open_id, handle)) = ACTIVE_PANEL.lock().unwrap().take() {
        let _ = cx.update_window(handle, |_, window, _cx| {
            window.remove_window();
        });

        if open_id == panel_id {
            return false;
        }
    }

    let window_options = WindowOptions {
        titlebar: None,
        // Only the size is ours; the compositor owns the position.
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: point(px(0.), px(0.)),
            size: config.size,
        })),
        app_id: Some(format!("gpuishell-panel-{panel_id}")),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::AnchoredPopup(PopupOptions {
            parent: config.parent,
            anchor_rect: config.anchor_rect,
            anchor: config.anchor,
            gravity: config.gravity,
            // Slide along the bar to stay on screen, never flip past it.
            constraint_adjustment: PopupConstraintAdjustment::SLIDE_X
                | PopupConstraintAdjustment::SLIDE_Y,
            offset: point(px(0.), px(0.)),
            // Keeps the keyboard on the panel however the pointer wanders, and
            // closes it when the user clicks another application.
            grab: true,
        }),
        focus: true,
        ..Default::default()
    };

    let opened_id = panel_id.to_string();
    let Ok(handle) = cx.open_window(window_options, move |window, cx| {
        window.set_rem_size(cx.theme().font_size);
        cx.new(|cx| {
            // The compositor closes grabbing popups on its own, so the
            // bookkeeping hangs off the view going away.
            cx.on_release(move |_, _| forget_panel(&opened_id)).detach();
            build(cx)
        })
    }) else {
        return false;
    };

    *ACTIVE_PANEL.lock().unwrap() = Some((panel_id.to_string(), handle.into()));
    true
}

/// Close the panel hosted by this window, whichever one it is.
pub fn dismiss(window: &mut Window) {
    let handle = window.window_handle();
    let mut guard = ACTIVE_PANEL.lock().unwrap();
    if guard.as_ref().is_some_and(|(_, open)| *open == handle) {
        guard.take();
    }
    drop(guard);
    window.remove_window();
}

/// Escape-to-close plumbing: hold one, [`arm`](Self::arm) it in `Render`, and
/// give the handle it returns to `track_focus`. A panel only sees key events
/// once something in it holds focus, and `Panel` is the context binding Escape.
pub struct PanelFocus(FocusHandle);

impl PanelFocus {
    pub fn new<V: 'static>(cx: &mut Context<V>) -> Self {
        Self(cx.focus_handle())
    }

    pub fn arm<V: 'static>(&self, window: &mut Window, cx: &mut Context<V>) -> &FocusHandle {
        if !self.0.is_focused(window) {
            self.0.focus(window, cx);
        }
        &self.0
    }
}

/// Close whichever panel is open.
pub fn close_panel(cx: &mut App) {
    let mut guard = ACTIVE_PANEL.lock().unwrap();
    if let Some((_, handle)) = guard.take() {
        let _ = cx.update_window(handle, |_, window, _cx| {
            window.remove_window();
        });
    }
}

/// Drop the bookkeeping for a panel that has already gone.
fn forget_panel(panel_id: &str) {
    let mut guard = ACTIVE_PANEL.lock().unwrap();
    if guard
        .as_ref()
        .is_some_and(|(active_panel_id, _)| active_panel_id == panel_id)
    {
        guard.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panel_hangs_off_the_far_edge_of_the_bar_at_the_click() {
        // The full height of the bar at the click, so the panel clears it.
        let (rect, anchor, gravity) = panel_anchor(
            BarPosition::Top,
            point(px(1500.), px(18.)),
            size(px(1920.), px(36.)),
        );
        assert_eq!(
            rect,
            Bounds::new(point(px(1500.), px(0.)), size(px(0.), px(36.)))
        );
        assert_eq!(anchor, PopupAnchor::Bottom);
        assert_eq!(gravity, PopupGravity::Bottom);

        // A side bar hands the panel the other axis, growing inwards.
        let (rect, anchor, gravity) = panel_anchor(
            BarPosition::Left,
            point(px(24.), px(600.)),
            size(px(48.), px(1080.)),
        );
        assert_eq!(
            rect,
            Bounds::new(point(px(0.), px(600.)), size(px(48.), px(0.)))
        );
        assert_eq!(anchor, PopupAnchor::Right);
        assert_eq!(gravity, PopupGravity::Right);
    }
}
