//! Bar widgets. Each directory is one widget with a [`BarWidgetDescriptor`];
//! adding one is a directory plus a line in [`REGISTRY`].

mod active_window;
mod battery;
mod clock;
pub mod control_center;
pub(crate) mod ctx;
mod keyboard_layout;
mod launcher_btn;
pub mod mpris;
pub(crate) mod style;
pub mod sysinfo;
mod tray;
mod widget;
mod workspaces;

pub use active_window::ActiveWindowConfig;
pub use battery::BatteryConfig;
pub use clock::ClockConfig;
pub use control_center::ControlCenterWidgetConfig;
pub(crate) use ctx::BarCtx;
pub use keyboard_layout::KeyboardLayoutConfig;
pub use launcher_btn::LauncherBtnConfig;
pub use mpris::MprisConfig;
pub use sysinfo::SysInfoConfig;
pub use tray::TrayConfig;
pub(crate) use widget::{BarWidget, BarWidgetShell, boxed};
pub use widget::{BarWidgetDescriptor, BarWidgetMeta, Widget};
pub use workspaces::WorkspacesConfig;

use gpui::App;

/// Every widget the config can name, in the order the editor lists them.
pub static REGISTRY: [BarWidgetDescriptor; 11] = [
    launcher_btn::descriptor(),
    workspaces::descriptor(),
    sysinfo::descriptor(),
    active_window::descriptor(),
    clock::descriptor(),
    battery::descriptor(),
    mpris::descriptor(),
    crate::notification::widget::descriptor(),
    tray::descriptor(),
    keyboard_layout::descriptor(),
    control_center::descriptor(),
];

/// The descriptor a config name refers to, aliases included.
pub fn find(name: &str) -> Option<&'static BarWidgetDescriptor> {
    REGISTRY.iter().find(|widget| widget.meta.matches(name))
}

/// What the editor offers: every widget's meta, in display order.
pub fn widgets() -> impl Iterator<Item = &'static BarWidgetMeta> {
    REGISTRY.iter().map(|widget| &widget.meta)
}

/// Build the widgets a section entry names; unknown names warn and are skipped.
pub(crate) fn build_many(names: &[String], ctx: &BarCtx, cx: &mut App) -> Vec<Widget> {
    names
        .iter()
        .filter_map(|name| match find(name) {
            Some(widget) => Some(widget.build(ctx.clone(), cx)),
            None => {
                tracing::warn!("Unknown widget: {name}");
                None
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_alias_finds_its_widget() {
        for (alias, name) in [
            ("WindowTitle", "ActiveWindow"),
            ("Tray", "Systray"),
            ("Launcher", "LauncherBtn"),
            ("Media", "Mpris"),
            ("Player", "Mpris"),
            ("Notification", "Notifications"),
            ("Settings", "ControlCenter"),
            ("Info", "ControlCenter"),
        ] {
            assert_eq!(find(alias).map(|w| w.meta.name), Some(name), "{alias}");
        }
        assert!(find("Nope").is_none());
    }

    #[test]
    fn names_are_unique_and_canonical() {
        for meta in widgets() {
            assert_eq!(find(meta.name).map(|w| w.meta.name), Some(meta.name));
            assert!(!meta.label.is_empty() && !meta.description.is_empty());
            let hits = REGISTRY
                .iter()
                .filter(|w| w.meta.matches(meta.name))
                .count();
            assert_eq!(hits, 1, "{} is claimed by {hits} widgets", meta.name);
        }
    }

    #[test]
    fn default_bar_names_resolve() {
        let bar = crate::bar::config::BarConfig::default();
        for item in bar.start.iter().chain(&bar.center).chain(&bar.end) {
            for name in item.names() {
                assert!(find(name).is_some(), "{name}");
            }
        }
    }
}
