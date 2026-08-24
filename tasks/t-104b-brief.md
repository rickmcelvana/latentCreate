# T-104b: Tauri managed state + job event pump
**Depends:** T-104a | **Crate/dir:** `src-tauri/` | **Executor:** Aider

**Files to create:** `src-tauri/src/jobs.rs`

**Files to modify:** `src-tauri/src/lib.rs`, `src-tauri/Cargo.toml`

> Second half of T-104 (job lifecycle + event pump). This brief is the Rust plumbing: the
> backend in managed state, `run`/`cancel` commands, and a cancellable poll task that re-emits
> `job://progress|done|failed`. The frontend bridge + jobs store + queue panel is a later task.
> The `ComfyBackend` trait stays deferred — managed state holds `Arc<LocalComfy>` concretely.

## Goal
`src-tauri` gains a `ComfyState` managed state (an `Arc<LocalComfy>` plus a map of active job
pumps), a `connect_comfy` command to establish the backend, and `run_workflow` / `cancel_job`
commands. `run_workflow` submits a workflow and spawns a cancellable poll task that streams
`job://progress` per poll and a terminal `job://done` or `job://failed` — the UI never polls
Rust; Rust pushes (ARCHITECTURE §3a).

## Verified, not recalled
The tauri 2.11 API was read from the crate source, not remembered:
- `Emitter::emit<S: Serialize + Clone>(&self, event: &str, payload: S) -> Result<()>` — so
  `app.emit("job://done", payload)` works with `use tauri::Emitter;`.
- `tauri::async_runtime::spawn<F: Future + Send + 'static>(task: F) -> JoinHandle<F::Output>`,
  and that `JoinHandle` exposes `.inner() -> &tokio::task::JoinHandle` (whose `.abort_handle()`
  yields the `tokio::task::AbortHandle` stored in managed state) and `.abort()`.
- `#[tauri::command]` takes `app: tauri::AppHandle` and `state: tauri::State<'_, T>` as
  injected parameters (the `synchronize` doc example).

The reference code compiles against the real `tauri` + `mcp-bridge`, is `cargo fmt`- and
`clippy -D warnings`-clean, and 6 scratch tests pass. The tokio features `["time", "sync"]` are
the minimal set that compiled (`time` for `sleep` and, transitively, the `task` module's
`AbortHandle`; `sync` for `RwLock`).

## Reference code

### `src-tauri/src/jobs.rs` — full file
```rust
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
```

### `src-tauri/src/lib.rs`
Three changes. `ConfigDir` stays as-is (crate-visible, so `jobs.rs`'s `use crate::ConfigDir` and
`.0` field access both resolve).

1. Add the module + import, after the existing `use` lines:
```rust
mod jobs;

use jobs::ComfyState;
```

2. In `setup`, after `app.manage(ConfigDir(dir));`:
```rust
            app.manage(ComfyState::default());
```

3. In `invoke_handler`, add the three commands to the existing `generate_handler!` list:
```rust
            jobs::connect_comfy,
            jobs::run_workflow,
            jobs::cancel_job,
```

### `src-tauri/Cargo.toml`
Add `tokio` to `[dependencies]` and a new `[dev-dependencies]` section:
```toml
tokio = { version = "1.53", features = ["time", "sync"] }
```
```toml
[dev-dependencies]
tokio = { version = "1.53", features = ["macros", "rt"] }
```

## Tests
Six new tests in `jobs.rs`'s `tests` module, none needing a live ComfyUI or a Tauri window.
`poll_until_terminal` takes the status source as a closure, so a canned sequence stands in for
`job_status`; `terminal_outcome`/`failure_reason` are pure.

- `test_poll_emits_non_terminal_and_returns_terminal` — **protects:** the loop — non-terminal
  statuses are handed to `on_update` and the terminal one is returned, not emitted. A pump that
  emitted the terminal status as "progress" would double-report.
- `test_poll_terminal_immediately_emits_nothing` — **protects:** the first-poll-terminal case
  emits no progress; the terminal event alone carries the result.
- `test_poll_error_returns_the_error` — **protects:** a poll error propagates instead of looping
  forever on a dead job. The caller maps it to `job://failed`.
- `test_terminal_outcome_maps_completed_to_done` — **protects:** `"completed"` (T-104a's finding)
  becomes `Done`, carrying the outputs.
- `test_terminal_outcome_maps_error_status_to_failed` — **protects:** a failed status becomes
  `Failed` with the error payload's string message.
- `test_terminal_outcome_maps_poll_error_to_failed` — **protects:** a transport error becomes
  `Failed` too, so a wedged connection surfaces as a failure, not a silent stop.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p app --all-targets -- -D warnings` clean
- [ ] All six named tests present and passing
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] No changes outside the three listed files (plus `Cargo.lock` for the `tokio` dep)
- [ ] No new dependency crate — only `tokio` (already in the workspace tree)

## Out of scope
The frontend bridge wrappers (`app/src/bridge/jobs.ts`), the jobs Zustand store, and the queue
panel UI — a later task. `health`/`stats`/`launch_comfyui`/`stop_comfyui` commands (T-110 wizard).
The full §7 pipeline (fetch_template → set_slots → run) and `job(action="wait"|"watch")`.
Graceful backend shutdown (`LocalComfy::shutdown` needs `Arc` unwrapping — deferred). The
`ComfyBackend` trait (deferred — decisions log 2026-08-24).

## Notes for the executor
- `ConfigDir` is private in `lib.rs` but crate-visible; `jobs.rs` uses it as `crate::ConfigDir`
  and reads `.0`. Do not move it or make it `pub` unless the compiler demands it.
- `poll_until_terminal` takes the poll source by `String` (owned), not `&str` — an owned id lets
  the `async move` block be `'static`, which `async_runtime::spawn` requires. Do not change it
  back to a borrow.
- The `jobs` map holds `tokio::task::AbortHandle` (obtained via `handle.inner().abort_handle()`);
  `tauri::async_runtime::JoinHandle` itself is not `Clone`, which is why the map stores the
  `AbortHandle`, not the `JoinHandle`.
- `on_update` is `FnMut(&JobStatus)`; the terminal status is deliberately NOT passed to it.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
The `mcp-bridge` modules are `--read`: `jobs.rs` constructs `LocalComfy`/`SessionLog`, calls
`run`/`job_status`/`cancel_job`, and reads `JobRun.prompt_id` + `JobStatus` fields/helpers.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/lib.rs --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/local.rs --read crates/mcp-bridge/src/session_log.rs --read crates/mcp-bridge/src/jobs.rs --file src-tauri/Cargo.toml --file src-tauri/src/lib.rs --file src-tauri/src/jobs.rs
```
