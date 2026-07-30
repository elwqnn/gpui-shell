//! The menu a tray item drops down when it has one.
//!
//! Lives beside its widget, like every other bar module's panel.

use gpui::{
    AvailableSpace, Context, ElementId, Render, SharedString, Size, Window, div, prelude::*, px,
};
use services::{MenuLayout, MenuLayoutProps, TrayCommand};
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, Clickable, Color, Disableable, Divider, Icon, IconName, IconSize, Label,
    LabelCommon, List, ListItem, Spacing, TextSize,
};

use crate::keybinds::Cancel;
use crate::panel::PanelFocus;
use crate::state::watch;

/// The menu card's width. Fixed: only the height follows the content.
pub(super) const TRAY_MENU_WIDTH: f32 = 250.0;
/// How tall the menu may grow before it starts scrolling instead.
pub(super) const TRAY_MENU_MAX_HEIGHT: f32 = 400.0;

/// Panel content for displaying a tray item's menu.
pub(super) struct TrayMenuPanel {
    menu: MenuLayout,
    item_name: String,
    subscriber: services::TraySubscriber,
    /// Track which submenus are expanded (by menu ID)
    expanded_submenus: Vec<i32>,
    focus: PanelFocus,
}

impl TrayMenuPanel {
    pub(super) fn new(
        menu: MenuLayout,
        item_name: String,
        subscriber: services::TraySubscriber,
        cx: &mut Context<Self>,
    ) -> Self {
        // Subscribe to tray data updates so the menu refreshes live
        // (e.g. after about_to_show triggers a layout_updated signal)
        let name = item_name.clone();
        watch(cx, subscriber.subscribe(), move |this, data, cx| {
            if let Some(item) = data.items.iter().find(|i| i.name == name)
                && let Some(menu) = &item.menu
            {
                this.menu = menu.clone();
                cx.notify();
            }
        });

        Self {
            menu,
            item_name,
            subscriber,
            expanded_submenus: Vec::new(),
            focus: PanelFocus::new(cx),
        }
    }

    /// Handle clicking on a menu item.
    fn activate_menu_item(&self, menu_id: i32, window: &mut Window, cx: &mut Context<Self>) {
        let subscriber = self.subscriber.clone();
        let item_name = self.item_name.clone();
        cx.spawn(async move |_, _| {
            let _ = subscriber
                .dispatch(TrayCommand::MenuItemClicked { item_name, menu_id })
                .await;
        })
        .detach();

        // Close the menu panel
        crate::panel::dismiss(window);
    }

    /// Toggle submenu expansion state.
    /// Calls about_to_show when expanding to trigger lazy menu population.
    fn toggle_submenu(&mut self, menu_id: i32, cx: &mut Context<Self>) {
        if let Some(pos) = self.expanded_submenus.iter().position(|&id| id == menu_id) {
            self.expanded_submenus.remove(pos);
        } else {
            // Notify the app to populate the submenu before expanding
            let subscriber = self.subscriber.clone();
            let item_name = self.item_name.clone();
            cx.spawn(async move |_, _| {
                let _ = subscriber
                    .dispatch(TrayCommand::AboutToShow { item_name, menu_id })
                    .await;
            })
            .detach();
            self.expanded_submenus.push(menu_id);
        }
        cx.notify();
    }

    /// Check if a submenu is expanded
    fn is_submenu_expanded(&self, menu_id: i32) -> bool {
        self.expanded_submenus.contains(&menu_id)
    }

    /// Render menu items recursively with collapsible submenus.
    fn render_menu_items(
        &self,
        items: &[MenuLayout],
        depth: usize,
        cx: &mut Context<Self>,
    ) -> Vec<gpui::AnyElement> {
        let mut elements = Vec::new();

        for layout in items {
            let MenuLayout(id, props, children) = layout;

            // Skip invisible items
            if props.visible == Some(false) {
                continue;
            }

            let label = props
                .label
                .as_ref()
                .map(|l| l.replace('_', ""))
                .unwrap_or_default();

            // Handle separator
            if props.type_.as_deref() == Some("separator")
                || (label.is_empty() && children.is_empty())
            {
                elements.push(render_menu_separator().into_any_element());
                continue;
            }

            let menu_id = *id;
            let is_enabled = props.enabled.unwrap_or(true);
            let has_submenu = !children.is_empty();
            let is_expanded = has_submenu && self.is_submenu_expanded(menu_id);

            elements.push(
                render_menu_item(
                    menu_id,
                    &label,
                    props,
                    is_enabled,
                    has_submenu,
                    is_expanded,
                    depth,
                    cx,
                )
                .into_any_element(),
            );

            // Render submenu items if expanded
            if is_expanded {
                let submenu_elements = self.render_menu_items(children, depth + 1, cx);
                elements.extend(submenu_elements);
            }
        }

        elements
    }
}

/// Render a separator line
fn render_menu_separator() -> impl IntoElement {
    div()
        .w_full()
        .px(Spacing::Medium.pixels())
        .py(Spacing::XSmall.pixels())
        .child(Divider::horizontal())
}

/// Render a single menu item
#[allow(clippy::too_many_arguments)]
fn render_menu_item(
    menu_id: i32,
    label: &str,
    props: &MenuLayoutProps,
    is_enabled: bool,
    has_submenu: bool,
    is_expanded: bool,
    depth: usize,
    cx: &mut Context<TrayMenuPanel>,
) -> impl IntoElement {
    // Checkbox/radio indicator
    let toggle_indicator = props.toggle_type.as_ref().and_then(|toggle_type| {
        let is_checked = props.toggle_state == Some(1);
        let icon = match (toggle_type.as_str(), is_checked) {
            ("checkmark", true) => IconName::Check,
            ("radio", true) => IconName::CheckCircle,
            ("checkmark" | "radio", false) => IconName::Square,
            _ => return None,
        };
        Some((icon, is_checked))
    });

    ListItem::new(ElementId::Name(SharedString::from(format!(
        "menu-item-{}",
        menu_id
    ))))
    // A submenu row stays clickable even when the item itself is disabled -
    // collapsing it is the panel's own affordance, not the app's action.
    .disabled(!is_enabled && !has_submenu)
    .indent_level(depth)
    .indent_step_size(Spacing::Large.pixels())
    .on_click(cx.listener(move |this, _, window, cx| {
        if has_submenu {
            this.toggle_submenu(menu_id, cx);
        } else {
            this.activate_menu_item(menu_id, window, cx);
        }
    }))
    .when_some(toggle_indicator, |el, (icon, is_checked)| {
        el.start_slot(Icon::new(icon).size(IconSize::Small).color(if is_checked {
            Color::Accent
        } else {
            Color::Muted
        }))
    })
    .child(
        Label::new(label.to_string())
            .size(TextSize::Small)
            .color(if is_enabled {
                Color::Default
            } else {
                Color::Disabled
            })
            .truncate(),
    )
    .when(has_submenu, |el| {
        el.end_slot(
            Icon::new(if is_expanded {
                IconName::ChevronDown
            } else {
                IconName::ChevronRight
            })
            .size(IconSize::XSmall)
            .color(Color::Muted),
        )
    })
}

impl Render for TrayMenuPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.arm(window, cx).clone();

        // Built twice: once to measure the menu, once to show it. The card
        // scrolls itself, so its own `max_h` is what bounds a long menu.
        let build_root = |cx: &mut Context<Self>| {
            let menu_items = self.render_menu_items(&self.menu.2, 0, cx);

            div()
                .id("systray-menu-panel")
                .track_focus(&focus)
                .key_context("Panel")
                .on_action(|_: &Cancel, window, _| crate::panel::dismiss(window))
                .w_full()
                .max_h(px(TRAY_MENU_MAX_HEIGHT))
                .py(Spacing::XSmall.pixels())
                .panel_surface(cx)
                .text_color(cx.theme().colors.text)
                .overflow_x_hidden()
                .overflow_y_scroll()
                .child(
                    List::new()
                        .empty_message("This item has no menu")
                        .children(menu_items),
                )
        };

        // A three-entry menu should not open a 400px card. The window keeps
        // the anchor it was placed on, so it grows and shrinks from there.
        let content_height = build_root(cx)
            .into_any_element()
            .layout_as_root(
                Size {
                    width: AvailableSpace::Definite(px(TRAY_MENU_WIDTH)),
                    height: AvailableSpace::MaxContent,
                },
                window,
                cx,
            )
            .height;
        let desired = Size::new(
            px(TRAY_MENU_WIDTH),
            content_height.min(px(TRAY_MENU_MAX_HEIGHT)),
        );
        if window.viewport_size() != desired {
            window.resize(desired);
        }

        build_root(cx)
    }
}
