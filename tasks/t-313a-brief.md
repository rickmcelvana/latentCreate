# T-313a — the pipeline honours `comfy.workflow`

**Lane: architect-direct.** One extracted function, one call-site change, and their tests.
WORKFLOW section 1: writing the reference *is* writing the task.

**Depends:** nothing unbuilt. **Crate/dir:** `src-tauri`.

**Files to modify:**

- `src-tauri/src/generate.rs` — `place_working_copy`, the first step of `build_and_submit`, tests
- nothing else

## Why

`build_and_submit` opens by refusing:

```rust
let template = profile.comfy.template.as_deref().ok_or_else(|| {
    format!("{} declares no gallery template; imported workflows are not wired up yet", profile.id)
})?;
```

`ComfySpec.workflow` has existed in the profile schema since T-107 and nothing has ever read it.
That refusal is the only thing standing between this app and ARCHITECTURE 5b's entire purpose.

**Why this is first of the five, and not the UI.** User profiles already load from
`config_dir/profiles` (`library::profiles::load(shipped, user)`, called from five commands) and the
T-303 picker already lists whatever it finds. So the moment this lands, a person can hand-write a
profile pointing at any frontend-format workflow on disk and **generate from it** — no import
screen, no mapping screen. That is the pressure-release valve working, three tasks before the valve
has a UI. It is also the only part of T-313 that T-314's "an imported user workflow generates
successfully" strictly requires.

## What the scoping settled (MCP-SURFACE 29)

**The stored workflow must be frontend-format** (`nodes[]`/`links[]`, from `File > Save (As)`).
Not a preference — every step after this one requires it:

- `set_workflow_slot` refuses API format (`workflow_not_frontend_format`)
- `audit_slots` reads `nodes[]` directly
- the T-305 graph edits walk the same structure

`validate_workflow` is the one tool that takes *either*, which is exactly why it cannot be relied
on to catch the mistake — an API-format file **validates clean** (29.1) and would then fail three
steps later with something unreadable about inert slots. So this task checks the shape itself, up
front, and says which menu item produces the right file.

## Spec

### 1. Place the working copy

Replace the `let template = ...` refusal with a call to this, added just above `build_and_submit`.

```rust
/// Put this job's own copy of the graph at `workflow`.
///
/// Two sources, one contract: when this returns `Ok`, `workflow` holds a
/// **frontend-format** graph that the later steps may freely rewrite. Never a
/// shared path -- the MCP docs warn about TOCTOU, and two generations at once
/// would edit each other's graph.
///
/// A profile declares a gallery `template` **or** an imported `workflow`
/// (ARCHITECTURE 5b), never both. The imported file is copied rather than
/// fetched because nothing remote owns it.
async fn place_working_copy(
    comfy: &LocalComfy,
    profile: &ModelProfile,
    workflow: &Path,
) -> Result<(), String> {
    match (
        profile.comfy.template.as_deref(),
        profile.comfy.workflow.as_deref(),
    ) {
        (Some(_), Some(_)) => Err(format!(
            "{} declares both a gallery template and an imported workflow; it must declare one",
            profile.id
        )),
        (Some(template), None) => comfy
            .fetch_template(template, workflow)
            .await
            .map_err(|e| e.to_string()),
        (None, Some(source)) => {
            let source = Path::new(source);
            std::fs::copy(source, workflow).map_err(|e| {
                format!(
                    "{} could not read its workflow at {}: {e}. \
                     Re-import it, or point the profile at the file's new location.",
                    profile.id,
                    source.display()
                )
            })?;
            ensure_frontend_format(&read_workflow(workflow)?, &profile.id)
        }
        (None, None) => Err(format!(
            "{} declares neither a gallery template nor an imported workflow",
            profile.id
        )),
    }
}

/// Refuse a graph the later steps cannot edit.
///
/// The check is the presence of a top-level `nodes` array, which is what
/// separates the frontend ("editing") export from the API export (MCP-SURFACE
/// 29). It is done here rather than left to `validate_workflow`, because
/// validate accepts **both** formats and reports an API export as `valid: true`
/// (29.1) -- the run would then fail three steps later with a message about
/// inert slots, which describes nothing the user did.
///
/// The remedy names the menu item, taken from comfy-cli's own refusal, which
/// words it better than this app could.
fn ensure_frontend_format(graph: &Value, profile_id: &str) -> Result<(), String> {
    if graph.get("nodes").and_then(Value::as_array).is_some() {
        return Ok(());
    }
    Err(format!(
        "{profile_id}'s workflow is not the format latentCreate can edit. \
         In ComfyUI use File > Save (As) to export the editing format -- \
         the File > Export (API) output cannot be used here."
    ))
}
```

**Both-set is an error, not a precedence rule.** Silently preferring one would make a profile that
says two contradictory things generate from whichever the code happened to check first. Both
shipped profiles set `template` only, so nothing existing is affected, and T-313d's emitter must
not produce such a profile — this is the test that says so.

### 2. The call site

`build_and_submit`'s step 1 becomes:

```rust
    // 1. This job's own copy, from a gallery template or an imported file.
    place_working_copy(comfy, profile, workflow).await?;
```

Delete the `let template = ...` binding. Everything downstream already works on the working copy
and needs no change — which is the point of putting the seam here.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_an_imported_workflow_is_copied_and_runs`** — a profile with `comfy.workflow` pointing
      at the captured ACE-Step frontend template, `comfy.template` absent. Assert the mock's call
      sequence contains **no `fetch_template`**, that the working copy exists with the source's
      content, and that the run is submitted. The invariant: an imported profile reaches ComfyUI by
      the same path a gallery one does.
- [ ] **`test_an_api_format_workflow_is_refused_with_the_menu_item`** — point a profile at
      `testdata/workflows/minimax_music3.api-format.json` (a **real** API export — the executed
      T-315 graph). Assert the error names `File > Save (As)`. The invariant: the one shape that
      validates clean but cannot be edited is caught here, not three steps later.
- [ ] **`test_a_profile_declaring_both_is_refused`** — and assert nothing was submitted.
- [ ] **`test_a_profile_declaring_neither_is_refused`** — the old message's replacement; it must no
      longer say "not wired up yet", which will have stopped being true.
- [ ] **`test_a_missing_imported_file_says_how_to_fix_it`** — a path that does not exist. Assert the
      message contains the path and a next step (CONVENTIONS line 29).
- [ ] The existing gallery-template tests still pass **unchanged**. If one needs editing, the seam
      is in the wrong place.
- [ ] Mutation: making both-set prefer the template must fail a test; dropping the
      `ensure_frontend_format` call must fail a test.
- [ ] No changes outside `src-tauri/src/generate.rs`.

## Manual verify (producer click-through)

This is the part worth doing by hand, because it is the first time the app runs a graph it did not
fetch. **No UI exists yet — the profile is hand-written on purpose.**

1. In ComfyUI, open any working workflow and `File > Save (As)` it somewhere.

2. Copy `profiles/ace-step-1.5-turbo.json` to
   `%APPDATA%\com.latentbeats.create\profiles\my-import.json`. Change `id` to something new, change
   `display_name`, **delete `comfy.template`**, and add `"workflow": "<the path from step 1>"`.
   (Its `inputs` addresses will only resolve if the graph is ACE-Step-shaped — for a different
   graph expect a slot-resolution refusal, which is T-313c/d's job to fix and **is a pass here**:
   it means the file was read.)

3. Restart the app. The new profile appears in the picker.

4. Generate. It should queue and produce a track exactly as a shipped profile does.

5. Now point the same profile at a `File > Export (API)` export and generate: the refusal should
   name `File > Save (As)`.

Step 5 is the one to watch — it is the mistake a real user will make, and the message is the whole
value of catching it here.

## Out of scope

- **Importing anything.** No file picker, no copy-into-app-storage, no validation screen. A path in
  a hand-written profile is the whole surface. That is T-313b.
- **Suggesting or mapping roles.** T-313c.
- **Writing profiles.** T-313d.
- **`vram_gb_min` on an imported graph.** Nothing can estimate it; T-314 is settling the number.
- **Storing the workflow inside the app dir.** A profile points at wherever the file is. Where an
  *imported* copy lives is T-313b's decision, and making it here would prejudge it.

## Changed during review

**One defect the brief's own reference code would have shipped.** The imported arm ended
`ensure_frontend_format(&read_workflow(workflow)?, ...)`, and `read_workflow` reports against the
path it is handed — the working copy, buried under `jobs/<id>/`. A user who picked a PNG, or a
half-saved file, would have been shown an internal path and `expected value at line 1 column 1`:
neither their file nor a next step, which is the CONVENTIONS rule this task exists to satisfy on
the *format* mistake. Now parsed inline so the message names the user's file and points at
`File > Save (As)`.

Added `test_a_file_that_is_not_json_names_the_users_file`, which asserts the message contains
their filename and **not** ours. Three mutations run, three killed — including reverting this fix,
which the new test catches.

The wrong-file case is worth this much attention because it is the *only* failure here a user
reaches by doing something reasonable. Both other refusals (declaring both sources, declaring
neither) are profile bugs that only T-313d's emitter can cause.

## Click-through result — passed 2026-08-30

Both halves. A hand-written `my-import.json` pointing at a `File > Save (As)` export **generated**,
and repointing it at an API export gave the refusal naming the right menu item.

**It found one copy defect a test could not.** The message carried a literal `--` — this repo's
em-dash convention for comments and docs, leaked into user-facing copy. It is the only user-facing
string in the codebase that did; every other one uses a sentence break. Now three sentences. Note
that `assert!(err.contains("File > Save (As)"))` passes identically either way, which is exactly
why this needed a person to look at it.
