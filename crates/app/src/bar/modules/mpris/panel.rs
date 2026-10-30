//! MPRIS panel with a list of players and transport controls.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use image::imageops::{self, FilterType};
use image::{Frame, ImageBuffer, Rgba as ImageRgba};

use gpui::{
    Animation, AnimationExt, AnyElement, App, Bounds, ContentMask, Context, Corners, ElementId,
    FontWeight, Hsla, ImageAssetLoader, ImageSource, ObjectFit, Path, PathBuilder, Pixels,
    RenderImage, Resource, Rgba, SharedString, StyledImage, WeakEntity, Window, canvas, div,
    img, point, prelude::*, px,
};
use services::{
    ManagedService, MprisCommand, MprisData, MprisSubscriber, PlaybackStatus, PlayerCommand,
    SpectrumData,
};
use ui::patterns::{PANEL_GAP, PanelSurface};
use ui::{
    ActiveTheme, ButtonCommon, ButtonSize, ButtonStyle, Clickable, Color, Disableable, Icon,
    IconButton, IconName, IconSize, Label, LabelCommon, Motion, Radius, Slider, Spacing, TextSize,
    Tooltip, h_flex, motion, v_flex,
};

use super::{followed_player, forget_missing_player, player_key};
use crate::config::{ActiveConfig, State};
use crate::keybinds::Cancel;
use crate::panel::{PanelFocus, panel_background};
use crate::state::{AppState, watch};

// Fixed size: the transport controls are laid out against it.
pub(super) const MPRIS_PANEL_WIDTH: f32 = 380.0;
pub(super) const MPRIS_PANEL_HEIGHT: f32 = 420.0;

/// Panel content for controlling media players exposed via MPRIS.
pub struct MprisPanel {
    subscriber: MprisSubscriber,
    data: MprisData,
    focus: PanelFocus,
    /// What each cover gave us, keyed by art, so one decode serves every frame.
    covers: HashMap<String, Cover>,
    /// Where the followed player is in its track, polled while the panel is up.
    position_us: Option<i64>,
    /// The state a click just asked for, shown until the player reports back.
    optimistic: Option<Optimistic>,
    /// Frequency bars behind the hero card, empty while the analyser is off.
    spectrum: SpectrumData,
    /// Held so the panel can stop the analyser when it closes.
    spectrum_service: Option<services::SpectrumSubscriber>,
}

impl Drop for MprisPanel {
    fn drop(&mut self) {
        // The analyser is only worth its CPU while it is on screen.
        if let Some(spectrum) = self.spectrum_service.as_ref() {
            spectrum.stop();
        }
    }
}

/// What a decoded cover is worth to the panel.
#[derive(Default)]
struct Cover {
    tint: Option<Hsla>,
    backdrop: Option<Arc<RenderImage>>,
}

/// A play/pause the user asked for, held until the player confirms it.
struct Optimistic {
    service: String,
    assumed: PlaybackStatus,
    /// What the player said before the click, so any change clears this.
    was: PlaybackStatus,
}

/// Side of the square album-art slot in a row, and in the hero card.
const COVER_SIZE: f32 = 40.0;
const HERO_COVER_SIZE: f32 = 96.0;

/// Side of the round play button facing the art, and the corner radius it
/// morphs between: a full circle when stopped, squarer while playing.
const PLAY_BUTTON_SIZE: f32 = 38.0;
const IDLE_RADIUS: f32 = PLAY_BUTTON_SIZE / 2.0;
const PLAYING_RADIUS: f32 = 14.0;

/// How much of the card's height the loudest point of the wave may take.
const SPECTRUM_HEIGHT: f32 = 0.75;

/// Side of the downscaled cover the card stretches into a backdrop.
const BACKDROP_SIZE: u32 = 16;

/// Points either side of each one that the wave averages over, as end-4 does.
const WAVE_SMOOTHING: usize = 2;

/// The radius everything painting edge to edge inside the hero card rounds
/// itself to, since `overflow_hidden` masks to a rectangle and leaves square
/// corners showing through the rounded ones.
///
/// The card's own radius, not the tighter curve its 1px border encloses: a
/// child sitting a pixel inside the card and curving on the same centre puts
/// its antialiased fringe straight on top of the border, which reads as the
/// child bleeding out of the corners. Rounding it as if it were the card
/// tucks that fringe inside instead.
fn card_radius() -> Pixels {
    Radius::XLarge.pixels()
}

/// A local cover has to reach gpui as a path: a `String` it reads as an
/// embedded asset name or a URL to fetch, never as a file on disk.
fn cover_resource(art: &str) -> Resource {
    if art.starts_with('/') {
        Resource::Path(PathBuf::from(art).into())
    } else {
        Resource::Uri(art.to_string().into())
    }
}

/// A tiny copy of the art, for the card to stretch back up as its backdrop.
/// GPUI has no blur, but a 16px source scaled over the whole card is one, and
/// a cheaper one than any filter: end-4 blurs the cover the same way, we just
/// let the sampler do it.
fn blurred_backdrop(image: &RenderImage) -> Option<Arc<RenderImage>> {
    let size = image.size(0);
    let (width, height) = (size.width.0.max(0) as u32, size.height.0.max(0) as u32);
    if width == 0 || height == 0 {
        return None;
    }

    let source: ImageBuffer<ImageRgba<u8>, Vec<u8>> =
        ImageBuffer::from_raw(width, height, image.as_bytes(0)?.to_vec())?;
    let small = imageops::resize(&source, BACKDROP_SIZE, BACKDROP_SIZE, FilterType::Triangle);
    Some(Arc::new(RenderImage::new(vec![Frame::new(small)])))
}

/// A moving average over the neighbours of each point, so a wave reads as one
/// line rather than fifty spikes.
fn smoothed(bars: &[f32], window: usize) -> Vec<f32> {
    (0..bars.len())
        .map(|index| {
            let low = index.saturating_sub(window);
            let high = (index + window).min(bars.len() - 1);
            let slice = &bars[low..=high];
            slice.iter().sum::<f32>() / slice.len() as f32
        })
        .collect()
}

/// The area under the wave: along the points, then back along the floor.
///
/// Tessellated by [`PathBuilder`] rather than assembled as a [`Path`], whose
/// `line_to` fans triangles out of the first vertex - correct for a convex
/// shape, and for a wave it draws spokes back to where the path started.
fn wave_path(bounds: Bounds<Pixels>, points: &[f32]) -> Option<Path<Pixels>> {
    if points.len() < 2 {
        return None;
    }

    let floor = bounds.bottom();
    let step = bounds.size.width / (points.len() - 1) as f32;
    let height = bounds.size.height * SPECTRUM_HEIGHT;

    let mut builder = PathBuilder::fill();
    builder.move_to(point(bounds.left(), floor));
    for (index, value) in points.iter().enumerate() {
        let x = bounds.left() + step * index as f32;
        builder.line_to(point(x, floor - height * value.clamp(0., 1.)));
    }
    builder.line_to(point(bounds.right(), floor));
    builder.close();
    builder.build().ok()
}

/// The card's rounded rectangle, as the bands a content mask can express.
///
/// gpui masks to rectangles - the same reason a square corner shows through a
/// rounded one - so the wave is painted once per band and its corners are
/// stepped in rather than curved away. Everything above the corners is one
/// band; the rest is a staircase down the arc, one band per pixel of radius,
/// which is as fine as the step can get before it stops being visible at all.
fn wave_masks(bounds: Bounds<Pixels>, radius: Pixels) -> Vec<Bounds<Pixels>> {
    let radius = radius.min(bounds.size.width / 2.).min(bounds.size.height);
    let floor = bounds.bottom();
    let steps = (radius.ceil().as_f32() as usize).max(1);
    let step = radius / steps as f32;

    let mut masks = vec![Bounds::from_corners(
        bounds.origin,
        point(bounds.right(), floor - radius),
    )];
    masks.extend((0..steps).map(|index| {
        let top = floor - radius + step * index as f32;
        let bottom = top + step;
        // Taken where the band is narrowest, at its bottom edge, so no part of
        // it reaches past the arc.
        let radius = radius.as_f32();
        let into_corner = radius - (floor - bottom).as_f32();
        let inset = px(radius - (radius * radius - into_corner * into_corner).max(0.).sqrt());

        Bounds::from_corners(
            point(bounds.left() + inset, top),
            point(bounds.right() - inset, bottom),
        )
    }));
    masks
}

/// The average colour of the art, pushed into a range that stays readable
/// behind text whatever the cover looks like.
fn cover_tint(image: &RenderImage) -> Option<Hsla> {
    let pixels = image.as_bytes(0)?;
    // BGRA, and every 16th pixel is plenty for an average.
    let (mut blue, mut green, mut red, mut count) = (0u64, 0u64, 0u64, 0u64);
    for pixel in pixels.chunks_exact(4).step_by(16) {
        blue += pixel[0] as u64;
        green += pixel[1] as u64;
        red += pixel[2] as u64;
        count += 1;
    }
    if count == 0 {
        return None;
    }

    let channel = |sum: u64| sum as f32 / count as f32 / 255.0;
    let mut tint: Hsla = Rgba {
        r: channel(red),
        g: channel(green),
        b: channel(blue),
        a: 1.0,
    }
    .into();
    tint.s = tint.s.min(0.7);
    tint.l = tint.l.clamp(0.35, 0.65);
    Some(tint)
}

impl MprisPanel {
    pub fn new(subscriber: MprisSubscriber, cx: &mut Context<Self>) -> Self {
        let data = subscriber.get();

        watch(cx, subscriber.subscribe(), |this, data, cx| {
            this.data = data;
            this.settle_optimistic();
            forget_missing_player(&this.data, cx);
            cx.notify();
        });

        Self::poll_position(subscriber.clone(), cx);

        cx.on_release(|this, cx| {
            for backdrop in this.covers.values_mut().filter_map(|c| c.backdrop.take()) {
                cx.drop_image(backdrop, None);
            }
        })
        .detach();

        let spectrum_service = cx
            .config()
            .bar
            .modules
            .mpris
            .show_spectrum
            .then(|| Self::start_spectrum(cx));

        Self {
            subscriber,
            data,
            focus: PanelFocus::new(cx),
            covers: HashMap::new(),
            position_us: None,
            optimistic: None,
            spectrum: SpectrumData::default(),
            spectrum_service,
        }
    }

    /// Bring the analyser up for as long as this panel lives, unless config
    /// turned the service off, and stream its frames in.
    fn start_spectrum(cx: &mut Context<Self>) -> services::SpectrumSubscriber {
        let spectrum = AppState::spectrum(cx).clone();
        spectrum.ensure_started();

        watch(cx, spectrum.subscribe(), |this, bars, cx| {
            this.spectrum = bars;
            cx.notify();
        });

        spectrum
    }

    /// MPRIS only reports a position when asked, and only the followed player
    /// shows one, so the panel asks once a second for as long as it is open.
    fn poll_position(subscriber: MprisSubscriber, cx: &mut Context<Self>) {
        cx.spawn(async move |panel, cx| {
            // Both reads fail once the panel closes, which is what ends this.
            while let Ok(service) = panel.read_with(cx, |this, cx| {
                followed_player(&this.data, cx).map(|player| player.service.clone())
            }) {
                let position = match service {
                    Some(service) => subscriber.position(&service).await.ok(),
                    None => None,
                };

                let updated = panel.update(cx, |this, cx| {
                    if this.position_us != position {
                        this.position_us = position;
                        cx.notify();
                    }
                });
                if updated.is_err() {
                    break;
                }

                cx.background_executor().timer(Duration::from_secs(1)).await;
            }
        })
        .detach();
    }

    /// The state to render: what the user asked for while the player is still
    /// answering, what it reports the rest of the time.
    fn shown_state(&self, player: &services::MprisPlayerData) -> PlaybackStatus {
        match &self.optimistic {
            Some(pending) if pending.service == player.service => pending.assumed,
            _ => player.state,
        }
    }

    /// Drop the assumption as soon as the player moves off the state it was
    /// made against, whether or not it landed where we guessed.
    fn settle_optimistic(&mut self) {
        let settled = self.optimistic.as_ref().is_some_and(|pending| {
            self.data
                .players
                .iter()
                .find(|player| player.service == pending.service)
                .is_none_or(|player| player.state != pending.was)
        });
        if settled {
            self.optimistic = None;
        }
    }

    /// Flip the button now and let the player catch up: a browser can take
    /// most of a second to answer a PlayPause.
    fn toggle_playback(
        panel: &WeakEntity<Self>,
        subscriber: MprisSubscriber,
        service: String,
        was: PlaybackStatus,
        cx: &mut App,
    ) {
        let assumed = if was == PlaybackStatus::Playing {
            PlaybackStatus::Paused
        } else {
            PlaybackStatus::Playing
        };

        panel
            .update(cx, |this, cx| {
                this.optimistic = Some(Optimistic {
                    service: service.clone(),
                    assumed,
                    was,
                });
                cx.notify();
            })
            .ok();

        Self::run_command(cx, subscriber, service, PlayerCommand::PlayPause);
    }

    /// Persist the choice, and redraw so the promoted player moves up.
    fn pick(panel: &WeakEntity<Self>, key: Option<String>, cx: &mut App) {
        State::set_mpris_player(key, cx);
        panel.update(cx, |_, cx| cx.notify()).ok();
    }

    fn run_command(
        cx: &mut App,
        subscriber: MprisSubscriber,
        service_name: String,
        command: PlayerCommand,
    ) {
        cx.spawn(async move |_| {
            let _ = subscriber
                .dispatch(MprisCommand {
                    service_name,
                    command,
                })
                .await;
        })
        .detach();
    }

    fn status_text(state: PlaybackStatus) -> &'static str {
        match state {
            PlaybackStatus::Playing => "Playing",
            PlaybackStatus::Paused => "Paused",
            PlaybackStatus::Stopped => "Stopped",
        }
    }

    fn title_for(player: &services::MprisPlayerData) -> String {
        player
            .metadata
            .as_ref()
            .and_then(|m| m.title.clone())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| player_key(&player.service).to_string())
    }

    fn artist_for(player: &services::MprisPlayerData) -> Option<String> {
        player
            .metadata
            .as_ref()
            .and_then(|m| m.artists.clone())
            .map(|artists| artists.join(", "))
            .filter(|artists| !artists.is_empty())
    }

    /// Decode a cover once, and keep what the panel draws from it: gpui caches
    /// the image, we cache the average colour and the backdrop.
    fn load_cover(&mut self, art: &str, window: &mut Window, cx: &mut App) {
        if self.covers.contains_key(art) {
            return;
        }
        let Some(Ok(image)) = window.use_asset::<ImageAssetLoader>(&cover_resource(art), cx) else {
            return;
        };

        self.covers.insert(
            art.to_string(),
            Cover {
                tint: cover_tint(&image),
                backdrop: blurred_backdrop(&image),
            },
        );
    }

    fn subtitle_for(player: &services::MprisPlayerData) -> String {
        let artist = player
            .metadata
            .as_ref()
            .and_then(|m| m.artists.clone())
            .map(|a| a.join(", "))
            .unwrap_or_default();

        let status = Self::status_text(player.state);
        if artist.is_empty() {
            status.to_string()
        } else {
            format!("{artist} - {status}")
        }
    }

    /// Every player, the picked one first, then by playback state.
    fn sorted_players(&self, cx: &App) -> Vec<services::MprisPlayerData> {
        let picked = State::mpris_player(cx).map(str::to_string);
        let mut players = self.data.players.clone();
        players.sort_by_key(|p| match p.state {
            _ if picked.as_deref() == Some(player_key(&p.service)) => 0u8,
            PlaybackStatus::Playing => 1u8,
            PlaybackStatus::Paused => 2u8,
            PlaybackStatus::Stopped => 3u8,
        });
        players
    }

    fn format_duration(us: Option<i64>) -> String {
        let total_secs = us.unwrap_or(0).max(0) / 1_000_000;
        let minutes = total_secs / 60;
        let seconds = total_secs % 60;
        format!("{minutes}:{seconds:02}")
    }

    fn render_control_button(
        id: impl Into<gpui::ElementId>,
        icon: IconName,
        size: ButtonSize,
        on_click: impl Fn(&mut App) + 'static,
    ) -> impl IntoElement {
        IconButton::new(id.into(), icon)
            .style(ButtonStyle::Subtle)
            .size(size)
            .icon_size(IconSize::XSmall)
            .on_click(move |_, _, cx| on_click(cx))
    }

    /// The transport row, shared by both card sizes.
    fn render_controls(
        &self,
        player: &services::MprisPlayerData,
        size: ButtonSize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if !player.can_control {
            return div().into_any_element();
        }

        let service = player.service.clone();
        let volume = player.volume.unwrap_or(0.0);
        let state = self.shown_state(player);
        let panel = cx.weak_entity();
        let play_icon = if state == PlaybackStatus::Playing {
            IconName::Pause
        } else {
            IconName::Play
        };

        let mut buttons = [
            (IconName::SkipBack, "prev", PlayerCommand::Prev),
            (play_icon, "play", PlayerCommand::PlayPause),
            (IconName::SkipForward, "next", PlayerCommand::Next),
        ]
        .to_vec();

        if player.volume.is_some() {
            buttons.push((IconName::Dash, "dec", PlayerCommand::Volume(volume - 5.0)));
            buttons.push((IconName::Plus, "inc", PlayerCommand::Volume(volume + 5.0)));
        }

        h_flex()
            .gap(Spacing::XSmall.pixels())
            .children(buttons.into_iter().map(|(icon, name, command)| {
                let subscriber = self.subscriber.clone();
                let service = service.clone();
                let panel = panel.clone();
                Self::render_control_button(
                    format!("mpris-{name}-{service}"),
                    icon,
                    size,
                    move |cx| match command {
                        PlayerCommand::PlayPause => Self::toggle_playback(
                            &panel,
                            subscriber.clone(),
                            service.clone(),
                            state,
                            cx,
                        ),
                        command => {
                            Self::run_command(cx, subscriber.clone(), service.clone(), command)
                        }
                    },
                )
            }))
            .into_any_element()
    }

    /// A row's art, which promotes its player to the hero card when clicked.
    fn render_promotable_cover(
        &self,
        player: &services::MprisPlayerData,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let group = SharedString::from(format!("mpris-cover-{}", player.service));
        let key = player_key(&player.service).to_string();
        let panel = cx.weak_entity();

        div()
            .id(SharedString::from(format!(
                "mpris-promote-{}",
                player.service
            )))
            .group(group.clone())
            .relative()
            .flex_none()
            .cursor_pointer()
            .tooltip(Tooltip::text("Make this the main player"))
            .on_click(move |_, _, cx| Self::pick(&panel, Some(key.clone()), cx))
            .child(self.render_cover(player, COVER_SIZE, cx))
            .child(
                h_flex()
                    .absolute()
                    .inset_0()
                    .items_center()
                    .justify_center()
                    .rounded(Radius::Large.pixels())
                    .bg(gpui::black().opacity(0.55))
                    .opacity(0.)
                    .group_hover(group, |style| style.opacity(1.))
                    .child(
                        Icon::new(IconName::ChevronUp)
                            .size(IconSize::Small)
                            .color(Color::Custom(gpui::white())),
                    ),
            )
    }

    /// The album art, centre-cropped to a square slot, or a placeholder when
    /// there is none.
    fn render_cover(&self, player: &services::MprisPlayerData, side: f32, cx: &App) -> AnyElement {
        let colors = cx.theme().colors();
        let slot = div()
            .size(px(side))
            .flex_none()
            .rounded(Radius::Large.pixels())
            .overflow_hidden()
            .border_1()
            .border_color(colors.border_variant);

        let art = cx
            .config()
            .bar
            .modules
            .mpris
            .show_cover
            .then(|| player.art_url.clone())
            .flatten();

        match art {
            Some(art) => slot
                .child(
                    // The rounding rides on the picture: an overflow mask is a
                    // rectangle and never clips to a radius.
                    //
                    // The square is spelled out because gpui otherwise gives
                    // the element the picture's own aspect ratio, which taffy
                    // honours over `size_full` - portrait art would sit in the
                    // top of the slot rather than fill it. Saying 1:1 leaves
                    // `Cover` to centre-crop it on the GPU.
                    img(ImageSource::Resource(cover_resource(&art)))
                        .size_full()
                        .aspect_ratio(1.)
                        .rounded(Radius::Large.pixels())
                        .object_fit(ObjectFit::Cover),
                )
                .into_any_element(),
            None => slot
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.elevated_surface_background)
                .child(
                    Icon::new(IconName::Music)
                        .size(IconSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element(),
        }
    }

    /// The followed player, given room and the colour of its own art.
    fn render_hero_card(
        &self,
        player: &services::MprisPlayerData,
        tint: Option<Hsla>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let artist = Self::artist_for(player);

        let card = v_flex()
            .w_full()
            .p(Spacing::Large.pixels())
            .gap(Spacing::Large.pixels())
            .rounded(Radius::XLarge.pixels())
            .border_1();

        let card = match tint {
            Some(tint) => card
                .bg(Hsla { a: 0.35, ..tint })
                .border_color(Hsla { a: 0.45, ..tint }),
            None => card
                .bg(colors.elevated_surface_background)
                .border_color(colors.border_variant),
        };

        card.relative()
            .overflow_hidden()
            .children(self.render_backdrop(player, tint, cx))
            .children(self.render_spectrum(player, tint))
            .child(
                h_flex()
                    .w_full()
                    .gap(Spacing::Large.pixels())
                    .items_start()
                    .child(self.render_cover(player, HERO_COVER_SIZE, cx))
                    .child(
                        // As tall as the art, so the text starts at its top edge
                        // and the seek row ends on its bottom one. A minimum, not
                        // a height: a cramped column grows the card rather than
                        // clipping the time away.
                        v_flex()
                            .flex_1()
                            .min_w(px(0.))
                            .min_h(px(HERO_COVER_SIZE))
                            .justify_between()
                            .child(
                                h_flex()
                                    .w_full()
                                    .gap(Spacing::Medium.pixels())
                                    .items_center()
                                    .child(
                                        v_flex()
                                            .flex_1()
                                            .min_w(px(0.))
                                            .overflow_hidden()
                                            .gap(Spacing::XXSmall.pixels())
                                            .child(
                                                Label::new(Self::title_for(player))
                                                    .size(TextSize::Medium)
                                                    .weight(FontWeight::SEMIBOLD)
                                                    .truncate(),
                                            )
                                            .when_some(artist, |el, artist| {
                                                el.child(
                                                    Label::new(artist)
                                                        .size(TextSize::Small)
                                                        .color(Color::Muted)
                                                        .truncate(),
                                                )
                                            })
                                            .when_some(
                                                Self::elapsed_of(player, self.position_us),
                                                |el, times| {
                                                    el.child(
                                                        Label::new(times)
                                                            .size(TextSize::XSmall)
                                                            .color(Color::Muted),
                                                    )
                                                },
                                            ),
                                    )
                                    .child(self.render_play_button(player, tint, cx)),
                            )
                            .child(self.render_timeline(player, tint)),
                    ),
            )
            .into_any_element()
    }

    /// The cover, blown up and dimmed, as the card's own backdrop.
    fn render_backdrop(
        &self,
        player: &services::MprisPlayerData,
        tint: Option<Hsla>,
        cx: &App,
    ) -> Option<impl IntoElement> {
        let backdrop = player
            .art_url
            .as_ref()
            .and_then(|art| self.covers.get(art))
            .and_then(|cover| cover.backdrop.clone())?;

        let radius = card_radius();

        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    // Painted straight into the box rather than through `img`,
                    // which cannot be made to fill it: gpui gives any `img`
                    // whose caller left one unset the aspect ratio of the
                    // picture itself, and taffy honours that over `size_full`.
                    // A 16px square blur resolves to a 345px *square* whose
                    // rounded corners land a couple hundred pixels below the
                    // card, so the corners it does show inside it are square.
                    canvas(
                        |_, _, _| {},
                        move |bounds, _, window, _| {
                            // Covered, not stretched: the art keeps its own
                            // proportions and the card crops what hangs over.
                            // `paint_image` renders the intersection of the two
                            // rects, which is the second argument's whole job.
                            let art = ObjectFit::Cover.get_bounds(bounds, backdrop.size(0));
                            let _ = window.paint_image(
                                bounds,
                                art,
                                Corners::all(radius),
                                backdrop.clone(),
                                0,
                                false,
                            );
                        },
                    )
                    .size_full(),
                )
                // The scrim the text is read against. Tinted rather than
                // neutral, which is what gives end-4's card its colour: the
                // whole surface is made of the art.
                .child(div().absolute().inset_0().rounded(radius).bg(match tint {
                    Some(tint) => Hsla {
                        l: 0.14,
                        s: tint.s * 0.8,
                        a: 0.72,
                        ..tint
                    },
                    None => Hsla {
                        a: 0.7,
                        ..cx.theme().colors().background
                    },
                })),
        )
    }

    /// cava's frame as one filled wave across the card, the way end-4 draws
    /// it: smoothed, faint, and gone the moment the sound stops.
    fn render_spectrum(
        &self,
        player: &services::MprisPlayerData,
        tint: Option<Hsla>,
    ) -> Option<impl IntoElement> {
        if self.shown_state(player) != PlaybackStatus::Playing || self.spectrum.is_silent() {
            return None;
        }

        let points = smoothed(&self.spectrum.bars, WAVE_SMOOTHING);
        let color = Hsla {
            a: 0.15,
            ..tint.map_or(gpui::white(), |tint| Hsla { l: 0.75, ..tint })
        };

        Some(
            canvas(
                |_, _, _| {},
                move |bounds, _, window, _| {
                    let Some(path) = wave_path(bounds, &points) else {
                        return;
                    };
                    // One pass per band of the card's rounded rectangle: the
                    // wave keeps its flat floor and the corners clip it.
                    for mask in wave_masks(bounds, card_radius()) {
                        window.with_content_mask(Some(ContentMask { bounds: mask }), |window| {
                            window.paint_path(path.clone(), color);
                        });
                    }
                },
            )
            .absolute()
            .inset_0()
            .size_full(),
        )
    }

    /// `0:12 / 2:57`, once the player reports a length to count against.
    fn elapsed_of(player: &services::MprisPlayerData, position_us: Option<i64>) -> Option<String> {
        let duration = player.duration_us?;
        let position = position_us.unwrap_or(0).clamp(0, duration);
        Some(format!(
            "{} / {}",
            Self::format_duration(Some(position)),
            Self::format_duration(Some(duration))
        ))
    }

    /// The one control that gets weight, sitting across from the art.
    fn render_play_button(
        &self,
        player: &services::MprisPlayerData,
        tint: Option<Hsla>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let colors = cx.theme().colors();
        let idle = tint.map_or(colors.element_background, |tint| Hsla { a: 0.4, ..tint });
        let hovered = tint.map_or(colors.element_hover, |tint| Hsla { a: 0.6, ..tint });
        let state = self.shown_state(player);
        let playing = state == PlaybackStatus::Playing;
        let icon = if playing {
            IconName::Pause
        } else {
            IconName::Play
        };
        let (from, to) = if playing {
            (IDLE_RADIUS, PLAYING_RADIUS)
        } else {
            (PLAYING_RADIUS, IDLE_RADIUS)
        };

        let subscriber = self.subscriber.clone();
        let service = player.service.clone();
        let panel = cx.weak_entity();

        h_flex()
            .id("mpris-hero-play")
            .size(px(PLAY_BUTTON_SIZE))
            .flex_none()
            .items_center()
            .justify_center()
            .bg(idle)
            .when(player.can_control, |el| {
                el.cursor_pointer()
                    .hover(|style| style.bg(hovered))
                    .on_click(move |_, _, cx| {
                        Self::toggle_playback(
                            &panel,
                            subscriber.clone(),
                            service.clone(),
                            state,
                            cx,
                        );
                    })
            })
            .child(Icon::new(icon).size(IconSize::Large).color(Color::Default))
            // A circle at rest, a squarer shape while playing. The element id
            // carries the state, so flipping it hands the animation a fresh
            // state slot and the morph replays from the other radius.
            .with_animation(
                ElementId::from(("mpris-hero-play-radius", playing as u64)),
                Animation::new(Motion::Standard.duration()).with_easing(motion::decelerate()),
                move |el, delta| el.rounded(px(from + (to - from) * delta)),
            )
    }

    /// Previous and next flanking a seekable track.
    fn render_timeline(
        &self,
        player: &services::MprisPlayerData,
        tint: Option<Hsla>,
    ) -> impl IntoElement {
        let duration = player.duration_us.unwrap_or(0);
        let position = self.position_us.unwrap_or(0).clamp(0, duration.max(1));
        let subscriber = self.subscriber.clone();
        let service = player.service.clone();
        let playing = self.shown_state(player) == PlaybackStatus::Playing;

        h_flex()
            .w_full()
            .gap(Spacing::Small.pixels())
            .items_center()
            .child(self.render_step_button(player, IconName::SkipBack, PlayerCommand::Prev))
            .child(
                div().flex_1().child(
                    Slider::new(
                        SharedString::from(format!("mpris-seek-{service}")),
                        position as f32,
                    )
                    .max(duration.max(1) as f32)
                    // A drag commits every distinct value, and every commit is
                    // a seek over D-Bus: one percent of the track is a fine
                    // enough grain to scrub with.
                    .step((duration.max(1) / 100) as f32)
                    .track_color(tint.map_or(gpui::white().opacity(0.15), |tint| Hsla {
                        l: 0.25,
                        a: 0.5,
                        ..tint
                    }))
                    .fill_color(tint.map_or(gpui::white(), |tint| Hsla {
                        l: 0.85,
                        a: 1.0,
                        ..tint
                    }))
                    // Wavy only while the track is actually moving, and the
                    // wave travels a cycle every two seconds.
                    .wavy(playing)
                    .disabled(!player.can_seek || duration == 0)
                    .on_change(move |value, _, cx| {
                        Self::run_command(
                            cx,
                            subscriber.clone(),
                            service.clone(),
                            PlayerCommand::SeekTo(value as i64),
                        );
                    })
                    .with_animation(
                        ElementId::from(("mpris-seek-wave", playing as u64)),
                        Animation::new(Motion::Drift.duration()).repeat(),
                        |slider, delta| slider.wave_phase(delta * std::f32::consts::TAU),
                    ),
                ),
            )
            .child(self.render_step_button(player, IconName::SkipForward, PlayerCommand::Next))
            .into_any_element()
    }

    fn render_step_button(
        &self,
        player: &services::MprisPlayerData,
        icon: IconName,
        command: PlayerCommand,
    ) -> impl IntoElement {
        let subscriber = self.subscriber.clone();
        let service = player.service.clone();
        let id = format!("mpris-step-{icon:?}-{service}");

        IconButton::new(id, icon)
            .style(ButtonStyle::Ghost)
            .size(ButtonSize::Default)
            .icon_size(IconSize::Small)
            .disabled(!player.can_control)
            .on_click(move |_, _, cx| {
                Self::run_command(cx, subscriber.clone(), service.clone(), command);
            })
    }

    /// Every other player, one dense row each.
    fn render_player_row(
        &self,
        player: &services::MprisPlayerData,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        h_flex()
            .w_full()
            .p(Spacing::Small.pixels())
            .gap(Spacing::Medium.pixels())
            .items_center()
            .panel_card(cx)
            .child(self.render_promotable_cover(player, cx))
            .child(
                v_flex()
                    .flex_1()
                    .min_w(px(0.))
                    .overflow_hidden()
                    .child(
                        Label::new(Self::title_for(player))
                            .size(TextSize::Small)
                            .weight(FontWeight::MEDIUM)
                            .truncate(),
                    )
                    .child(
                        Label::new(Self::subtitle_for(player))
                            .size(TextSize::XSmall)
                            .color(Color::Muted)
                            .truncate(),
                    ),
            )
            .child(self.render_controls(player, ButtonSize::Compact, cx))
            .into_any_element()
    }
}

impl Render for MprisPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let focus = self.focus.arm(window, cx).clone();
        let players = self.sorted_players(cx);
        let is_empty = players.is_empty();
        let hero = players.first().cloned();
        for art in players
            .iter()
            .filter_map(|player| player.art_url.clone())
            .collect::<Vec<_>>()
        {
            self.load_cover(&art, window, cx);
        }
        let hero_tint = hero
            .as_ref()
            .and_then(|player| player.art_url.as_ref())
            .and_then(|art| self.covers.get(art))
            .and_then(|cover| cover.tint);

        div()
            .id("mpris-panel")
            .track_focus(&focus)
            .key_context("Panel")
            .on_action(|_: &Cancel, window, _| crate::panel::dismiss(window))
            .w_full()
            .h(px(MPRIS_PANEL_HEIGHT))
            .bar_panel(panel_background(cx), cx)
            .overflow_hidden()
            .child(
                v_flex()
                    .w_full()
                    .h_full()
                    .gap(PANEL_GAP.pixels())
                    .when(is_empty, |el| {
                        el.child(
                            h_flex().flex_1().justify_center().child(
                                Label::new("No MPRIS players detected")
                                    .size(TextSize::Small)
                                    .color(Color::Muted),
                            ),
                        )
                    })
                    .when_some(hero, |el, hero| {
                        el.child(
                            v_flex()
                                .flex_1()
                                .overflow_hidden()
                                .gap(Spacing::Medium.pixels())
                                .child(self.render_hero_card(&hero, hero_tint, cx))
                                .children(
                                    players
                                        .iter()
                                        .skip(1)
                                        .map(|player| self.render_player_row(player, cx)),
                                ),
                        )
                    }),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::{Point, size};

    /// How far outside the card's rounded rectangle a point sits.
    fn escape(corner: Point<Pixels>, bounds: Bounds<Pixels>, radius: Pixels) -> f32 {
        let (radius, x, y) = (radius.as_f32(), corner.x.as_f32(), corner.y.as_f32());
        let (left, right) = (bounds.left().as_f32(), bounds.right().as_f32());
        let floor = bounds.bottom().as_f32();

        let out_of_rect = (left - x).max(x - right).max(y - floor);
        // Distance past the arc, for a point down among the bottom corners.
        let past = (left + radius - x).max(x - (right - radius)).max(0.);
        let below = (y - (floor - radius)).max(0.);
        out_of_rect.max(past.hypot(below) - radius)
    }

    #[test]
    fn the_mask_bands_tile_the_card_without_leaving_its_corners() {
        let bounds = Bounds::new(point(px(0.), px(0.)), size(px(300.), px(120.)));
        let radius = px(15.);
        let masks = wave_masks(bounds, radius);

        for mask in &masks {
            for corner in [
                mask.origin,
                point(mask.right(), mask.top()),
                point(mask.left(), mask.bottom()),
                point(mask.right(), mask.bottom()),
            ] {
                let escape = escape(corner, bounds, radius);
                assert!(escape < 0.01, "{corner:?} escapes the card by {escape}px");
            }
        }

        // Bands meet edge to edge, from the top of the card down to its floor:
        // a gap between two of them is a line across the wave.
        assert_eq!(masks.first().expect("a band").top(), bounds.top());
        assert_eq!(masks.last().expect("a band").bottom(), bounds.bottom());
        for pair in masks.windows(2) {
            assert_eq!(pair[0].bottom(), pair[1].top());
        }

        // The wave itself keeps its flat floor, corner to corner.
        let path = wave_path(bounds, &[0.5; 50]).expect("a path");
        assert!(!path.vertices.is_empty());
        assert!(wave_path(bounds, &[0.5]).is_none());
    }
}
