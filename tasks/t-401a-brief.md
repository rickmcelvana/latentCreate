# T-401a — projects become first-class: the backend seam

**Lane: Aider.** Ten files of mechanical cross-file work — the reference code below is the core,
the rest is one-line call-site swaps and fixture discipline. **Depends:** Phase 4 opened
(2026-08-30, [tasks/phase-4.md](phase-4.md)). | **Crate/dir:** `crates/library`, `src-tauri`,
`testdata/wire`, plus two one-line frontend mirrors that the shared wire fixture forces.

**Files to create/modify:**

- `crates/library/src/config.rs` — `Config` gains `default_project_slug`
- `testdata/wire/loaded-config.json` — the shared wire fixture gains the field
- `app/src/bridge/config.ts` — the `Config` interface gains the field (**one line; mandated by the
  fixture test, not frontend work** — see §2)
- `app/src/state/config.test.ts` — the typed fixture gains the field (**one line, same reason**)
- `src-tauri/src/projectctx.rs` — `default_project` becomes `selected_project` + `resolve_selected`
- `src-tauri/src/projects.rs` — **new**: `projects_list`, `projects_create`
- `src-tauri/src/lib.rs` — `mod projects;` and the two registrations
- `src-tauri/src/generate.rs` — call site (submit-time resolution)
- `src-tauri/src/lyricdoc.rs` — call sites (open and save)
- `src-tauri/src/tracks.rs` — call site

---

## Goal

The single-project seam (`projectctx::default_project`, "first project or create") becomes a
**selected**-project seam: `config.default_project_slug` names the working project, every command
resolves through one function that honours it, and the persistence mechanism exists. The picker
that *sets* the selection is **T-401b**; this task is everything behind it, testable with no UI.

## The one deviation from phase-4.md, and why

The phase file names **`projects_select(slug)`** as a third new command. **It is not built.** The
selection persists through the existing `save_config` path, exactly as `default_profile_id` does
(T-303): the frontend's config store is the single writer of config, and a second command would be
a second writer — the repo's most-repeated defect. The picker in T-401b calls
`useConfigStore.save({ default_project_slug })`; nothing in this task or T-401b needs a command.
Recorded in PROJECT.md's decisions log.

## Spec

### 1. `library::config::Config` gains `default_project_slug`

```rust
    /// `Project::slug` the studios and the Library are working in.
    ///
    /// `None` means "first project, or a fresh one on an empty root" --
    /// `projectctx::selected_project` owns that fallback chain; this field only
    /// records an explicit choice. Same shape as `default_profile_id`: a
    /// top-level `Option` field written through `save_config`.
    #[serde(default)]
    pub default_project_slug: Option<String>,
```

Insert after `default_profile_id` in the struct; add `default_project_slug: None,` to the
`Default` impl; add the field to the two tests that construct a `Config` literal
(`test_save_then_load_roundtrips` → `Some("night-drive".to_string())` so the round trip proves a
real value survives; `test_secrets_never_appear_in_config_json` → `None`).

**`#[serde(default)]` is load-bearing**: `test_load_missing_accept_reasoning_effort_defaults_to_none`
writes a hand-made pre-T-302b `config.json` with no such field and must still parse with no
warnings. That test is the "configs that predate this field" guard — it passes unchanged.

### 2. The shared wire fixture, and the two frontend lines it forces

`testdata/wire/loaded-config.json` is asserted byte-for-byte by **both** `config.rs`'s
`test_wire_fixture_matches_current_types` and `app/src/state/config.test.ts`'s
`test_typed_fixture_matches_shared_wire_file` — that is the entire point of the shared file, so all
three change in this commit, and `app/src/bridge/config.ts` is in this task's file list **only**
for that reason:

- `testdata/wire/loaded-config.json`: add `"default_project_slug": null,` after the
  `default_profile_id` line.
- `app/src/bridge/config.ts` `Config` interface: add
  `/** `Project::slug` last selected; `null` means the first project. */` +
  `default_project_slug: string | null`.
- `app/src/state/config.test.ts` typed fixture: add `default_project_slug: null,`.

JSON key order does not matter to `toEqual`; only presence does.

### 3. `projectctx.rs` — the resolution chain, one public function

```rust
//! Which project the app is working in.
//!
//! Shared rather than duplicated, and that is the whole point of the module:
//! lyrics and tracks must agree on where they are filed. Two copies of this
//! rule would drift on the first change, and the symptom -- a track saved into
//! one project while the lyrics it was generated from sit in another -- would
//! look like data loss rather than a policy disagreement.
//!
//! Policy, not storage, so it lives here and not in `library`.

use std::path::Path;

use create_core::project::Project;

/// Name of the project things are written under, before the user has named one.
pub const DEFAULT_PROJECT_NAME: &str = "My First Song";

/// The project every command writes to, resolved from the persisted selection.
///
/// The selection is `config.default_project_slug` when that project still
/// exists, else the first project in slug order; a fresh root has none, so
/// `My First Song` is created. Deterministic, so a restart lands on the same
/// project. The fallback chain is the whole point: a configured slug whose
/// project has been deleted (or a garbage slug in a hand-edited config)
/// degrades to the first project, never to an error, and never to a different
/// project per caller.
pub fn selected_project(root: &Path) -> Result<Project, library::LibraryError> {
    let configured = library::config::load(root).config.default_project_slug;
    resolve_selected(root, configured.as_deref())
}

/// The resolution chain, pure and testable: the configured slug when it
/// exists, else the first project, else a freshly created
/// [`DEFAULT_PROJECT_NAME`].
fn resolve_selected(
    root: &Path,
    configured: Option<&str>,
) -> Result<Project, library::LibraryError> {
    if let Some(slug) = configured {
        if let Ok(project) = library::projects::load_project(root, slug) {
            return Ok(project);
        }
    }
    let set = library::projects::list_projects(root);
    if let Some(first) = set.projects.into_iter().next() {
        return Ok(first);
    }
    library::projects::create_project(root, DEFAULT_PROJECT_NAME, &library::projects::now_rfc3339())
}
```

`default_project` is **deleted**, not kept as an alias — a function named "default" is a trap a
future call site will use when it means "selected". The module's whole value is one seam; two
names for it is how the phase file's trap comes back.

**Tests** (replace the existing `mod tests`; carry over the two existing tests where noted):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const NOW: &str = "2026-08-30T10:00:00Z";

    fn write_config(root: &Path, slug: Option<&str>) {
        let mut config = library::Config::default();
        config.default_project_slug = slug.map(str::to_string);
        library::config::save(root, &config).unwrap();
    }

    /// Protects: the configured slug beats the first project.
    ///
    /// The trap this task exists to avoid. The old `default_project` returned
    /// the first project in slug order, and a half-done refactor keeps doing
    /// that while claiming to honour the selection -- every caller "resolves
    /// to a project", and none of them to the *selected* one.
    #[test]
    fn test_selected_project_uses_the_configured_slug() {
        let root = tempfile::tempdir().unwrap();
        library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        let beta =
            library::projects::create_project(root.path(), "Beta", "2026-08-30T10:00:01Z")
                .unwrap();
        write_config(root.path(), Some("beta"));

        let project = selected_project(root.path()).unwrap();
        assert_eq!(project.slug, beta.slug);
        assert_ne!(project.slug, "alpha");
    }

    /// Protects: a configured slug whose project has gone falls to the first
    /// project rather than erroring -- the phase file's specified fallback.
    #[test]
    fn test_selected_project_falls_back_when_the_configured_one_is_gone() {
        let root = tempfile::tempdir().unwrap();
        let alpha = library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        write_config(root.path(), Some("deleted-project"));

        let project = selected_project(root.path()).unwrap();
        assert_eq!(project.slug, alpha.slug);
    }

    /// Protects: a garbage slug in a hand-edited config cannot break the app.
    /// It degrades to the same fallback as a missing project, silently and
    /// consistently -- `load_project` refuses the slug, the chain falls
    /// through, and every caller still gets the same project.
    #[test]
    fn test_a_garbage_configured_slug_degrades_to_the_fallback() {
        let root = tempfile::tempdir().unwrap();
        let alpha = library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        write_config(root.path(), Some("../../etc/passwd"));

        let project = selected_project(root.path()).unwrap();
        assert_eq!(project.slug, alpha.slug);
    }

    /// Protects: the default project exists after the first open, and a second
    /// open lands on the same one rather than minting a second. Carried over
    /// from the pre-selection `default_project`, now through the config path.
    #[test]
    fn test_selected_project_is_created_once_and_reused() {
        let root = tempfile::tempdir().unwrap();
        let first = selected_project(root.path()).unwrap();
        assert_eq!(first.name, DEFAULT_PROJECT_NAME);
        assert_eq!(first.slug, "my-first-song");

        let again = selected_project(root.path()).unwrap();
        assert_eq!(again.slug, "my-first-song");
        assert_eq!(again.created_at, first.created_at);
    }

    /// Protects: lyrics and tracks resolving to different projects.
    ///
    /// The reason this module exists. Both callers go through one function, so
    /// this asserts the property that matters rather than the call graph: two
    /// resolutions against the same root with the same selection are the same
    /// project -- and that project is the *selected* one, not whichever came
    /// first.
    #[test]
    fn test_every_caller_resolves_to_the_same_project() {
        let root = tempfile::tempdir().unwrap();
        library::projects::create_project(root.path(), "Alpha", NOW).unwrap();
        library::projects::create_project(root.path(), "Beta", NOW).unwrap();
        write_config(root.path(), Some("beta"));

        let for_lyrics = selected_project(root.path()).unwrap();
        let for_tracks = selected_project(root.path()).unwrap();
        assert_eq!(for_lyrics.slug, for_tracks.slug);
        assert_eq!(for_lyrics.slug, "beta");
    }
}
```

### 4. `src-tauri/src/projects.rs` — the two commands

```rust
//! Tauri commands over the on-disk project store.
//!
//! The selection is deliberately **not** a command here: it persists through
//! the existing `save_config` path exactly as `default_profile_id` does
//! (T-303), so the config store stays the single writer of config. These two
//! commands only list and create.

use create_core::project::Project;
use tauri::State;

use crate::ConfigDir;

/// Every project the app could read, with warnings for the ones it could not.
///
/// Never fails: a project that cannot be read is a warning, not an error that
/// hides every other one (`library::projects::list_projects`).
#[tauri::command]
pub fn projects_list(config_dir: State<'_, ConfigDir>) -> library::ProjectSet {
    library::projects::list_projects(&config_dir.0)
}

/// Create a project and return its record.
///
/// Selecting it is the frontend's next step, kept separate so creating and
/// selecting stay independently testable. `name` is user text; the slug comes
/// from `slugify`, and a taken slug gets a numeric suffix rather than the
/// existing project being returned.
#[tauri::command]
pub fn projects_create(
    config_dir: State<'_, ConfigDir>,
    name: String,
) -> Result<Project, String> {
    library::projects::create_project(&config_dir.0, &name, &library::projects::now_rfc3339())
        .map_err(|e| e.to_string())
}
```

No tests in this module: each command is one library call, and the library functions they wrap
carry the tests (`projects.rs` in `crates/library`). `projects_list` returns `ProjectSet` directly
(not `Result`), the same never-fails shape `load_config` uses for `LoadedConfig`.

### 5. `src-tauri/src/lib.rs`

Add `mod projects;` (alphabetically after `mod projectctx;`) and register both commands in the
`invoke_handler`, e.g. right after `tracks::library_tracks`:

```rust
            projects::projects_list,
            projects::projects_create,
```

### 6. The four call sites — one function, no drift

Each currently calls `default_project`; each becomes `selected_project`. The signature is
identical (`&Path` → `Result<Project, _>`), so the swaps are one token each:

- `src-tauri/src/generate.rs` — `use crate::projectctx::default_project;` →
  `use crate::projectctx::selected_project;` and the call at line ~106. **This is the
  submit-time resolution**: the project is captured into `PendingTrack.project_slug` here, and
  ingest writes to that slug — never re-resolving. A track therefore lands in the project that
  was selected when Generate was clicked, even if the selection changes while it renders. That
  is the correct behaviour; do not "fix" it to resolve later.
- `src-tauri/src/lyricdoc.rs` — `use crate::projectctx::default_project;` →
  `use crate::projectctx::selected_project;` and both calls (`lyrics_open` and `lyrics_save`).
- `src-tauri/src/tracks.rs` — `crate::projectctx::default_project(&config_dir.0)` →
  `crate::projectctx::selected_project(&config_dir.0)`.

`ingest.rs` does **not** change: it uses `pending.project_slug` by design.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] The five `projectctx` tests above, each naming its invariant; the flagship one is
      `test_selected_project_uses_the_configured_slug` — **if the configured slug were ignored,
      it must fail** (mutation check: delete the `if let Ok` block and watch it fail).
- [ ] `grep -rn "fn default_project" --include=*.rs src-tauri crates` finds **nothing** — the old
      function name is gone, so no call site can silently keep the pre-selection behaviour. (The
      new `default_project_slug` field contains that string, so grep for the function, not the
      word.)
- [ ] `testdata/wire/loaded-config.json`, `app/src/bridge/config.ts` and
      `app/src/state/config.test.ts` move together — the two fixture tests are the enforcement.
- [ ] No changes outside the listed files.

## Out of scope

- **The frontend picker and `state/projects.ts`** — **T-401b**, the next brief.
- **`projects_create` selecting the new project** — the frontend store composes create-then-select;
      the commands stay independent.
- **`projects_select`** — see the deviation note; the selection persists via `save_config`.
- **Anything about tracks, albums, delete, rename, export, send-to** — later T-numbers.
- **ARCHITECTURE.md** — verified: it contains no single-project claim to update.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-401a-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/library/src/projects.rs --read crates/library/src/lib.rs --read crates/create-core/src/project.rs --file crates/library/src/config.rs --file testdata/wire/loaded-config.json --file app/src/bridge/config.ts --file app/src/state/config.test.ts --file src-tauri/src/projectctx.rs --file src-tauri/src/projects.rs --file src-tauri/src/lib.rs --file src-tauri/src/generate.rs --file src-tauri/src/lyricdoc.rs --file src-tauri/src/tracks.rs
```

`crates/library/src/projects.rs` and `lib.rs` are `--read` because the new code calls
`create_project`/`list_projects`/`now_rfc3339` and constructs `library::LibraryError`/`ProjectSet`
return types (WORKFLOW §3: definitions in view, not editable). `create-core/src/project.rs` is
`--read` because the new commands return `create_core::project::Project`. `generate.rs` and
`lyricdoc.rs` are `--file` because their `use` lines and call sites change.
