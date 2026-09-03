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
//! `LocalCheck` (T-505), the same way `state/library.ts` derives its rows -- so
//! there is deliberately no second verdict enum in Rust.

use std::path::Path;

use mcp_bridge::{LocalCheck, TemplateInfo, TemplateSearch};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::comfy::{ensure_connected, EnsureError};
use crate::import::{import_into, ImportReport};
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

/// Flatten an `ensure_connected` failure to a message. A log failure and a
/// Comfy failure both mean the gallery cannot be read; the string carries which.
fn ensure_detail(error: EnsureError) -> String {
    match error {
        EnsureError::Comfy(e) => e.to_string(),
        EnsureError::Log(detail) => detail,
    }
}

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
        let log =
            mcp_bridge::SessionLog::open(std::env::temp_dir().join("latentcreate-catalog.log"))
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

    /// The adopt seam against a real comfy-mcp: fetch a template and drive the
    /// whole import path. Asserts only the shape of the report, not which models
    /// are installed. Excluded from CI; run with `cargo test -p app -- --ignored`
    /// at the T-505 milestone.
    #[tokio::test]
    #[ignore = "needs comfy-mcp and a running ComfyUI"]
    async fn test_adopt_against_a_live_comfyui() {
        let log =
            mcp_bridge::SessionLog::open(std::env::temp_dir().join("latentcreate-catalog.log"))
                .expect("session log opens");
        let comfy = mcp_bridge::LocalComfy::connect("comfy-mcp", log)
            .await
            .expect("comfy-mcp connects");

        let tmp = tempfile::tempdir().expect("tempdir");
        let fetched = tmp.path().join("audio_ace_step_1_5_split.json");
        comfy
            .fetch_template("audio_ace_step_1_5_split", &fetched)
            .await
            .expect("fetch a real template");

        let report = adopt_from_fetched(&comfy, tmp.path(), &fetched)
            .await
            .expect("adopt produces a report");
        assert!(!report.workflow_id.is_empty());
        assert!(!report.slots.is_empty());
    }
}

#[cfg(test)]
mod adopt_tests {
    use mcp_bridge::mock::Reply;
    use mcp_bridge::test_helpers::client_and_log;
    use serde_json::json;

    use super::adopt_from_fetched;

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
        assert!(!report.slots.is_empty());
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
        assert!(
            !fetched.exists(),
            "the fetch temp must be removed on failure too"
        );
    }
}
