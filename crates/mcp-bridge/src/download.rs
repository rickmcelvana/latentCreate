//! Model download: submit a background transfer and track it.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 11.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// `download_model(wait=false)` result. `download_id` is the handle for
/// [`LocalComfy::download_status`] and [`LocalComfy::download_cancel`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadSubmit {
    /// The handle `download` polls with.
    pub download_id: String,
    /// comfy-cli's worker process id.
    #[serde(default)]
    pub pid: Option<u32>,
    /// Final path the model is being written to.
    #[serde(default)]
    pub dest: Option<PathBuf>,
    /// Unknown at submit (`null`).
    #[serde(default)]
    pub total_bytes: Option<u64>,
    /// `"starting"` at submit.
    #[serde(default)]
    pub status: String,
}

/// `download(action="status"|"wait"|"cancel")` result.
///
/// All three actions return this same shape. `status` is `"starting"`,
/// `"downloading"`, or terminal (`"failed"` verified; `"completed"` inferred --
/// needs a real download).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadState {
    /// The download this describes.
    #[serde(default)]
    pub id: String,
    /// Run state.
    pub status: String,
    /// Bytes transferred so far. Null-tolerant: kept `Option` because comfy-cli
    /// may report a null before the worker starts writing.
    #[serde(default)]
    pub completed_bytes: Option<u64>,
    /// Total size; `null` until the server sends a content length.
    #[serde(default)]
    pub total_bytes: Option<u64>,
    /// `0..100`; `null` while `total_bytes` is unknown.
    #[serde(default)]
    pub percent: Option<f64>,
    /// Seconds since the transfer started.
    #[serde(default)]
    pub elapsed_seconds: Option<f64>,
    /// Where the file is being written.
    #[serde(default)]
    pub dest: Option<PathBuf>,
    /// Non-null when the download failed.
    #[serde(default)]
    pub error: Option<String>,
}

impl DownloadState {
    /// Whether the download has reached a terminal state.
    ///
    /// `"failed"` is verified; `"completed"` is inferred (not reproduced live).
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "failed")
    }

    /// Whether the download finished successfully.
    pub fn is_success(&self) -> bool {
        self.status == "completed"
    }
}

impl LocalComfy {
    /// Start a background model download into a `models/` folder.
    ///
    /// `relative_path` must start with `models` (e.g. `models/checkpoints`).
    /// `filename` should be supplied when the URL does not end in the file name
    /// -- comfy-cli fails the call with `missing_argument` otherwise.
    pub async fn download_model(
        &self,
        url: &str,
        relative_path: &str,
        filename: Option<&str>,
    ) -> Result<DownloadSubmit, ComfyError> {
        let mut args = Map::new();
        args.insert("url".into(), Value::String(url.to_string()));
        args.insert(
            "relative_path".into(),
            Value::String(relative_path.to_string()),
        );
        if let Some(name) = filename {
            args.insert("filename".into(), Value::String(name.to_string()));
        }
        args.insert("wait".into(), Value::Bool(false));
        self.call("download_model", args).await
    }

    /// Poll a download's progress.
    pub async fn download_status(&self, id: &str) -> Result<DownloadState, ComfyError> {
        let mut args = Map::new();
        args.insert("action".into(), Value::String("status".into()));
        args.insert("download_id".into(), Value::String(id.to_string()));
        self.call("download", args).await
    }

    /// Cancel a running download.
    pub async fn download_cancel(&self, id: &str) -> Result<DownloadState, ComfyError> {
        let mut args = Map::new();
        args.insert("action".into(), Value::String("cancel".into()));
        args.insert("download_id".into(), Value::String(id.to_string()));
        self.call("download", args).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::download::{DownloadState, DownloadSubmit};
    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;

    #[tokio::test]
    async fn test_download_model_sends_url_relative_path_and_wait_false() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "download_id": "abc123", "status": "starting"
        }))])
        .await;

        let _: DownloadSubmit = client
            .download_model(
                "https://x/y.safetensors",
                "models/checkpoints",
                Some("y.safetensors"),
            )
            .await
            .expect("submit");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("download_model"));
        assert_eq!(log[0]["arguments"]["url"], json!("https://x/y.safetensors"));
        assert_eq!(
            log[0]["arguments"]["relative_path"],
            json!("models/checkpoints")
        );
        assert_eq!(log[0]["arguments"]["filename"], json!("y.safetensors"));
        assert_eq!(log[0]["arguments"]["wait"], json!(false));
    }

    #[tokio::test]
    async fn test_download_submit_decodes_the_download_id() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "download_id": "bb982d2f2b6e",
            "pid": 26296,
            "dest": "C:/Comfy/models/checkpoints/test.safetensors",
            "total_bytes": null,
            "status": "starting"
        }))])
        .await;

        let submit: DownloadSubmit = client
            .download_model(
                "https://x/y.safetensors",
                "models/checkpoints",
                Some("y.safetensors"),
            )
            .await
            .expect("submit");
        assert_eq!(submit.download_id, "bb982d2f2b6e");
        assert_eq!(submit.status, "starting");
        assert!(submit.total_bytes.is_none());
    }

    /// Protects: the terminal statuses -- `"failed"` is verified, `"completed"`
    /// is inferred (not reproduced live, same honesty note as `JobStatus`).
    #[test]
    fn test_download_state_is_terminal_and_success() {
        let failed = DownloadState {
            id: "x".to_string(),
            status: "failed".to_string(),
            completed_bytes: Some(0),
            total_bytes: None,
            percent: None,
            elapsed_seconds: Some(8.0),
            dest: None,
            error: Some("Download failed after 3 attempts".to_string()),
        };
        assert!(failed.is_terminal());
        assert!(!failed.is_success());

        let completed = DownloadState {
            id: "x".to_string(),
            status: "completed".to_string(),
            completed_bytes: Some(100),
            total_bytes: Some(100),
            percent: Some(100.0),
            elapsed_seconds: Some(10.0),
            dest: None,
            error: None,
        };
        assert!(completed.is_terminal());
        assert!(completed.is_success());
    }

    #[tokio::test]
    async fn test_download_status_and_cancel_send_action_and_id() {
        let (client, recorded) = client_and_log(vec![
            Reply::Json(json!({ "id": "x", "status": "downloading", "completed_bytes": 0 })),
            Reply::Json(json!({ "id": "x", "status": "failed", "completed_bytes": 0 })),
        ])
        .await;

        let _: DownloadState = client.download_status("x").await.expect("status");
        let _: DownloadState = client.download_cancel("x").await.expect("cancel");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("download"));
        assert_eq!(log[0]["arguments"]["action"], json!("status"));
        assert_eq!(log[0]["arguments"]["download_id"], json!("x"));
        assert_eq!(log[1]["name"], json!("download"));
        assert_eq!(log[1]["arguments"]["action"], json!("cancel"));
        assert_eq!(log[1]["arguments"]["download_id"], json!("x"));
    }
}
