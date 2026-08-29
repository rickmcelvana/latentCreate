//! Job pump: poll a running job and re-emit its lifecycle as Tauri events.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcp_bridge::{ComfyError, JobCancel, JobStatus, LocalComfy, SessionLog};
use serde::Serialize;
use serde_json::Value;
use tauri::{async_runtime, AppHandle, Emitter, State};
use tokio::sync::RwLock;

use crate::ingest::{ingest_outputs, PendingTrack};
use crate::ConfigDir;

/// Poll interval for the job pump.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// Emitted on every poll while a job is not yet terminal.
#[derive(Debug, Clone, Serialize)]
pub struct JobProgress {
    pub id: String,
    pub status: String,
    pub outputs: Vec<String>,
}

/// Emitted once when a job finishes successfully.
#[derive(Debug, Clone, Serialize)]
pub struct JobDone {
    pub id: String,
    pub outputs: Vec<String>,
}

/// Emitted once when a job fails or polling it errors.
#[derive(Debug, Clone, Serialize)]
pub struct JobFailed {
    pub id: String,
    pub error: String,
}

/// Emitted once per track saved after a successful generation.
#[derive(Debug, Clone, Serialize)]
pub struct TrackSaved {
    pub id: String,
    pub project_slug: String,
    pub file: String,
}

/// The backend and the active job pumps, held as Tauri managed state.
///
/// `comfy` is `None` until [`connect_comfy`] succeeds. `jobs` maps a prompt id
/// to the abort handle of its monitor task, so [`cancel_job`] can stop a pump
/// that is stuck polling (CONVENTIONS: no detached fire-and-forget loops).
/// `pending` holds the provenance records for jobs submitted through
/// `generate_audio`; a bare `run_workflow` job has no record and is not ingested.
#[derive(Default)]
pub struct ComfyState {
    comfy: RwLock<Option<Arc<LocalComfy>>>,
    jobs: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
    pending: Arc<Mutex<HashMap<String, PendingTrack>>>,
}

impl ComfyState {
    /// The connected backend, or `None` before the first successful connect.
    pub async fn connected(&self) -> Option<Arc<LocalComfy>> {
        self.comfy.read().await.clone()
    }

    /// Store a freshly connected backend, replacing any existing one, and
    /// hand back the shared handle.
    pub async fn store(&self, comfy: LocalComfy) -> Arc<LocalComfy> {
        let comfy = Arc::new(comfy);
        *self.comfy.write().await = Some(Arc::clone(&comfy));
        comfy
    }

    /// How many pumps are running.
    ///
    /// Exists so a test can pin that cancelling does **not** retire one; a pump
    /// is retired by itself, on reaching a terminal status.
    #[cfg(test)]
    pub(crate) fn pump_count(&self) -> usize {
        self.jobs.lock().expect("jobs lock poisoned").len()
    }

    /// Start the lifecycle pump for a job ComfyUI has already accepted.
    ///
    /// The one way a submitted job becomes `job://` events. Every submitter
    /// calls this rather than spawning its own monitor -- a second lifecycle
    /// would emit a second set of events for the same prompt id, and
    /// [`cancel_job`] would only know about one of them.
    /// Record a job's provenance, when it has any.
    ///
    /// Split out of [`ComfyState::pump`] so the rule can be tested without an
    /// `AppHandle`: no test in this crate builds one, and reaching for
    /// `tauri::test` would turn a one-line invariant into a new dependency
    /// feature. `None` is a bare `run_workflow` submission, which has no
    /// profile behind it and therefore no sidecar to write.
    fn remember(&self, id: &str, pending: Option<PendingTrack>) {
        if let Some(p) = pending {
            self.pending
                .lock()
                .expect("pending lock poisoned")
                .insert(id.to_string(), p);
        }
    }

    /// The provenance record for a finished job, if it had one.
    #[cfg(test)]
    fn remembered(&self, id: &str) -> Option<PendingTrack> {
        self.pending
            .lock()
            .expect("pending lock poisoned")
            .get(id)
            .cloned()
    }

    pub(crate) fn pump(
        &self,
        app: AppHandle,
        comfy: Arc<LocalComfy>,
        id: String,
        pending: Option<PendingTrack>,
        root: PathBuf,
    ) {
        self.remember(&id, pending);
        let jobs = Arc::clone(&self.jobs);
        let pending_map = Arc::clone(&self.pending);
        let handle =
            async_runtime::spawn(monitor_job(app, comfy, id.clone(), root, pending_map, jobs));
        self.jobs
            .lock()
            .expect("jobs lock poisoned")
            .insert(id, handle.inner().abort_handle());
    }
}

/// Connect to `comfy-mcp`, replacing any existing connection.
#[tauri::command]
pub async fn connect_comfy(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    bin: Option<String>,
) -> Result<(), String> {
    let bin = bin.unwrap_or_else(|| "comfy-mcp".to_string());
    let log = SessionLog::open(config_dir.0.join("session.log")).map_err(|e| e.to_string())?;
    let comfy = LocalComfy::connect(&bin, log)
        .await
        .map_err(|e| e.to_string())?;
    *state.comfy.write().await = Some(Arc::new(comfy));
    Ok(())
}

/// Submit a workflow and stream its lifecycle as `job://progress|done|failed`
/// events. Returns the prompt id the events are keyed on.
#[tauri::command]
pub async fn run_workflow(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    workflow_path: String,
) -> Result<String, String> {
    let comfy = state
        .comfy
        .read()
        .await
        .clone()
        .ok_or_else(|| "comfy is not connected".to_string())?;

    let run = comfy
        .run(std::path::Path::new(&workflow_path))
        .await
        .map_err(|e| e.to_string())?;
    let id = run.prompt_id.clone();
    state.pump(app, comfy, id.clone(), None, config_dir.0.clone());
    Ok(id)
}

/// Ask ComfyUI to stop a job, and report what it actually managed.
///
/// **The pump is deliberately left running.** Aborting it here is what made
/// cancel look broken: comfy-cli stops the job within seconds and reports
/// `status: "cancelled"`, but killing the monitor removed the only thing that
/// could ever say so, leaving the row stuck on "running" for ever. The next
/// job then ran normally beside a stale row, which reads exactly like two
/// generations at once (MCP-SURFACE 21).
///
/// The pump observes the cancellation on its next poll, emits
/// `job://cancelled`, and retires itself -- and if the cancel did **not** take,
/// it keeps reporting the job that is still running, which is the truth.
#[tauri::command]
pub async fn cancel_job(state: State<'_, ComfyState>, id: String) -> Result<JobCancel, String> {
    cancel_on(&state, &id).await
}

/// The body of [`cancel_job`], taking the state directly so a test can hold one.
///
/// Split out for exactly one reason: the rule that matters here is that this
/// function **does not** retire the job's pump, and an absence of code is not
/// something a test can reach through a `tauri::State`.
async fn cancel_on(state: &ComfyState, id: &str) -> Result<JobCancel, String> {
    let comfy = state
        .comfy
        .read()
        .await
        .clone()
        .ok_or_else(|| "comfy is not connected".to_string())?;
    comfy.cancel_job(id).await.map_err(|e| e.to_string())
}

/// `job://cancelled` payload: the job stopped because somebody stopped it.
#[derive(Debug, Clone, Serialize)]
pub struct JobCancelled {
    pub id: String,
}

/// Poll a job until terminal, emitting progress for each non-terminal status.
async fn monitor_job(
    app: AppHandle,
    comfy: Arc<LocalComfy>,
    id: String,
    root: PathBuf,
    pending: Arc<Mutex<HashMap<String, PendingTrack>>>,
    jobs: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
) {
    let result = poll_until_terminal(
        id.clone(),
        POLL_INTERVAL,
        |id| {
            let comfy = Arc::clone(&comfy);
            async move { comfy.job_status(&id).await }
        },
        |status| {
            let _ = app.emit(
                "job://progress",
                JobProgress {
                    id: id.clone(),
                    status: status.status.clone(),
                    outputs: status.outputs.clone(),
                },
            );
        },
    )
    .await;

    match terminal_outcome(&result) {
        TerminalOutcome::Done { outputs } => {
            let _ = app.emit(
                "job://done",
                JobDone {
                    id: id.clone(),
                    outputs,
                },
            );
            ingest_if_pending(&app, &comfy, &id, &root, &pending).await;
        }
        TerminalOutcome::Cancelled => {
            let _ = app.emit("job://cancelled", JobCancelled { id: id.clone() });
        }
        TerminalOutcome::Failed { error } => {
            let _ = app.emit(
                "job://failed",
                JobFailed {
                    id: id.clone(),
                    error,
                },
            );
        }
    }

    if let Ok(mut p) = pending.lock() {
        p.remove(&id);
    }
    if let Ok(mut jobs) = jobs.lock() {
        jobs.remove(&id);
    }
}

/// Fetch outputs and turn them into tracks when there is a pending record.
async fn ingest_if_pending(
    app: &AppHandle,
    comfy: &Arc<LocalComfy>,
    id: &str,
    root: &Path,
    pending: &Arc<Mutex<HashMap<String, PendingTrack>>>,
) {
    let maybe_pending = pending.lock().ok().and_then(|m| m.get(id).cloned());
    let Some(p) = maybe_pending else {
        return;
    };

    let tracks_dir = match library::tracks::tracks_dir(root, &p.project_slug) {
        Ok(d) => d,
        Err(e) => {
            log_ingest_failure(app, id, root, &e.to_string());
            return;
        }
    };
    if let Err(e) = std::fs::create_dir_all(&tracks_dir) {
        log_ingest_failure(app, id, root, &e.to_string());
        return;
    }

    let batch = match comfy.outputs(id, &tracks_dir).await {
        Ok(b) => b,
        Err(e) => {
            log_ingest_failure(app, id, root, &e.to_string());
            return;
        }
    };

    let now = library::projects::now_rfc3339();
    match ingest_outputs(root, &p, &batch, &now) {
        Ok(tracks) => {
            for track in tracks {
                let _ = app.emit(
                    "track://saved",
                    TrackSaved {
                        id: track.id.0.clone(),
                        project_slug: p.project_slug.clone(),
                        file: track.file.clone(),
                    },
                );
            }
        }
        Err(e) => {
            log_ingest_failure(app, id, root, &e.to_string());
        }
    }
}

/// Log an ingest failure and re-emit it as a job failure so the row does not
/// show Done for a track that was never saved.
fn log_ingest_failure(app: &AppHandle, id: &str, root: &Path, message: &str) {
    if let Ok(log) = SessionLog::open(root.join("session.log")) {
        log.log_result("ingest_outputs", false, message);
    }
    let _ = app.emit(
        "job://failed",
        JobFailed {
            id: id.to_string(),
            error: format!("ingestion failed: {message}"),
        },
    );
}

/// Poll `job_status` until the job is terminal, calling `on_update` for each
/// non-terminal status. Returns the terminal status, or the first poll error.
///
/// `poll` is the status source (a closure over `LocalComfy::job_status` in
/// production, a canned sequence in tests); `on_update` receives each
/// non-terminal status. Terminal statuses are returned, not emitted, so the
/// caller can map them to their final event.
async fn poll_until_terminal<F, Fut>(
    id: String,
    interval: Duration,
    mut poll: F,
    mut on_update: impl FnMut(&JobStatus),
) -> Result<JobStatus, ComfyError>
where
    F: FnMut(String) -> Fut,
    Fut: std::future::Future<Output = Result<JobStatus, ComfyError>>,
{
    loop {
        let status = poll(id.clone()).await?;
        if status.is_terminal() {
            return Ok(status);
        }
        on_update(&status);
        tokio::time::sleep(interval).await;
    }
}

/// The terminal event a finished poll sequence produces.
#[derive(Debug, PartialEq)]
enum TerminalOutcome {
    Done {
        outputs: Vec<String>,
    },
    /// Stopped on purpose. Distinct from `Failed` because nothing went wrong,
    /// and a row reading "failed" with an error text would misreport the
    /// user's own decision back to them.
    Cancelled,
    Failed {
        error: String,
    },
}

fn terminal_outcome(result: &Result<JobStatus, ComfyError>) -> TerminalOutcome {
    match result {
        Ok(status) if status.is_success() => TerminalOutcome::Done {
            outputs: status.outputs.clone(),
        },
        Ok(status) if status.is_cancelled() => TerminalOutcome::Cancelled,
        Ok(status) => TerminalOutcome::Failed {
            error: failure_reason(status),
        },
        Err(e) => TerminalOutcome::Failed {
            error: e.to_string(),
        },
    }
}

/// The sentence a failed job's row shows.
///
/// **This read `error.as_str()` and nothing else until 2026-08-28**, which is
/// correct for exactly none of the shapes comfy-cli actually sends. A real
/// failure's payload is an *object*, so `as_str()` returned `None` and the
/// fallback rendered the whole error as the bare word `"error"`. Every test
/// passed, because every test was written with `error: json!("node blew up")`
/// -- a string nobody has ever observed.
///
/// The three shapes, captured verbatim in `testdata/mcp/job_outcomes.json`
/// (MCP-SURFACE 24.3):
///
/// - ComfyUI's raw history record: `exception_message` plus `node_type`, and
///   **no `code` key**. This is what an ordinary node failure looks like.
/// - comfy-cli's normalized record: `{code, message, details}`, and no
///   `exception_message`. A cancel arrives this way -- though a cancel never
///   reaches here, because [`TerminalOutcome::Cancelled`] catches it first.
/// - a bare string, which the old tests assumed and which is kept because
///   handling it costs one line and dropping it would be a guess in the other
///   direction.
///
/// The node name leads because it is the actionable half: "VAEDecodeAudio
/// failed" tells someone where to look, where a bare `RuntimeError` does not.
/// `traceback` is deliberately never read -- twelve frames of absolute paths
/// into the user's install (24.2).
fn failure_reason(status: &JobStatus) -> String {
    let Some(error) = status.error.as_ref() else {
        return status.status.clone();
    };
    if let Some(text) = error.as_str() {
        return text.to_string();
    }

    let field = |name: &str| {
        error
            .get(name)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string)
    };

    // `exception_message` first: on the one payload that carries both, it is
    // the specific one. `message` is the normalized record's equivalent.
    let Some(detail) = field("exception_message").or_else(|| field("message")) else {
        return status.status.clone();
    };
    match field("node_type") {
        Some(node) => format!("{node} failed: {detail}"),
        None => detail,
    }
}

#[cfg(test)]
mod tests {

    /// Protects: cancelling leaves the job's pump running.
    ///
    /// This is the defect a producer found by pressing the button, and the one
    /// that made cancel look completely broken. comfy-cli stops the job within
    /// seconds and reports `status: "cancelled"` -- but this command used to
    /// abort the monitor task, which was **the only thing that could ever say
    /// so**. The row froze on "running" for ever, and the next job ran normally
    /// beside it, which reads as two generations at once (MCP-SURFACE 21).
    ///
    /// The pump retires itself on a terminal status. Nothing else may retire
    /// it, because nothing else knows whether the cancel actually took.
    #[tokio::test]
    async fn test_cancelling_does_not_retire_the_pump() {
        use mcp_bridge::mock::Reply;
        use mcp_bridge::test_helpers::client_and_log;

        let (comfy, _calls) = client_and_log(vec![Reply::Json(serde_json::json!({
            "found": true,
            "queue_delete_ok": true,
            "interrupt_ok": true
        }))])
        .await;

        let state = ComfyState::default();
        state.store(comfy).await;
        let handle = async_runtime::spawn(async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        });
        state
            .jobs
            .lock()
            .expect("jobs lock poisoned")
            .insert("p-1".to_string(), handle.inner().abort_handle());
        assert_eq!(state.pump_count(), 1);

        let cancelled = cancel_on(&state, "p-1").await.expect("the call succeeds");

        assert!(cancelled.interrupt_ok);
        assert_eq!(
            state.pump_count(),
            1,
            "the pump must survive to report the cancellation"
        );
        assert!(
            !handle.inner().is_finished(),
            "the monitor task must not have been aborted"
        );
    }

    /// Protects: the three booleans reach the caller instead of being dropped.
    ///
    /// The command used to return `Ok(())` whatever came back, so a cancel that
    /// found nothing was indistinguishable from one that stopped a run.
    #[tokio::test]
    async fn test_a_cancel_that_found_nothing_says_so() {
        use mcp_bridge::mock::Reply;
        use mcp_bridge::test_helpers::client_and_log;

        let (comfy, _calls) = client_and_log(vec![Reply::Json(serde_json::json!({
            "found": false,
            "queue_delete_ok": false,
            "interrupt_ok": false
        }))])
        .await;
        let state = ComfyState::default();
        state.store(comfy).await;

        let cancelled = cancel_on(&state, "gone").await.expect("the call succeeds");

        assert!(!cancelled.found);
        assert!(!cancelled.interrupt_ok);
    }

    /// Protects: a bare `run_workflow` job growing a provenance record.
    ///
    /// `run_workflow` submits a workflow with no profile behind it, so there is
    /// nothing to write a sidecar from -- and inventing one would file a track
    /// under a model that never ran. The `Option` states that in the type; this
    /// asserts the type is actually honoured.
    ///
    /// Tests [`ComfyState::remember`] rather than [`ComfyState::pump`]: pump
    /// needs an `AppHandle`, no test in this crate builds one, and enabling
    /// `tauri`'s `test` feature to reach a one-line rule would be a dependency
    /// change made in service of a test.
    #[test]
    fn test_a_bare_run_workflow_job_has_no_pending_record() {
        let state = ComfyState::default();

        state.remember("bare", None);
        assert!(
            state.remembered("bare").is_none(),
            "a job with no profile must not be remembered"
        );

        state.remember("generated", Some(sample_pending()));
        assert!(
            state.remembered("generated").is_some(),
            "a generated job must be remembered, or its provenance is lost"
        );
    }

    /// A minimal pending record; only its presence is under test here.
    fn sample_pending() -> PendingTrack {
        PendingTrack {
            project_slug: "night-drive".to_string(),
            profile_id: "ace-step-1.5-turbo".to_string(),
            profile_display_name: "ACE-Step 1.5 XL Turbo".to_string(),
            model_license: "Apache-2.0".to_string(),
            template: None,
            spec: create_core::generation::GenerationSpec {
                profile_id: "ace-step-1.5-turbo".to_string(),
                inputs: Default::default(),
                loras: Vec::new(),
                lyrics: None,
            },
            resolved_slots: Default::default(),
            comfy: None,
        }
    }

    /// Protects: a cancelled poll result becomes a cancellation, not a failure.
    ///
    /// The queue row is the only place a user learns what happened, and
    /// "failed" beside an error string reports their own decision back to them
    /// as a fault.
    #[test]
    fn test_a_cancelled_status_is_its_own_outcome() {
        let cancelled: JobStatus = serde_json::from_value(serde_json::json!({
            "prompt_id": "p-1",
            "status": "cancelled",
            "outputs": [],
            "error": { "code": "cancelled", "message": "Job was interrupted/cancelled." }
        }))
        .expect("decodes");

        assert_eq!(terminal_outcome(&Ok(cancelled)), TerminalOutcome::Cancelled);
    }

    /// Protects: a real failure is still a failure, carrying its reason.
    #[test]
    fn test_a_failed_status_still_carries_its_error() {
        let failed: JobStatus = serde_json::from_value(serde_json::json!({
            "prompt_id": "p-1", "status": "failed", "outputs": [], "error": "node blew up"
        }))
        .expect("decodes");

        assert_eq!(
            terminal_outcome(&Ok(failed)),
            TerminalOutcome::Failed {
                error: "node blew up".to_string()
            }
        );
    }

    /// Every job outcome this project has observed, captured verbatim.
    const OUTCOMES: &str = include_str!("../../testdata/mcp/job_outcomes.json");

    /// The `error` object ComfyUI really sent for the deliberate failure of
    /// MCP-SURFACE 24, read out of the committed capture rather than retyped.
    fn captured_execution_error() -> Value {
        let outcomes: Value = serde_json::from_str(OUTCOMES).expect("the capture decodes");
        outcomes["execution_error"]["action_status_error_object"].clone()
    }

    /// Protects: the payload the server actually sends produces a sentence a
    /// person can act on.
    ///
    /// **This is the test that was missing.** `failure_reason` read
    /// `error.as_str()`, which is `None` for every real failure, so the whole
    /// message rendered as the bare word `"error"` -- and the suite was green
    /// throughout, because its only fixture was a hand-written string.
    #[test]
    fn test_a_real_execution_error_names_the_node_and_the_exception() {
        let failed = JobStatus {
            prompt_id: Some("p-1".to_string()),
            status: "error".to_string(),
            workflow_size: None,
            outputs: vec![],
            outputs_by_node: Default::default(),
            error: Some(captured_execution_error()),
        };

        let reason = failure_reason(&failed);
        assert_eq!(
            reason,
            "VAEDecodeAudio failed: shape '[2, 64, 250]' is invalid for input of size 16000"
        );
        // The bug in one assertion: the fallback must not be what comes out.
        assert_ne!(reason, "error");
        assert!(
            !reason.contains("Comfy-Installs"),
            "no traceback, and no absolute paths into the user's install: {reason}"
        );
    }

    /// Protects: comfy-cli's *other* error shape still yields its message.
    ///
    /// It has no `exception_message` and no `node_type`, so a reader that only
    /// knows the ComfyUI shape falls through to the status string here.
    #[test]
    fn test_the_normalized_error_shape_yields_its_message() {
        let failed = JobStatus {
            prompt_id: Some("p-1".to_string()),
            status: "error".to_string(),
            workflow_size: None,
            outputs: vec![],
            outputs_by_node: Default::default(),
            error: Some(json!({
                "code": "execution_error", "message": "the node blew up", "details": {}
            })),
        };
        assert_eq!(failure_reason(&failed), "the node blew up");
    }

    /// Protects: an error payload carrying nothing sayable falls back to the
    /// status rather than rendering an empty row.
    #[test]
    fn test_an_unreadable_error_payload_falls_back_to_the_status() {
        let failed = JobStatus {
            prompt_id: Some("p-1".to_string()),
            status: "error".to_string(),
            workflow_size: None,
            outputs: vec![],
            outputs_by_node: Default::default(),
            error: Some(json!({ "details": {}, "exception_message": "   " })),
        };
        assert_eq!(failure_reason(&failed), "error");
    }
    use super::*;
    use serde_json::json;

    fn job_status(status: &str, error: Option<&str>) -> JobStatus {
        JobStatus {
            prompt_id: Some("id".to_string()),
            status: status.to_string(),
            workflow_size: None,
            outputs: vec![],
            outputs_by_node: Default::default(),
            error: error.map(|e| json!(e)),
        }
    }

    #[tokio::test]
    async fn test_poll_emits_non_terminal_and_returns_terminal() {
        let seq = vec![
            job_status("running", None),
            job_status("running", None),
            job_status("completed", None),
        ];
        let seq = std::sync::Mutex::new(seq.into_iter());
        let mut emitted = Vec::new();

        let result = poll_until_terminal(
            "id".to_string(),
            Duration::from_millis(1),
            |_| {
                let next = seq
                    .lock()
                    .expect("seq lock")
                    .next()
                    .expect("ran out of canned statuses");
                async move { Ok(next) }
            },
            |s| emitted.push(s.status.clone()),
        )
        .await;

        assert_eq!(result.expect("polls Ok").status, "completed");
        assert_eq!(emitted, vec!["running".to_string(), "running".to_string()]);
    }

    /// Protects: a first-poll terminal status must not be emitted as progress --
    /// it goes to the terminal event instead.
    #[tokio::test]
    async fn test_poll_terminal_immediately_emits_nothing() {
        let mut emitted = 0;
        let result = poll_until_terminal(
            "id".to_string(),
            Duration::from_millis(1),
            |_| async move { Ok(job_status("completed", None)) },
            |_| emitted += 1,
        )
        .await;

        assert!(result.expect("polls Ok").is_success());
        assert_eq!(emitted, 0);
    }

    /// Protects: a poll error is returned, not swallowed -- the caller maps it
    /// to a failed event rather than polling forever.
    #[tokio::test]
    async fn test_poll_error_returns_the_error() {
        let result = poll_until_terminal(
            "id".to_string(),
            Duration::from_millis(1),
            |_| async move { Err(ComfyError::Transport("closed".to_string())) },
            |_| {},
        )
        .await;

        assert!(result.is_err());
    }

    #[test]
    fn test_terminal_outcome_maps_completed_to_done() {
        let outcome = terminal_outcome(&Ok(job_status("completed", None)));
        assert_eq!(outcome, TerminalOutcome::Done { outputs: vec![] });
    }

    #[test]
    fn test_terminal_outcome_maps_error_status_to_failed() {
        let outcome = terminal_outcome(&Ok(job_status("error", Some("node failed"))));
        assert_eq!(
            outcome,
            TerminalOutcome::Failed {
                error: "node failed".to_string()
            }
        );
    }

    #[test]
    fn test_terminal_outcome_maps_poll_error_to_failed() {
        let outcome = terminal_outcome(&Err(ComfyError::Transport("closed".to_string())));
        assert!(matches!(
            outcome,
            TerminalOutcome::Failed { error } if error.contains("closed")
        ));
    }
}
