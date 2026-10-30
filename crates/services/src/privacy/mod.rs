//! Privacy service for monitoring camera, microphone, and screenshare access.
//!
//! This module provides a reactive subscriber for tracking media stream access
//! via PipeWire and webcam device usage via inotify.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::rc::Rc;
use std::thread;

use futures_signals::signal::{Mutable, MutableSignalCloned};
use inotify::{EventMask, Inotify, WatchMask};
use tracing::{debug, error, warn};

use crate::lifecycle::{Lifecycle, ManagedService, RunToken};

const WEBCAM_DEVICE_PATH: &str = "/dev/video0";

/// Media type being accessed.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Media {
    /// Video stream (screenshare).
    Video,
    /// Audio stream (microphone).
    Audio,
}

/// An application node accessing media via PipeWire.
#[derive(Debug, Clone)]
pub struct ApplicationNode {
    /// PipeWire node ID.
    pub id: u32,
    /// Type of media being accessed.
    pub media: Media,
}

/// Privacy-related data.
#[derive(Debug, Clone, Default)]
pub struct PrivacyData {
    /// Active PipeWire media stream nodes.
    pub nodes: Vec<ApplicationNode>,
    /// Number of processes with webcam device open.
    pub webcam_access: i32,
}

impl PrivacyData {
    /// Returns true if nothing is accessing camera/mic.
    pub fn no_access(&self) -> bool {
        self.nodes.is_empty() && self.webcam_access == 0
    }

    /// Returns true if microphone is being accessed.
    pub fn microphone_access(&self) -> bool {
        self.nodes.iter().any(|n| n.media == Media::Audio)
    }

    /// Returns true if webcam is being accessed.
    pub fn webcam_access(&self) -> bool {
        self.webcam_access > 0
    }

    /// Returns true if screen is being shared.
    pub fn screenshare_access(&self) -> bool {
        self.nodes.iter().any(|n| n.media == Media::Video)
    }
}

/// Event-driven privacy subscriber.
///
/// This subscriber monitors media access via PipeWire and webcam device
/// usage via inotify, providing reactive state updates through `futures_signals`.
#[derive(Debug, Clone, Default)]
pub struct PrivacySubscriber {
    data: Mutable<PrivacyData>,
    lifecycle: Lifecycle,
}

impl PrivacySubscriber {
    /// Create a new privacy subscriber. Monitoring starts with
    /// [`ManagedService::start`].
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a signal that emits when privacy state changes.
    pub fn subscribe(&self) -> MutableSignalCloned<PrivacyData> {
        self.data.signal_cloned()
    }

    /// Get the current privacy data snapshot.
    pub fn get(&self) -> PrivacyData {
        self.data.get_cloned()
    }
}

impl ManagedService for PrivacySubscriber {
    fn name(&self) -> &'static str {
        "Privacy"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        size_of::<PrivacyData>() + self.data.lock_ref().nodes.len() * size_of::<ApplicationNode>()
    }

    /// Both watchers share one run token: an error in either surfaces as the
    /// service's status, and stopping silences both.
    fn start(&self) {
        let token = self.lifecycle.begin();
        self.data.lock_mut().webcam_access = is_device_in_use(WEBCAM_DEVICE_PATH);
        start_pipewire_listener(self.data.clone(), token.clone());
        start_webcam_watcher(self.data.clone(), token.clone());
        token.active();
    }
}

/// Start the PipeWire listener thread for media stream tracking.
///
/// ponytail: `pipewire::MainLoopBox::run` has no cross-thread quit handle, so
/// a stopped listener's thread lives until the process exits - it just stops
/// publishing. Restarting privacy repeatedly leaks one idle thread per
/// restart; wire up a `pw_loop` quit signal if that ever matters.
fn start_pipewire_listener(data: Mutable<PrivacyData>, token: RunToken) {
    thread::spawn(move || {
        if let Err(e) = run_pipewire_listener(data, token.clone()) {
            error!("PipeWire listener error: {}", e);
            token.error(e.to_string());
        }
    });
}

/// What a PipeWire node's properties say it is capturing, if it is worth an
/// indicator at all.
///
/// A capture stream reading the sink's own output - a visualiser, a recorder
/// taking desktop sound - is still `Stream/Input/Audio`, but PipeWire marks it
/// with `stream.capture.sink` and it never touches the microphone.
fn captured_media(media_class: &str, capture_sink: Option<&str>) -> Option<Media> {
    match media_class {
        "Stream/Input/Video" => Some(Media::Video),
        "Stream/Input/Audio" => {
            (!matches!(capture_sink, Some("true" | "1"))).then_some(Media::Audio)
        }
        _ => None,
    }
}

/// Add or drop a node in the published list, leaving it alone when nothing
/// changes: `info` repeats on every property update and every write here wakes
/// the subscribers.
fn track_node(data: &Mutable<PrivacyData>, id: u32, media: Option<Media>) {
    let tracked = data.lock_ref().nodes.iter().any(|n| n.id == id);

    match media {
        Some(media) if !tracked => {
            debug!("New media node: id={id}, media={media:?}");
            data.lock_mut().nodes.push(ApplicationNode { id, media });
        }
        None if tracked => {
            debug!("Removed tracked media node: {id}");
            data.lock_mut().nodes.retain(|n| n.id != id);
        }
        _ => {}
    }
}

/// Run the PipeWire listener (blocking).
///
/// The registry event only carries a whitelist of a node's properties, and
/// `stream.capture.sink` is not on it, so every capture stream has to be bound
/// to find out whether it is really the microphone. The proxy and its listener
/// live in `bound` until the node goes away.
fn run_pipewire_listener(data: Mutable<PrivacyData>, token: RunToken) -> anyhow::Result<()> {
    use pipewire::context::ContextRc;
    use pipewire::main_loop::MainLoopRc;
    use pipewire::node::{Node, NodeListener};

    let mainloop = MainLoopRc::new(None)?;
    let context = ContextRc::new(&mainloop, None)?;
    let core = context.connect_rc(None)?;
    let registry = core.get_registry_rc()?;

    let bound: Rc<RefCell<HashMap<u32, (Node, NodeListener)>>> = Rc::default();
    let bound_remove = bound.clone();
    let registry_bind = registry.clone();
    let data_add = data.clone();
    let data_remove = data.clone();
    let token_add = token.clone();

    let _listener = registry
        .add_listener_local()
        .global(move |global| {
            if !token_add.alive() {
                return;
            }
            let Some(class) = global.props.and_then(|props| props.get("media.class")) else {
                return;
            };
            if !matches!(class, "Stream/Input/Video" | "Stream/Input/Audio") {
                return;
            }

            let node: Node = match registry_bind.bind(global) {
                Ok(node) => node,
                Err(e) => {
                    warn!("Could not bind media node {}: {}", global.id, e);
                    return;
                }
            };

            let id = global.id;
            let data = data_add.clone();
            let listener = node
                .add_listener_local()
                .info(move |info| {
                    let media = info.props().and_then(|props| {
                        captured_media(props.get("media.class")?, props.get("stream.capture.sink"))
                    });
                    track_node(&data, id, media);
                })
                .register();

            bound.borrow_mut().insert(id, (node, listener));
        })
        .global_remove(move |id| {
            if !token.alive() {
                return;
            }
            bound_remove.borrow_mut().remove(&id);
            track_node(&data_remove, id, None);
        })
        .register();

    mainloop.run();

    Ok(())
}

/// Start the webcam watcher thread.
fn start_webcam_watcher(data: Mutable<PrivacyData>, token: RunToken) {
    thread::spawn(move || {
        if let Err(e) = run_webcam_watcher(data, token.clone()) {
            warn!("Webcam watcher error: {}", e);
            token.error(e.to_string());
        }
    });
}

/// Run the webcam watcher (blocking).
///
/// ponytail: `read_events_blocking` only returns on a device event, so a
/// stopped watcher exits at the next webcam open/close rather than at once.
fn run_webcam_watcher(data: Mutable<PrivacyData>, token: RunToken) -> anyhow::Result<()> {
    // Check if webcam device exists
    if !Path::new(WEBCAM_DEVICE_PATH).exists() {
        warn!("Webcam device not found: {}", WEBCAM_DEVICE_PATH);
        return Ok(());
    }

    let mut inotify = Inotify::init()?;

    inotify.watches().add(
        WEBCAM_DEVICE_PATH,
        WatchMask::CLOSE_WRITE
            | WatchMask::CLOSE_NOWRITE
            | WatchMask::DELETE_SELF
            | WatchMask::OPEN
            | WatchMask::ATTRIB,
    )?;

    let mut buffer = [0; 1024];

    while token.alive() {
        let events = inotify.read_events_blocking(&mut buffer)?;

        for event in events {
            debug!("Webcam event: {:?}", event.mask);

            if event.mask.contains(EventMask::OPEN) {
                data.lock_mut().webcam_access += 1;
                debug!("Webcam opened: {}", data.lock_ref().webcam_access);
            } else if event.mask.contains(EventMask::CLOSE_WRITE)
                || event.mask.contains(EventMask::CLOSE_NOWRITE)
            {
                let mut guard = data.lock_mut();
                guard.webcam_access = i32::max(guard.webcam_access - 1, 0);
                debug!("Webcam closed: {}", guard.webcam_access);
            } else if event.mask.contains(EventMask::DELETE_SELF) {
                warn!("Webcam device was deleted");
                return Ok(());
            }
        }
    }

    Ok(())
}

/// Check how many processes have a device file open.
fn is_device_in_use(target: &str) -> i32 {
    let mut used_by = 0;

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            let pid_path = entry.path();

            // Skip non-numeric directories (not process folders)
            let fd_path = pid_path.join("fd");
            if !fd_path.exists() {
                continue;
            }

            // Check file descriptors in each process folder
            if let Ok(fd_entries) = fs::read_dir(&fd_path) {
                for fd_entry in fd_entries.flatten() {
                    if let Ok(link_path) = fs::read_link(fd_entry.path())
                        && link_path == Path::new(target)
                    {
                        used_by += 1;
                    }
                }
            }
        }
    }

    used_by
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_sink_monitor_is_not_the_microphone() {
        assert_eq!(
            captured_media("Stream/Input/Audio", None),
            Some(Media::Audio)
        );
        assert_eq!(captured_media("Stream/Input/Audio", Some("true")), None);
        assert_eq!(captured_media("Stream/Input/Audio", Some("1")), None);
        assert_eq!(
            captured_media("Stream/Input/Video", None),
            Some(Media::Video)
        );
        assert_eq!(captured_media("Stream/Output/Audio", None), None);
    }
}
