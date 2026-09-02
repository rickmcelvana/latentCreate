# T-504 — Model catalog: gallery browse + bare-row readiness (backend seam)

**Lane: Aider.** A small, self-contained backend seam: one new bridge method, one new `src-tauri`
command module, two registrations. **Depends:** Phase 5 open ([tasks/phase-5.md](phase-5.md)); the
image/catalog surface verified live ([docs/MCP-SURFACE.md §32–§33](../docs/MCP-SURFACE.md)).
**Crate/dir:** `crates/mcp-bridge`, `src-tauri`.

**Files to create/modify:**

- `crates/mcp-bridge/src/templates.rs` — add `browse_templates` + its tests (the `TemplateInfo` /
  `TemplateSearch` / `LocalCheck` types already exist here; **do not touch them**)
- `src-tauri/src/catalog.rs` — **new**: `CatalogKind`, `CatalogPage`, `catalog_browse`,
  `catalog_readiness`
- `src-tauri/src/lib.rs` — `mod catalog;` and the two command registrations

---

## Goal

The Setup catalog needs to **list the built-in template gallery by kind** (audio / image, local
rows only) and **report whether one is installed**. This task is the two backend commands behind
that; the browse list UI, the curated one-click set, and adopt-to-profile are **T-505**.

## Why this shape (read before coding — it is the result of live verification, not preference)

Verified live 2026-09-02 ([MCP-SURFACE §32–§33](../docs/MCP-SURFACE.md)):

1. **The gallery is the catalog.** comfy-mcp has no model-hub search. `search_templates` with a
   `type` filter and `exclude_api:true` lists a kind's local rows with **no text query** — image
   returned 163 rows, audio 19. That is the browse surface.
2. **Readiness for a bare gallery row is `local_check`, and nothing else here reproduces it.** A
   not-installed template's `get_template` carries `local_check` with `runnable:false` and an
   `errors` list naming the missing file (`"node 30: 'flux1-schnell-fp8.safetensors' is unavailable
   … for ckpt_name"`). This is the **opposite** of the *models step* (`src-tauri/src/models.rs`),
   which must NOT read `local_check` because a shipped profile's `slot_overrides` make a working
   install report `runnable:false` (the MiniMax lesson). A bare gallery row has **no profile and no
   overrides**, so `local_check` is the honest readiness for it. Keep these two paths separate.
3. **This task does not install anything.** comfy-mcp exposes no download URL for a gallery row
   (§33), so one-click install is only ever the *curated* set, which reuses the existing
   `install.rs` path — that is T-505. Here, readiness is **report-only**.
4. **The verdict (Ready / not-ready / unknown) is derived in the frontend store (T-505), in TS.**
   This command returns the raw `LocalCheck` tri-state, which already serialises with a `state` tag
   and round-trips (its tests are in `templates.rs`). Do not add a second verdict enum in Rust — the
   repo derives display state in the store (`state/queue.ts`, `state/library.ts`).

## Spec

### 1. `crates/mcp-bridge/src/templates.rs` — `browse_templates`

Add this method to the existing `impl LocalComfy` block, right after `search_templates`. It differs
from `search_templates` only in the args it sends: a `type` filter (so a whole kind lists with no
text query), `exclude_api: true` (local rows only — the paid tier is out of v1), and `offset`.

```rust
    /// Browse the gallery for one output type (`audio`/`image`), local rows only.
    ///
    /// Unlike [`search_templates`], the whole kind lists with no text query --
    /// `type` + `exclude_api` is the browse surface the catalog is built on
    /// (MCP-SURFACE 32.1). `query` narrows within the kind when the user types;
    /// `None` lists the kind. `exclude_api` is always true: the paid hosted tier
    /// (`api: true`) is out of the v1 catalog.
    pub async fn browse_templates(
        &self,
        output_type: &str,
        query: Option<&str>,
        offset: u32,
        limit: u32,
    ) -> Result<TemplateSearch, ComfyError> {
        let mut args = Map::new();
        args.insert("type".into(), Value::String(output_type.to_string()));
        args.insert("exclude_api".into(), Value::Bool(true));
        args.insert("offset".into(), Value::Number(offset.into()));
        args.insert("limit".into(), Value::Number(limit.into()));
        if let Some(query) = query {
            args.insert("query".into(), Value::String(query.to_string()));
        }
        self.call("search_templates", args).await
    }
```

**Tests** (add to the existing `mod tests` in `templates.rs`, reusing `client_and_log` / `Reply`):

```rust
    #[tokio::test]
    async fn test_browse_sends_type_and_excludes_api_with_no_query() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "total": 163, "shown": 100, "offset": 0, "rows": []
        }))])
        .await;
        let _: TemplateSearch = client
            .browse_templates("image", None, 0, 100)
            .await
            .expect("browse");
        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("search_templates"));
        assert_eq!(log[0]["arguments"]["type"], json!("image"));
        assert_eq!(log[0]["arguments"]["exclude_api"], json!(true));
        assert_eq!(log[0]["arguments"]["offset"], json!(0));
        assert_eq!(log[0]["arguments"]["limit"], json!(100));
        // No text query means the whole kind lists -- the `query` key must be absent,
        // not an empty string (an empty query is a different comfy-cli code path).
        assert!(log[0]["arguments"].get("query").is_none());
    }

    #[tokio::test]
    async fn test_browse_forwards_a_narrowing_query() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "total": 1, "shown": 1, "offset": 0, "rows": []
        }))])
        .await;
        let _: TemplateSearch = client
            .browse_templates("audio", Some("ace"), 20, 100)
            .await
            .expect("browse");
        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["arguments"]["type"], json!("audio"));
        assert_eq!(log[0]["arguments"]["query"], json!("ace"));
        assert_eq!(log[0]["arguments"]["offset"], json!(20));
    }
```

### 2. `src-tauri/src/catalog.rs` — the two commands (new module)

Mirror `src-tauri/src/models.rs` exactly for structure: the same `ensure_connected` access, the
same "a service problem is a state, not a panic" philosophy. The verdict lives in the frontend, so
these commands stay thin.

```rust
//! The Setup catalog: browse the built-in template gallery, and check whether a
//! gallery row can run here.
//!
//! This is the **gallery** half of the catalog. Readiness here is `local_check`
//! -- the opposite of the *models step* (`models.rs`), which must not read it
//! (MCP-SURFACE 32.2, 33): a shipped profile's `slot_overrides` make a working
//! install report `runnable: false`, but a bare gallery row has no profile and
//! no overrides, so `local_check` is its honest readiness. The curated
//! one-click install set and adopt-to-profile are T-505; nothing here installs.
//!
//! The frontend derives the Ready/Not-ready/Unknown verdict from the returned
//! `LocalCheck` (T-505), the same way `state/queue.ts` derives its rows -- so
//! there is deliberately no second verdict enum in Rust.

use mcp_bridge::{LocalCheck, TemplateInfo, TemplateSearch};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::comfy::{ensure_connected, EnsureError};
use crate::jobs::ComfyState;
use crate::ConfigDir;

/// How many gallery rows one browse call returns. The largest kind (image) is
/// ~163 rows; one page over that is one cached comfy-cli read, so paging is
/// offered but rarely needed.
const CATALOG_PAGE: u32 = 100;

/// Which gallery kind the catalog is showing. Deserialised from the frontend as
/// `"audio"` / `"image"`; the mapping to comfy-mcp's `output_type` is the one
/// place a kind becomes a wire string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogKind {
    Audio,
    Image,
}

impl CatalogKind {
    fn output_type(self) -> &'static str {
        match self {
            CatalogKind::Audio => "audio",
            CatalogKind::Image => "image",
        }
    }
}

/// One page of gallery rows for a kind.
#[derive(Debug, Clone, Serialize)]
pub struct CatalogPage {
    pub rows: Vec<TemplateInfo>,
    /// Matches across the whole kind, so the frontend knows if more pages exist.
    pub total: usize,
    pub offset: usize,
    /// True when comfy-mcp broadened the query past an exact match -- the UI
    /// must say so, or a widened result reads as an exact one (MCP-SURFACE 9.5).
    pub widened: bool,
}

impl From<TemplateSearch> for CatalogPage {
    fn from(search: TemplateSearch) -> Self {
        CatalogPage {
            widened: search.was_widened(),
            rows: search.rows,
            total: search.total,
            offset: search.offset,
        }
    }
}

/// Browse the gallery for one kind, optionally narrowed by a text query.
///
/// Returns `Err` only when the gallery itself cannot be read (comfy-mcp is not
/// installed or would not start) -- there is nothing to render then, so the
/// frontend shows an error with Retry, the `state/library.ts` pattern. A browse
/// does **not** need ComfyUI running: the gallery is cached by comfy-cli
/// independently of the server (MCP-SURFACE 32.1).
#[tauri::command]
pub async fn catalog_browse(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    bin: Option<String>,
    kind: CatalogKind,
    query: Option<String>,
    offset: u32,
) -> Result<CatalogPage, String> {
    let comfy = ensure_connected(&state, &config_dir, bin)
        .await
        .map_err(ensure_detail)?;
    comfy
        .browse_templates(kind.output_type(), query.as_deref(), offset, CATALOG_PAGE)
        .await
        .map(CatalogPage::from)
        .map_err(|e| e.to_string())
}

/// Whether one gallery row can run on this install, as the raw `local_check`
/// tri-state. `Unknown` (no comparison made, usually ComfyUI stopped) is a
/// value, not an error -- the frontend renders it as "can't tell yet", never as
/// "not installed". Only a transport failure is `Err`.
#[tauri::command]
pub async fn catalog_readiness(
    state: State<'_, ComfyState>,
    config_dir: State<'_, ConfigDir>,
    bin: Option<String>,
    name: String,
) -> Result<LocalCheck, String> {
    let comfy = ensure_connected(&state, &config_dir, bin)
        .await
        .map_err(ensure_detail)?;
    let detail = comfy.get_template(&name).await.map_err(|e| e.to_string())?;
    Ok(detail.local_check.unwrap_or(LocalCheck::Unknown))
}

/// Flatten an `ensure_connected` failure to a message. A log failure and a
/// Comfy failure both mean the gallery cannot be read; the string carries which.
fn ensure_detail(error: EnsureError) -> String {
    match error {
        EnsureError::Comfy(e) => e.to_string(),
        EnsureError::Log(detail) => detail,
    }
}
```

**Tests** (a `mod tests` in `catalog.rs`). The command bodies are thin bridge wrappers, so the unit
tests cover the pure pieces (the kind mapping, the page conversion, the serde tags the frontend
depends on); the end-to-end path is an `#[ignore]` live test, exactly as `models.rs` does it.

```rust
#[cfg(test)]
mod tests {
    // `TemplateSearch`, `LocalCheck`, `CatalogKind`, `CatalogPage` all come in
    // via `super::*` (the module's own `use mcp_bridge::{...}` plus its types).
    use super::*;

    #[test]
    fn test_kind_maps_to_output_type() {
        assert_eq!(CatalogKind::Audio.output_type(), "audio");
        assert_eq!(CatalogKind::Image.output_type(), "image");
    }

    /// Protects: the kind crosses the boundary as the wire strings the frontend
    /// sends. A rename to `Title`-case silently breaks every browse call.
    #[test]
    fn test_kind_deserialises_snake_case() {
        let audio: CatalogKind = serde_json::from_value(serde_json::json!("audio")).unwrap();
        assert!(matches!(audio, CatalogKind::Audio));
        let image: CatalogKind = serde_json::from_value(serde_json::json!("image")).unwrap();
        assert!(matches!(image, CatalogKind::Image));
    }

    /// Protects: the widened flag is carried, not dropped. A widened result the
    /// UI cannot distinguish from an exact one is the trap MCP-SURFACE 9.5 names.
    #[test]
    fn test_page_carries_the_widened_flag() {
        let search: TemplateSearch = serde_json::from_value(serde_json::json!({
            "total": 1, "shown": 1, "offset": 0,
            "rows": [{ "name": "image_flux2", "api": false, "output_type": "image" }],
            "match": "all-words"
        }))
        .unwrap();
        let page = CatalogPage::from(search);
        assert!(page.widened);
        assert_eq!(page.total, 1);
        assert_eq!(page.rows[0].name, "image_flux2");

        let exact: TemplateSearch = serde_json::from_value(serde_json::json!({
            "total": 2, "shown": 2, "offset": 0, "rows": []
        }))
        .unwrap();
        assert!(!CatalogPage::from(exact).widened);
    }

    /// The whole path against a real comfy-mcp: browse the image gallery, and
    /// check readiness of a template the machine is unlikely to have installed.
    /// Asserts the SHAPE of the answer, not which models are on the box -- a
    /// browse returns local image rows, and a readiness call reaches a decided
    /// `LocalCheck` (Checked or Unknown, never a panic). Excluded from CI; run
    /// with `cargo test -p app -- --ignored` at the T-505 milestone.
    #[tokio::test]
    #[ignore = "needs comfy-mcp and a running ComfyUI"]
    async fn test_browse_and_readiness_against_a_live_comfyui() {
        let log = mcp_bridge::SessionLog::open(std::env::temp_dir().join("latentcreate-catalog.log"))
            .expect("session log opens");
        let comfy = mcp_bridge::LocalComfy::connect("comfy-mcp", log)
            .await
            .expect("comfy-mcp connects");

        let page: CatalogPage = comfy
            .browse_templates("image", None, 0, CATALOG_PAGE)
            .await
            .expect("browse image")
            .into();
        assert!(page.total > 0, "the image gallery is non-empty");
        assert!(
            page.rows.iter().all(|r| !r.api),
            "exclude_api must drop every hosted row"
        );
        assert!(
            page.rows
                .iter()
                .all(|r| r.output_type.as_deref() == Some("image")),
            "the type filter must keep the page to one kind"
        );

        let check = comfy
            .get_template("flux_schnell")
            .await
            .expect("get_template")
            .local_check
            .unwrap_or(LocalCheck::Unknown);
        // A decided tri-state either way: with ComfyUI up it is Checked, with it
        // down it is Unknown. The point is it never panics and never guesses.
        let _ = check.runnable();
    }
}
```

### 3. `src-tauri/src/lib.rs`

- Add `mod catalog;` alphabetically — **between `mod albums;` and `mod comfy;`** (line ~11).
- Register both commands in `invoke_handler`, next to the other Setup commands (after
  `models::models_status`):

```rust
            catalog::catalog_browse,
            catalog::catalog_readiness,
```

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] `browse_templates` sends `type` + `exclude_api:true` + `offset` + `limit`, and **omits**
      `query` when `None` — the two bridge tests, the first being the flagship (delete the
      `exclude_api` insert and it must fail).
- [ ] `catalog_readiness` returns `LocalCheck::Unknown` — not an `Err` — when `get_template` carries
      no `local_check` (the `.unwrap_or(LocalCheck::Unknown)`), so a stopped ComfyUI reads as
      "can't tell", never "not installed".
- [ ] `CatalogKind` round-trips `"audio"`/`"image"`; `CatalogPage` carries `widened`.
- [ ] `grep -rn "local_check" src-tauri/src/models.rs` still finds **only the doc comment** — this
      task must not make the *models step* read `local_check` (the MiniMax lesson stays intact).
- [ ] No changes outside the three listed files.

## Out of scope (later T-numbers, do not build)

- **The browse UI, the search box, the readiness verdict (Ready/Not-ready/Unknown), and the store**
  — **T-505**, in TS. This task returns the raw `LocalCheck`.
- **The curated one-click install set** — reuses `install.rs` (`models_install`/`models_progress`),
  wired in T-505. Nothing here installs.
- **Adopt-a-gallery-row-into-a-profile** — reuses the T-313 import path, T-505.
- **Cover-art generation** — T-506.
- **Paging past the first `CATALOG_PAGE` rows in the UI** — the command takes `offset`; whether the
  UI pages is T-505's call.
- **Parsing missing-file names out of `local_check.errors`** — the errors are third-party prose
  (§33); the frontend shows them verbatim. No regex over them.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-504-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read docs/MCP-SURFACE.md --read src-tauri/src/models.rs --read src-tauri/src/comfy.rs --read src-tauri/src/jobs.rs --read crates/mcp-bridge/src/local.rs --file crates/mcp-bridge/src/templates.rs --file src-tauri/src/catalog.rs --file src-tauri/src/lib.rs
```

`models.rs`, `comfy.rs`, `jobs.rs` are `--read`: the new command mirrors `models.rs`'s structure and
uses `ensure_connected`/`EnsureError` (`comfy.rs`) and `ComfyState` (`jobs.rs`) without editing them.
`crates/mcp-bridge/src/local.rs` is `--read` for the `test_helpers::client_and_log` rig and
`LocalComfy::connect`/`call` the new code and its tests rely on (WORKFLOW §3: definitions in view,
not editable).
