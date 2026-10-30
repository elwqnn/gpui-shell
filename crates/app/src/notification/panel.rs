use gpui::prelude::*;
use gpui::{Context, MouseButton, Render, ScrollHandle, Window, div, px};
use services::{NotificationCommand, NotificationData, NotificationSubscriber};
use ui::patterns::PanelSurface;
use ui::{ActiveTheme, Color, Icon, IconSize, Radius, Spacing, TextSize};

use crate::config::ActiveConfig;
use crate::keybinds::Cancel;
use crate::panel::{PanelFocus, panel_background};
use crate::state::watch;

use super::card::notification_card_body;
use super::dispatch_notification_command;

pub(crate) struct NotificationCenter {
    subscriber: NotificationSubscriber,
    data: NotificationData,
    scroll_handle: ScrollHandle,
    focus: PanelFocus,
}

impl NotificationCenter {
    pub(crate) fn new(subscriber: NotificationSubscriber, cx: &mut Context<Self>) -> Self {
        let data = subscriber.get();
        let scroll_handle = ScrollHandle::new();
        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            cx.notify();
        });

        Self {
            subscriber,
            data,
            scroll_handle,
            focus: PanelFocus::new(cx),
        }
    }
}

impl Render for NotificationCenter {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl gpui::IntoElement {
        let focus = self.focus.arm(window, cx).clone();
        let theme = cx.theme();
        let config = &cx.config().notification;
        let center_height = config.center_height;
        let notifications = self.data.notifications.clone();
        let has_notifications = !notifications.is_empty();
        let dnd_enabled = self.data.dnd;
        let dnd_subscriber = self.subscriber.clone();
        let clear_subscriber = self.subscriber.clone();
        let list_content = if has_notifications {
            div()
                .children(notifications.into_iter().map(|item| {
                    let dismiss_subscriber = self.subscriber.clone();
                    let id = item.id;
                    div()
                        .relative()
                        .w_full()
                        .p(Spacing::Medium.pixels())
                        .rounded(Radius::Large.pixels())
                        .bg(theme.colors.background)
                        .border_1()
                        .border_color(theme.colors.border)
                        .child(notification_card_body(&item, cx, true, &self.subscriber))
                        .child(
                            div()
                                .absolute()
                                .top(px(8.0))
                                .right(px(8.0))
                                .cursor_pointer()
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(move |_, _, _, _cx| {
                                        dispatch_notification_command(
                                            dismiss_subscriber.clone(),
                                            NotificationCommand::Dismiss(id),
                                        );
                                    }),
                                )
                                .child(
                                    Icon::new(config.icons.close())
                                        .size(IconSize::XSmall)
                                        .color(Color::Muted),
                                ),
                        )
                }))
                .into_any_element()
        } else {
            div()
                .py(Spacing::XXXLarge.pixels())
                .text_size(TextSize::Small.rems())
                .text_color(theme.colors.text_muted)
                .text_center()
                .child("No notifications")
                .into_any_element()
        };

        div()
            .id("notification-center")
            .track_focus(&focus)
            .key_context("Panel")
            .on_action(|_: &Cancel, window, _| crate::panel::dismiss(window))
            .w_full()
            .h(px(center_height))
            .flex()
            .flex_col()
            .bar_panel(panel_background(cx), cx)
            // No title, like every bar panel: the row above the list is what
            // acts on it, nothing else.
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(Spacing::Medium.pixels())
                    .child(
                        div()
                            .cursor_pointer()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, _, _, _cx| {
                                    dispatch_notification_command(
                                        dnd_subscriber.clone(),
                                        NotificationCommand::SetDnd(!dnd_enabled),
                                    );
                                }),
                            )
                            .child(Icon::new(config.icons.dnd()).size(IconSize::XSmall).color(
                                Color::Custom(if dnd_enabled {
                                    theme.colors.accent
                                } else {
                                    theme.colors.text
                                }),
                            )),
                    )
                    .child(
                        div()
                            .cursor_pointer()
                            .text_size(TextSize::Small.rems())
                            .text_color(theme.colors.text)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |_, _, _, _cx| {
                                    dispatch_notification_command(
                                        clear_subscriber.clone(),
                                        NotificationCommand::DismissAll,
                                    );
                                }),
                            )
                            .child("Clear"),
                    ),
            )
            .child(
                div()
                    .id("notification-center-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .track_scroll(&self.scroll_handle)
                    .flex()
                    .flex_col()
                    .gap(Spacing::XSmall.pixels())
                    .child(list_content),
            )
    }
}
