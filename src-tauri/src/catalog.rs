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
}
