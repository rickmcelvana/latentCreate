//! Job pump: poll a running job and re-emit its lifecycle as Tauri events.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use mcp_bridge::{ComfyError, JobStatus, LocalComfy, SessionLog};
use serde::Serialize;
use tauri::{async_runtime, AppHandle, Emitter, State};
use tokio::sync::RwLock;

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

/// The backend and the active job pumps, held as Tauri managed state.
///
/// `comfy` is `None` until [`connect_comfy`] succeeds. `jobs` maps a prompt id
/// to the abort handle of its monitor task, so [`cancel_job`] can stop a pump
/// that is stuck polling (CONVENTIONS: no detached fire-and-forget loops).
#[derive(Default)]
pub struct ComfyState {
    comfy: RwLock<Option<Arc<LocalComfy>>>,
    jobs: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
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

    let jobs = Arc::clone(&state.jobs);
    let handle = async_runtime::spawn(monitor_job(app, comfy, id.clone(), jobs));
    let abort = handle.inner().abort_handle();
    state
        .jobs
        .lock()
        .expect("jobs lock poisoned")
        .insert(id.clone(), abort);

    Ok(id)
}

/// Cancel a running job and stop its pump.
#[tauri::command]
pub async fn cancel_job(state: State<'_, ComfyState>, id: String) -> Result<(), String> {
    let comfy = state
        .comfy
        .read()
        .await
        .clone()
        .ok_or_else(|| "comfy is not connected".to_string())?;
    comfy.cancel_job(&id).await.map_err(|e| e.to_string())?;

    if let Some(abort) = state.jobs.lock().expect("jobs lock poisoned").remove(&id) {
        abort.abort();
    }
    Ok(())
}

/// Poll a job until terminal, emitting progress for each non-terminal status.
async fn monitor_job(
    app: AppHandle,
    comfy: Arc<LocalComfy>,
    id: String,
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

    if let Ok(mut jobs) = jobs.lock() {
        jobs.remove(&id);
    }
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
    Done { outputs: Vec<String> },
    Failed { error: String },
}

fn terminal_outcome(result: &Result<JobStatus, ComfyError>) -> TerminalOutcome {
    match result {
        Ok(status) if status.is_success() => TerminalOutcome::Done {
            outputs: status.outputs.clone(),
        },
        Ok(status) => TerminalOutcome::Failed {
            error: failure_reason(status),
        },
        Err(e) => TerminalOutcome::Failed {
            error: e.to_string(),
        },
    }
}

/// The error text for a failed job: the payload's message when it is a string,
/// else the status string.
fn failure_reason(status: &JobStatus) -> String {
    status
        .error
        .as_ref()
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| status.status.clone())
}

#[cfg(test)]
mod tests {
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
