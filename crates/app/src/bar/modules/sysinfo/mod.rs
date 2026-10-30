//! SysInfo widget showing CPU and memory usage in the bar.
//!
//! Clicking the widget opens a detailed system information panel.

use crate::panel::{PanelConfig, toggle_panel};
use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Point, Size, Window, prelude::*, px};
use services::SysInfoData;
use ui::{ActiveTheme, IconName};

mod config;
pub use config::SysInfoConfig;

use super::{BarClick, BarWidget, style};
use crate::config::{ActiveConfig, Config};
use crate::panel::panel_anchor;
use crate::state::AppState;
use crate::state::watch;

mod panel;
pub use panel::SysInfoPanel;
use panel::{SYSINFO_PANEL_HEIGHT, SYSINFO_PANEL_WIDTH};

/// SysInfo widget showing CPU and memory usage in the bar.
pub struct SysInfo {
    subscriber: services::SysInfoSubscriber,
    data: SysInfoData,
}

struct SysInfoStat {
    icon: IconName,
    text: String,
    color: gpui::Hsla,
}

impl SysInfo {
    /// Create a new SysInfo widget.
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::sysinfo(cx).clone();
        let initial_data = subscriber.get();

        // Subscribe to updates from the sysinfo service
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        SysInfo {
            subscriber,
            data: initial_data,
        }
    }

    fn toggle_panel(&mut self, at: Point<Pixels>, window: &Window, cx: &mut App) {
        let subscriber = self.subscriber.clone();
        let bar_position = Config::global(cx).bar.position;
        let (anchor_rect, anchor, gravity) = panel_anchor(bar_position, at, window.viewport_size());
        let config = PanelConfig {
            parent: window.window_handle(),
            anchor_rect,
            anchor,
            gravity,
            size: Size::new(px(SYSINFO_PANEL_WIDTH), px(SYSINFO_PANEL_HEIGHT)),
        };

        toggle_panel("sysinfo", config, cx, move |cx| {
            SysInfoPanel::new(subscriber, cx)
        });
    }

    fn cpu_icon(&self) -> IconName {
        if self.data.cpu_usage >= 90 {
            IconName::Flame
        } else {
            IconName::Cpu
        }
    }

    fn memory_icon(&self) -> IconName {
        if self.data.memory_usage >= 90 {
            IconName::ArrowDownUp
        } else {
            IconName::MemoryStick
        }
    }

    fn usage_text(usage: u32, is_vertical: bool) -> String {
        style::compact_percent(usage, is_vertical)
    }

    fn stats(
        &self,
        theme: &ui::Theme,
        config: &SysInfoConfig,
        is_vertical: bool,
    ) -> Vec<SysInfoStat> {
        let mut stats = Vec::new();

        if config.show_cpu {
            let cpu_usage = self.data.cpu_usage;
            stats.push(SysInfoStat {
                icon: self.cpu_icon(),
                text: Self::usage_text(cpu_usage, is_vertical),
                color: theme.colors.status.from_percentage(cpu_usage),
            });
        }

        if config.show_memory {
            let memory_usage = self.data.memory_usage;
            stats.push(SysInfoStat {
                icon: self.memory_icon(),
                text: Self::usage_text(memory_usage, is_vertical),
                color: theme.colors.status.from_percentage(memory_usage),
            });
        }

        if config.show_temp
            && let Some(temp) = self.data.temperature
        {
            stats.push(SysInfoStat {
                icon: if temp >= 70 {
                    IconName::Flame
                } else {
                    IconName::Thermometer
                },
                text: if is_vertical {
                    temp.to_string()
                } else {
                    format!("{temp}°C")
                },
                color: theme.colors.status.from_temperature(temp),
            });
        }

        stats
    }

    fn render_stats(stats: Vec<SysInfoStat>, is_vertical: bool) -> AnyElement {
        style::stack(is_vertical)
            .justify_center()
            .gap(px(style::CHIP_GAP))
            .children(
                stats
                    .into_iter()
                    .map(|stat| style::bar_stat(is_vertical, stat.icon, stat.text, stat.color)),
            )
            .into_any_element()
    }
}

impl BarWidget for SysInfo {
    fn on_click(&self, cx: &mut Context<Self>) -> Option<BarClick> {
        Some(Box::new(cx.listener(
            |this, event: &ClickEvent, window, cx| {
                this.toggle_panel(event.position(), window, cx);
            },
        )))
    }

    fn render_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let config = &cx.config().bar.modules.sysinfo;
        let stats = self.stats(cx.theme(), config, true);
        Self::render_stats(stats, true)
    }

    fn render_horizontal(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let config = &cx.config().bar.modules.sysinfo;
        let stats = self.stats(cx.theme(), config, false);
        Self::render_stats(stats, false)
    }
}

impl Render for SysInfo {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_bar_widget(window, cx)
    }
}
