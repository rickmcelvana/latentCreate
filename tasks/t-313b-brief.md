# T-313b — import and inspect

**Lane: architect-direct.** Two new modules whose difficulty is entirely judgment — what to refuse,
what to store, and in what order — rather than transcription. WORKFLOW section 1.

**Depends:** T-313a (landed). **Crate/dir:** `create-core`, `src-tauri`.

**Files to modify:**

- `crates/create-core/src/workflow.rs` — **new**: `WorkflowFormat`, `detect_format`, tests
- `crates/create-core/src/lib.rs` — `pub mod workflow;`
- `src-tauri/src/import.rs` — **new**: the `import_workflow` command and its tests
- `src-tauri/src/lib.rs` — declare the module, register the command
- `src-tauri/src/generate.rs` — `ensure_frontend_format` delegates to `detect_format`

## Why

T-313a made an imported workflow *runnable*; nothing can yet *import* one. Today the only way in is
hand-editing a JSON profile, which is the shape of the feature, not the feature.

This task is the front door: take a path, decide whether it is something this app can drive, keep a
copy, and hand back everything the mapping screen will need.

## The owner decision this task implements

**An imported workflow is copied into app storage, not referenced in place** (decisions log,
2026-08-30). Import takes a snapshot and the profile owns it.

The reason is provenance, not tidiness. A sidecar records the *inputs*; reproducing a track means
the graph those inputs were resolved against must still be the same graph. A profile pointing at a
live file in the user's ComfyUI folder would silently change behaviour when they edit it there, and
every sidecar written before the edit would quietly become a lie.

Two consequences that shape the code:

1. **The stored copy is the artifact of record**, so validation must describe the bytes that were
   **stored**, not the bytes that were picked. Validate the copy, not the source.
2. Editing the graph in ComfyUI does **not** flow through. Re-importing is how you pick up changes.
   That is a real cost and the UI (T-313e) will have to say so; it is not this task's job to hide it.

## What the scoping already settled (MCP-SURFACE 29)

- **Frontend format only** (`File > Save (As)`). `list_workflow_slots` refuses an API export.
- **`validate_workflow` accepts both formats** and calls an API export `valid: true`, so it cannot
  be the format check. The format is decided locally, first.
- **Gate on `valid`/`errors` only.** A real working graph — this project's own executed MiniMax run
  — carries three false `edge_type_mismatch` warnings (29.3). Blocking on warnings would reject the
  reference model.
- **Slots already carry `node_type` and `type`**, so the report needs no enrichment.

## Spec

### 1. `create-core/src/workflow.rs` — decide the shape

```rust
//! Which of ComfyUI's two export shapes a file is.
//!
//! ComfyUI writes two different JSONs and comfy-mcp's tools do not accept the
//! same one (MCP-SURFACE 29). Everything this app does to a graph -- slots,
//! the audit, the T-305 edits -- needs the **frontend** shape.

use serde_json::Value;

/// One of ComfyUI's two export shapes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkflowFormat {
    /// `File > Save (As)` -- `nodes[]`/`links[]`. The editable shape, and the
    /// only one this app can drive.
    Frontend,
    /// `File > Export (API)` -- a flat map of id -> `{class_type, inputs}`.
    /// Runnable, but nothing can enumerate its parameters.
    Api,
}

/// Which shape `graph` is, or `None` for neither.
///
/// **API format is detected positively rather than inferred from "not
/// frontend".** The three outcomes are different messages: a frontend file
/// proceeds, an API export gets the menu item that produces the right file, and
/// something that is neither gets told it is not a workflow at all. Collapsing
/// the last two would tell a user who picked their tax return to re-export it
/// from ComfyUI.
pub fn detect_format(graph: &Value) -> Option<WorkflowFormat> {
    if graph.get("nodes").and_then(Value::as_array).is_some() {
        return Some(WorkflowFormat::Frontend);
    }
    let object = graph.as_object()?;
    // A non-empty map whose every value carries `class_type`. "Every", not
    // "any": a frontend file with a stray `class_type` somewhere must not be
    // mistaken for an API one, and an empty object says nothing at all.
    if !object.is_empty()
        && object
            .values()
            .all(|node| node.get("class_type").and_then(Value::as_str).is_some())
    {
        return Some(WorkflowFormat::Api);
    }
    None
}
```

Tests, against **both real fixtures** rather than hand-made JSON:

- `testdata/workflows/ace_step_1_5_xl_turbo.json` → `Frontend`
- `testdata/workflows/minimax_music3_int8.json` → `Frontend`
- `testdata/workflows/minimax_music3.api-format.json` → `Api`
- `json!({})` → `None`, `json!([1,2])` → `None`, `json!({"a": 1})` → `None`

### 2. `src-tauri/src/import.rs` — the command

```rust
/// What an import produced: where it was stored, and what it exposes.
#[derive(Debug, Clone, Serialize)]
pub struct ImportReport {
    /// Stable id for the stored copy; the filename stem under `workflows/`.
    pub workflow_id: String,
    /// Absolute path of the stored copy -- what a profile's `comfy.workflow`
    /// will point at.
    pub stored_path: String,
    /// Every slot the graph exposes, each already carrying its node class and
    /// widget type. This is what T-313c ranks into role suggestions.
    pub slots: Vec<Slot>,
    /// Advisory findings from validation. **Never a reason to refuse** -- a
    /// graph that demonstrably produces audio carries three of these
    /// (MCP-SURFACE 29.3).
    pub warnings: Vec<String>,
}
```

The command, in order. **The order is the design**, so keep it:

1. **Read and parse the user's file.** Errors name *their* path and say what to do — the T-313a
   review defect, which must not come back through a new door.
2. **`detect_format`.** `Frontend` proceeds. `Api` gets the `File > Save (As)` message. `None` gets
   "not a ComfyUI workflow".
3. **Stage.** Copy to `config_dir/workflows/.staging-<id>.json`, creating the directory.
4. **Validate the staged copy** — not the source. Refuse on `Verdict::Invalid` (summarising the
   findings) and on `Verdict::Vacuous`. Ignore warnings except to report them.
5. **Read the staged copy's slots.** A graph with **zero** slots is refused: nothing could be mapped
   to it, so storing it would produce a profile with no controls.
6. **Commit** — rename staging to `config_dir/workflows/<id>.json`.
7. **On any failure in 4–6, delete the staging file.** A refused import leaves nothing behind.

**Steps 3–7 are why this is not four lines.** Validating the source and then copying would mean the
report describes bytes we did not keep; copying into place and validating after would leave rubbish
behind on refusal. Staging is what makes "the stored copy is the artifact of record" true rather
than aspirational.

**The id** comes from `library::projects::slugify` on the file stem — reuse it, do not write a
second slugifier — with a `-2`, `-3` … suffix for a name already taken, the same rule
`free_slug` uses for projects and for the same reason: two imports may share a filename, and
silently overwriting someone's earlier import is worse than a second file. Refuse a slug that
`is_safe_slug` rejects rather than joining it to a path.

**ComfyUI must be running** for steps 4 and 5. Reuse `comfy::ensure_connected` exactly as
`generate_audio` does, and let its existing error copy through — do not write a second "ComfyUI is
not running" string.

### 3. `generate.rs` delegates

`ensure_frontend_format` becomes a call to `detect_format`, keeping its own message. Two format
checks that could disagree is a bug waiting for a fixture, and T-313a's was written before there
was a shared home for it.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_the_three_shapes_are_told_apart`** (create-core) — the two real frontend fixtures, the
      real API fixture, and three not-a-workflow values. The invariant: API is recognised
      *positively*, so its message can name the menu item.
- [ ] **`test_an_api_export_is_refused_before_anything_is_stored`** — assert the message names
      `File > Save (As)`, that no ComfyUI call was made, and that **`workflows/` is empty**.
- [ ] **`test_an_invalid_workflow_leaves_nothing_behind`** — validation returns errors. Assert the
      error summarises the findings and that `workflows/` contains **no file, staging included**.
      This is the one that would rot silently.
- [ ] **`test_a_valid_import_stores_the_bytes_it_validated`** — read the stored file back from disk
      and assert it equals the source. Not the return value: the file is the artifact.
- [ ] **`test_warnings_do_not_refuse_an_import`** — validation returns `valid: true` with
      `edge_type_mismatch` warnings. Assert the import **succeeds** and the warnings are reported.
      The invariant is MCP-SURFACE 29.3: this project's own working reference graph produces them.
- [ ] **`test_a_graph_with_no_slots_is_refused`** — nothing could be mapped to it.
- [ ] **`test_a_second_import_of_the_same_filename_does_not_overwrite_the_first`** — two files both
      named `song.json`; assert two stored files and two ids.
- [ ] Mutation: validating the *source* instead of the staged copy must fail a test; treating
      warnings as errors must fail a test; skipping the staging cleanup must fail a test.
- [ ] No frontend changes. No profile is written.

## Manual verify (producer click-through)

No UI yet, so this is exercised the way T-313a's was — but **do it after T-313e**, when there is a
button; a Tauri command with no caller is not worth a hand-run. Note here so it is not lost:
import a real workflow, confirm the file appears under `%APPDATA%\com.latentbeats.create\workflows\`,
then import a deliberately broken graph and confirm that directory is unchanged.

## Out of scope

- **Suggesting roles.** T-313c ranks the slots this returns.
- **Writing a profile.** T-313d. This task stores a workflow and reports on it, nothing more.
- **Any UI.** T-313e.
- **Re-import / update-in-place / delete.** Managing stored workflows is its own task, and guessing
  its shape now would prejudge T-313e.
- **Model-file readiness.** A workflow naming a model the user lacks fails validation with
  `unknown_enum_value`, which is a refusal with a usable message. Installing the model and
  re-importing is the path; a "stored but broken" state is not being invented here.

## Changed during implementation and review

**1. The `is_safe_slug` guard was dropped, not made public.** The brief said to refuse a slug
`is_safe_slug` rejects. It is private in `library::projects`, and making it public would have been
asserting something its sibling already promises: `slugify` guarantees a safe slug by construction,
pinned by its own test `is_safe_slug(&slugify("../../etc/passwd"))`. A comment records that rather
than a redundant check.

**2. The staging design was untested, and the brief's own mutation caught it.** Swapping
`inspect(comfy, &staged)` for `inspect(comfy, source)` — validating the file the user picked
instead of the copy that was kept — **passed all 101 tests**. Every test compared the stored copy
against its own source, which is equal either way, so nothing observed *which* file ComfyUI was
asked about. That is the single guarantee the whole stage-then-commit order exists to provide.

Fixed by asserting, in the happy-path test, that every recorded ComfyUI call names a path whose
filename starts with `.staging-`. This is the same class of finding as T-311d's and T-310a's:
a test that reads as though it covers something because the code around it is right.

Three mutations run, three killed — after the first one was made killable.
