//! Wire types decoded out of comfy-mcp's JSON-in-text payloads.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// `system_stats` payload: `{"devices": [...], "system": {...}}`.
///
/// Left as `Value` deliberately -- T-105 types the device blocks once the
/// VRAM-gating rules need them, and guessing the shape now would be recall.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemStats {
    /// Per-device blocks, each carrying `vram_free` among other keys.
    #[serde(default)]
    pub devices: Vec<Value>,
    /// Host-level block.
    #[serde(default)]
    pub system: Value,
}
