//! SysInfo panel: a compact dashboard of the machine's live pressure.
//!
//! Three rings carry the numbers you read at a glance (CPU, memory, swap),
//! then two dense sections carry the ones you read on purpose: what the
//! network is moving, and how full each disk is.

use crate::keybinds::Cancel;
use crate::panel::{PanelFocus, panel_background};
use crate::state::watch;
use gpui::{App, Context, Div, FontWeight, ScrollHandle, Window, div, prelude::*, px};
use services::{DiskInfo, NetworkInfo, SysInfoData, SysInfoSubscriber};
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, ButtonCommon, ButtonSize, ButtonStyle, CircularProgress, Clickable, Color, Icon,
    IconButton, IconName, IconSize, Label, LabelCommon, ProgressBar, Spacing, TextSize, h_flex,
    v_flex,
};

/// SysInfo panel content showing detailed system information.
pub struct SysInfoPanel {
    data: SysInfoData,
    scroll_handle: ScrollHandle,
    focus: PanelFocus,
    /// Whether the address is spelled out. Off on every open: the panel is
    /// built fresh each time, so revealing it never outlives the session.
    show_ip: bool,
}

// Fixed size: the panel scrolls rather than grows.
pub(super) const SYSINFO_PANEL_WIDTH: f32 = 300.0;
pub(super) const SYSINFO_PANEL_HEIGHT: f32 = 300.0;

/// Diameter of a gauge ring and the width of the arc it draws.
const GAUGE_SIZE: f32 = 52.0;
const GAUGE_STROKE: f32 = 4.0;
/// Floor under a gauge tile, so the one without a detail line still sits on
/// the same baseline as the others. A floor rather than a height: the text
/// scales with the user's font size and has to be able to push past it.
const GAUGE_TILE_HEIGHT: f32 = 82.0;

/// An address with its digits struck out, separators kept so it still reads
/// as one: `192.168.1.20` -> `xxx.xxx.x.xx`.
fn mask_ip(ip: &str) -> String {
    ip.chars()
        .map(|c| if c == '.' || c == ':' { c } else { 'x' })
        .collect()
}

/// The card a section of this panel sits in. `flex_shrink_0` so a long disk
/// list scrolls instead of squashing the sections above it.
fn section(cx: &App) -> Div {
    v_flex()
        .w_full()
        .flex_shrink_0()
        .p(Spacing::Medium.pixels())
        .gap(Spacing::Small.pixels())
        .panel_card(cx)
}

impl SysInfoPanel {
    /// Create a new SysInfo panel with the given subscriber.
    pub fn new(subscriber: SysInfoSubscriber, cx: &mut Context<Self>) -> Self {
        let initial_data = subscriber.get();

        // Subscribe to updates from the sysinfo service
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        SysInfoPanel {
            data: initial_data,
            scroll_handle: ScrollHandle::new(),
            focus: PanelFocus::new(cx),
            show_ip: false,
        }
    }

    /// One gauge: a ring around its percentage, its name under it, and a
    /// second line for whatever the percentage leaves out.
    fn render_gauge(
        icon: IconName,
        name: &str,
        usage: u32,
        detail: Option<(String, Color)>,
        cx: &App,
    ) -> impl IntoElement {
        let colors = cx.theme().colors();
        let color = colors.status.from_percentage(usage);

        v_flex()
            .flex_1()
            .min_h(px(GAUGE_TILE_HEIGHT))
            .items_center()
            .gap(Spacing::XSmall.pixels())
            .child(
                div()
                    .relative()
                    .size(px(GAUGE_SIZE))
                    .child(
                        CircularProgress::new(usage.min(100) as f32, 100., px(GAUGE_SIZE))
                            .stroke_width(px(GAUGE_STROKE))
                            .progress_color(color)
                            .bg_color(colors.element_background),
                    )
                    .child(
                        h_flex().absolute().inset_0().justify_center().child(
                            Label::new(format!("{usage}%"))
                                .color(Color::Custom(color))
                                .weight(FontWeight::MEDIUM),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .gap(Spacing::XSmall.pixels())
                    .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
                    .child(Label::new(name.to_string()).size(TextSize::XSmall)),
            )
            .children(
                detail.map(|(text, color)| Label::new(text).size(TextSize::XSmall).color(color)),
            )
    }

    /// The meter under a disk row.
    fn render_meter(usage: u32, cx: &App) -> impl IntoElement {
        let colors = cx.theme().colors();

        ProgressBar::new(usage.min(100) as f32, 100.)
            .fg_color(colors.status.from_percentage(usage))
            .bg_color(colors.elevated_surface_background)
    }

    /// A throughput reading: an arrow and the speed beside it.
    fn render_speed(icon: IconName, speed: String) -> impl IntoElement {
        h_flex()
            .gap(Spacing::XSmall.pixels())
            .child(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
            .child(Label::new(speed).size(TextSize::Small))
    }

    /// Both directions on one line, with the address the traffic is on -
    /// masked until asked for, since the panel opens over whatever is on
    /// screen and whoever is looking at it.
    fn render_network(
        network: &NetworkInfo,
        show_ip: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let ip = match network.ip.as_deref() {
            Some(ip) if show_ip => ip.to_string(),
            Some(ip) => mask_ip(ip),
            None => "No IP".to_string(),
        };

        section(cx).child(
            h_flex()
                .justify_between()
                .gap(Spacing::Small.pixels())
                .child(
                    h_flex()
                        .gap(Spacing::Large.pixels())
                        .child(Self::render_speed(
                            IconName::Download,
                            network.download_str(),
                        ))
                        .child(Self::render_speed(IconName::Upload, network.upload_str())),
                )
                .child(
                    h_flex()
                        .gap(Spacing::XSmall.pixels())
                        .child(
                            Label::new(ip)
                                .size(TextSize::XSmall)
                                .color(Color::Muted)
                                .truncate(),
                        )
                        .children(network.ip.is_some().then(|| {
                            IconButton::new(
                                "sysinfo-ip",
                                if show_ip {
                                    IconName::EyeOff
                                } else {
                                    IconName::Eye
                                },
                            )
                            .style(ButtonStyle::Subtle)
                            .size(ButtonSize::Compact)
                            .icon_size(IconSize::XSmall)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.show_ip = !this.show_ip;
                                cx.notify();
                            }))
                        })),
                ),
        )
    }

    /// One disk: where it is mounted, what it holds, and how full that leaves it.
    fn render_disk(disk: &DiskInfo, cx: &App) -> impl IntoElement {
        let color = cx
            .theme()
            .colors()
            .status
            .from_percentage(disk.usage_percent);

        v_flex()
            .gap(Spacing::XXSmall.pixels())
            .child(
                h_flex()
                    .gap(Spacing::Small.pixels())
                    .child(
                        h_flex().flex_1().min_w_0().child(
                            Label::new(disk.mount_point.clone())
                                .size(TextSize::Small)
                                .truncate(),
                        ),
                    )
                    .child(
                        Label::new(format!("{:.0}/{:.0} GB", disk.used_gb, disk.total_gb))
                            .size(TextSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(format!("{}%", disk.usage_percent))
                            .size(TextSize::XSmall)
                            .color(Color::Custom(color))
                            .weight(FontWeight::MEDIUM),
                    ),
            )
            .child(Self::render_meter(disk.usage_percent, cx))
    }
}

impl Render for SysInfoPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.arm(window, cx).clone();
        let data = self.data.clone();

        // The CPU's temperature reads as part of the CPU, not as a row of its own.
        let temperature = data.temperature.map(|t| {
            (
                format!("{t}°C"),
                Color::Custom(cx.theme().colors().status.from_temperature(t)),
            )
        });
        let memory = Some((
            format!("{:.1}/{:.0} GB", data.memory_used_gb, data.memory_total_gb),
            Color::Muted,
        ));

        v_flex()
            .id("sysinfo-panel")
            .track_focus(&focus)
            .key_context("Panel")
            .on_action(|_: &Cancel, window, _| crate::panel::dismiss(window))
            .w_full()
            .h(px(SYSINFO_PANEL_HEIGHT))
            .bar_panel(panel_background(cx), cx)
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            // Gauges: swap only earns a ring once something is in it.
            .child(
                section(cx).child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .child(Self::render_gauge(
                            IconName::Cpu,
                            "CPU",
                            data.cpu_usage,
                            temperature,
                            cx,
                        ))
                        .child(Self::render_gauge(
                            IconName::MemoryStick,
                            "RAM",
                            data.memory_usage,
                            memory,
                            cx,
                        ))
                        .when(data.swap_usage > 0, |el| {
                            el.child(Self::render_gauge(
                                IconName::ArrowDownUp,
                                "SWAP",
                                data.swap_usage,
                                None,
                                cx,
                            ))
                        }),
                ),
            )
            .child(Self::render_network(&data.network, self.show_ip, cx))
            .when(!data.disks.is_empty(), |el| {
                el.child(
                    section(cx).children(data.disks.iter().map(|disk| Self::render_disk(disk, cx))),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::mask_ip;

    #[test]
    fn masks_digits_and_keeps_separators() {
        assert_eq!(mask_ip("192.168.1.20"), "xxx.xxx.x.xx");
        assert_eq!(mask_ip("fe80::1"), "xxxx::x");
    }
}
