//! MCP client for a local ComfyUI via `comfy-mcp`.
//!
//! Implements the `ComfyBackend` seam (ARCHITECTURE.md section 3) over stdio.
//! Tool names are the verified LOCAL ones -- see docs/MCP-SURFACE.md, never the
//! cloud documentation.

mod download;
mod error;
mod health;
mod jobs;
mod local;
mod models;
mod nodes;
mod preflight;
mod session_log;
mod slots;
mod templates;
mod types;

pub use download::{DownloadState, DownloadSubmit};
pub use error::ComfyError;
pub use health::{
    Compatibility, CoreFreshness, Freshness, GpuInfo, Hardware, LaunchResult, RunningServer,
    ServerInfo, Workspace,
};
pub use jobs::{JobCancel, JobRun, JobStatus, OutputBatch, OutputFile};
pub use local::{with_timeout, LocalComfy};
pub use models::{ModelFile, ModelFolder, ModelFolderEntry, ModelFolders, ModelHit, ModelSearch};
pub use nodes::{NodeInput, NodeOptions, NodeOutput, NodeSchema, NodeWarning, OBJECT_INFO_STALE};
pub use preflight::{node_id_to_instance, Finding, Note, NoteList, Validation, Verdict};
pub use session_log::SessionLog;
pub use slots::{split_address, Slot, SlotList, SlotOverride, SlotWrite};
pub use templates::{FetchedTemplate, LocalCheck, TemplateDetail, TemplateInfo, TemplateSearch};
pub use types::SystemStats;

/// The fake MCP peer, compiled for this crate's own tests and for downstream
/// crates that enable `test-support` as a **dev**-dependency (T-306b: the
/// pipeline's call sequence is asserted offline). Never in a release build.
#[cfg(any(test, feature = "test-support"))]
pub mod mock;

#[cfg(any(test, feature = "test-support"))]
pub use local::test_helpers;

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "mcp-bridge");
    }
}
