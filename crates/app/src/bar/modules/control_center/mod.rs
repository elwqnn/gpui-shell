//! Control center widget showing status icons that opens the control center panel.
//!
//! This widget displays:
//! - Privacy indicators (mic, webcam, screenshare) when active
//! - Volume icon
//! - WiFi/Network icon
//! - Bluetooth icon (when connected)
//! - Power profile icon
//! - Battery icon + percentage
//!
//! Clicking opens the Control Center panel.

use gpui::{AnyElement, Context, Pixels, Point, Window, prelude::*};
use services::{
    ActiveConnectionInfo, AudioData, BluetoothData, BluetoothState, NetworkData, PrivacyData,
    UPowerData,
};
use ui::{ButtonCommon, ButtonLike, ButtonStyle, Clickable, IconName};

mod config;
pub use config::ControlCenterWidgetConfig;

use super::{BarCtx, BarWidget, BarWidgetDescriptor, BarWidgetMeta, boxed, style};
use crate::config::Config;
use crate::control_center::{
    CONTROL_CENTER_PANEL_MAX_HEIGHT, CONTROL_CENTER_PANEL_WIDTH, ControlCenter,
};
use crate::state::{AppState, watch};
use crate::windows::{PanelConfig, PanelSize, ShellWindows, panel_placement_from_event};

use crate::icons;

pub const fn descriptor() -> BarWidgetDescriptor {
    BarWidgetDescriptor::new(
        BarWidgetMeta {
            name: "ControlCenter",
            aliases: &["Settings", "Info"],
            label: "Control Center",
            description: "Opens the control center.",
            icon: IconName::Settings,
        },
        |cx| boxed(cx, ControlCenterWidget::new),
    )
}

/// Control center widget for the bar that shows system status icons.
pub struct ControlCenterWidget {
    audio: AudioData,
    bluetooth: BluetoothData,
    network: NetworkData,
    privacy: PrivacyData,
    upower: UPowerData,
}

impl ControlCenterWidget {
    /// Create a new control center widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let audio = AppState::audio(cx).get();
        let bluetooth = AppState::bluetooth(cx).get();
        let network = AppState::network(cx).get();
        let privacy = AppState::privacy(cx).get();
        let upower = AppState::upower(cx).get();

        watch(cx, AppState::audio(cx).subscribe(), |this, data, cx| {
            this.audio = data;
            cx.notify();
        });
        watch(cx, AppState::bluetooth(cx).subscribe(), |this, data, cx| {
            this.bluetooth = data;
            cx.notify();
        });
        watch(cx, AppState::network(cx).subscribe(), |this, data, cx| {
            this.network = data;
            cx.notify();
        });
        watch(cx, AppState::privacy(cx).subscribe(), |this, data, cx| {
            this.privacy = data;
            cx.notify();
        });
        watch(cx, AppState::upower(cx).subscribe(), |this, data, cx| {
            this.upower = data;
            cx.notify();
        });

        ControlCenterWidget {
            audio,
            bluetooth,
            network,
            privacy,
            upower,
        }
    }

    /// Toggle the control center panel.
    fn toggle_panel(&self, at: Point<Pixels>, window: &Window, cx: &mut gpui::App) {
        let config = Config::global(cx);
        let size = PanelSize::FitHeight {
            width: CONTROL_CENTER_PANEL_WIDTH,
            max_height: CONTROL_CENTER_PANEL_MAX_HEIGHT,
        };
        let (anchor, margin) =
            panel_placement_from_event(config.bar.position, at, window, cx, size.initial());
        let config = PanelConfig {
            size,
            anchor,
            margin,
            namespace: "control-center".to_string(),
        };

        ShellWindows::toggle_panel(cx, window, "control-center", config, move |cx| {
            ControlCenter::new(cx)
        });
    }

    /// Privacy indicator icons, only while something is being accessed. A
    /// horizontal bar shows the most serious one to avoid icon crowding.
    fn privacy_icons(&self, is_vertical: bool) -> Vec<IconName> {
        let all = [
            (self.privacy.screenshare_access(), IconName::ScreenShare),
            (self.privacy.webcam_access(), IconName::Camera),
            (self.privacy.microphone_access(), IconName::Mic),
        ];
        let mut icons: Vec<IconName> = all
            .into_iter()
            .filter(|(active, _)| *active)
            .map(|(_, icon)| icon)
            .collect();
        if is_vertical {
            // Vertical bars list them in the order they were reported.
            icons.reverse();
        } else {
            icons.truncate(1);
        }
        icons
    }

    /// Get the network icon based on current state.
    fn network_icon(&self) -> IconName {
        if self
            .network
            .active_connections
            .iter()
            .any(|c| matches!(c, ActiveConnectionInfo::Wired { .. }))
        {
            return IconName::Ethernet;
        }

        if !self.network.wifi_enabled {
            return IconName::WifiOff;
        }

        if self
            .network
            .active_connections
            .iter()
            .any(|c| matches!(c, ActiveConnectionInfo::WiFi { .. }))
        {
            IconName::Wifi
        } else {
            IconName::WifiOff
        }
    }

    /// Get bluetooth icon if any device is connected.
    fn bluetooth_icon(&self) -> Option<IconName> {
        if self.bluetooth.state == BluetoothState::Active
            && self.bluetooth.devices.iter().any(|d| d.connected)
        {
            Some(IconName::BluetoothConnected)
        } else {
            None
        }
    }

    fn render_battery_block(&self, ctx: &BarCtx) -> AnyElement {
        let (icon_color, text_color) = style::battery_ink(ctx, self.upower.battery.as_ref());
        let text = self
            .upower
            .battery
            .as_ref()
            .map(|battery| style::compact_percent(ctx, battery.percentage.into()))
            .unwrap_or_default();

        style::stack(ctx)
            .justify_center()
            .gap(ctx.metrics.gap)
            .child(style::icon_in(
                ctx,
                icons::battery_data_icon(self.upower.battery.as_ref()),
                icon_color,
            ))
            .when(!text.is_empty(), |el| {
                el.child(style::value(ctx, style::label_in(ctx, text, text_color)))
            })
            .into_any_element()
    }

    fn render(&self, ctx: &BarCtx, cx: &mut Context<Self>) -> AnyElement {
        let privacy_icons = self.privacy_icons(ctx.vertical);
        let volume_icon = icons::volume_icon(self.audio.sink_volume, self.audio.sink_muted);
        let power_profile_icon = icons::power_profile_icon(self.upower.power_profile);
        // A horizontal bar separates the privacy and battery blocks with
        // hairlines; a vertical one just stacks, and spaces every item alike.
        let gap = if ctx.vertical {
            ctx.metrics.gap
        } else {
            ctx.metrics.icon_gap
        };

        ButtonLike::new("control-center-widget")
            .style(ButtonStyle::Transparent)
            .on_click(cx.listener(|this, event: &gpui::ClickEvent, window, cx| {
                this.toggle_panel(event.position(), window, cx);
            }))
            .child(
                style::stack(ctx)
                    .justify_center()
                    .gap(gap)
                    .children(
                        privacy_icons
                            .iter()
                            .map(|icon| style::icon_in(ctx, *icon, ctx.ink.status.error)),
                    )
                    .when(!ctx.vertical && !privacy_icons.is_empty(), |el| {
                        el.child(style::divider(ctx))
                    })
                    .child(style::icon(ctx, volume_icon))
                    .child(style::icon(ctx, self.network_icon()))
                    .children(self.bluetooth_icon().map(|icon| style::icon(ctx, icon)))
                    .when(ctx.vertical, |el| {
                        el.child(style::icon(ctx, power_profile_icon))
                    })
                    .when(!ctx.vertical, |el| el.child(style::divider(ctx)))
                    .child(self.render_battery_block(ctx)),
            )
            .into_any_element()
    }
}

impl BarWidget for ControlCenterWidget {
    fn is_interactive(&self) -> bool {
        true
    }

    fn render_vertical(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }

    fn render_horizontal(
        &mut self,
        ctx: &BarCtx,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.render(ctx, cx)
    }
}
