//! Control center widget configuration.

use serde::{Deserialize, Serialize};

/// Control center widget configuration.
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ControlCenterWidgetConfig {}
