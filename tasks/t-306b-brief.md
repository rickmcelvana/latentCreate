# T-306b: the pipeline command -- fetch, write, edit, validate, submit

**Depends:** T-306a | **Crate/dir:** `src-tauri`, plus a dev-only feature flag on `mcp-bridge`
**Files to modify:**
- `src-tauri/src/generate.rs` *(new -- the whole task)*
- `src-tauri/src/jobs.rs` -- **one method extracted**, see §3
- `src-tauri/src/lib.rs` -- two lines: `mod generate;` and the handler entry
- `src-tauri/Cargo.toml` -- one dev-dependency
- `crates/mcp-bridge/Cargo.toml` -- a `test-support` feature
- `crates/mcp-bridge/src/lib.rs` -- two `cfg` lines
- `crates/mcp-bridge/src/local.rs` -- **one line**, the visibility of `test_helpers`

This closes T-306 and makes the app able to generate audio for the first time. Everything it
calls already exists and is tested; **this task is the sequencing, and the sequencing is the
part nothing has ever run.**

## The finding that changes the design: `local_check` must NOT gate the run

The phase file says *"`local_check` gated before running"*. **That is wrong, and building it
would ship a bug** -- MiniMax Music 3 could never generate.

`fetch_template` reports `local_check` for the template **as fetched**, before the profile's
`slot_overrides` are applied. MiniMax's template hardcodes the fp16 DiT; the int8 is what is on
disk; the profile's `37/6.unet_name` override already corrects it. With all three model files
installed and the workflow perfectly runnable, the fetch says:

```json
{ "checked": true, "runnable": false, "error_count": 1,
  "errors": ["node 37:6: 'minimax_music3_dit_fp16.safetensors' not in 2 known options for unet_name"] }
```

Verified 2026-08-25, MCP-SURFACE 14.4. This is the same trap the models step already refuses to
fall into -- `models.rs` opens with a paragraph on why it never reads `local_check` -- and the
pipeline inherits it for a second reason on top: **the fetch happens before the app has fixed
anything.**

**So `local_check` is not read at all, and the module doc says why.** What replaces it is
`validate_workflow` at step 5, which runs on the edited copy -- after the overrides, after the
slot writes, after the graph edits -- and is therefore looking at what will actually be
submitted. The phase file's line is corrected as part of this task.

## The second decision: the call sequence gets an offline test

Every `src-tauri` command before this one made **one** MCP call (`comfy_status` -> `health`,
`models_status` -> `search_models`). This one makes **four**, in order, against one file, where
each step depends on the last having written it. Everything that can go wrong here is an
ordering or a routing mistake:

- the graph edits applied before `set_slots`, so a slot write lands on a retyped node;
- `validate` or `run` handed a different path from the one that was edited;
- the audit run *after* the write it exists to prevent;
- the edited graph never written back, so ComfyUI renders the template.

**None of those are visible to a test over pure functions**, and three tasks running, this
repo's finding has been that the convenient signal says yes. `mcp-bridge`'s mock transport
already records every `tools/call` it receives, for exactly this reason -- it "lets a test
assert what the bridge **sent**" -- but it is `#[cfg(test)]` and invisible outside its crate.

This task exposes it behind a `test-support` feature and takes it as a **dev**-dependency. Four
lines of `cfg` plus a feature stanza (§2), and it buys the only offline proof this
pipeline can have. **Cargo does not build dev-dependencies for `cargo build`**, so nothing
reaches a shipped binary -- verified: `cargo build -p app --lib` recompiles `mcp-bridge`
*without* the feature.

## Spec

### 1. What the pipeline does, in order

`build_and_submit(comfy, workflow_path, profile, spec)`:

1. **`fetch_template`** to this job's own path. Never a shared file (TOCTOU, and two
   generations at once would edit each other's graph). `local_check` is **not** read.
2. **`resolve_slots`**, then **`audit_slots` over the resolved addresses**, refusing the run if
   any is inert. This happens **before** the first write, so a profile bug costs nothing and
   reports the address rather than a mystery. `unchecked` addresses are **not** a refusal; they
   are carried out in the result. MiniMax's seed is `unchecked`, and blocking on it would block
   the model (18.5).
3. **`set_slots`** with every resolved address in one batch. A bad address fails the whole batch
   and writes nothing, so there is no partial-write recovery to write.
4. **The graph edits** -- `splice_loras`, then `ensure_lossless_output` -- on the file
   `set_slots` just wrote, and **written back to that same path**.
5. **`validate_workflow`**, with `Verdict::Vacuous` treated as failure.
6. **`run`**, then hand the prompt id to the existing pump.

⚠ **Slot writes first, graph edits second, and never a slot write after an edit.** The
lossless swap retypes the save node, and this app has not re-read the slot list a new node class
produces. `ensure_lossless_output` preserves `filename_prefix`, so a prefix written in step 3
survives step 4 -- the order works in exactly one direction.

On step 5, be precise in the code about what it buys. Measured: it catches `unknown_enum_value`,
`above_max` and `required_input_missing` before any GPU time (18.3). It is blind to reachability
-- a LoRA chain feeding nothing passes (17.1) -- and blind to the save format's dynamic combo
(16.1). It proves nothing about step 4. `run_workflow` pre-validates too (10.1), so this step is
partly redundant for the run path; it earns its place by returning **structured findings** with
node ids, where a rejected run returns one string.

### 2. The `test-support` feature

`crates/mcp-bridge/Cargo.toml`, above `[dependencies]`:

```toml
[features]
# The in-memory fake peer, for offline tests in this crate and in the crates
# that drive it. A dev-dependency only: `cargo build` never sees it, so no
# mock reaches a shipped binary.
test-support = []
```

`crates/mcp-bridge/src/lib.rs` -- replace the two lines `#[cfg(test)]` / `mod mock;` with:

```rust
/// The fake MCP peer, compiled for this crate's own tests and for downstream
/// crates that enable `test-support` as a **dev**-dependency (T-306b: the
/// pipeline's call sequence is asserted offline). Never in a release build.
#[cfg(any(test, feature = "test-support"))]
pub mod mock;

#[cfg(any(test, feature = "test-support"))]
pub use local::test_helpers;
```

`crates/mcp-bridge/src/local.rs` -- the one line above `mod test_helpers`, which is currently
`#[cfg(test)]` / `pub(crate) mod test_helpers {`:

```rust
#[cfg(any(test, feature = "test-support"))]
pub mod test_helpers {
```

**Nothing inside it changes**, and `local.rs`'s own `transport_tests` keep working.

`src-tauri/Cargo.toml`, in `[dev-dependencies]`:

```toml
# The fake MCP peer. Dev-only: the same path dependency as above, with the test
# transport switched on, so the pipeline's call sequence can be asserted with
# no ComfyUI running.
mcp-bridge = { path = "../crates/mcp-bridge", features = ["test-support"] }
```

### 3. `jobs.rs`: one method extracted, no behaviour changed

The pump must not be duplicated. `run_workflow` currently spawns `monitor_job` and records its
abort handle inline; that block becomes a method on `ComfyState`, placed on the existing
`impl ComfyState` under `store`:

```rust
    /// Start the lifecycle pump for a job ComfyUI has already accepted.
    ///
    /// The one way a submitted job becomes `job://` events. Every submitter
    /// calls this rather than spawning its own monitor -- a second lifecycle
    /// would emit a second set of events for the same prompt id, and
    /// [`cancel_job`] would only know about one of them.
    pub(crate) fn pump(&self, app: AppHandle, comfy: Arc<LocalComfy>, id: String) {
        let jobs = Arc::clone(&self.jobs);
        let handle = async_runtime::spawn(monitor_job(app, comfy, id.clone(), jobs));
        self.jobs
            .lock()
            .expect("jobs lock poisoned")
            .insert(id, handle.inner().abort_handle());
    }
```

Then `run_workflow`'s tail becomes:

```rust
    let id = run.prompt_id.clone();
    state.pump(app, comfy, id.clone());
    Ok(id)
```

replacing the four statements that spawned and inserted by hand. **`jobs.rs` changes in no other
way** -- its four tests must pass untouched.

### 4. `lib.rs`: two lines

`mod generate;` after `mod comfy;`, and `generate::generate_audio,` in the `invoke_handler!`
list above `jobs::connect_comfy`.

### 5. The working copy lives under the app data dir

`<app config dir>/jobs/<epoch-millis>-<counter>/workflow.json`. The counter is what makes it
unique: two jobs queued in the same millisecond are ordinary (batch-by-seeds is T-312), and a
collision would have one run editing the other's graph.

**The file is kept, not cleaned up.** It is the record of what was actually submitted, which is
what T-311's provenance and any later "why did that track sound like that" needs. A job
directory is a few hundred KB.

### 6. What this command does NOT resolve: the lyric text

`GenerationSpec` carries both `inputs["lyrics"]` (the text, which reaches `94.lyrics`) and
`lyrics: Option<LyricRef>` (doc id + version). **This command submits the text it is given and
never opens the lyric document.** Resolving the ref would need a project slug the spec does not
carry, and the Library view that owns that question is Phase 4.

The honesty gap this leaves -- a caller could send version 3's ref with version 2's text --
closes at T-311, where ARCHITECTURE 8 already requires the sidecar to record **the resolved slot
values actually submitted**, not the UI's idea of them. Note it there; do not build it here.

## Reference implementation

⚠ **Written, compiled, run and mutation-checked before this brief was published.** The six
tests below pass, `cargo fmt --all --check` and `cargo clippy --all-targets -- -D warnings` are
clean, and `cargo test --workspace` is green with `app` at 51 tests (45 + these 6). All five
mutations listed further down were run against this code and every one turned the suite red.

### `src-tauri/src/generate.rs`

```rust
//! The generation pipeline: a `GenerationSpec` becomes a queued ComfyUI job.
//!
//! ARCHITECTURE 7, in order: `fetch_template` to a **per-job** working copy,
//! `set_slots` for everything addressable, the graph edits slots cannot
//! express, `validate_workflow`, then submit into the job pump T-104b already
//! runs. Nothing here duplicates that lifecycle.
//!
//! Three things this module deliberately does NOT do, each of them a signal
//! that looks like it should be trusted and is not:
//!
//! - **`local_check` is not a gate.** It is evaluated when the template is
//!   fetched, before the profile's `slot_overrides` are applied, and MiniMax
//!   Music 3 is `runnable: false` for exactly the filename its own overrides
//!   correct (MCP-SURFACE 14.4). Gating on it would refuse to generate with a
//!   fully installed model.
//! - **A clean validation is not evidence an edit took effect.** A LoRA chain
//!   spliced in but feeding nothing validates clean, runs, and writes audio
//!   with no LoRA applied (17.1). Reachability is asserted in `create-core`'s
//!   own tests, before anything is submitted.
//! - **`applied` is not `effective`.** `set_workflow_slot` accepts a write to
//!   an input a real backend node drives, and the engine ignores it (18.1), so
//!   the resolved addresses are audited before the first write, not after.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use create_core::audit::{audit_slots, SlotAudit};
use create_core::generation::{GenerationSpec, ResolvedSlots};
use create_core::graph::{ensure_lossless_output, splice_loras, GraphError, LoraChoice};
use create_core::profile::ModelProfile;
use mcp_bridge::{Finding, LocalComfy, SlotOverride, Validation, Verdict};
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, State};

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::{ConfigDir, ProfilesDir};

/// What was queued, and what the app could not check while queueing it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Submission {
    /// The handle every later job call keys on.
    pub prompt_id: String,
    /// The working copy actually submitted -- the record of what ran.
    pub workflow_path: String,
    /// Resolved addresses the audit could not resolve: subgraph interiors, and
    /// addresses naming a node the top-level graph does not have. Reported
    /// rather than swallowed; MiniMax's seed is in here, unverified rather
    /// than working (MCP-SURFACE 18.5).
    pub unchecked_slots: Vec<String>,
    /// Ids of the LoRA loader nodes spliced in, in apply order.
    pub lora_nodes: Vec<String>,
    /// The save format the graph edit wrote, `None` when the profile opted out
    /// of the lossless swap.
    pub output_format: Option<String>,
}

/// What the graph edits changed, for the submission record.
#[derive(Debug, Clone, PartialEq)]
struct GraphEdits {
    lora_nodes: Vec<String>,
    output_format: Option<String>,
}

/// Queue one generation and start its pump. Returns as soon as ComfyUI has the
/// job; progress arrives on the `job://` events T-104b already emits.
#[tauri::command]
pub async fn generate_audio(
    app: AppHandle,
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    profiles_dir: State<'_, ProfilesDir>,
    spec: GenerationSpec,
) -> Result<Submission, String> {
    let set = library::profiles::load(&profiles_dir.0, &config_dir.0.join("profiles"));
    let profile = set
        .profiles
        .get(&spec.profile_id)
        .map(|loaded| loaded.profile.clone())
        .ok_or_else(|| format!("no profile named {}", spec.profile_id))?;

    let comfy = match ensure_connected(&state, &config_dir, None).await {
        Ok(comfy) => comfy,
        Err(EnsureError::Comfy(e)) => return Err(e.to_string()),
        Err(EnsureError::Log(detail)) => return Err(detail),
    };

    let workflow = workflow_path(&config_dir.0, &mint_job_id());
    if let Some(dir) = workflow.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    let submission = build_and_submit(&comfy, &workflow, &profile, &spec).await?;
    state.pump(app, comfy, submission.prompt_id.clone());
    Ok(submission)
}

/// Build this job's working copy and submit it.
///
/// Takes the workflow path rather than minting one so a test can place a real
/// captured template where `fetch_template` would have written it: a mock
/// transport reproduces comfy-mcp's replies, not its side effects.
pub(crate) async fn build_and_submit(
    comfy: &LocalComfy,
    workflow: &Path,
    profile: &ModelProfile,
    spec: &GenerationSpec,
) -> Result<Submission, String> {
    let template = profile.comfy.template.as_deref().ok_or_else(|| {
        format!(
            "{} declares no gallery template; imported workflows are not wired up yet",
            profile.id
        )
    })?;

    // 1. This job's own copy. Never a shared path: the MCP docs warn about
    //    TOCTOU, and two generations at once would edit each other's graph.
    comfy
        .fetch_template(template, workflow)
        .await
        .map_err(|e| e.to_string())?;

    // 2. Resolve, then refuse a write the engine would ignore -- before the
    //    write, so a profile bug costs nothing.
    let resolved = profile.resolve_slots(spec).map_err(|e| e.to_string())?;
    let addresses: Vec<String> = resolved.keys().map(|a| a.0.clone()).collect();
    let audit = audit_slots(&read_workflow(workflow)?, &addresses);
    let inert = inert_slots(&audit);
    if !inert.is_empty() {
        return Err(format!(
            "{} writes {} to inputs a node drives, so the engine would ignore them",
            profile.id,
            inert.join(", ")
        ));
    }

    // 3. Slot writes first, graph edits second, and never a slot write after an
    //    edit: the swap retypes the save node, and this app has not re-read the
    //    slot list that a new node class produces.
    let overrides = slot_overrides(&resolved);
    if !overrides.is_empty() {
        comfy
            .set_slots(workflow, &overrides)
            .await
            .map_err(|e| e.to_string())?;
    }

    // 4. The two edits slots cannot express, on the file set_slots just wrote.
    let mut graph = read_workflow(workflow)?;
    let edits = apply_graph_edits(&mut graph, profile, spec).map_err(|e| e.to_string())?;
    write_workflow(workflow, &graph)?;

    // 5. Cheap, and worth being precise about: this catches unknown enum
    //    values, out-of-range numbers and missing required inputs before any
    //    GPU time. It is blind to reachability (17.1) and to the save format's
    //    dynamic combo (16.1), so it proves nothing about step 4.
    let report = comfy.validate(workflow).await.map_err(|e| e.to_string())?;
    if let Some(reason) = validation_error(&report) {
        return Err(reason);
    }

    let run = comfy.run(workflow).await.map_err(|e| e.to_string())?;
    Ok(Submission {
        prompt_id: run.prompt_id,
        workflow_path: workflow.display().to_string(),
        unchecked_slots: audit.unchecked,
        lora_nodes: edits.lora_nodes,
        output_format: edits.output_format,
    })
}

/// The slot writes for one resolved spec, in the shape `set_slots` wants.
///
/// **`to_slot_value`, never `serde_json::to_value`.** `InputValue` is
/// adjacently tagged, so serialising it whole sends `{"type":"seed","value":42}`
/// where the slot wants `42` -- rejected with `[workflow_slot_invalid]` for
/// every generation (MCP-SURFACE 18.2).
fn slot_overrides(resolved: &ResolvedSlots) -> Vec<SlotOverride> {
    resolved
        .iter()
        .map(|(address, value)| SlotOverride::new(address.0.clone(), value.to_slot_value()))
        .collect()
}

/// Addresses whose write would be accepted, persisted, and then ignored.
fn inert_slots(audit: &SlotAudit) -> Vec<String> {
    audit
        .link_fed
        .iter()
        .filter(|fed| fed.is_inert())
        .map(|fed| fed.address.clone())
        .collect()
}

/// Why this validation must stop the run, or `None` to go ahead.
///
/// `Vacuous` is a failure: a report that examined no nodes is not a pass, and
/// treating it as one greenlights a graph nothing looked at (9.3).
fn validation_error(report: &Validation) -> Option<String> {
    match report.verdict() {
        Verdict::Valid => None,
        Verdict::Invalid => Some(format!(
            "ComfyUI rejected the workflow: {}",
            summarise(&report.errors)
        )),
        Verdict::Vacuous => {
            Some("validation examined no nodes, so it proves nothing about this workflow".into())
        }
    }
}

/// One line per finding, node id included so the user can be pointed at it.
fn summarise(findings: &[Finding]) -> String {
    findings
        .iter()
        .map(|f| match (&f.node_id, &f.message) {
            (Some(id), Some(msg)) => format!("node {id}: {msg}"),
            (None, Some(msg)) => msg.clone(),
            (Some(id), None) => format!("node {id}"),
            (None, None) => "unspecified error".to_string(),
        })
        .collect::<Vec<_>>()
        .join("; ")
}

/// Apply the edits no slot can express: the LoRA stack, then the save format.
fn apply_graph_edits(
    graph: &mut Value,
    profile: &ModelProfile,
    spec: &GenerationSpec,
) -> Result<GraphEdits, GraphError> {
    let stack: Vec<LoraChoice> = spec
        .active_loras()
        .map(|lora| LoraChoice {
            name: lora.file.clone(),
            strength: lora.strength,
        })
        .collect();

    let lora_nodes = match (&profile.loras, stack.is_empty()) {
        (_, true) => Vec::new(),
        (Some(support), false) => splice_loras(graph, support, &stack)?.nodes,
        (None, false) => {
            return Err(GraphError::Malformed {
                detail: format!("{} declares no LoRA support", profile.id),
            })
        }
    };

    let save = ensure_lossless_output(graph, &profile.comfy.output)?;
    Ok(GraphEdits {
        lora_nodes,
        output_format: save.format,
    })
}

/// Where one job's working copy lives.
fn workflow_path(root: &Path, job_id: &str) -> PathBuf {
    root.join("jobs").join(job_id).join("workflow.json")
}

/// A per-job directory name: epoch millis plus a counter.
///
/// The counter is what makes it unique -- two jobs queued in the same
/// millisecond are ordinary, and a collision would have one run editing the
/// other's graph.
fn mint_job_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or_default();
    format!("{millis:013}-{:04}", SEQ.fetch_add(1, Ordering::Relaxed))
}

/// Read the working copy back after a tool wrote it.
fn read_workflow(path: &Path) -> Result<Value, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

/// Write the edited graph back. A plain write, not an atomic swap: this file is
/// this job's alone and nothing else reads it until it is submitted.
fn write_workflow(path: &Path, graph: &Value) -> Result<(), String> {
    let text = serde_json::to_string(graph).map_err(|e| e.to_string())?;
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}
```

### The tests, same file

`tempfile` and `tokio` (`macros`, `rt`) are already `[dev-dependencies]` of this crate.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use create_core::generation::{InputValue, LoraRef};
    use create_core::profile::SlotAddress;
    use mcp_bridge::mock::Reply;
    use mcp_bridge::test_helpers::client_and_log;
    use serde_json::json;
    use std::collections::BTreeMap;

    const ACE: &str = include_str!("../../profiles/ace-step-1.5-turbo.json");

    fn ace() -> ModelProfile {
        serde_json::from_str(ACE).expect("profile decodes")
    }

    /// The real captured template, as `fetch_template` would have written it.
    fn fixture() -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/workflows/ace_step_1_5_xl_turbo.json");
        std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
    }

    fn spec() -> GenerationSpec {
        let mut inputs = BTreeMap::new();
        inputs.insert("tags".to_string(), InputValue::Text("synthwave".into()));
        inputs.insert("seed".to_string(), InputValue::Seed(42));
        inputs.insert("duration_s".to_string(), InputValue::Float(30.0));
        GenerationSpec {
            profile_id: "ace-step-1.5-turbo".to_string(),
            inputs,
            loras: Vec::new(),
            lyrics: None,
        }
    }

    /// The four replies a clean run consumes, in call order.
    fn happy_replies() -> Vec<Reply> {
        vec![
            Reply::Json(
                json!({ "path": "ignored", "local_check": { "checked": true, "runnable": true } }),
            ),
            Reply::Json(json!({ "applied": [], "warnings": [], "wrote": "ignored" })),
            Reply::Json(
                json!({ "valid": true, "errors": [], "warnings": [], "converted_from_ui": true, "converted_node_count": 30 }),
            ),
            Reply::Json(json!({ "prompt_id": "abc-123", "status": "queued", "outputs": [] })),
        ]
    }

    /// `set_slots` refuses a write whose address is not echoed back, so the
    /// canned reply has to applaud exactly what was sent.
    fn applied(overrides: &[SlotOverride]) -> Vec<String> {
        overrides.iter().map(|o| o.address.clone()).collect()
    }

    #[test]
    fn test_slot_overrides_send_bare_values_not_the_tag() {
        let mut resolved: ResolvedSlots = BTreeMap::new();
        resolved.insert(SlotAddress("109.value".into()), InputValue::Seed(u64::MAX));
        let sent = slot_overrides(&resolved);
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].value, json!(u64::MAX));
        assert!(sent[0].value.get("type").is_none());
    }

    #[test]
    fn test_vacuous_validation_is_a_failure() {
        let vacuous: Validation = serde_json::from_value(json!({
            "valid": true,
            "errors": [],
            "warnings": [{ "code": "non_node_key" }]
        }))
        .expect("decodes");
        assert!(validation_error(&vacuous).is_some());
    }

    #[test]
    fn test_job_ids_do_not_collide_within_a_millisecond() {
        assert_ne!(mint_job_id(), mint_job_id());
    }

    #[tokio::test]
    async fn test_pipeline_calls_the_tools_in_order_on_one_working_copy() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = dir.path().join("workflow.json");
        std::fs::write(&workflow, fixture()).expect("place the fixture");

        let profile = ace();
        let spec = spec();
        let overrides = slot_overrides(&profile.resolve_slots(&spec).expect("resolves"));
        let mut replies = happy_replies();
        replies[1] = Reply::Json(json!({
            "applied": applied(&overrides),
            "warnings": [],
            "wrote": workflow.display().to_string()
        }));

        let (comfy, calls) = client_and_log(replies).await;
        let submission = build_and_submit(&comfy, &workflow, &profile, &spec)
            .await
            .expect("pipeline runs");

        assert_eq!(submission.prompt_id, "abc-123");
        assert_eq!(submission.output_format.as_deref(), Some("flac"));

        // The edit has to be on disk, not only in the return value: what is
        // submitted is the file, and a pipeline that skipped the write-back
        // would report `flac` while ComfyUI rendered the template's MP3.
        let submitted: Value = serde_json::from_str(
            &std::fs::read_to_string(&workflow).expect("the working copy survives"),
        )
        .expect("submitted workflow decodes");
        let save = submitted
            .get("nodes")
            .and_then(|n| n.as_array())
            .expect("nodes")
            .iter()
            .find(|n| n.get("id").and_then(|i| i.as_i64()) == Some(107))
            .expect("the save node");
        assert_eq!(
            save.get("type").and_then(|t| t.as_str()),
            Some("SaveAudioAdvanced")
        );
        assert_eq!(
            save.pointer("/widgets_values/1").and_then(|f| f.as_str()),
            Some("flac")
        );

        let calls = calls.lock().expect("calls lock");
        let names: Vec<&str> = calls
            .iter()
            .map(|c| c.get("name").and_then(|n| n.as_str()).unwrap_or(""))
            .collect();
        assert_eq!(
            names,
            vec![
                "fetch_template",
                "set_workflow_slot",
                "validate_workflow",
                "run_workflow"
            ]
        );

        let want = workflow.display().to_string();
        for call in calls.iter() {
            let args = call.get("arguments").expect("arguments");
            let path = args
                .get("workflow_path")
                .or_else(|| args.get("out_path"))
                .and_then(|p| p.as_str())
                .expect("every call names the working copy");
            assert_eq!(path, want);
        }

        let seed = calls[1]
            .pointer("/arguments/overrides")
            .and_then(|o| o.as_array())
            .expect("overrides")
            .iter()
            .find(|o| o.get("address").and_then(|a| a.as_str()) == Some("109.value"))
            .expect("the seed is written");
        assert_eq!(seed.get("value"), Some(&json!(42)));
    }

    #[tokio::test]
    async fn test_an_inert_slot_stops_the_run_before_anything_is_written() {
        let dir = tempfile::tempdir().expect("tempdir");
        let workflow = dir.path().join("workflow.json");
        std::fs::write(&workflow, fixture()).expect("place the fixture");

        // The profile as it read before T-306a: a seed written to two inputs
        // that `PrimitiveInt` 109 drives.
        let mut profile = ace();
        profile.inputs.insert(
            "seed".to_string(),
            serde_json::from_value(json!({ "type": "seed", "slots": ["94.seed", "3.seed"] }))
                .expect("input decodes"),
        );

        let (comfy, calls) = client_and_log(happy_replies()).await;
        let error = build_and_submit(&comfy, &workflow, &profile, &spec())
            .await
            .expect_err("an inert write must not be submitted");

        assert!(error.contains("94.seed"), "{error}");
        let calls = calls.lock().expect("calls lock");
        assert_eq!(calls.len(), 1, "nothing past fetch_template may be called");
    }

    #[test]
    fn test_a_bypassed_lora_is_not_spliced() {
        let mut graph: Value = serde_json::from_str(&fixture()).expect("fixture decodes");
        let profile = ace();
        let mut spec = spec();
        spec.loras = vec![LoraRef {
            file: "whatever.safetensors".into(),
            strength: 1.0,
            enabled: false,
        }];
        let edits = apply_graph_edits(&mut graph, &profile, &spec).expect("edits apply");
        assert!(edits.lora_nodes.is_empty());
    }
}
```

## Acceptance criteria

- [ ] `generate_audio` is registered and reachable; `npm run gate` clean.
- [ ] The pipeline calls **exactly** `fetch_template`, `set_workflow_slot`, `validate_workflow`,
      `run_workflow`, **in that order**, and every one of them names the **same** working-copy
      path. Asserted from the mock's recorded calls, not by reading the code.
- [ ] ⚠ The seed reaches the wire as the bare integer `42`, read out of the recorded
      `set_workflow_slot` arguments. `to_slot_value`, never `serde_json::to_value` (18.2).
- [ ] ⚠ **The graph edit is asserted on disk**, by re-reading the submitted file: node 107
      is `SaveAudioAdvanced` and `widgets_values[1]` is `"flac"`. Asserting only the returned
      `output_format` passes a pipeline that never wrote the file back -- the vacuity hole that
      has now appeared in four consecutive tasks.
- [ ] ⚠ **An inert slot stops the run before anything is written.** Use the ACE-Step
      profile with its pre-T-306a `seed` slots (`94.seed`, `3.seed`) put back in memory; assert
      the error names the address **and** that exactly one call was made. One, not zero:
      `fetch_template` has to happen first, because the audit needs the graph.
- [ ] `Verdict::Vacuous` is a failure. Build the report from the documented signature --
      `valid: true`, a `non_node_key` warning, no `converted_from_ui` (9.3).
- [ ] A bypassed LoRA (`enabled: false`) splices nothing.
- [ ] Two job ids minted back to back differ.
- [ ] `local_check` is **not read anywhere**, and the module doc says why in terms of MiniMax.
- [ ] `jobs.rs`'s existing four tests pass with no edit to them.
- [ ] `cargo build -p app --lib` succeeds, proving the shipped path does not need
      `test-support`.

## Mutation check before you call it done

Each of these was run against the reference implementation and each turned the suite red.

1. `slot_overrides` uses `serde_json::to_value(value)` instead of `to_slot_value`.
   *(Fails two tests -- the unit test and the recorded-wire assertion.)*
2. Drop the `write_workflow` call after the graph edits. *(Only the on-disk assertion catches
   this. If it does not fail, that assertion is missing or wrong.)*
3. `validation_error` returns `None` for `Verdict::Vacuous`.
4. `inert_slots` filters on `!fed.is_inert()`.
5. Move the audit block **below** the `set_slots` call. *(The inert test's call-count assertion
   is the only thing that catches it -- and it is the difference between refusing a bad run and
   performing it.)*

## Out of scope

- **No UI.** No param panel, no LoRA panel, no Generate button, no frontend bridge or store
  change. T-308 and T-309 build the caller; until then this command is unreached from the
  frontend, exactly as `models_install` was.
- **No output ingestion, no provenance sidecar, no library write** -- T-311. This command
  returns when the job is queued.
- **No batch.** One spec, one job. N seeds is T-312.
- **No custom-workflow path.** A profile with `comfy.workflow` and no `template` gets a clear
  error; wiring it is T-313.
- Do not touch `create-core`. `resolve_slots`, `audit_slots`, `splice_loras` and
  `ensure_lossless_output` are landed and tested, and are used here exactly as they are.
- Do not add a retry, a cleanup sweep of old job directories, or a second validation pass.
- Do not read `local_check`. Not as a gate, not as a warning.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --edit-format diff --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read tasks/t-306b-brief.md --read src-tauri/src/models.rs --file src-tauri/src/generate.rs --file src-tauri/src/jobs.rs --file src-tauri/src/lib.rs --file src-tauri/Cargo.toml --file crates/mcp-bridge/Cargo.toml --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/local.rs
```

**Working set: about 36 KB across seven files**, four of them a handful of lines each. That is
under the ~60 KB the successful T-306a run carried. `models.rs` is `--read` because its opening
paragraph is the precedent for the `local_check` decision and shows the house style for stating
a refusal in module docs. **`create-core` is not passed at all** -- every signature this task
needs is in the reference code above, and `graph.rs` alone would add 49 KB that buys nothing.
