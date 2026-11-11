//! Job pump: follow a submitted job through ComfyUI's `/queue` and `/history`
//! and re-emit its lifecycle as Tauri events (ARCHITECTURE 3).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use comfy_bridge::{ComfyError, ComfyHttp, Outcome, OutputRef, QueuePosition, SessionLog};
use create_core::profile::ModelKind;
use serde::Serialize;
use serde_json::Value;
use tauri::{async_runtime, AppHandle, Emitter, State};
use tokio::sync::RwLock;

use crate::comfy::{ensure_connected, EnsureError};
use crate::ingest::{ingest_outputs, OutputBatch, OutputFile, PendingOutput, Saved};
use crate::ConfigDir;

/// Poll interval for the job pump.
const POLL_INTERVAL: Duration = Duration::from_millis(1000);

/// How many consecutive polls may find a job in neither the queue nor history
/// before it is reported lost. ComfyUI moved a finished job from the queue to
/// history within one poll in every observed run (COMFY-HTTP-SURFACE 6); three
/// misses tolerates a gap nobody has seen without hanging on one that is real.
const LOST_AFTER_MISSES: u8 = 3;

/// Emitted on every poll while a job is not yet terminal.
#[derive(Debug, Clone, Serialize)]
pub struct JobProgress {
    pub id: String,
    /// `pending` or `running` -- the words the queue panel renders.
    pub status: String,
    pub outputs: Vec<String>,
}

/// Emitted once when a job finishes successfully.
#[derive(Debug, Clone, Serialize)]
pub struct JobDone {
    pub id: String,
    /// The output filenames ComfyUI reported.
    pub outputs: Vec<String>,
}

/// Emitted once when a job fails or following it fails.
#[derive(Debug, Clone, Serialize)]
pub struct JobFailed {
    pub id: String,
    pub error: String,
}

/// `job://cancelled` payload: the job stopped because somebody stopped it.
#[derive(Debug, Clone, Serialize)]
pub struct JobCancelled {
    pub id: String,
}

/// Emitted once per track saved after a successful generation.
#[derive(Debug, Clone, Serialize)]
pub struct TrackSaved {
    pub id: String,
    pub project_slug: String,
    pub file: String,
}

/// Emitted once per artwork saved after a successful generation.
#[derive(Debug, Clone, Serialize)]
pub struct ArtSaved {
    pub id: String,
    pub project_slug: String,
    pub file: String,
}

/// What a cancel request did. Field names are the wire shape the queue panel
/// already reads.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct JobCancel {
    /// ComfyUI had the job queued or running.
    pub found: bool,
    /// It was pending and was removed from the queue.
    pub queue_delete_ok: bool,
    /// It was running and was interrupted.
    pub interrupt_ok: bool,
}

/// The connected ComfyUI and the live job pumps, held as Tauri managed state.
///
/// `cancelled` remembers which prompts this app asked to stop: a pending job
/// deleted from the queue never reaches history, so without the memory the
/// pump could not tell "cancelled" from "lost" (COMFY-HTTP-SURFACE 6).
#[derive(Default)]
pub struct ComfyState {
    comfy: RwLock<Option<Arc<ComfyHttp>>>,
    /// Serialises the check-probe-store in `ensure_connected`, so two
    /// first-use commands do not probe and store twice.
    pub(crate) connect: tokio::sync::Mutex<()>,
    jobs: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
    pending: Arc<Mutex<HashMap<String, PendingOutput>>>,
    cancelled: Arc<Mutex<HashSet<String>>>,
}

impl ComfyState {
    /// The connected client, or `None` before the first successful probe.
    pub async fn connected(&self) -> Option<Arc<ComfyHttp>> {
        self.comfy.read().await.clone()
    }

    /// Store a freshly found client, replacing any existing one.
    pub async fn store(&self, comfy: ComfyHttp) -> Arc<ComfyHttp> {
        let comfy = Arc::new(comfy);
        *self.comfy.write().await = Some(Arc::clone(&comfy));
        comfy
    }

    /// Forget the client, so the next use probes again.
    pub async fn forget(&self) {
        *self.comfy.write().await = None;
    }

    /// Start the lifecycle pump for a job ComfyUI has accepted. The one way a
    /// submitted job becomes `job://` events.
    pub(crate) fn pump(
        &self,
        app: AppHandle,
        comfy: Arc<ComfyHttp>,
        id: String,
        pending: Option<PendingOutput>,
        root: PathBuf,
    ) {
        if let Some(p) = pending {
            self.pending
                .lock()
                .expect("pending lock poisoned")
                .insert(id.clone(), p);
        }
        let jobs = Arc::clone(&self.jobs);
        let pending_map = Arc::clone(&self.pending);
        let cancelled = Arc::clone(&self.cancelled);
        let handle = async_runtime::spawn(monitor_job(
            app,
            comfy,
            id.clone(),
            root,
            pending_map,
            jobs,
            cancelled,
        ));
        self.jobs
            .lock()
            .expect("jobs lock poisoned")
            .insert(id, handle.inner().abort_handle());
    }
}

/// Probe for ComfyUI now, replacing any existing connection.
#[tauri::command]
pub async fn connect_comfy(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
) -> Result<(), String> {
    state.forget().await;
    ensure_connected(&state, &config_dir)
        .await
        .map(|_| ())
        .map_err(EnsureError::into_message)
}

/// Submit an API-format workflow file and follow it with `job://` events.
/// Returns the prompt id the events are keyed on. A bare run has no profile,
/// so nothing is ingested.
#[tauri::command]
pub async fn run_workflow(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    workflow_path: String,
) -> Result<String, String> {
    let comfy = ensure_connected(&state, &config_dir)
        .await
        .map_err(EnsureError::into_message)?;
    let text =
        std::fs::read_to_string(&workflow_path).map_err(|e| format!("{workflow_path}: {e}"))?;
    let graph: Value = serde_json::from_str(&text).map_err(|e| format!("{workflow_path}: {e}"))?;
    let id = comfy.submit(&graph).await.map_err(|e| e.to_string())?;
    state.pump(app, comfy, id.clone(), None, config_dir.0.clone());
    Ok(id)
}

/// Ask ComfyUI to stop a job.
///
/// **The pump is deliberately left running**: it observes the stop on its next
/// poll, emits `job://cancelled` and retires itself -- and if the cancel did
/// not take, it keeps reporting the job that is still running, which is the
/// truth (MCP-SURFACE 21).
#[tauri::command]
pub async fn cancel_job(state: State<'_, ComfyState>, id: String) -> Result<JobCancel, String> {
    let comfy = state
        .connected()
        .await
        .ok_or_else(|| "ComfyUI is not connected. Open Setup and Retry.".to_string())?;
    cancel_on(&comfy, &state.cancelled, &id).await
}

/// The body of [`cancel_job`]: interrupt it if running, delete it if pending,
/// and remember the request either way.
async fn cancel_on(
    comfy: &ComfyHttp,
    cancelled: &Mutex<HashSet<String>>,
    id: &str,
) -> Result<JobCancel, String> {
    let queue = comfy.queue().await.map_err(|e| e.to_string())?;
    let mut report = JobCancel::default();
    match queue.position(id) {
        Some(QueuePosition::Running) => {
            report.found = true;
            cancelled
                .lock()
                .expect("cancel lock")
                .insert(id.to_string());
            comfy.interrupt(id).await.map_err(|e| e.to_string())?;
            report.interrupt_ok = true;
        }
        Some(QueuePosition::Pending) => {
            report.found = true;
            cancelled
                .lock()
                .expect("cancel lock")
                .insert(id.to_string());
            comfy.delete_pending(id).await.map_err(|e| e.to_string())?;
            report.queue_delete_ok = true;
        }
        None => {}
    }
    Ok(report)
}

/// How a followed job ended.
#[derive(Debug, PartialEq)]
enum Terminal {
    Done {
        outputs: Vec<OutputRef>,
    },
    /// Stopped on purpose. Distinct from `Failed`: a row reading "failed" with
    /// an error text would misreport the user's own decision back to them.
    Cancelled,
    Failed {
        error: String,
    },
}

/// Poll until the job is terminal, calling `on_status` with `pending` or
/// `running` on every non-terminal poll.
async fn follow(
    comfy: &ComfyHttp,
    id: &str,
    was_cancelled: impl Fn() -> bool,
    interval: Duration,
    mut on_status: impl FnMut(&str),
    log: Option<&SessionLog>,
) -> Terminal {
    let mut misses = 0u8;
    loop {
        let observed = observe(comfy, id).await;
        match observed {
            Err(e) => {
                if let Some(log) = log {
                    log.log_result("job_follow", false, &format!("{e:?}"));
                }
                return Terminal::Failed {
                    error: transport_reason(&e),
                };
            }
            Ok(Seen::InQueue(position)) => {
                misses = 0;
                on_status(match position {
                    QueuePosition::Running => "running",
                    QueuePosition::Pending => "pending",
                });
            }
            Ok(Seen::Finished(outcome, outputs)) => {
                return match outcome {
                    Outcome::Success => Terminal::Done { outputs },
                    Outcome::Interrupted => Terminal::Cancelled,
                    Outcome::Failed { node_type, message } => Terminal::Failed {
                        error: failure_reason(node_type.as_deref(), message.as_deref()),
                    },
                };
            }
            Ok(Seen::Absent) => {
                if was_cancelled() {
                    return Terminal::Cancelled;
                }
                misses += 1;
                if misses >= LOST_AFTER_MISSES {
                    return Terminal::Failed {
                        error: "ComfyUI no longer has this job -- it may have restarted, or the \
                                queue was cleared. Queue the generation again."
                            .into(),
                    };
                }
            }
        }
        tokio::time::sleep(interval).await;
    }
}

/// One poll's answer.
enum Seen {
    InQueue(QueuePosition),
    Finished(Outcome, Vec<OutputRef>),
    Absent,
}

/// Queue first (cheap, and the common answer), history only when the job has
/// left the queue.
async fn observe(comfy: &ComfyHttp, id: &str) -> Result<Seen, ComfyError> {
    if let Some(position) = comfy.queue().await?.position(id) {
        return Ok(Seen::InQueue(position));
    }
    Ok(match comfy.history(id).await? {
        Some(record) => Seen::Finished(record.outcome(), record.output_files()),
        None => Seen::Absent,
    })
}

/// Follow a job to the end, emit its terminal event, ingest its outputs.
async fn monitor_job(
    app: AppHandle,
    comfy: Arc<ComfyHttp>,
    id: String,
    root: PathBuf,
    pending: Arc<Mutex<HashMap<String, PendingOutput>>>,
    jobs: Arc<Mutex<HashMap<String, tokio::task::AbortHandle>>>,
    cancelled: Arc<Mutex<HashSet<String>>>,
) {
    let log = SessionLog::open(root.join("session.log")).ok();
    let terminal = follow(
        &comfy,
        &id,
        || cancelled.lock().map(|c| c.contains(&id)).unwrap_or(false),
        POLL_INTERVAL,
        |status| {
            let _ = app.emit(
                "job://progress",
                JobProgress {
                    id: id.clone(),
                    status: status.to_string(),
                    outputs: Vec::new(),
                },
            );
        },
        log.as_ref(),
    )
    .await;

    match terminal {
        Terminal::Done { outputs } => {
            let _ = app.emit(
                "job://done",
                JobDone {
                    id: id.clone(),
                    outputs: outputs.iter().map(|o| o.filename.clone()).collect(),
                },
            );
            ingest_if_pending(&app, &comfy, &id, &root, &pending, &outputs).await;
        }
        Terminal::Cancelled => {
            let _ = app.emit("job://cancelled", JobCancelled { id: id.clone() });
        }
        Terminal::Failed { error } => {
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
    if let Ok(mut c) = cancelled.lock() {
        c.remove(&id);
    }
    if let Ok(mut jobs) = jobs.lock() {
        jobs.remove(&id);
    }
}

/// Download the outputs over `/view` and file them as tracks or artwork, when
/// the job has a pending provenance record.
async fn ingest_if_pending(
    app: &AppHandle,
    comfy: &ComfyHttp,
    id: &str,
    root: &Path,
    pending: &Arc<Mutex<HashMap<String, PendingOutput>>>,
    outputs: &[OutputRef],
) {
    let maybe_pending = pending.lock().ok().and_then(|m| m.get(id).cloned());
    let Some(p) = maybe_pending else {
        return;
    };
    let target_dir = match p.kind {
        ModelKind::Music => library::tracks::tracks_dir(root, &p.project_slug),
        ModelKind::Image => library::art::art_dir(root, &p.project_slug),
    };
    let target_dir = match target_dir {
        Ok(d) => d,
        Err(e) => return log_ingest_failure(app, id, root, &e.to_string()),
    };
    let batch = match fetch_outputs(comfy, id, &target_dir, outputs).await {
        Ok(b) => b,
        Err(e) => return log_ingest_failure(app, id, root, &e),
    };
    let now = library::projects::now_rfc3339();
    match ingest_outputs(root, &p, &batch, &now, id) {
        Ok(items) => {
            for saved in items {
                match saved {
                    Saved::Track(track) => {
                        let _ = app.emit(
                            "track://saved",
                            TrackSaved {
                                id: track.id.0.clone(),
                                project_slug: p.project_slug.clone(),
                                file: track.file.clone(),
                            },
                        );
                    }
                    Saved::Art(art) => {
                        let _ = app.emit(
                            "art://saved",
                            ArtSaved {
                                id: art.id.0.clone(),
                                project_slug: p.project_slug.clone(),
                                file: art.file.clone(),
                            },
                        );
                    }
                }
            }
        }
        Err(e) => log_ingest_failure(app, id, root, &e.to_string()),
    }
}

/// Download every output into `dir` under a name unique to this job, so the
/// library's rename into place stays on one volume and two jobs never collide.
async fn fetch_outputs(
    comfy: &ComfyHttp,
    prompt_id: &str,
    dir: &Path,
    outputs: &[OutputRef],
) -> Result<OutputBatch, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let stem: String = prompt_id
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(12)
        .collect();
    let mut files = Vec::new();
    for (n, output) in outputs.iter().enumerate() {
        let bytes = comfy.view(output).await.map_err(|e| e.to_string())?;
        let name = Path::new(&output.filename)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "output".into());
        let path = dir.join(format!(".incoming-{stem}-{n}-{name}"));
        std::fs::write(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
        files.push(OutputFile {
            url: format!("{}/view ({})", comfy.base_url(), output.filename),
            path,
            size: bytes.len() as u64,
        });
    }
    Ok(OutputBatch {
        prompt_id: Some(prompt_id.to_string()),
        out_dir: Some(dir.to_path_buf()),
        files,
    })
}

/// Log an ingest failure and re-emit it as a job failure so the row does not
/// show Done for an asset that was never saved.
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

/// The sentence a failed job's row shows. The node class leads because it says
/// where to look; ComfyUI's traceback is never read (COMFY-HTTP-SURFACE 6).
fn failure_reason(node_type: Option<&str>, message: Option<&str>) -> String {
    match (node_type, message) {
        (Some(node), Some(detail)) => format!("{node} failed: {detail}"),
        (None, Some(detail)) => detail.to_string(),
        (Some(node), None) => format!("{node} failed. Check the ComfyUI console for details."),
        (None, None) => "ComfyUI reported an error. Check the ComfyUI console for details.".into(),
    }
}

/// The sentence a row shows when *following* a job failed, as opposed to the
/// job itself failing.
fn transport_reason(error: &ComfyError) -> String {
    if error.is_unreachable() {
        "Lost the connection to ComfyUI. Start ComfyUI, then queue this generation again."
            .to_string()
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use comfy_bridge::transport::{FakeTransport, Method, TransportError};
    use std::cell::RefCell;

    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/../testdata/comfy-http/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("JSON")
    }

    fn queue_with(running: &[&str], pending: &[&str]) -> Value {
        let item = |id: &&str| serde_json::json!([1, id, {}, {}, []]);
        serde_json::json!({
            "queue_running": running.iter().map(item).collect::<Vec<_>>(),
            "queue_pending": pending.iter().map(item).collect::<Vec<_>>(),
        })
    }

    /// A captured history record re-keyed to `id`.
    fn history_as(name: &str, id: &str) -> Value {
        let value = fixture(name);
        let record = value
            .as_object()
            .expect("obj")
            .values()
            .next()
            .expect("record")
            .clone();
        serde_json::json!({ id: record })
    }

    fn client(fake: &Arc<FakeTransport>) -> ComfyHttp {
        ComfyHttp::with_transport("127.0.0.1:8188", fake.clone(), None)
    }

    async fn run(fake: &Arc<FakeTransport>, cancelled: bool) -> (Terminal, Vec<String>) {
        let seen = RefCell::new(Vec::new());
        let terminal = follow(
            &client(fake),
            "job-1",
            || cancelled,
            Duration::ZERO,
            |s| seen.borrow_mut().push(s.to_string()),
            None,
        )
        .await;
        (terminal, seen.into_inner())
    }

    /// Protects: a job is reported pending, then running, then done with the
    /// outputs history lists -- from the real success record.
    #[tokio::test]
    async fn test_follow_reports_pending_running_then_done() {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(Method::Get, "/queue", &queue_with(&["other"], &["job-1"]));
        fake.on_json(Method::Get, "/queue", &queue_with(&["job-1"], &[]));
        fake.on_json(Method::Get, "/queue", &queue_with(&[], &[]));
        fake.on_json(
            Method::Get,
            "/history/job-1",
            &history_as("history_audio_success.json", "job-1"),
        );
        let (terminal, seen) = run(&fake, false).await;
        assert_eq!(seen, ["pending", "running"]);
        match terminal {
            Terminal::Done { outputs } => assert!(outputs[0].filename.ends_with(".flac")),
            other => panic!("expected Done, got {other:?}"),
        }
    }

    /// Protects: an interrupted job is a cancel, never a failure, even though
    /// ComfyUI records it with `status_str: "error"`.
    #[tokio::test]
    async fn test_follow_reads_an_interrupt_as_cancelled() {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(Method::Get, "/queue", &queue_with(&[], &[]));
        fake.on_json(
            Method::Get,
            "/history/job-1",
            &history_as("history_interrupted.json", "job-1"),
        );
        assert_eq!(run(&fake, false).await.0, Terminal::Cancelled);
    }

    /// Protects: a deleted pending job never reaches history; the remembered
    /// cancel is what makes it read as cancelled rather than lost.
    #[tokio::test]
    async fn test_follow_reads_a_vanished_cancelled_job_as_cancelled() {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(Method::Get, "/queue", &queue_with(&[], &[]));
        fake.on(Method::Get, "/history/job-1", 200, "{}");
        assert_eq!(run(&fake, true).await.0, Terminal::Cancelled);
    }

    /// Protects: a job that vanishes without a cancel is reported lost with a
    /// next step, after exactly three misses -- not on the first.
    #[tokio::test]
    async fn test_follow_reports_a_vanished_job_lost_after_three_misses() {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(Method::Get, "/queue", &queue_with(&[], &[]));
        fake.on(Method::Get, "/history/job-1", 200, "{}");
        match run(&fake, false).await.0 {
            Terminal::Failed { error } => {
                assert!(error.contains("Queue the generation again"), "{error}")
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        let history_polls = fake
            .call_lines()
            .iter()
            .filter(|l| l.contains("/history"))
            .count();
        assert_eq!(history_polls, 3);
    }

    #[tokio::test]
    async fn test_follow_names_the_failing_node() {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(Method::Get, "/queue", &queue_with(&[], &[]));
        fake.on_json(
            Method::Get,
            "/history/job-1",
            &history_as("history_node_error.json", "job-1"),
        );
        match run(&fake, false).await.0 {
            Terminal::Failed { error } => {
                assert!(
                    error.starts_with("LoraLoaderModelOnly failed: The safetensors header"),
                    "{error}"
                );
                assert!(!error.contains("Traceback"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Protects: ComfyUI dying mid-job gives one sentence with a next step.
    #[tokio::test]
    async fn test_follow_turns_a_dead_server_into_a_next_step() {
        let fake = Arc::new(FakeTransport::new());
        fake.fail(
            Method::Get,
            "/queue",
            TransportError::Connect("refused".into()),
        );
        match run(&fake, false).await.0 {
            Terminal::Failed { error } => assert_eq!(
                error,
                "Lost the connection to ComfyUI. Start ComfyUI, then queue this generation again."
            ),
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    /// Protects: cancel interrupts a running job and deletes a pending one, and
    /// remembers both -- the memory is what `follow` reads.
    #[tokio::test]
    async fn test_cancel_routes_by_queue_position_and_remembers() {
        let fake = Arc::new(FakeTransport::new());
        fake.on_json(Method::Get, "/queue", &queue_with(&["run-1"], &["wait-2"]));
        fake.on(Method::Post, "/interrupt", 200, "");
        fake.on(Method::Post, "/queue", 200, "");
        let comfy = client(&fake);
        let memory = Mutex::new(HashSet::new());

        let running = cancel_on(&comfy, &memory, "run-1").await.expect("ok");
        assert_eq!(
            running,
            JobCancel {
                found: true,
                queue_delete_ok: false,
                interrupt_ok: true
            }
        );
        let waiting = cancel_on(&comfy, &memory, "wait-2").await.expect("ok");
        assert_eq!(
            waiting,
            JobCancel {
                found: true,
                queue_delete_ok: true,
                interrupt_ok: false
            }
        );
        let gone = cancel_on(&comfy, &memory, "gone").await.expect("ok");
        assert!(!gone.found);

        let memory = memory.into_inner().expect("lock");
        assert!(memory.contains("run-1") && memory.contains("wait-2") && !memory.contains("gone"));
        assert!(fake.call_lines().contains(&"POST /interrupt".to_string()));
    }

    /// Protects: outputs are downloaded over `/view` into the target directory
    /// under job-unique names, ready for the library's rename.
    #[tokio::test]
    async fn test_fetch_outputs_downloads_each_file_over_view() {
        let dir = tempfile::tempdir().expect("tempdir");
        let fake = Arc::new(FakeTransport::new());
        fake.on(
            Method::Get,
            "/view?filename=ace_00001.flac&subfolder=latentcreate_probe&type=output",
            200,
            "fLaC-bytes",
        );
        let output = OutputRef {
            node_id: "111".into(),
            kind: "audio".into(),
            filename: "ace_00001.flac".into(),
            subfolder: "latentcreate_probe".into(),
            file_type: "output".into(),
        };
        let batch = fetch_outputs(&client(&fake), "abc-123", dir.path(), &[output])
            .await
            .expect("fetched");
        assert_eq!(batch.files.len(), 1);
        let file = &batch.files[0];
        assert!(
            file.path.ends_with(".incoming-abc123-0-ace_00001.flac"),
            "{:?}",
            file.path
        );
        assert_eq!(std::fs::read(&file.path).expect("read"), b"fLaC-bytes");
        assert_eq!(file.size, 10);
    }

    #[test]
    fn test_failure_reason_always_ends_in_something_to_do() {
        assert_eq!(
            failure_reason(Some("VAEDecode"), Some("out of memory")),
            "VAEDecode failed: out of memory"
        );
        assert!(failure_reason(None, None).contains("ComfyUI console"));
    }
}
