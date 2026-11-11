//! HTTP client for the user's own ComfyUI.
//!
//! Every route here is one ComfyUI serves itself, observed live and recorded in
//! docs/COMFY-HTTP-SURFACE.md -- that file is the authority, not ComfyUI's
//! documentation. ARCHITECTURE section 3 is the contract; section 1 says why
//! this replaced comfy-mcp (T-519).

mod client;
pub mod download;
mod error;
mod session_log;
pub mod transport;
mod types;

pub use client::{discover, encode_segment, normalize_url, ComfyHttp, DEFAULT_URLS};
pub use error::{ComfyError, InputProblem, NodeProblem, Rejection};
pub use session_log::SessionLog;
pub use types::{
    Device, HistoryRecord, HistoryStatus, NodeInput, NodeOptions, NodeSchema, Outcome, OutputRef,
    QueuePosition, QueueState, SystemInfo, SystemStats,
};

#[cfg(test)]
mod tests {
    /// Pins the published name, so a rename cannot silently break the Tauri
    /// shell's path dependency.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "comfy-bridge");
    }
}
