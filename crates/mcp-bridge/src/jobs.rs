//! Job lifecycle: run, poll, cancel, fetch outputs.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 10.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// `run_workflow(wait=false)` result. `prompt_id` is the handle every other
/// job tool takes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRun {
    /// ComfyUI's prompt id -- the job handle.
    pub prompt_id: String,
    /// Queue state at submit time: `"queued"`.
    #[serde(default)]
    pub status: String,
    /// The workflow file that was submitted.
    #[serde(default)]
    pub workflow: Option<PathBuf>,
    /// comfy-cli's client id for the watcher.
    #[serde(default)]
    pub client_id: Option<String>,
    /// Empty at submit; a `job_status` poll reflects progress instead.
    #[serde(default)]
    pub outputs: Vec<Value>,
    /// Null until the job starts running.
    #[serde(default)]
    pub elapsed_seconds: Option<f64>,
    /// On-disk record `fetch_outputs` reads back for this job.
    #[serde(default)]
    pub state_file: Option<PathBuf>,
    /// comfy-cli spawned a background watcher for this job.
    #[serde(default)]
    pub watcher_spawned: bool,
}

/// `job(action="status"|"wait")` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobStatus {
    /// The prompt id this describes.
    #[serde(default)]
    pub prompt_id: Option<String>,
    /// Run state. Verified values: `queued`, `running`, `completed` -- the
    /// terminal-success value is `"completed"`, NOT `"success"`. A failure is
    /// signalled by a non-null [`JobStatus::error`], not necessarily a distinct
    /// status string.
    pub status: String,
    /// Node count while running; omitted once terminal.
    #[serde(default)]
    pub workflow_size: Option<usize>,
    /// Full `view?` URLs of the outputs produced so far.
    #[serde(default)]
    pub outputs: Vec<String>,
    /// Output URLs keyed by node id (e.g. the save node).
    #[serde(default)]
    pub outputs_by_node: BTreeMap<String, Vec<String>>,
    /// Non-null when the job failed. The failure payload's shape is NOT yet
    /// captured (needs a job that passes validation but fails a node), so it is
    /// kept as `Value` rather than narrowed.
    #[serde(default)]
    pub error: Option<Value>,
}

impl JobStatus {
    /// Whether the job has reached a terminal state and no more polling is needed.
    ///
    /// `"completed"` is verified; `"error"`/`"failed"` are inferred from
    /// ComfyUI's own status vocabulary and are the likely failure strings.
    pub fn is_terminal(&self) -> bool {
        matches!(self.status.as_str(), "completed" | "error" | "failed")
    }

    /// Whether the job finished successfully.
    pub fn is_success(&self) -> bool {
        self.status == "completed" && self.error.is_none()
    }
}

/// `job(action="cancel")` result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobCancel {
    /// Whether the job was known to comfy-cli.
    #[serde(default)]
    pub found: bool,
    /// Whether it was removed from the queue.
    #[serde(default)]
    pub queue_delete_ok: bool,
    /// Whether a running job was interrupted.
    #[serde(default)]
    pub interrupt_ok: bool,
}

/// One downloaded output file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputFile {
    /// The `view?` URL the file came from.
    pub url: String,
    /// Local path it was written to.
    pub path: PathBuf,
    /// Size in bytes.
    pub size: u64,
}

/// `fetch_outputs` result: every output of one job, downloaded into `out_dir`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OutputBatch {
    /// The job these files came from.
    #[serde(default)]
    pub prompt_id: Option<String>,
    /// Directory the files were written into.
    #[serde(default)]
    pub out_dir: Option<PathBuf>,
    /// The downloaded files.
    #[serde(default)]
    pub files: Vec<OutputFile>,
}

impl LocalComfy {
    /// Submit a workflow to run without waiting.
    ///
    /// `wait` is always `false`: the app polls [`LocalComfy::job_status`] on its
    /// own task rather than blocking (ARCHITECTURE 3).
    pub async fn run(&self, workflow: &Path) -> Result<JobRun, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        args.insert("wait".into(), Value::Bool(false));
        self.call("run_workflow", args).await
    }

    /// Poll a job's status.
    pub async fn job_status(&self, id: &str) -> Result<JobStatus, ComfyError> {
        let mut args = Map::new();
        args.insert("action".into(), Value::String("status".into()));
        args.insert("prompt_id".into(), Value::String(id.to_string()));
        self.call("job", args).await
    }

    /// Cancel a queued or running job.
    ///
    /// Read [`JobCancel::found`]/`interrupt_ok` rather than expecting a distinct
    /// `"cancelled"` status -- cancel is racy against a fast job (MCP-SURFACE
    /// 10.5), and the call's booleans are the confirmation.
    pub async fn cancel_job(&self, id: &str) -> Result<JobCancel, ComfyError> {
        let mut args = Map::new();
        args.insert("action".into(), Value::String("cancel".into()));
        args.insert("prompt_id".into(), Value::String(id.to_string()));
        self.call("job", args).await
    }

    /// Download a completed job's outputs into `out_dir`.
    pub async fn outputs(&self, id: &str, out_dir: &Path) -> Result<OutputBatch, ComfyError> {
        let mut args = Map::new();
        args.insert("prompt_id".into(), Value::String(id.to_string()));
        args.insert(
            "out_dir".into(),
            Value::String(out_dir.display().to_string()),
        );
        self.call("fetch_outputs", args).await
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use serde_json::json;

    use crate::jobs::{JobCancel, JobRun, JobStatus, OutputBatch};
    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;

    const PROMPT_ID: &str = "196a0dc9-4b7e-437f-a16f-ce3ef61e1849";

    #[tokio::test]
    async fn test_run_sends_workflow_path_and_wait_false() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "workflow": "wf.json",
            "status": "queued",
            "prompt_id": PROMPT_ID
        }))])
        .await;

        let _: JobRun = client
            .run(std::path::Path::new("wf.json"))
            .await
            .expect("run decodes");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("run_workflow"));
        assert_eq!(log[0]["arguments"]["workflow_path"], json!("wf.json"));
        assert_eq!(log[0]["arguments"]["wait"], json!(false));
    }

    #[tokio::test]
    async fn test_run_decodes_the_prompt_id() {
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "workflow": "wf.json",
            "status": "queued",
            "prompt_id": PROMPT_ID,
            "client_id": "bf502ccb-1cb7-4fe8-8447-c6c529d85559",
            "outputs": [],
            "elapsed_seconds": null,
            "state_file": "C:/jobs/196a0dc9.json",
            "watcher_spawned": true
        }))])
        .await;

        let run: JobRun = client
            .run(std::path::Path::new("wf.json"))
            .await
            .expect("run");
        assert_eq!(run.prompt_id, PROMPT_ID);
        assert_eq!(run.status, "queued");
        assert!(run.watcher_spawned);
        assert!(run.elapsed_seconds.is_none());
    }

    #[tokio::test]
    async fn test_job_status_sends_action_and_prompt_id() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "prompt_id": PROMPT_ID,
            "status": "running",
            "outputs": []
        }))])
        .await;

        let _: JobStatus = client.job_status(PROMPT_ID).await.expect("status");

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("job"));
        assert_eq!(log[0]["arguments"]["action"], json!("status"));
        assert_eq!(log[0]["arguments"]["prompt_id"], json!(PROMPT_ID));
    }

    #[tokio::test]
    async fn test_job_status_decodes_a_completed_job() {
        let url = "http://127.0.0.1:8188/view?filename=ACE_Step1.5_xl_turbo_00001.mp3&subfolder=audio&type=output";
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "prompt_id": PROMPT_ID,
            "status": "completed",
            "workflow_size": null,
            "outputs": [url],
            "outputs_by_node": { "107": [url] },
            "outputs_by_item": {},
            "text_outputs": {},
            "error": null
        }))])
        .await;

        let status: JobStatus = client.job_status(PROMPT_ID).await.expect("status");
        assert_eq!(status.status, "completed");
        assert_eq!(status.outputs, vec![url.to_string()]);
        assert_eq!(
            status.outputs_by_node.get("107").unwrap(),
            &vec![url.to_string()]
        );
        assert!(status.error.is_none());
    }

    /// Protects: the terminal-success status is `"completed"`, not `"success"` --
    /// the finding MCP-SURFACE 10.4 records. A pump keyed on `"success"` never
    /// sees a job finish.
    #[test]
    fn test_completed_is_terminal_and_success() {
        let status = JobStatus {
            prompt_id: Some(PROMPT_ID.to_string()),
            status: "completed".to_string(),
            workflow_size: None,
            outputs: vec![],
            outputs_by_node: BTreeMap::new(),
            error: None,
        };
        assert!(status.is_terminal());
        assert!(status.is_success());
    }

    /// Protects: a failure is terminal but not success. `"error"` is inferred
    /// (the failure path was not reproduced live -- see the `error` field docs).
    #[test]
    fn test_error_is_terminal_but_not_success() {
        let status = JobStatus {
            prompt_id: Some(PROMPT_ID.to_string()),
            status: "error".to_string(),
            workflow_size: None,
            outputs: vec![],
            outputs_by_node: BTreeMap::new(),
            error: Some(json!({ "message": "node failed" })),
        };
        assert!(status.is_terminal());
        assert!(!status.is_success());
    }

    /// Protects: a non-terminal status keeps the pump polling.
    #[test]
    fn test_running_is_not_terminal() {
        let status = JobStatus {
            prompt_id: Some(PROMPT_ID.to_string()),
            status: "running".to_string(),
            workflow_size: Some(11),
            outputs: vec![],
            outputs_by_node: BTreeMap::new(),
            error: None,
        };
        assert!(!status.is_terminal());
        assert!(!status.is_success());
    }

    #[tokio::test]
    async fn test_cancel_job_sends_action_and_decodes_booleans() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "prompt_id": PROMPT_ID,
            "where": "local",
            "found": true,
            "queue_delete_ok": true,
            "interrupt_ok": true
        }))])
        .await;

        let cancel: JobCancel = client.cancel_job(PROMPT_ID).await.expect("cancel");
        assert!(cancel.found);
        assert!(cancel.queue_delete_ok);
        assert!(cancel.interrupt_ok);

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("job"));
        assert_eq!(log[0]["arguments"]["action"], json!("cancel"));
        assert_eq!(log[0]["arguments"]["prompt_id"], json!(PROMPT_ID));
    }

    #[tokio::test]
    async fn test_outputs_sends_prompt_id_and_out_dir_and_decodes_files() {
        let url = "http://127.0.0.1:8188/view?filename=ACE_Step1.5_xl_turbo_00001.mp3&subfolder=audio&type=output";
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "prompt_id": PROMPT_ID,
            "out_dir": "C:/out",
            "files": [{ "url": url, "path": "C:/out/196a0dc9_000.mp3", "size": 293906 }]
        }))])
        .await;

        let batch: OutputBatch = client
            .outputs(PROMPT_ID, std::path::Path::new("C:/out"))
            .await
            .expect("outputs");
        assert_eq!(batch.files.len(), 1);
        assert_eq!(batch.files[0].url, url);
        assert_eq!(batch.files[0].size, 293906);
        assert_eq!(
            batch.files[0].path,
            std::path::PathBuf::from("C:/out/196a0dc9_000.mp3")
        );

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("fetch_outputs"));
        assert_eq!(log[0]["arguments"]["prompt_id"], json!(PROMPT_ID));
        assert_eq!(log[0]["arguments"]["out_dir"], json!("C:/out"));
    }
}
