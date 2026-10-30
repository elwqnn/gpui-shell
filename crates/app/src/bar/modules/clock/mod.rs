//! Clock widget that displays the current date and time.

mod config;
pub use config::ClockConfig;

use chrono::Local;
use gpui::{AnyElement, Context, Window, prelude::*, px};
use std::fmt::Write as _;
use std::time::Duration;
use ui::{ActiveTheme, h_flex, v_flex};

use super::{BarWidget, style};
use crate::config::ActiveConfig;

/// Shown when the configured format holds a specifier chrono does not know.
const FALLBACK_FORMAT: &str = "%H:%M";

/// Format the current time, falling back when the format is not valid strftime.
fn format_now(format: &str) -> String {
    let mut out = String::new();
    if write!(out, "{}", Local::now().format(format)).is_err() {
        out.clear();
        let _ = write!(out, "{}", Local::now().format(FALLBACK_FORMAT));
    }
    out
}

/// Time left in the current wall-clock second, so a tick lands on the second
/// boundary instead of drifting by however long the last redraw took.
fn until_next_second() -> Duration {
    let nanos = u64::from(Local::now().timestamp_subsec_nanos()) % 1_000_000_000;
    Duration::from_nanos(1_000_000_000 - nanos)
}

/// A clock widget that redraws when its text changes.
pub struct Clock;

impl Clock {
    /// Create a new clock widget that auto-updates.
    pub fn new(cx: &mut Context<Self>) -> Self {
        cx.spawn(async move |this, cx| {
            let mut shown = String::new();
            loop {
                cx.background_executor().timer(until_next_second()).await;

                // Ticking every second is cheap; repainting the bar is not, so
                // a format without seconds only redraws once a minute.
                let alive = this.update(cx, |_, cx| {
                    let text = {
                        let bar = &cx.config().bar;
                        format_now(if bar.is_vertical() {
                            &bar.modules.clock.format_vertical
                        } else {
                            &bar.modules.clock.format_horizontal
                        })
                    };
                    if text != shown {
                        shown = text;
                        cx.notify();
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();

        Clock
    }

    fn render_vertical_content(
        &self,
        theme: &ui::Theme,
        vertical_format: &str,
        horizontal_fallback: &str,
    ) -> AnyElement {
        let mut lines: Vec<String> = format_now(vertical_format)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect();
        if lines.is_empty() {
            lines.push(format_now(horizontal_fallback));
        }

        v_flex()
            .id("clock")
            .items_center()
            .gap(px(style::CHIP_GAP))
            .children(lines.into_iter().enumerate().map(|(idx, line)| {
                let color = if idx == 0 {
                    theme.colors.text
                } else {
                    theme.colors.text_muted
                };
                style::vertical_text_line(style::bar_label(line, true, color))
            }))
            .into_any_element()
    }

    fn render_horizontal_content(&self, theme: &ui::Theme, format: &str) -> AnyElement {
        h_flex()
            .id("clock")
            .gap(px(style::CHIP_GAP))
            .child(style::bar_label(
                format_now(format),
                false,
                theme.colors.text,
            ))
            .into_any_element()
    }
}

impl BarWidget for Clock {
    fn render_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let config = &cx.config().bar.modules.clock;

        self.render_vertical_content(theme, &config.format_vertical, &config.format_horizontal)
    }
    fn render_horizontal(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme();
        let config = &cx.config().bar.modules.clock;

        self.render_horizontal_content(theme, &config.format_horizontal)
    }
}

impl Render for Clock {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_bar_widget(window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bad_format_falls_back_instead_of_panicking() {
        let config = ClockConfig::default();
        assert_eq!(format_now("%Q"), format_now(FALLBACK_FORMAT));
        // A valid format renders itself rather than the fallback.
        assert_ne!(format_now(&config.format_horizontal), format_now("%Q"));
    }

    #[test]
    fn tick_lands_inside_the_next_second() {
        let delay = until_next_second();
        assert!(delay > Duration::ZERO && delay <= Duration::from_secs(1));
    }
}
