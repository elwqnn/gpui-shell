//! The settings window: every `config.toml` value, and a way to change it.
//!
//! A surface of its own - a centred overlay, not a bar panel - opened from the
//! control center's gear. It reads the live config, flattened into
//! `bar.position` rows by [`crate::config::flatten`], and writes changes
//! straight back into `config.toml`, the one place a setting lives. The rest
//! of the file is left alone, and a value the config cannot hold is refused
//! before anything is written.
//!
//! Sections live in a sidebar; the section's values fill the pane beside it.
//! A value with a known set of spellings - an edge, a hide mode, a bar section's
//! widgets - is shown as all of them at once, so nothing has to be typed from
//! memory. Everything else is edited as the TOML text it is.

use std::collections::HashMap;

use gpui::{
    Action, App, Bounds, Context, Entity, FocusHandle, InteractiveElement, MouseButton, Point,
    Render, Size, Window, WindowBackgroundAppearance, WindowBounds, WindowHandle, WindowKind,
    WindowOptions, div, layer_shell::*, prelude::*, px,
};
use toml::Value;
use ui::patterns::PanelSurface;
use ui::{
    ActiveTheme, Button, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Icon, IconName,
    IconSize, InputBuffer, Label, LabelCommon, Radius, SelectableButton, Spacing, Switch, TextSize,
    TintColor, Toggleable, render_input_line,
};

use crate::bar::modules::Widget;
use crate::config::{ActiveConfig, Config, Setting, flatten, parse_value};
use crate::keybinds::{
    Backspace, Cancel, Confirm, CursorLeft, CursorRight, DeleteWordBack, SelectAll, SelectLeft,
    SelectRight, SelectWordLeft, SelectWordRight, WordLeft, WordRight,
};

const WINDOW_WIDTH: f32 = 760.0;
const WINDOW_HEIGHT: f32 = 520.0;
const SIDEBAR_WIDTH: f32 = 176.0;

/// The settings surface: a sidebar of sections, and the open section's values.
pub struct SettingsWindow {
    focus: FocusHandle,
    /// What every setting is worth with no config at all, so a row can say
    /// which values this machine actually changed.
    defaults: HashMap<String, Value>,
    /// The section the pane is showing.
    section: String,
    /// The row being typed into, if any.
    editing: Option<Edit>,
    /// Why the last change did not apply.
    error: Option<String>,
}

/// A value being typed. One at a time - the pane is a list, not a form.
struct Edit {
    path: String,
    input: InputBuffer,
}

impl SettingsWindow {
    fn new(cx: &mut Context<Self>) -> Self {
        let defaults = flatten(&Config::default());
        let section = defaults
            .first()
            .map_or_else(|| "general".to_string(), |setting| setting.section().into());

        Self {
            focus: cx.focus_handle(),
            defaults: defaults
                .into_iter()
                .map(|setting| (setting.path, setting.value))
                .collect(),
            section,
            editing: None,
            error: None,
        }
    }

    /// Start typing a value, seeded with the TOML it currently holds.
    fn edit(&mut self, setting: &Setting, cx: &mut Context<Self>) {
        self.editing = Some(Edit {
            path: setting.path.clone(),
            input: InputBuffer::new(setting.text()),
        });
        self.error = None;
        cx.notify();
    }

    /// Apply what was typed. The config refuses anything it cannot hold, which
    /// is the only validation the window needs: the value has to survive a real
    /// parse before it reaches `config.toml`.
    fn commit(&mut self, cx: &mut Context<Self>) {
        let Some(edit) = self.editing.take() else {
            return;
        };

        let value = parse_value(edit.input.text());
        self.apply(&edit.path, Config::set(&edit.path, Some(value), cx));
        cx.notify();
    }

    fn set(&mut self, path: &str, value: Value, cx: &mut Context<Self>) {
        self.apply(path, Config::set(path, Some(value), cx));
        cx.notify();
    }

    /// Drop the key from `config.toml`, which is what puts a setting back to
    /// the value it has when nobody configures it.
    fn reset(&mut self, path: &str, cx: &mut Context<Self>) {
        self.apply(path, Config::set(path, None, cx));
        cx.notify();
    }

    /// Whether this machine's config changed the setting at all.
    fn is_changed(&self, setting: &Setting) -> bool {
        self.defaults.get(&setting.path) != Some(&setting.value)
    }

    fn apply(&mut self, path: &str, result: anyhow::Result<()>) {
        self.error = match result {
            Ok(()) => None,
            Err(err) => {
                tracing::warn!("Failed to change {path}: {err:#}");
                Some(format!("{path}: not a value this setting accepts"))
            }
        };
    }

    /// Escape backs out one step: the value being typed, then the window.
    fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing.take().is_some() {
            self.error = None;
            cx.notify();
        } else {
            window.remove_window();
        }
    }

    /// Title bar: what this window is, and the way out of it.
    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        div()
            .flex()
            .items_center()
            .justify_between()
            .px(Spacing::XLarge.pixels())
            .py(Spacing::Large.pixels())
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .child(
                        Icon::new(IconName::Settings)
                            .size(IconSize::Small)
                            .color(Color::Muted),
                    )
                    .child(Label::new("Settings").size(TextSize::Medium)),
            )
            .child(
                div()
                    .id("settings-close")
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(24.))
                    .rounded(Radius::Small.pixels())
                    .cursor_pointer()
                    .hover(|s| s.bg(colors.element_hover))
                    .on_mouse_down(MouseButton::Left, |_, window, _| {
                        window.remove_window();
                    })
                    .child(
                        Icon::new(IconName::Close)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }

    /// One row per top-level section, with how many values it holds.
    fn render_sidebar(&self, settings: &[Setting], cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();

        div()
            .id("settings-sidebar")
            .w(px(SIDEBAR_WIDTH))
            .flex_none()
            .flex()
            .flex_col()
            .gap(px(2.))
            .p(Spacing::Medium.pixels())
            .border_r_1()
            .border_color(colors.border)
            .overflow_y_scroll()
            .children(sections(settings).into_iter().map(|(name, count)| {
                let open = name == self.section;
                let entity = cx.entity().clone();
                let section = name.to_string();

                div()
                    .id(gpui::SharedString::from(format!("section-{name}")))
                    .flex()
                    .items_center()
                    .gap(Spacing::Small.pixels())
                    .px(Spacing::Medium.pixels())
                    .py(Spacing::Small.pixels())
                    .rounded(Radius::Medium.pixels())
                    .cursor_pointer()
                    .when(open, |el| el.bg(colors.element_selected))
                    .when(!open, |el| el.hover(|s| s.bg(colors.element_hover)))
                    .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                        let section = section.clone();
                        entity.update(cx, |this, cx| {
                            this.section = section;
                            this.editing = None;
                            this.error = None;
                            cx.notify();
                        });
                    })
                    .child(
                        Icon::new(section_icon(name))
                            .size(IconSize::Small)
                            .color(if open { Color::Accent } else { Color::Muted }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .child(Label::new(section_title(name)).size(TextSize::Small)),
                    )
                    .child(
                        Label::new(count.to_string())
                            .size(TextSize::XSmall)
                            .color(Color::Disabled),
                    )
            }))
    }

    /// One row per value in the open section.
    fn render_setting(&self, setting: &Setting, cx: &mut Context<Self>) -> gpui::AnyElement {
        let changed = self.is_changed(setting);
        let path = setting.path.clone();
        let value = self.render_value(setting, cx);
        let colors = cx.theme().colors();
        // Choice chips take the width they need, so they sit under the label
        // rather than fighting it for the row.
        let stacked = choices(&setting.path).is_some_and(|choices| choices.len() > 3);
        let (inline, below) = match stacked {
            true => (None, Some(value)),
            false => (Some(value), None),
        };

        div()
            .flex()
            .flex_col()
            .gap(Spacing::Small.pixels())
            .px(Spacing::Medium.pixels())
            .py(Spacing::Small.pixels())
            .rounded(Radius::Medium.pixels())
            .hover(|s| s.bg(colors.element_hover))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(Spacing::Medium.pixels())
                    .child(
                        Label::new(setting.label().to_string())
                            .size(TextSize::Small)
                            .color(if changed {
                                Color::Accent
                            } else {
                                Color::Default
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(Spacing::Small.pixels())
                            .children(inline)
                            .when(changed, |el| {
                                let entity = cx.entity().clone();
                                el.child(
                                    div()
                                        .id(gpui::SharedString::from(format!("reset-{path}")))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .size(px(20.))
                                        .rounded(Radius::Small.pixels())
                                        .cursor_pointer()
                                        .hover(|s| s.bg(colors.element_active))
                                        .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                                            let path = path.clone();
                                            entity.update(cx, |this, cx| this.reset(&path, cx));
                                        })
                                        .child(
                                            Icon::new(IconName::RotateCcw)
                                                .size(IconSize::XSmall)
                                                .color(Color::Muted),
                                        ),
                                )
                            }),
                    ),
            )
            .children(below)
            .into_any_element()
    }

    /// Booleans toggle in place, a value with a known set of spellings shows
    /// all of them, and everything else is edited as TOML text - which is what
    /// makes numbers, formats, arrays and tables all work at once.
    fn render_value(&self, setting: &Setting, cx: &mut Context<Self>) -> gpui::AnyElement {
        let colors = cx.theme().colors();
        let entity = cx.entity().clone();
        let path = setting.path.clone();

        if let Value::Boolean(on) = setting.value {
            return Switch::new(format!("switch-{}", setting.path), on)
                .on_click(move |state, _, cx| {
                    let path = path.clone();
                    let on = *state == ui::ToggleState::Selected;
                    entity.update(cx, |this, cx| this.set(&path, Value::Boolean(on), cx));
                })
                .into_any_element();
        }

        if let Some(choices) = choices(&setting.path) {
            return self.render_choices(setting, choices, cx);
        }

        if let Some(edit) = self
            .editing
            .as_ref()
            .filter(|edit| edit.path == setting.path)
        {
            return div()
                .min_w(px(140.))
                .max_w(px(280.))
                .px(Spacing::Small.pixels())
                .py(px(2.))
                .rounded(Radius::Small.pixels())
                .bg(colors.surface_background)
                .border_1()
                .border_color(colors.border_focused)
                .text_size(TextSize::XSmall.rems())
                .child(render_input_line(&edit.input, "value", cx))
                .into_any_element();
        }

        let text = setting_text(setting);
        let setting = setting.clone();
        div()
            .id(gpui::SharedString::from(format!("value-{}", setting.path)))
            .max_w(px(280.))
            .px(Spacing::Small.pixels())
            .py(px(2.))
            .rounded(Radius::Small.pixels())
            .bg(colors.element_background)
            .cursor_pointer()
            .hover(|s| s.bg(colors.element_active))
            .on_mouse_down(MouseButton::Left, move |_, _, cx| {
                let setting = setting.clone();
                entity.update(cx, |this, cx| this.edit(&setting, cx));
            })
            .child(Label::new(text).size(TextSize::XSmall).color(Color::Muted))
            .into_any_element()
    }

    /// Every spelling the setting takes, the current one lit. A list-valued
    /// setting - a bar section's widgets - toggles each choice in and out
    /// instead of replacing the value.
    fn render_choices(
        &self,
        setting: &Setting,
        choices: &'static [&'static str],
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let list = setting.value.as_array().map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        });

        div()
            .flex()
            .flex_wrap()
            .gap(Spacing::XSmall.pixels())
            .children(choices.iter().map(|choice| {
                let selected = match &list {
                    Some(list) => list.iter().any(|held| held == choice),
                    None => setting.value.as_str() == Some(choice),
                };
                let entity = cx.entity().clone();
                let path = setting.path.clone();
                let list = list.clone();
                let choice = *choice;

                Button::new(gpui::SharedString::from(format!("{path}-{choice}")), choice)
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Outlined)
                    .toggle_state(selected)
                    .selected_style(ButtonStyle::Tinted(TintColor::Accent))
                    .on_click(move |_, _, cx| {
                        let value = match &list {
                            // Toggling a widget keeps the order the rest are in.
                            Some(list) => Value::Array(
                                match selected {
                                    true => list
                                        .iter()
                                        .filter(|held| *held != choice)
                                        .cloned()
                                        .collect(),
                                    false => {
                                        let mut list = list.clone();
                                        list.push(choice.to_string());
                                        list
                                    }
                                }
                                .into_iter()
                                .map(Value::String)
                                .collect(),
                            ),
                            None => Value::String(choice.to_string()),
                        };
                        let path = path.clone();
                        entity.update(cx, |this, cx| this.set(&path, value, cx));
                    })
            }))
            .into_any_element()
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            self.focus.focus(window, cx);
        }

        let entity = cx.entity().clone();
        let settings = flatten(cx.config());
        let rows: Vec<_> = settings
            .iter()
            .filter(|setting| setting.section() == self.section)
            .map(|setting| self.render_setting(setting, cx))
            .collect();

        let mut root = div()
            .id("settings-window")
            .track_focus(&self.focus)
            .key_context("Settings")
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .text_color(cx.theme().colors.text)
            .panel_surface(cx)
            .on_action({
                let entity = entity.clone();
                move |_: &Cancel, window, cx| {
                    entity.update(cx, |this, cx| this.back(window, cx));
                }
            })
            .on_action({
                let entity = entity.clone();
                move |_: &Confirm, _, cx| {
                    entity.update(cx, |this, cx| this.commit(cx));
                }
            })
            .on_key_down({
                let entity = entity.clone();
                move |event, _, cx| {
                    if event.keystroke.modifiers.control || event.keystroke.modifiers.alt {
                        return;
                    }

                    let Some(ch) = typed_char(event) else {
                        return;
                    };

                    entity.update(cx, |this, cx| {
                        if let Some(edit) = &mut this.editing {
                            edit.input.insert_str(&ch.to_string());
                            cx.notify();
                        }
                    });
                }
            });

        // The editing keys, all doing the same thing to the same buffer.
        root = on_edit::<Backspace, _>(root, &entity, |input| input.backspace());
        root = on_edit::<DeleteWordBack, _>(root, &entity, |input| input.delete_word_back());
        root = on_edit::<CursorLeft, _>(root, &entity, |input| input.move_left(false));
        root = on_edit::<CursorRight, _>(root, &entity, |input| input.move_right(false));
        root = on_edit::<WordLeft, _>(root, &entity, |input| input.move_word_left(false));
        root = on_edit::<WordRight, _>(root, &entity, |input| input.move_word_right(false));
        root = on_edit::<SelectLeft, _>(root, &entity, |input| input.move_left(true));
        root = on_edit::<SelectRight, _>(root, &entity, |input| input.move_right(true));
        root = on_edit::<SelectWordLeft, _>(root, &entity, |input| input.move_word_left(true));
        root = on_edit::<SelectWordRight, _>(root, &entity, |input| input.move_word_right(true));
        root = on_edit::<SelectAll, _>(root, &entity, |input| input.select_all());

        root.child(self.render_header(cx)).child(
            div()
                .flex_1()
                .flex()
                .overflow_hidden()
                .child(self.render_sidebar(&settings, cx))
                .child(
                    div()
                        .id("settings-pane")
                        .flex_1()
                        .flex()
                        .flex_col()
                        .gap(Spacing::XSmall.pixels())
                        .p(Spacing::Large.pixels())
                        .overflow_y_scroll()
                        .children(self.error.clone().map(|error| {
                            Label::new(error).size(TextSize::XSmall).color(Color::Error)
                        }))
                        .children(rows),
                ),
        )
    }
}

/// Bind one editing action to the buffer being typed into.
fn on_edit<A: Action, E: InteractiveElement>(
    element: E,
    entity: &Entity<SettingsWindow>,
    change: impl Fn(&mut InputBuffer) + 'static,
) -> E {
    let entity = entity.clone();
    element.on_action(move |_: &A, _, cx| {
        entity.update(cx, |this, cx| {
            if let Some(edit) = &mut this.editing {
                change(&mut edit.input);
                cx.notify();
            }
        });
    })
}

/// The character a keystroke types, if it types one.
fn typed_char(event: &gpui::KeyDownEvent) -> Option<char> {
    event
        .keystroke
        .key_char
        .as_ref()
        .and_then(|key| key.chars().next())
        .or_else(|| {
            let key = event.keystroke.key.as_str();
            (key.chars().count() == 1)
                .then(|| key.chars().next())
                .flatten()
        })
        .filter(|ch| !ch.is_control())
}

/// Values are shown as the TOML they are, cut to what a row can hold.
fn setting_text(setting: &Setting) -> String {
    const MAX: usize = 34;
    let text = setting.text();

    match text.char_indices().nth(MAX) {
        Some((end, _)) => format!("{}…", &text[..end]),
        None => text,
    }
}

/// The sections the settings fall into, in the order they arrive, with how
/// many values each holds.
fn sections(settings: &[Setting]) -> Vec<(&str, usize)> {
    let mut sections: Vec<(&str, usize)> = Vec::new();
    for setting in settings {
        match sections.last_mut() {
            Some((name, count)) if *name == setting.section() => *count += 1,
            _ => sections.push((setting.section(), 1)),
        }
    }
    sections
}

/// Every spelling a setting accepts, when it accepts a fixed set of them. The
/// config's own enums, and the widgets a bar section can name - the one list
/// the user would otherwise have to remember.
fn choices(path: &str) -> Option<&'static [&'static str]> {
    const EDGES: &[&str] = &["left", "right", "top", "bottom"];

    match path {
        "bar.position" | "dock.position" | "osd.position" => Some(EDGES),
        "bar.start" | "bar.center" | "bar.end" => Some(Widget::ALL),
        "notification.popup_position" => {
            Some(&["top-left", "top-right", "bottom-left", "bottom-right"])
        }
        "dock.monitors" => Some(&["primaryonly", "all"]),
        "dock.visibility" => Some(&["always-visible", "intelligent-hide", "dodge-windows"]),
        "dock.hover_effect" => Some(&["none", "lift", "magnify", "glow", "magnify-lift"]),
        _ if path.starts_with("services.") => Some(&["eager", "lazy", "off"]),
        _ => None,
    }
}

/// The section as a sidebar entry reads it: `control_center` is "Control center".
fn section_title(section: &str) -> String {
    let mut title = section.replace('_', " ");
    if let Some(first) = title.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    title
}

fn section_icon(section: &str) -> IconName {
    match section {
        "bar" => IconName::Layout,
        "launcher" => IconName::FolderSearch,
        "osd" => IconName::Sun,
        "notification" => IconName::Bell,
        "control_center" => IconName::Settings,
        "dock" => IconName::Menu,
        "services" => IconName::Gauge,
        _ => IconName::Sliders,
    }
}

/// The one settings window, if it is up.
static SETTINGS_WINDOW: std::sync::Mutex<Option<WindowHandle<SettingsWindow>>> =
    std::sync::Mutex::new(None);

/// Show the settings window, or do nothing when it is already up. A centred
/// overlay: it belongs to no bar, so no compositor anchor holds it.
pub fn open(cx: &mut App) {
    // Never held across an open or a close: both run a release handler that
    // takes it too.
    let open = SETTINGS_WINDOW.lock().unwrap().is_some();
    if open {
        return;
    }

    let options = WindowOptions {
        titlebar: None,
        window_bounds: Some(WindowBounds::Windowed(Bounds {
            origin: Point::new(px(0.), px(0.)),
            size: Size::new(px(WINDOW_WIDTH), px(WINDOW_HEIGHT)),
        })),
        app_id: Some("gpuishell-settings".to_string()),
        window_background: WindowBackgroundAppearance::Transparent,
        kind: WindowKind::LayerShell(LayerShellOptions {
            namespace: "settings".to_string(),
            layer: Layer::Overlay,
            // No anchor: the compositor centres it on the screen.
            anchor: Anchor::empty(),
            keyboard_interactivity: KeyboardInteractivity::Exclusive,
            ..Default::default()
        }),
        focus: true,
        ..Default::default()
    };

    let Ok(handle) = cx.open_window(options, |window, cx| {
        window.set_rem_size(cx.theme().font_size);
        cx.new(|cx| {
            cx.on_release(|_, _| {
                *SETTINGS_WINDOW.lock().unwrap() = None;
            })
            .detach();
            SettingsWindow::new(cx)
        })
    }) else {
        return;
    };

    *SETTINGS_WINDOW.lock().unwrap() = Some(handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    /// The sidebar's entries are the config's own top-level tables, and each
    /// has to hold some values - an empty pane would mean the flattening broke.
    #[test]
    fn the_sidebar_lists_the_config_by_section() {
        let settings = flatten(&Config::default());
        let sections = sections(&settings);

        for wanted in ["bar", "launcher", "osd", "notification", "dock"] {
            assert!(
                sections
                    .iter()
                    .any(|(name, count)| *name == wanted && *count > 0),
                "{wanted} has no editable values"
            );
        }
    }

    /// Every choice offered has to be one the config actually accepts, or the
    /// UI would write a file that then stops loading.
    #[test]
    fn every_choice_is_one_the_config_accepts() {
        for setting in flatten(&Config::default()) {
            let Some(choices) = choices(&setting.path) else {
                continue;
            };

            for choice in choices {
                // A list-valued setting takes its choices one per entry.
                let value = match setting.value.is_array() {
                    true => format!("[\"{choice}\"]"),
                    false => format!("\"{choice}\""),
                };
                let file = format!("{} = {value}\n", setting.path);

                toml::from_str::<Config>(&file)
                    .unwrap_or_else(|err| panic!("{} rejects {choice}: {err}", setting.path));
            }
        }
    }

    /// The widgets the bar ships with have to be offered, or a default config
    /// would show chips that are off while the bar renders them.
    #[test]
    fn every_default_bar_widget_is_offered() {
        let bar = Config::default().bar;

        for widget in bar.start.iter().chain(&bar.center).chain(&bar.end) {
            assert!(
                Widget::ALL.contains(&widget.as_str()),
                "{widget} is in the default bar but not offered"
            );
        }
    }

    #[test]
    fn a_long_value_is_cut_to_the_row() {
        let setting = Setting {
            path: "bar.font".to_string(),
            value: Value::String("x".repeat(60)),
        };

        assert_eq!(setting_text(&setting).chars().count(), 35);
    }

    #[test]
    fn a_section_reads_as_a_title() {
        assert_eq!(section_title("control_center"), "Control center");
        assert_eq!(section_title("bar"), "Bar");
    }
}
