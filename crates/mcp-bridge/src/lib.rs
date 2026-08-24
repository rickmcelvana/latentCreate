//! MCP client for a local ComfyUI via `comfy-mcp`.
//!
//! Implements the `ComfyBackend` seam (ARCHITECTURE.md §3) over stdio.
//! Tool names are the verified LOCAL ones -- see docs/MCP-SURFACE.md, never the
//! cloud documentation.

mod error;
mod local;
mod types;

pub use error::ComfyError;
pub use local::{with_timeout, LocalComfy};
pub use types::{ServerInfo, SystemStats};

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "mcp-bridge");
    }
}
