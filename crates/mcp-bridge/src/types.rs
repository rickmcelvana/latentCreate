//! Wire types decoded out of comfy-mcp's JSON-in-text payloads.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Subset of `server_info` the app actually uses.
///
/// Every field is optional: `server` is absent when ComfyUI itself is down,
/// and `hardware` is absent on older comfy-cli builds. A missing block means
/// "unknown", never "none" -- the setup wizard must ask rather than assume.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    /// Running-server block: `{"running": true, "url": "http://127.0.0.1:8188"}`.
    #[serde(default)]
    pub server: Option<Value>,
    /// comfy-cli's hardware snapshot, captured once at comfy-mcp start.
    #[serde(default)]
    pub hardware: Option<Value>,
    /// Resolved ComfyUI workspace on disk.
    #[serde(default)]
    pub workspace: Option<Value>,
}

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
