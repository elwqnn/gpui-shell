//! MPRIS media player service for playback state and controls.

mod dbus;

use std::collections::{HashMap, HashSet};
use std::fmt::Display;
use std::thread;

use dbus::MprisPlayerProxy;
use futures_signals::signal::{Mutable, MutableSignalCloned};
use futures_util::StreamExt;
use futures_util::future::join_all;
use futures_util::stream::select_all;
use tracing::{debug, error, info, trace, warn};
use url::Url;
use zbus::{Connection, fdo::DBusProxy, zvariant::OwnedValue};

use crate::lifecycle::{Lifecycle, ManagedService, RunToken};

const MPRIS_PLAYER_SERVICE_PREFIX: &str = "org.mpris.MediaPlayer2.";
const EVENT_DEBOUNCE_MS: u64 = 200;

#[derive(Debug, Clone)]
enum MprisEvent {
    TopologyChanged,
    Metadata(String),
    Volume(String),
    Playback(String),
}

/// Current playback state reported by MPRIS.
#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaybackStatus {
    Playing,
    Paused,
    #[default]
    Stopped,
}

impl From<String> for PlaybackStatus {
    fn from(value: String) -> Self {
        Self::from(value.as_str())
    }
}

impl From<&str> for PlaybackStatus {
    fn from(value: &str) -> Self {
        match value {
            "Playing" => Self::Playing,
            "Paused" => Self::Paused,
            "Stopped" => Self::Stopped,
            _ => Self::Stopped,
        }
    }
}

/// Simplified MPRIS metadata for display.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MprisPlayerMetadata {
    pub artists: Option<Vec<String>>,
    pub title: Option<String>,
}

impl Display for MprisPlayerMetadata {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let label = match (&self.artists, &self.title) {
            (None, None) => String::new(),
            (None, Some(t)) => t.clone(),
            (Some(a), None) => a.join(", "),
            (Some(a), Some(t)) => format!("{} - {}", a.join(", "), t),
        };
        write!(f, "{label}")
    }
}

impl From<HashMap<String, OwnedValue>> for MprisPlayerMetadata {
    fn from(value: HashMap<String, OwnedValue>) -> Self {
        let artists = value
            .get("xesam:artist")
            .and_then(|v| v.clone().try_into().ok());
        let title = value
            .get("xesam:title")
            .and_then(|v| v.clone().try_into().ok());

        Self { artists, title }
    }
}

/// Album art as something a UI can load: a `file:` URL becomes the plain
/// path it points at, since only http(s) is worth fetching.
fn art_source(source: &str) -> Option<String> {
    let Ok(url) = Url::parse(source) else {
        return source.starts_with('/').then(|| source.to_string());
    };

    match url.scheme() {
        "file" => url.to_file_path().ok()?.into_os_string().into_string().ok(),
        "http" | "https" => Some(source.to_string()),
        _ => None,
    }
}

fn metadata_art_url(value: &HashMap<String, OwnedValue>) -> Option<String> {
    value
        .get("mpris:artUrl")
        .and_then(|v| v.clone().try_into().ok())
        .and_then(|source: String| art_source(&source))
}

fn metadata_duration_us(value: &HashMap<String, OwnedValue>) -> Option<i64> {
    value
        .get("mpris:length")
        .and_then(|v| {
            v.clone()
                .try_into()
                .ok()
                .or_else(|| v.clone().try_into().ok().map(|n: u64| n as i64))
        })
        .filter(|v: &i64| *v > 0)
}

/// Per-player data snapshot.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MprisPlayerData {
    pub service: String,
    pub metadata: Option<MprisPlayerMetadata>,
    pub art_url: Option<String>,
    pub duration_us: Option<i64>,
    pub volume: Option<f64>,
    pub state: PlaybackStatus,
    pub can_control: bool,
    pub can_seek: bool,
}

/// Complete MPRIS service state.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MprisData {
    pub players: Vec<MprisPlayerData>,
}

/// Commands for controlling a specific MPRIS player.
#[derive(Debug, Clone)]
pub struct MprisCommand {
    pub service_name: String,
    pub command: PlayerCommand,
}

/// Player control command.
#[derive(Debug, Clone, Copy)]
pub enum PlayerCommand {
    Prev,
    PlayPause,
    Next,
    Volume(f64),
    /// Jump to an absolute position, in microseconds.
    SeekTo(i64),
}

/// Reactive MPRIS subscriber.
#[derive(Debug, Clone, Default)]
pub struct MprisSubscriber {
    data: Mutable<MprisData>,
    lifecycle: Lifecycle,
}

impl MprisSubscriber {
    /// Create a new MPRIS subscriber. Monitoring starts with
    /// [`ManagedService::start`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a signal that emits when MPRIS state changes.
    pub fn subscribe(&self) -> MutableSignalCloned<MprisData> {
        self.data.signal_cloned()
    }

    /// Get the current MPRIS state snapshot.
    pub fn get(&self) -> MprisData {
        self.data.get_cloned()
    }

    /// Playback position in microseconds. MPRIS never signals it, so whoever
    /// displays it polls for it.
    pub async fn position(&self, service_name: &str) -> anyhow::Result<i64> {
        let conn = crate::bus::session().await?;
        let proxy = MprisPlayerProxy::builder(&conn)
            .destination(service_name)?
            .build()
            .await?;

        Ok(proxy.position().await?)
    }

    /// Execute a command for a specific player.
    pub async fn dispatch(&self, command: MprisCommand) -> anyhow::Result<()> {
        let conn = crate::bus::session().await?;
        let proxy = MprisPlayerProxy::builder(&conn)
            .destination(command.service_name.as_str())?
            .build()
            .await?;

        match command.command {
            PlayerCommand::Prev => {
                proxy.previous().await?;
            }
            PlayerCommand::PlayPause => {
                proxy.play_pause().await?;
            }
            PlayerCommand::Next => {
                proxy.next().await?;
            }
            PlayerCommand::Volume(v) => {
                let normalized = (v.clamp(0.0, 100.0)) / 100.0;
                proxy.set_volume(normalized).await?;
            }
            PlayerCommand::SeekTo(position) => {
                // MPRIS only seeks relatively, and the caller's idea of the
                // position is up to a poll old, so measure from a fresh read.
                let current = proxy.position().await?;
                proxy.seek(position - current).await?;
            }
        }

        if let Ok(new_data) = fetch_mpris_data(&conn).await {
            *self.data.lock_mut() = new_data;
        }

        Ok(())
    }
}

impl ManagedService for MprisSubscriber {
    fn name(&self) -> &'static str {
        "MPRIS"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        let data = self.data.lock_ref();
        size_of::<MprisData>()
            + data
                .players
                .iter()
                .map(|player| {
                    let metadata = player.metadata.as_ref().map_or(0, |meta| {
                        meta.title.as_ref().map_or(0, String::len)
                            + meta
                                .artists
                                .as_ref()
                                .map_or(0, |artists| artists.iter().map(String::len).sum::<usize>())
                    });
                    size_of::<MprisPlayerData>()
                        + player.service.len()
                        + player.art_url.as_ref().map_or(0, String::len)
                        + metadata
                })
                .sum::<usize>()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();
        let data = self.data.clone();

        tokio::spawn(async move {
            let conn = match crate::bus::session().await {
                Ok(conn) => conn,
                Err(e) => {
                    error!("Failed to connect to the session bus: {}", e);
                    token.error(e.to_string());
                    return;
                }
            };

            start_listener(data, token, conn);
        });
    }
}

/// Fetch current MPRIS data from all available players.
async fn fetch_mpris_data(conn: &Connection) -> anyhow::Result<MprisData> {
    let dbus = DBusProxy::new(conn).await?;
    let names: Vec<String> = dbus
        .list_names()
        .await?
        .iter()
        .filter(|name| name.as_str().starts_with(MPRIS_PLAYER_SERVICE_PREFIX))
        .map(|name| name.to_string())
        .collect();

    let players = join_all(names.iter().map(|name| async move {
        let proxy = match MprisPlayerProxy::builder(conn).destination(name.as_str()) {
            Ok(builder) => match builder.build().await {
                Ok(proxy) => proxy,
                Err(_) => return None,
            },
            Err(_) => return None,
        };

        let metadata_raw = proxy.metadata().await.ok();
        let metadata = metadata_raw.clone().map(MprisPlayerMetadata::from);
        let art_url = metadata_raw.as_ref().and_then(metadata_art_url);
        let duration_us = metadata_raw.as_ref().and_then(metadata_duration_us);
        let volume = proxy.volume().await.ok().map(|v| v * 100.0);
        let state = proxy
            .playback_status()
            .await
            .map(PlaybackStatus::from)
            .unwrap_or_default();
        let can_control = proxy.can_control().await.unwrap_or(false);
        let can_seek = proxy.can_seek().await.unwrap_or(false);

        Some(MprisPlayerData {
            service: name.clone(),
            metadata,
            art_url,
            duration_us,
            volume,
            state,
            can_control,
            can_seek,
        })
    }))
    .await
    .into_iter()
    .flatten()
    .collect();

    Ok(MprisData { players })
}

/// Start the MPRIS listener in a dedicated thread.
fn start_listener(data: Mutable<MprisData>, token: RunToken, conn: Connection) {
    thread::spawn(move || {
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(e) => {
                error!("Failed to create Tokio runtime for MPRIS listener: {}", e);
                token.error(e.to_string());
                return;
            }
        };

        rt.block_on(async move {
            if let Err(e) = run_listener(data, token.clone(), conn).await {
                error!("MPRIS listener error: {}", e);
                token.error(e.to_string());
            }
        });
    });
}

/// Run the MPRIS event listener loop.
async fn run_listener(
    data: Mutable<MprisData>,
    token: RunToken,
    conn: Connection,
) -> anyhow::Result<()> {
    info!("MPRIS subscriber started");

    while token.alive() {
        let current = match fetch_mpris_data(&conn).await {
            Ok(value) => {
                token.active();
                value
            }
            Err(err) => {
                error!("Failed to fetch MPRIS data: {}", err);
                token.error(err.to_string());
                tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
                continue;
            }
        };

        if data.get_cloned() != current {
            *data.lock_mut() = current.clone();
        }

        let dbus = DBusProxy::new(&conn).await?;
        let mut streams = vec![
            dbus.receive_name_owner_changed()
                .await?
                .filter_map(|signal| async move {
                    let args = signal.args().ok()?;
                    args.name
                        .as_str()
                        .starts_with(MPRIS_PLAYER_SERVICE_PREFIX)
                        .then_some(MprisEvent::TopologyChanged)
                })
                .boxed(),
        ];

        let mut proxies = HashMap::new();
        for player in &current.players {
            let proxy = match MprisPlayerProxy::builder(&conn).destination(player.service.as_str())
            {
                Ok(builder) => match builder.build().await {
                    Ok(proxy) => proxy,
                    Err(err) => {
                        debug!(
                            "Failed to build MPRIS proxy for {}: {}",
                            player.service, err
                        );
                        continue;
                    }
                },
                Err(err) => {
                    debug!("Invalid MPRIS destination {}: {}", player.service, err);
                    continue;
                }
            };

            let service = player.service.clone();
            streams.push(
                proxy
                    .receive_metadata_changed()
                    .await
                    .map(move |_| MprisEvent::Metadata(service.clone()))
                    .boxed(),
            );

            let service = player.service.clone();
            streams.push(
                proxy
                    .receive_volume_changed()
                    .await
                    .map(move |_| MprisEvent::Volume(service.clone()))
                    .boxed(),
            );

            let service = player.service.clone();
            streams.push(
                proxy
                    .receive_playback_status_changed()
                    .await
                    .map(move |_| MprisEvent::Playback(service.clone()))
                    .boxed(),
            );

            proxies.insert(player.service.clone(), proxy);
        }

        let mut events = select_all(streams);
        while token.alive() {
            let first = match events.next().await {
                Some(event) => event,
                None => {
                    warn!("MPRIS event stream ended unexpectedly");
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                    break;
                }
            };

            let mut batch = vec![first];
            while let Ok(Some(event)) = tokio::time::timeout(
                tokio::time::Duration::from_millis(EVENT_DEBOUNCE_MS),
                events.next(),
            )
            .await
            {
                batch.push(event);
            }

            let mut topology_changed = false;
            let mut metadata_services = HashSet::new();
            let mut volume_services = HashSet::new();
            let mut playback_services = HashSet::new();

            for event in batch {
                match event {
                    MprisEvent::TopologyChanged => topology_changed = true,
                    MprisEvent::Metadata(service) => {
                        trace!("MPRIS metadata changed: {}", service);
                        metadata_services.insert(service);
                    }
                    MprisEvent::Volume(service) => {
                        trace!("MPRIS volume changed: {}", service);
                        volume_services.insert(service);
                    }
                    MprisEvent::Playback(service) => {
                        trace!("MPRIS playback changed: {}", service);
                        playback_services.insert(service);
                    }
                }
            }

            if topology_changed {
                debug!("MPRIS topology changed, rebuilding player snapshot/streams");
                break;
            }

            for service in metadata_services {
                let Some(proxy) = proxies.get(service.as_str()) else {
                    continue;
                };
                let Ok(raw) = proxy.metadata().await else {
                    continue;
                };
                let new_metadata = Some(MprisPlayerMetadata::from(raw.clone()));
                let new_art_url = metadata_art_url(&raw);
                let new_duration_us = metadata_duration_us(&raw);

                let mut guard = data.lock_mut();
                if let Some(player) = guard.players.iter_mut().find(|p| p.service == service) {
                    if player.metadata != new_metadata {
                        player.metadata = new_metadata;
                    }
                    if player.art_url != new_art_url {
                        player.art_url = new_art_url;
                    }
                    if player.duration_us != new_duration_us {
                        player.duration_us = new_duration_us;
                    }
                }
            }

            for service in volume_services {
                let Some(proxy) = proxies.get(service.as_str()) else {
                    continue;
                };
                let Ok(raw_volume) = proxy.volume().await else {
                    continue;
                };
                let new_volume = raw_volume * 100.0;

                let mut guard = data.lock_mut();
                if let Some(player) = guard.players.iter_mut().find(|p| p.service == service) {
                    let changed = player
                        .volume
                        .map(|v| (v - new_volume).abs() > 0.01)
                        .unwrap_or(true);
                    if changed {
                        player.volume = Some(new_volume);
                    }
                }
            }

            for service in playback_services {
                let Some(proxy) = proxies.get(service.as_str()) else {
                    continue;
                };
                let Ok(raw_status) = proxy.playback_status().await else {
                    continue;
                };
                let new_state = PlaybackStatus::from(raw_status);

                let mut guard = data.lock_mut();
                if let Some(player) = guard.players.iter_mut().find(|p| p.service == service)
                    && player.state != new_state
                {
                    player.state = new_state;
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::art_source;

    #[test]
    fn art_reaches_the_ui_as_a_path_or_a_remote_url() {
        assert_eq!(
            art_source("file:///tmp/a%20cover.jpg").as_deref(),
            Some("/tmp/a cover.jpg")
        );
        assert_eq!(
            art_source("/tmp/cover.jpg").as_deref(),
            Some("/tmp/cover.jpg")
        );
        assert_eq!(
            art_source("https://i.scdn.co/image/ab6761").as_deref(),
            Some("https://i.scdn.co/image/ab6761")
        );
        assert_eq!(art_source("data:image/png;base64,AAAA"), None);
    }
}
