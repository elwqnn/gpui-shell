//! Audio spectrum service: frequency bars for the sound leaving the default
//! sink, read from `cava`.
//!
//! cava does the capture and the FFT; this drives it as a child process the
//! way end-4's dots do, over a raw ascii pipe. The Rust bindings that vendor
//! cavacore pull in a service framework that flips `zbus` onto its Tokio
//! executor, which breaks every D-Bus call the shell makes from a GPUI task -
//! a pipe costs nothing by comparison.
//!
//! Nothing runs until someone starts the service: a spectrum is a constant
//! stream of work and only a visible visualiser justifies it.

use std::process::Stdio;

use futures_signals::signal::{Mutable, MutableSignalCloned};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tracing::{error, info, warn};

use crate::lifecycle::{Lifecycle, ManagedService, RunToken, ServiceMode};

/// Bars to ask cava for, and the ceiling its ascii output counts up to.
const BAR_COUNT: usize = 50;
const ASCII_MAX: f32 = 1000.0;

/// cava's configuration, straight to its stdout. `mode = waves` is what makes
/// neighbouring bars lean on each other instead of flickering apart.
fn cava_config() -> String {
    format!(
        "[general]
mode = waves
framerate = 60
autosens = 1
bars = {BAR_COUNT}

[output]
method = raw
raw_target = /dev/stdout
data_format = ascii
channels = mono
mono_option = average

[smoothing]
noise_reduction = 20
"
    )
}

/// Frequency bars, each in `0.0..=1.0`, low frequencies first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SpectrumData {
    pub bars: Vec<f32>,
}

impl SpectrumData {
    /// True while there is no sound worth drawing.
    pub fn is_silent(&self) -> bool {
        self.bars.iter().all(|bar| *bar < 0.01)
    }
}

/// Reactive audio spectrum subscriber.
#[derive(Debug, Clone)]
pub struct SpectrumSubscriber {
    data: Mutable<SpectrumData>,
    lifecycle: Lifecycle,
}

impl Default for SpectrumSubscriber {
    fn default() -> Self {
        Self {
            data: Mutable::default(),
            // Never on its own: the visualiser starts it when it is on screen.
            lifecycle: Lifecycle::new(ServiceMode::Lazy),
        }
    }
}

impl SpectrumSubscriber {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get a signal that emits on every frame of bars.
    pub fn subscribe(&self) -> MutableSignalCloned<SpectrumData> {
        self.data.signal_cloned()
    }

    /// Get the current bars.
    pub fn get(&self) -> SpectrumData {
        self.data.get_cloned()
    }
}

impl ManagedService for SpectrumSubscriber {
    fn name(&self) -> &'static str {
        "Spectrum"
    }

    fn lifecycle(&self) -> &Lifecycle {
        &self.lifecycle
    }

    fn memory_bytes(&self) -> usize {
        size_of::<SpectrumData>() + self.data.lock_ref().bars.len() * size_of::<f32>()
    }

    fn start(&self) {
        let token = self.lifecycle.begin();
        let data = self.data.clone();

        tokio::spawn(async move {
            if let Err(err) = run_cava(data.clone(), token.clone()).await {
                error!("Spectrum listener error: {:#}", err);
                token.error(err.to_string());
            }
            data.set(SpectrumData::default());
        });
    }
}

/// One frame of cava's ascii output: `12;340;...;7;` scaled to `0.0..=1.0`.
fn parse_frame(line: &str) -> Option<Vec<f32>> {
    let bars: Vec<f32> = line
        .split(';')
        .filter(|value| !value.trim().is_empty())
        .map(|value| {
            value
                .trim()
                .parse::<f32>()
                .map(|bar| (bar / ASCII_MAX).clamp(0.0, 1.0))
        })
        .collect::<Result<_, _>>()
        .ok()?;

    (!bars.is_empty()).then_some(bars)
}

async fn run_cava(data: Mutable<SpectrumData>, token: RunToken) -> anyhow::Result<()> {
    let config = std::env::temp_dir().join("gpuishell-cava.conf");
    std::fs::write(&config, cava_config())?;

    let mut child = Command::new("cava")
        .arg("-p")
        .arg(&config)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|err| anyhow::anyhow!("cava is needed for the spectrum: {err}"))?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("cava gave us no stdout"))?;

    info!("Spectrum subscriber started");
    token.active();

    let mut lines = BufReader::new(stdout).lines();
    while token.alive() {
        let Some(line) = lines.next_line().await? else {
            warn!("cava stopped producing frames");
            break;
        };

        if let Some(bars) = parse_frame(&line) {
            data.set(SpectrumData { bars });
        }
    }

    let _ = child.kill().await;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_frame_scales_to_the_unit_range() {
        let bars = parse_frame("0;500;1000;250;").expect("a frame");

        assert_eq!(bars, vec![0.0, 0.5, 1.0, 0.25]);
        assert!(parse_frame("").is_none());
        assert!(parse_frame("not;a;frame;").is_none());
    }

    /// Needs cava and a running sound server, so it is opt-in:
    /// `cargo test -p services -- --ignored spectrum`.
    #[tokio::test]
    #[ignore = "requires cava and a sound server"]
    async fn cava_feeds_us_frames() {
        use futures_signals::signal::SignalExt;
        use futures_util::StreamExt;

        let spectrum = SpectrumSubscriber::new();
        spectrum.start();

        let mut frames = spectrum.subscribe().to_stream();
        let bars = tokio::time::timeout(tokio::time::Duration::from_secs(5), async {
            loop {
                let frame = frames.next().await.expect("the signal never ends");
                if !frame.bars.is_empty() {
                    return frame.bars;
                }
            }
        })
        .await
        .expect("a frame within five seconds");

        assert_eq!(bars.len(), BAR_COUNT);
        assert!(bars.iter().all(|bar| (0.0..=1.0).contains(bar)));
    }
}
