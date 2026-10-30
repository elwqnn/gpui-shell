//! MPRIS widget showing active media state and opening a players panel.

mod config;
pub use config::MprisConfig;

use gpui::{AnyElement, App, ClickEvent, Context, Pixels, Point, Size, Window, prelude::*, px};
use services::{MprisData, MprisPlayerData, PlaybackStatus};
use ui::{ActiveTheme, Color, Icon, IconName, LabelCommon};

use super::{BarClick, BarWidget, style};
use crate::config::{ActiveConfig, Config, State};
use crate::panel::{PanelConfig, panel_anchor, toggle_panel};
use crate::state::AppState;
use crate::state::watch;

mod panel;
pub use panel::MprisPanel;
use panel::{MPRIS_PANEL_HEIGHT, MPRIS_PANEL_WIDTH};

/// Bus name prefix every MPRIS player shares.
const SERVICE_PREFIX: &str = "org.mpris.MediaPlayer2.";

/// The app part of a bus name, without the prefix and the per-launch
/// instance suffix: `org.mpris.MediaPlayer2.firefox.instance_1_88` is
/// `firefox`. Stable across restarts, so a saved choice stores this.
pub fn player_key(service: &str) -> &str {
    let name = service.strip_prefix(SERVICE_PREFIX).unwrap_or(service);
    name.split_once('.').map_or(name, |(app, _instance)| app)
}

/// The player the bar chip reports on: the one picked in the panel, else
/// whatever is playing.
pub fn followed_player<'a>(data: &'a MprisData, cx: &App) -> Option<&'a MprisPlayerData> {
    let picked = State::mpris_player(cx)
        .and_then(|key| data.players.iter().find(|p| player_key(&p.service) == key));

    picked.or_else(|| {
        data.players
            .iter()
            .find(|p| p.state == PlaybackStatus::Playing)
            .or_else(|| data.players.first())
    })
}

/// Drop a saved choice once its player leaves the bus, so the shell goes back
/// to following playback until the user picks again. Only while something is
/// playing at all: an empty list is also what startup looks like.
pub fn forget_missing_player(data: &MprisData, cx: &mut App) {
    let Some(picked) = State::mpris_player(cx) else {
        return;
    };
    let gone = !data.players.is_empty()
        && !data
            .players
            .iter()
            .any(|player| player_key(&player.service) == picked);
    if gone {
        State::set_mpris_player(None, cx);
    }
}

/// Bar widget for media status and controls.
pub struct Mpris {
    subscriber: services::MprisSubscriber,
    data: MprisData,
}

impl Mpris {
    pub fn new(cx: &mut Context<Self>) -> Self {
        let subscriber = AppState::mpris(cx).clone();
        let data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            forget_missing_player(&this.data, cx);
            cx.notify();
        });

        // The panel picks the player from another window, through `state.toml`.
        cx.observe_global::<State>(|_, cx| cx.notify()).detach();

        Self { subscriber, data }
    }

    fn toggle_panel(&self, at: Point<Pixels>, window: &Window, cx: &mut App) {
        let subscriber = self.subscriber.clone();
        let bar_position = Config::global(cx).bar.position;
        let (anchor_rect, anchor, gravity) = panel_anchor(bar_position, at, window.viewport_size());
        let config = PanelConfig {
            parent: window.window_handle(),
            anchor_rect,
            anchor,
            gravity,
            size: Size::new(px(MPRIS_PANEL_WIDTH), px(MPRIS_PANEL_HEIGHT)),
        };

        toggle_panel("mpris", config, cx, move |cx| {
            MprisPanel::new(subscriber, cx)
        });
    }

    fn icon(&self, cx: &App) -> IconName {
        match followed_player(&self.data, cx).map(|p| p.state) {
            Some(PlaybackStatus::Playing) => IconName::Play,
            Some(PlaybackStatus::Paused) => IconName::Pause,
            Some(PlaybackStatus::Stopped) => IconName::Stop,
            None => IconName::Stop,
        }
    }

    fn label(&self, cx: &App) -> String {
        let Some(player) = followed_player(&self.data, cx) else {
            return "No media".to_string();
        };

        if let Some(metadata) = &player.metadata {
            let value = metadata.to_string();
            if !value.is_empty() {
                return value;
            }
        }

        player_key(&player.service).to_string()
    }

    fn render_widget_content(
        &self,
        theme: &ui::Theme,
        icon: IconName,
        label: Option<String>,
        is_vertical: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let max_width = cx.config().bar.modules.mpris.max_width;

        style::stack(is_vertical)
            .justify_center()
            .gap(px(style::CHIP_GAP))
            .max_w(px(max_width))
            .overflow_hidden()
            .child(
                Icon::new(icon)
                    .size(style::icon(is_vertical))
                    .color(Color::Default),
            )
            .when_some(label, |el, label| {
                el.child(style::bar_label(label, is_vertical, theme.colors.text).truncate())
            })
            .into_any_element()
    }
}

impl BarWidget for Mpris {
    fn on_click(&self, cx: &mut Context<Self>) -> Option<BarClick> {
        Some(Box::new(cx.listener(
            |this, event: &ClickEvent, window, cx| {
                this.toggle_panel(event.position(), window, cx);
            },
        )))
    }

    fn render_vertical(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let icon = self.icon(cx);
        self.render_widget_content(&theme, icon, None, true, cx)
    }

    fn render_horizontal(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let icon = self.icon(cx);
        let label = self.label(cx);
        self.render_widget_content(&theme, icon, Some(label), false, cx)
    }
}

impl Render for Mpris {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.render_bar_widget(window, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::player_key;

    #[test]
    fn the_player_key_drops_the_instance_suffix() {
        assert_eq!(
            player_key("org.mpris.MediaPlayer2.firefox.instance_1_88"),
            "firefox"
        );
        assert_eq!(
            player_key("org.mpris.MediaPlayer2.plasma-browser-integration"),
            "plasma-browser-integration"
        );
        assert_eq!(player_key("org.mpris.MediaPlayer2.spotify"), "spotify");
        assert_eq!(player_key("weird-name"), "weird-name");
    }
}
