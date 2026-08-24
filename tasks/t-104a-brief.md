# T-104a: job lifecycle wrappers — run, poll, cancel, fetch outputs
**Depends:** T-102c | **Crate/dir:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/jobs.rs`

**Files to modify:** `crates/mcp-bridge/src/lib.rs`

> First half of T-104 (job lifecycle + event pump), split the T-103 way. This brief is the four
> typed wrappers and their types. **T-104b** (Tauri managed state + event pump) consumes these.
> The `ComfyBackend` trait is **deferred** (decisions log 2026-08-24) — these are methods on
> `LocalComfy`, like every other landed wrapper.

## Goal
`run_workflow(wait=false)` → `JobRun` (carrying the `prompt_id` handle), `job(action="status")` →
`JobStatus` (with `is_terminal`/`is_success`), `job(action="cancel")` → `JobCancel`, and
`fetch_outputs` → `OutputBatch` (downloaded files). All four are `call`-based two-stage decodes,
exactly like the T-103 wrappers.

## Verified, not recalled
Every shape was captured live 2026-08-24 against the running server (comfy-cli 1.16.0, ComfyUI
v0.33.3) via a real short ACE-Step 1.5 turbo generation — recorded in **docs/MCP-SURFACE.md §10**.
The reference code compiles, is `cargo fmt`- and `clippy -D warnings`-clean, and all 35 scratch
tests (9 new) pass.

Two findings the types encode, both from the live capture:
- ⚠ **The terminal-success status is `"completed"`, not `"success"`.** A pump keyed on `"success"`
  never sees a job finish. Also, there is **no `progress`/`total` numeric field** on this shape —
  progress is conveyed by status transitions and `outputs` filling in (T-104b polls on an
  interval).
- ⚠ **`run_workflow` pre-validates** (§10.1): a bad workflow is rejected with `[workflow_unknown_nodes]`
  *before* queueing, so the wrapper's error granularity is comfy-cli's, not `/prompt`'s.

**One honesty note for the reviewer:** the *failure* shape — a non-null `error` and a distinct
failure status — was **not reproduced live** (it needs a workflow that passes validation but fails
a node). `is_terminal` treats `"error"`/`"failed"` as terminal on ComfyUI's known vocabulary, and
`error` is `Option<Value>` rather than narrowed. Say so if a real failure capture ever turns up.

## Reference code

### `crates/mcp-bridge/src/jobs.rs` — full file
```rust
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
    /// Full `view?…` URLs of the outputs produced so far.
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
    /// The `view?…` URL the file came from.
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
    /// own task rather than blocking (ARCHITECTURE §3).
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
    /// §10.5), and the call's booleans are the confirmation.
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
    /// the finding MCP-SURFACE §10.4 records. A pump keyed on `"success"` never
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
```

### `crates/mcp-bridge/src/lib.rs`
Add the module (alphabetical, between `error` and `local`) and the re-export (between `error` and
`local`):
```rust
mod jobs;
```
```rust
pub use jobs::{JobCancel, JobRun, JobStatus, OutputBatch, OutputFile};
```

## Tests
Nine new tests in `jobs.rs`'s `tests` module. The existing 55 crate tests are unchanged and must
keep passing. Per test, the invariant:

- `test_run_sends_workflow_path_and_wait_false` — **protects:** argument naming (MCP-SURFACE §8.7)
  and the hard `wait: false`. `run_workflow` rejects a misnamed argument outright, so this is the
  test a misspelling fails.
- `test_run_decodes_the_prompt_id` — **protects:** the essential decode — `prompt_id` extracted
  from the envelope, and the `null`-able `elapsed_seconds` reads as `None` rather than failing.
- `test_job_status_sends_action_and_prompt_id` — **protects:** the `action`/`prompt_id` argument
  pair, sent verbatim.
- `test_job_status_decodes_a_completed_job` — **protects:** the captured terminal shape decodes —
  status, the `outputs` URL list, and `outputs_by_node` keyed by node id, with `error: null` →
  `None`.
- `test_completed_is_terminal_and_success` — **protects:** the headline finding — `"completed"`
  (not `"success"`) is terminal *and* success. A pump gated on `"success"` never finishes a job.
- `test_error_is_terminal_but_not_success` — **protects:** the failure arm of the tri-state. The
  `"error"` status is terminal but not success; documented as inferred (failure not reproduced live).
- `test_running_is_not_terminal` — **protects:** the poll keeps going for a non-terminal status.
  Without this, `is_terminal` could collapse to "always true" and the pump would stop early.
- `test_cancel_job_sends_action_and_decodes_booleans` — **protects:** the cancel path — the three
  booleans decode, and the arg pair is `action=cancel`.
- `test_outputs_sends_prompt_id_and_out_dir_and_decodes_files` — **protects:** the fetch path —
  `files` decode with `url`/`path`/`size`, and `prompt_id`/`out_dir` go out verbatim.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All nine named tests present and passing; the pre-existing tests still pass
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] No changes outside the two listed files
- [ ] No new dependencies

## Out of scope
The Tauri event pump and managed state (T-104b). `job(action="wait"|"watch")` — the pump uses
`job_status` polling on its own interval; wait/watch are not wrapped yet (they're a T-104b choice
about how to block). `job(action="queue")` listing. Anything that maps a job to a library track.
The `ComfyBackend` trait (deferred — decisions log 2026-08-24).

## Notes for the executor
- `error` stays `Option<Value>`, not `Option<String>` — the failure payload shape is unverified;
  do not "helpfully" narrow it.
- Do not model `outputs` in `JobStatus` as anything but `Vec<String>` — the live capture showed
  URL strings. Keep `outputs_by_node` a `BTreeMap` for stable ordering (CONVENTIONS).
- `run` always sends `wait: false`. There is no `wait` parameter on the method.
- Keep the status strings as plain `String` with the two helper methods; do not invent a status
  enum (the set is open and only `completed` is verified).
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
`error.rs`, `local.rs` and `mock.rs` are `--read`: `jobs.rs` does `impl LocalComfy` (so the
executor needs the struct + `call` in view), returns `ComfyError`, and the tests use
`client_and_log`/`Reply`.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/local.rs --read crates/mcp-bridge/src/mock.rs --file crates/mcp-bridge/src/jobs.rs --file crates/mcp-bridge/src/lib.rs
```
