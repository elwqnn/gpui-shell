//! MPRIS module configuration.

use serde::{Deserialize, Serialize};

/// MPRIS module configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MprisConfig {
    pub show_cover: bool,
    pub max_width: f32,
    /// Draw the audio spectrum behind the main player. It runs an analyser for
    /// as long as the panel is open, so it is worth turning off on a laptop.
    pub show_spectrum: bool,
}

impl Default for MprisConfig {
    fn default() -> Self {
        Self {
            show_cover: true,
            max_width: 220.0,
            show_spectrum: true,
        }
    }
}
