//! MCP client for a local ComfyUI via `comfy-mcp`.
//!
//! Implements the `ComfyBackend` seam (ARCHITECTURE.md §3) over stdio.
//! Tool names are the verified LOCAL ones -- see docs/MCP-SURFACE.md, never the
//! cloud documentation.

mod download;
mod error;
mod jobs;
mod local;
mod models;
mod preflight;
mod session_log;
mod slots;
mod templates;
mod types;

pub use download::{DownloadState, DownloadSubmit};
pub use error::ComfyError;
pub use jobs::{JobCancel, JobRun, JobStatus, OutputBatch, OutputFile};
pub use local::{with_timeout, LocalComfy};
pub use models::{ModelFile, ModelFolder, ModelFolderEntry, ModelFolders, ModelHit, ModelSearch};
pub use preflight::{node_id_to_instance, Finding, Note, NoteList, Validation, Verdict};
pub use session_log::SessionLog;
pub use slots::{split_address, Slot, SlotList, SlotOverride, SlotWrite};
pub use templates::{FetchedTemplate, LocalCheck, TemplateDetail, TemplateInfo, TemplateSearch};
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
