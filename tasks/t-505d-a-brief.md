# T-505d-a — Adopt a gallery row: the fetch→import backend seam

**Lane: Aider.** A thin Rust seam + its TS mirror: one new Tauri command that fetches a gallery
template and hands it to the **existing** import path, one factored helper with tests, the bridge
mirror, and command registration. **Depends:** T-504 (`catalog.rs`, `mcp_bridge::fetch_template`),
T-313 (`import.rs`'s `import_into`). **Dirs:** `src-tauri/src`, `app/src`. **No UI, no click-through**
— the UI is T-505d-b, which wires a "Bring in" button on a ready bare row to this seam.

**Files to create/modify:**

- `src-tauri/src/catalog.rs` — add `catalog_adopt_begin` command + an `adopt_from_fetched` helper +
  tests.
- `src-tauri/src/lib.rs` — register `catalog::catalog_adopt_begin`.
- `app/src/bridge/catalog.ts` — mirror: `catalogAdoptBegin(name) -> ImportReport`.

---

## Goal

Turn a browsable gallery row into the app's own profile by feeding its workflow through the T-313
import machinery **unchanged**. A gallery template is a workflow the user did not have to hunt for;
adopting one is the same fetch → copy → validate → role-suggest path an imported file takes, so **no
second import mechanism is built** (owner decision, phase-5.md). This lane lands only the seam that
produces an `ImportReport` from a template name; T-505d-b renders the mapping and calls the existing
`save_imported_profile` to finish.

## Why this is a seam and not a rewrite (verified live 2026-09-03)

`import_into` (`src-tauri/src/import.rs`) already does the whole job — read, **refuse anything but
frontend format**, stage a copy, validate, list slots, suggest roles, return an `ImportReport`. The
only thing adopt adds is *where the file comes from*: instead of a path the user picked, it is a
gallery template written to disk by `mcp_bridge::fetch_template(name, out_path)`.

Two facts make the reuse sound, both checked against the running gallery today, not recalled:

1. **`fetch_template` writes the frontend/editing format.** A live fetch of `audio_ace_step_1_5_split`
   produced a file with top-level `nodes[]` + `links` — exactly what `create_core::detect_format`
   keys `WorkflowFormat::Frontend` on (`workflow.rs:29`), and what `import_into` accepts. (The
   generation pipeline already relies on this: it fetches then `list_workflow_slots`, which refuses
   API format.) So a fetched template passes `import_into`'s format gate.
2. **An un-installed row is refused by validation, with a filename.** The same live fetch returned
   `local_check runnable:false` naming `acestep_v1.5_turbo.safetensors` as absent — and `import_into`
   runs `validate_workflow`, which rejects a checkpoint enum value ComfyUI does not know
   (`unknown_enum_value`, the T-309d finding). So adopting a row whose model files are not installed
   fails **loudly** at import, with the missing filename — it never emits a broken profile. T-505d-b
   only offers the button on a `ready` row; this is the backstop.

## The one new command

`catalog_adopt_begin(name)` = ensure connected → fetch the template to a temp file → hand that file to
`import_into` → clean the temp up → return the `ImportReport`. That is the whole command. It calls
**no** import logic of its own.

### Spec — `src-tauri/src/catalog.rs`

Add to the existing `use` block:

```rust
use std::path::Path;

use crate::import::{import_into, ImportReport};
use crate::ProfilesDir; // only if not already imported; catalog.rs currently imports ConfigDir
```

`import_into` and `ImportReport` are `pub(crate)` / `pub` in the same crate (`app`), so
`crate::import::import_into` resolves. Confirm the exact visibility when wiring — `import_into` is
`pub(crate)`, `ImportReport` is `pub`.

The command:

```rust
/// Adopt a gallery row into an app profile: fetch its workflow and run it
/// through the same import path a user-picked file takes (T-313). Returns the
/// `ImportReport` the mapping screen works from; T-505d-b renders that and calls
/// `save_imported_profile` to finish. Nothing is written to the profile set here.
///
/// The row must be one this install can run -- an un-installed template is
/// refused by `import_into`'s validation, naming the missing file (MCP-SURFACE
/// 33). The UI only offers this on a `ready` row; this is the backstop.
#[tauri::command]
pub async fn catalog_adopt_begin(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    bin: Option<String>,
    name: String,
) -> Result<ImportReport, String> {
    let comfy = ensure_connected(&state, &config_dir, bin)
        .await
        .map_err(ensure_detail)?;

    // Fetch to a temp file named after the template, so the workflow_id and the
    // default profile name `import_into` derives read as the model, not a uuid.
    // import_into copies this into `workflows/`, so the temp is scratch.
    let temp = std::env::temp_dir().join(format!("latentcreate-adopt-{name}.json"));
    comfy
        .fetch_template(&name, &temp)
        .await
        .map_err(|e| e.to_string())?;

    adopt_from_fetched(&comfy, &config_dir.0, &temp).await
}

/// Run a fetched workflow file through `import_into`, then remove it whatever
/// happened. Split from the command so a test can drive it with a real file on
/// disk and a mock transport -- `fetch_template` itself writes via comfy-cli and
/// cannot be mocked into producing a file.
async fn adopt_from_fetched(
    comfy: &mcp_bridge::LocalComfy,
    root: &Path,
    fetched: &Path,
) -> Result<ImportReport, String> {
    let result = import_into(comfy, root, fetched).await;
    // import_into copied what it needed into `workflows/`; the fetch temp is
    // scratch either way. A leftover would rot silently, so remove it on the
    // error path too.
    let _ = std::fs::remove_file(fetched);
    result
}
```

Notes for the executor:

- `ensure_connected` / `ensure_detail` are already in this file (used by `catalog_browse`).
- `mcp_bridge::LocalComfy` is the type `ensure_connected` returns (an `Arc<LocalComfy>` derefs to it);
  match the signature `import_into` expects — it takes `&LocalComfy`, and `&*comfy` / `&comfy` coerces.
  Use whatever the compiler accepts with the existing `comfy` binding; `catalog_browse` already calls
  `comfy.browse_templates(...)`, so a `&comfy` deref is available.
- Do **not** thread a `ProfilesDir` — `import_into` writes under `config_dir` (`workflows/` and
  `profiles/`), exactly as `import_workflow` does. `config_dir.0` is the `root`.

### Tests — `src-tauri/src/catalog.rs`

The command's own body (fetch + delegate) is thin glue over `fetch_template` (tested in mcp-bridge)
and `import_into` (thoroughly tested in `import.rs`). Test the part that is this lane's: that
`adopt_from_fetched` returns the report **and removes the temp**, on both the success and the failure
path. Pre-write the temp file the way `fetch_template`'s comfy-cli would, and drive the import calls
with a mock transport — reuse `import.rs`'s test helpers (`client_and_log`, and the frontend fixture
`testdata/workflows/ace_step_1_5_xl_turbo.json` via a small local reader, or copy `import.rs`'s
`named_fixture`/`ok_replies` shape).

```rust
#[cfg(test)]
mod adopt_tests {
    use super::*;
    use mcp_bridge::mock::Reply;
    use mcp_bridge::test_helpers::client_and_log;
    use serde_json::json;

    // The frontend fixture import.rs uses; a fetched template is this shape.
    fn frontend_fixture() -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../testdata/workflows/ace_step_1_5_xl_turbo.json");
        std::fs::read_to_string(&path).expect("fixture reads")
    }

    /// A clean inspect: validate, then slots (import.rs's `ok_replies`).
    fn ok_replies() -> Vec<Reply> {
        vec![
            Reply::Json(json!({
                "valid": true, "errors": [], "warnings": [],
                "converted_from_ui": true, "converted_node_count": 11
            })),
            Reply::Json(json!({
                "workflow": "staged", "count": 1,
                "slots": [{
                    "address": "94.tags", "name": "tags", "type": "STRING",
                    "current_value": "synthwave", "instance_id": "94",
                    "node_type": "TextEncodeAceStepAudio1.5"
                }]
            })),
        ]
    }

    /// Protects: a fetched workflow is imported, and the fetch temp is removed.
    /// The temp is `fetch_template`'s scratch output; a leftover would rot and
    /// collide with the next adopt of the same template.
    #[tokio::test]
    async fn test_adopt_imports_the_fetched_file_and_cleans_it_up() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fetched = tmp.path().join("audio_ace_step_1_5_split.json");
        std::fs::write(&fetched, frontend_fixture()).expect("write the fetched file");

        let (comfy, _calls) = client_and_log(ok_replies()).await;
        let report = adopt_from_fetched(&comfy, tmp.path(), &fetched)
            .await
            .expect("a fetched frontend workflow imports");

        assert_eq!(report.workflow_id, "audio-ace-step-1-5-split");
        assert!(!fetched.exists(), "the fetch temp must be removed");
        assert!(report.slots.len() >= 1);
    }

    /// Protects: a refused import still removes the fetch temp. An un-installed
    /// template fails validation here (unknown_enum_value); the scratch file must
    /// not survive the failure.
    #[tokio::test]
    async fn test_a_refused_adopt_still_cleans_up_the_temp() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let fetched = tmp.path().join("audio_ace_step_1_5_split.json");
        std::fs::write(&fetched, frontend_fixture()).expect("write the fetched file");

        let replies = vec![Reply::Json(json!({
            "valid": false,
            "errors": [{ "node_id": "104", "message": "not in 3 known options for unet_name" }],
            "warnings": []
        }))];
        let (comfy, _calls) = client_and_log(replies).await;
        let err = adopt_from_fetched(&comfy, tmp.path(), &fetched)
            .await
            .expect_err("an un-runnable template is refused at validation");

        assert!(err.contains("node 104"), "{err}");
        assert!(!fetched.exists(), "the fetch temp must be removed on failure too");
    }
}
```

Add one `#[ignore]` live test in the existing catalog test module (beside
`test_browse_and_readiness_against_a_live_comfyui`) that fetches a real template and drives the whole
adopt against a live comfy-mcp — asserting only the **shape** (a report with a `workflow_id` and at
least one slot for a runnable template), not which models are on the box. Gate it the same way, run at
the T-505 milestone with `cargo test -p app -- --ignored`.

### `app/src/bridge/catalog.ts`

Mirror the command. Import the `ImportReport` type from the import bridge (do not redefine it):

```ts
import type { ImportReport } from './import'
```

```ts
/**
 * Adopt a gallery row: fetch its workflow and run it through the T-313 import
 * path, returning the report the mapping screen works from. Nothing is written
 * to the profile set yet -- `saveImportedProfile` (bridge/import) finishes it.
 *
 * The row must be runnable here; an un-installed template is refused by import
 * validation with the missing filename.
 */
export async function catalogAdoptBegin(name: string, bin?: string): Promise<ImportReport> {
  return await invoke<ImportReport>('catalog_adopt_begin', { name, bin })
}
```

### `src-tauri/src/lib.rs`

Register the command beside the other catalog commands:

```rust
            catalog::catalog_browse,
            catalog::catalog_readiness,
            catalog::catalog_adopt_begin,
```

## Acceptance criteria

- [ ] `npm run gate` green (Rust + TS).
- [ ] `catalog_adopt_begin` fetches a template and returns an `ImportReport` via `import_into`, writing
      nothing to the profile set (that is `save_imported_profile`, unchanged, called later by the UI).
- [ ] The fetch temp file is removed on both the success and the failure path (both tests present).
- [ ] `import.rs`, `save_imported_profile`, and the import store are **unchanged** — this lane reuses
      them, it does not touch them.
- [ ] Only the three listed files change.

## Out of scope (T-505d-b, and beyond)

- **The "Bring in" button, the adopt mapping UI, and the `adopt(name)` import-store action** — all
  T-505d-b, which renders `import_into`'s report through the existing role-mapping helpers and calls
  `save_imported_profile`. This lane has **no click-through**; it cannot, with no UI.
- **Gating the button on a `ready` row** — a UI concern (T-505d-b). The backend refuses an
  un-installed template at validation regardless.
- **Installing a bare row's model files** — there is no download URL for a gallery row (MCP-SURFACE
  33); adopt is for a model the user already has. Curated install is T-505c (shipped-URL models only).

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505d-a-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read src-tauri/src/import.rs --read crates/mcp-bridge/src/templates.rs --file src-tauri/src/catalog.rs --file src-tauri/src/lib.rs --file app/src/bridge/catalog.ts
```

`import.rs` is `--read`: the seam reuses `import_into` and must not change it — it is here as the
reference for the helper's signature and the test-helper shapes (`client_and_log`, `ok_replies`, the
fixture path). `templates.rs` is `--read` for the `fetch_template(name, out_path)` signature.
