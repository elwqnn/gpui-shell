//! Notification widget: unread count in the bar, notification center on click.
//!
//! The center, popups and cards it opens live in [`crate::notification`] -
//! they are windows of their own, not bar chrome.

use gpui::{
    AnyElement, App, ClickEvent, Context, Pixels, Point, Render, Size, Window, prelude::*, px,
};
use services::{NotificationCommand, NotificationData, NotificationSubscriber};
use ui::{ActiveTheme, Color, Icon, IconSource, Label, LabelCommon, TextSize};

use super::{BarClick, BarWidget, style};
use crate::config::{ActiveConfig, Config};
use crate::notification::{NotificationCenter, dispatch_notification_command};
use crate::panel::{PanelConfig, panel_placement_from_event, toggle_panel};
use crate::state::{AppState, watch};

/// Notification widget for the bar.
pub struct NotificationWidget {
    subscriber: NotificationSubscriber,
    data: NotificationData,
}

impl NotificationWidget {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::notification(cx).clone();
        let data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        Self { subscriber, data }
    }

    fn toggle_center(&self, at: Point<Pixels>, window: &Window, cx: &mut App) {
        let config = Config::global(cx);
        let notification_config = &config.notification;
        let panel_size = Size::new(
            px(notification_config.center_width),
            px(notification_config.center_height),
        );
        let (anchor, margin) =
            panel_placement_from_event(config.bar.position, at, window, cx, panel_size);
        let subscriber = self.subscriber.clone();
        dispatch_notification_command(subscriber.clone(), NotificationCommand::MarkAllRead);

        let panel_config = PanelConfig {
            width: notification_config.center_width,
            height: notification_config.center_height,
            anchor,
            margin,
            namespace: "notification-center".to_string(),
        };

        toggle_panel("notification-center", panel_config, cx, move |cx| {
            NotificationCenter::new(subscriber, cx)
        });
    }

    fn render_widget_content(
        &self,
        theme: &ui::Theme,
        icon: IconSource,
        is_vertical: bool,
    ) -> AnyElement {
        let unread = self.data.unread_count;

        style::stack(is_vertical)
            .justify_center()
            .gap(px(style::CHIP_GAP))
            .child(
                Icon::new(icon)
                    .size(style::icon(is_vertical))
                    .color(Color::Custom(if self.data.dnd {
                        theme.colors.text_muted
                    } else {
                        theme.colors.text
                    })),
            )
            .when(unread > 0, |el| {
                el.child(
                    Label::new(unread.to_string())
                        .size(TextSize::XSmall)
                        .color(Color::Custom(theme.colors.accent)),
                )
            })
            .into_any_element()
    }

    /// The bell icon for the current do-not-disturb state.
    fn icon(&self, cx: &Context<Self>) -> IconSource {
        let config = &cx.config().notification;
        if self.data.dnd {
            config.icons.bell_off()
        } else {
            config.icons.bell()
        }
    }
}

impl BarWidget for NotificationWidget {
    fn on_click(&self, cx: &mut Context<Self>) -> Option<BarClick> {
        Some(Box::new(cx.listener(
            |this, event: &ClickEvent, window, cx| this.toggle_center(event.position(), window, cx),
        )))
    }

    fn render_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let icon = self.icon(cx);
        self.render_widget_content(&theme, icon, true)
    }

    fn render_horizontal(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let icon = self.icon(cx);
        self.render_widget_content(&theme, icon, false)
    }
}

impl Render for NotificationWidget {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        self.render_bar_widget(window, cx)
    }
}
