//! Panel system for popup menus and overlays.
//!
//! Panels are layer shell windows that appear on top of other content,
//! typically used for dropdown menus, context menus, and popup dialogs.

use gpui::{
    AnyWindowHandle, App, Bounds, Context, FocusHandle, Pixels, Point, Render, Size, Window,
    WindowBackgroundAppearance, WindowBounds, WindowKind, WindowOptions, layer_shell::*, point,
    prelude::*, px,
};
use std::sync::Mutex;
use ui::ActiveTheme;

use crate::config::BarPosition;

/// Panel configuration for positioning and sizing.
#[derive(Clone)]
pub struct PanelConfig {
    pub width: f32,
    pub height: f32,
    pub anchor: Anchor,
    pub margin: (f32, f32, f32, f32), // top, right, bottom, left
    pub namespace: String,
}

impl Default for PanelConfig {
    fn default() -> Self {
        PanelConfig {
            width: 320.0,
            height: 400.0,
            anchor: Anchor::TOP | Anchor::RIGHT,
            margin: (5.0, 5.0, 5.0, 5.0),
            namespace: "panel".to_string(),
        }
    }
}

/// Resolve panel anchor/margin from a click position, in window coordinates.
pub fn panel_placement_from_event(
    bar_position: BarPosition,
    position: Point<Pixels>,
    window: &Window,
    cx: &App,
    panel_size: Size<Pixels>,
) -> (Anchor, (f32, f32, f32, f32)) {
    let (display_bounds, usable_bounds) = window
        .display(cx)
        .map(|display| (display.bounds(), display.visible_bounds()))
        .unwrap_or_else(|| {
            let bounds = window.bounds();
            (bounds, bounds)
        });
    let click = point(
        window.bounds().origin.x + position.x,
        window.bounds().origin.y + position.y,
    );
    panel_placement_from_click(
        bar_position,
        click,
        panel_size,
        display_bounds,
        usable_bounds,
    )
}

/// Resolve panel anchor/margin from a click position.
pub fn panel_placement_from_click(
    bar_position: BarPosition,
    click: Point<Pixels>,
    panel_size: Size<Pixels>,
    display_bounds: Bounds<Pixels>,
    usable_bounds: Bounds<Pixels>,
) -> (Anchor, (f32, f32, f32, f32)) {
    let anchor_horizontal = match bar_position {
        BarPosition::Left => Anchor::LEFT,
        BarPosition::Right => Anchor::RIGHT,
        BarPosition::Top | BarPosition::Bottom => {
            if click.x < display_bounds.center().x {
                Anchor::LEFT
            } else {
                Anchor::RIGHT
            }
        }
    };

    let anchor_vertical = match bar_position {
        BarPosition::Top => Anchor::TOP,
        BarPosition::Bottom => Anchor::BOTTOM,
        BarPosition::Left | BarPosition::Right => {
            if click.y < display_bounds.center().y {
                Anchor::TOP
            } else {
                Anchor::BOTTOM
            }
        }
    };

    let anchor = anchor_horizontal | anchor_vertical;
    let margin = margin_from_click(
        bar_position,
        anchor,
        click,
        panel_size,
        display_bounds,
        usable_bounds,
    );

    (anchor, margin)
}

fn margin_from_click(
    bar_position: BarPosition,
    anchor: Anchor,
    click: Point<Pixels>,
    panel_size: Size<Pixels>,
    display_bounds: Bounds<Pixels>,
    usable_bounds: Bounds<Pixels>,
) -> (f32, f32, f32, f32) {
    let display_origin = display_bounds.origin;
    let display_size = display_bounds.size;
    let usable_origin = usable_bounds.origin;
    let usable_size = usable_bounds.size;

    let click_x: f32 = (click.x - display_origin.x).into();
    let click_y: f32 = (click.y - display_origin.y).into();
    let panel_w: f32 = panel_size.width.into();
    let panel_h: f32 = panel_size.height.into();
    let disp_w: f32 = display_size.width.into();
    let disp_h: f32 = display_size.height.into();

    let avail_x = (disp_w - panel_w).max(0.0);
    let avail_y = (disp_h - panel_h).max(0.0);

    let inset_left: f32 = (usable_origin.x - display_origin.x).into();
    let inset_top: f32 = (usable_origin.y - display_origin.y).into();
    let inset_right: f32 =
        (display_origin.x + display_size.width - (usable_origin.x + usable_size.width)).into();
    let inset_bottom: f32 =
        (display_origin.y + display_size.height - (usable_origin.y + usable_size.height)).into();

    let mut min_x = inset_left;
    let mut max_x = disp_w - inset_right - panel_w;
    if max_x < min_x {
        min_x = 0.0;
        max_x = avail_x;
    }

    let mut min_y = inset_top;
    let mut max_y = disp_h - inset_bottom - panel_h;
    if max_y < min_y {
        min_y = 0.0;
        max_y = avail_y;
    }

    let mut origin_x = clamp_f32(click_x - (panel_w / 2.0), min_x, max_x);
    let mut origin_y = clamp_f32(click_y - (panel_h / 2.0), min_y, max_y);

    match bar_position {
        BarPosition::Top => {
            origin_y = min_y;
        }
        BarPosition::Bottom => {
            origin_y = max_y;
        }
        BarPosition::Left => {
            origin_x = min_x;
        }
        BarPosition::Right => {
            origin_x = max_x;
        }
    }

    let mut top = 0.0;
    let mut right = 0.0;
    let mut bottom = 0.0;
    let mut left = 0.0;

    if anchor.contains(Anchor::TOP) {
        top = origin_y;
    }
    if anchor.contains(Anchor::BOTTOM) {
        bottom = disp_h - (origin_y + panel_h);
    }
    if anchor.contains(Anchor::LEFT) {
        left = origin_x;
    }
    if anchor.contains(Anchor::RIGHT) {
        right = disp_w - (origin_x + panel_w);
    }

    (top, right, bottom, left)
}

fn clamp_f32(value: f32, min: f32, max: f32) -> f32 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        value
    }
}

/// Global panel manager to ensure only one panel is open at a time.
static ACTIVE_PANEL: Mutex<Option<(String, AnyWindowHandle)>> = Mutex::new(None);

/// The panel that just closed itself, and when.
///
/// Clicking a bar widget whose panel is open reaches us twice: the compositor
/// takes keyboard focus off the panel (which dismisses it) before the click
/// lands on the bar. Without this the widget would close its panel and
/// immediately reopen it.
///
/// ponytail: a time window, not event provenance - the compositor gives us no
/// way to tell the two apart. Widen it if a slow frame ever slips through.
static JUST_DISMISSED: Mutex<Option<(String, std::time::Instant)>> = Mutex::new(None);
const REOPEN_GUARD: std::time::Duration = std::time::Duration::from_millis(250);

/// Open a panel with the given ID and content.
/// If a panel with the same ID is already open, it will be closed.
/// If a different panel is open, it will be closed first.
/// Returns true if the panel was opened, false if it was closed (toggled off).
pub fn toggle_panel<V: Render + 'static>(
    panel_id: &str,
    config: PanelConfig,
    cx: &mut App,
    build: impl FnOnce(&mut gpui::Context<V>) -> V + 'static,
) -> bool {
    toggle_panel_on_display(panel_id, config, None, cx, build)
}

/// Open a panel on an explicitly selected output.
pub fn toggle_panel_on_display<V: Render + 'static>(
    panel_id: &str,
    config: PanelConfig,
    display_id: Option<gpui::DisplayId>,
    cx: &mut App,
    build: impl FnOnce(&mut gpui::Context<V>) -> V + 'static,
) -> bool {
    let mut guard = ACTIVE_PANEL.lock().unwrap();

    // Check if any panel is open
    if let Some((open_id, handle)) = guard.take() {
        // Close the existing panel
        let _ = cx.update_window(handle, |_, window, _cx| {
            window.remove_window();
        });

        // If it was the same panel, just close it (toggle off)
        if open_id == panel_id {
            return false;
        }
    } else if let Some((id, at)) = JUST_DISMISSED.lock().unwrap().take()
        && closed_by_this_click(&id, at.elapsed(), panel_id)
    {
        // This click is what closed the panel; it is a toggle off, not a
        // request for a new one.
        return false;
    }

    // Open new panel
    let window_options = WindowOptions {
        display_id,
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::new(px(0.), px(0.)),
            size: Size::new(px(config.width), px(config.height)),
        })),
        app_id: Some(format!("gpuishell-panel-{}", panel_id)),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: config.namespace,
            layer: Layer::Overlay,
            anchor: config.anchor,
            exclusive_zone: None,
            margin: Some((
                px(config.margin.0),
                px(config.margin.1),
                px(config.margin.2),
                px(config.margin.3),
            )),
            keyboard_interactivity: KeyboardInteractivity::OnDemand,
            ..Default::default()
        }),
        focus: true,
        ..Default::default()
    };

    if let Ok(handle) = cx.open_window(window_options, move |window, cx| {
        window.set_rem_size(cx.theme().font_size);
        cx.new(|cx| {
            // A panel is transient: the moment the compositor hands keyboard
            // focus to something else - a click anywhere outside - it goes.
            cx.observe_window_activation(window, |_, window, _| {
                if !window.is_window_active() {
                    dismiss(window);
                }
            })
            .detach();
            build(cx)
        })
    }) {
        *guard = Some((panel_id.to_string(), handle.into()));
        true
    } else {
        false
    }
}

/// Whether an open request for `panel_id` is really the tail of the click
/// that just dismissed it.
fn closed_by_this_click(dismissed_id: &str, since: std::time::Duration, panel_id: &str) -> bool {
    dismissed_id == panel_id && since < REOPEN_GUARD
}

/// Close the panel hosted by this window, whichever one it is.
pub fn dismiss(window: &mut Window) {
    let handle = window.window_handle();
    let mut guard = ACTIVE_PANEL.lock().unwrap();
    if guard.as_ref().is_some_and(|(_, open)| *open == handle)
        && let Some((id, _)) = guard.take()
    {
        *JUST_DISMISSED.lock().unwrap() = Some((id, std::time::Instant::now()));
    }
    drop(guard);
    window.remove_window();
}

/// Escape-to-close plumbing for a panel view: hold one, [`arm`](Self::arm) it
/// in `Render`, and give the handle it returns to `track_focus`.
///
/// A panel only sees key events once something in it holds focus, and the
/// `Panel` key context is what binds Escape.
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

/// Close any open panel.
#[allow(dead_code)]
pub fn close_panel(cx: &mut App) {
    let mut guard = ACTIVE_PANEL.lock().unwrap();
    if let Some((_, handle)) = guard.take() {
        let _ = cx.update_window(handle, |_, window, _cx| {
            window.remove_window();
        });
    }
}

/// Forget a panel that has already closed itself through its window lifecycle.
pub fn forget_panel(panel_id: &str) {
    let mut guard = ACTIVE_PANEL.lock().unwrap();
    if guard
        .as_ref()
        .is_some_and(|(active_panel_id, _)| active_panel_id == panel_id)
    {
        guard.take();
    }
}

/// Check if a specific panel is open.
#[allow(dead_code)]
pub fn is_panel_open(panel_id: &str) -> bool {
    ACTIVE_PANEL
        .lock()
        .map(|guard| {
            guard
                .as_ref()
                .map(|(id, _)| id == panel_id)
                .unwrap_or(false)
        })
        .unwrap_or(false)
}

/// Get the ID of the currently open panel, if any.
#[allow(dead_code)]
pub fn active_panel_id() -> Option<String> {
    ACTIVE_PANEL
        .lock()
        .ok()
        .and_then(|guard| guard.as_ref().map(|(id, _)| id.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn reopen_guard_only_swallows_the_click_that_closed_the_panel() {
        // The click that dismissed it: toggle off, do not reopen.
        assert!(closed_by_this_click("sysinfo", Duration::ZERO, "sysinfo"));
        // A later click on the same widget opens it again.
        assert!(!closed_by_this_click(
            "sysinfo",
            REOPEN_GUARD + Duration::from_millis(1),
            "sysinfo"
        ));
        // A different widget always opens, however fast.
        assert!(!closed_by_this_click("sysinfo", Duration::ZERO, "mpris"));
    }
}
