//! Template gallery: search, inspect, fetch.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 9.4-9.5.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One row from the template gallery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateInfo {
    /// Gallery id, e.g. `audio_ace_step1_5_xl_turbo`. The key every other
    /// template tool takes.
    pub name: String,
    /// Human title, e.g. `ACE-Step 1.5XL Turbo: Text to Music`.
    #[serde(default)]
    pub title: String,
    /// One-paragraph blurb; may be truncated mid-sentence by the gallery.
    #[serde(default)]
    pub description: String,
    /// `audio`, `image`, `video`, ... Absent on some rows.
    #[serde(default)]
    pub output_type: Option<String>,
    /// Gallery tags. `API` here means the row runs on paid hosted infrastructure.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Section the gallery files this under, e.g. `Audio`.
    #[serde(default)]
    pub category_title: Option<String>,
    /// True when this row spends the user's Comfy credits rather than running
    /// locally. Free and paid siblings can share a title, so this flag -- not
    /// the title -- is what tells them apart.
    #[serde(default)]
    pub api: bool,
}

/// A page of template search results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSearch {
    /// Matches across the whole gallery, not just this page.
    #[serde(default)]
    pub total: usize,
    /// Rows in this page.
    #[serde(default)]
    pub shown: usize,
    /// Offset this page starts at.
    #[serde(default)]
    pub offset: usize,
    /// The page itself.
    #[serde(default)]
    pub rows: Vec<TemplateInfo>,
    /// `Some("all-words")` when the exact-phrase pass found nothing and the
    /// query was broadened. The UI must say so -- otherwise a widened result
    /// reads as an exact one (docs/MCP-SURFACE.md 9.5).
    #[serde(rename = "match", default)]
    pub match_kind: Option<String>,
}

impl TemplateSearch {
    /// Whether these results came from a broadened query.
    pub fn was_widened(&self) -> bool {
        self.match_kind.as_deref() == Some("all-words")
    }
}

/// Whether this install can run a given template.
///
/// A tri-state, never a boolean. `{"checked": false}` means the comparison
/// could not be made -- usually ComfyUI is not running -- and carries no
/// `runnable` key at all. Reading that as "cannot run" sends the user to fix a
/// problem they do not have (docs/MCP-SURFACE.md 9.4).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(from = "RawLocalCheck", tag = "state", rename_all = "snake_case")]
pub enum LocalCheck {
    /// The graph was compared against the live local `object_info`.
    Checked {
        /// True when every node class and input option is present here.
        runnable: bool,
        /// comfy-cli's own prose summary of the verdict.
        summary: Option<String>,
        /// What is missing, when it is not runnable. Third-party content.
        errors: Vec<Value>,
    },
    /// No comparison was made. Not a verdict.
    Unknown,
}

impl LocalCheck {
    /// `Some(true)`/`Some(false)` only when a comparison actually ran.
    ///
    /// Returns `None` for [`LocalCheck::Unknown`], so a caller cannot collapse
    /// "unknown" into "no" without saying so.
    pub fn runnable(&self) -> Option<bool> {
        match self {
            LocalCheck::Checked { runnable, .. } => Some(*runnable),
            LocalCheck::Unknown => None,
        }
    }
}

/// Everything [`LocalCheck`] accepts on the way in.
///
/// Two shapes, deliberately: comfy-mcp's own (`checked` + `runnable`), and
/// this enum's serialized form (`state`). Without the second, the type does
/// not survive its own round trip -- serialising `Checked { runnable: true }`
/// and reading it back yields `Unknown`, silently, which is exactly the
/// misreport the tri-state exists to prevent.
#[derive(Debug, Clone, Deserialize)]
struct RawLocalCheck {
    /// comfy-mcp's flag: was the comparison actually made?
    #[serde(default)]
    checked: bool,
    /// This enum's own tag, present only when re-reading our output.
    #[serde(default)]
    state: Option<String>,
    #[serde(default)]
    runnable: Option<bool>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    errors: Vec<Value>,
}

impl From<RawLocalCheck> for LocalCheck {
    fn from(raw: RawLocalCheck) -> Self {
        let compared = match raw.state.as_deref() {
            Some(tag) => tag == "checked",
            None => raw.checked,
        };
        match (compared, raw.runnable) {
            (true, Some(runnable)) => LocalCheck::Checked {
                runnable,
                summary: raw.summary,
                errors: raw.errors,
            },
            // compared-but-no-verdict is drift, and is treated as unknown
            // rather than guessed at.
            _ => LocalCheck::Unknown,
        }
    }
}

/// `fetch_template` result: where the workflow landed, and whether it can run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FetchedTemplate {
    /// Absolute path of the workflow JSON that was written.
    pub path: PathBuf,
    /// Absent entirely on a drifted payload -- also "unknown".
    #[serde(default)]
    pub local_check: Option<LocalCheck>,
}

/// `get_template` result: gallery metadata plus the same tri-state check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateDetail {
    /// The gallery row.
    pub template: TemplateInfo,
    /// Absent entirely on a drifted payload.
    #[serde(default)]
    pub local_check: Option<LocalCheck>,
}

impl LocalComfy {
    /// Search the built-in ComfyUI template gallery.
    ///
    /// Check [`TemplateSearch::was_widened`] on the result before presenting
    /// rows as exact matches.
    pub async fn search_templates(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<TemplateSearch, ComfyError> {
        let mut args = Map::new();
        args.insert("query".into(), Value::String(query.to_string()));
        args.insert("limit".into(), Value::Number(limit.into()));
        self.call("search_templates", args).await
    }

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

    /// Inspect one template, writing nothing to disk.
    pub async fn get_template(&self, name: &str) -> Result<TemplateDetail, ComfyError> {
        let mut args = Map::new();
        args.insert("name".into(), Value::String(name.to_string()));
        self.call("get_template", args).await
    }

    /// Write a template's runnable workflow JSON to `out_path`.
    ///
    /// The returned [`LocalCheck`] is the gate before running it: the gallery
    /// catalog is cached independently of the install, so a successful fetch is
    /// not evidence the graph can run here.
    pub async fn fetch_template(
        &self,
        name: &str,
        out_path: &Path,
    ) -> Result<FetchedTemplate, ComfyError> {
        let mut args = Map::new();
        args.insert("name".into(), Value::String(name.to_string()));
        args.insert(
            "out_path".into(),
            Value::String(out_path.display().to_string()),
        );
        self.call("fetch_template", args).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::local::test_helpers::client_and_log;
    use crate::mock::Reply;
    use crate::templates::{FetchedTemplate, LocalCheck, TemplateSearch};

    #[test]
    fn test_local_check_reads_a_real_runnable_verdict() {
        let raw = json!({
            "checked": true,
            "runnable": true,
            "summary": "All nodes present",
            "error_count": 0,
            "errors": []
        });
        let check: LocalCheck = serde_json::from_value(raw).expect("decodes");
        assert_eq!(check.runnable(), Some(true));
    }

    #[test]
    fn test_local_check_reads_a_not_runnable_verdict() {
        let raw = json!({
            "checked": true,
            "runnable": false,
            "errors": [{"x": 1}]
        });
        let check: LocalCheck = serde_json::from_value(raw).expect("decodes");
        assert_eq!(check.runnable(), Some(false));
        match check {
            LocalCheck::Checked { errors, .. } => assert_eq!(errors.len(), 1),
            other => panic!("expected Checked, got {:?}", other),
        }
    }

    #[test]
    fn test_local_check_unknown_is_not_false() {
        let raw = json!({ "checked": false });
        let check: LocalCheck = serde_json::from_value(raw).expect("decodes");
        assert_eq!(check.runnable(), None);
    }

    #[test]
    fn test_local_check_checked_without_a_verdict_is_unknown() {
        let raw = json!({ "checked": true });
        let check: LocalCheck = serde_json::from_value(raw).expect("decodes");
        assert_eq!(check.runnable(), None);
    }

    #[test]
    fn test_fetched_template_without_local_check_is_unknown() {
        let raw = json!({ "path": "C:/x/wf.json" });
        let fetched: FetchedTemplate = serde_json::from_value(raw).expect("decodes");
        assert_eq!(fetched.path, std::path::PathBuf::from("C:/x/wf.json"));
        assert!(fetched.local_check.is_none());
    }

    /// Protects: the verdict must survive crossing the Tauri boundary and
    /// coming back. `LocalCheck` reads comfy-mcp's `checked`/`runnable` but
    /// serialises a `state` tag for the frontend, so without the second input
    /// shape a re-read silently degrades `Some(true)` to `None` -- the type
    /// misreporting its own output, which is what it exists to prevent.
    /// CONVENTIONS requires boundary types to round-trip their fixtures.
    #[test]
    fn test_local_check_survives_its_own_round_trip() {
        for wire in [
            json!({"checked": true, "runnable": true, "summary": "ok", "errors": []}),
            json!({"checked": true, "runnable": false, "errors": [{"x": 1}]}),
            json!({"checked": false}),
        ] {
            let decoded: LocalCheck = serde_json::from_value(wire).expect("decodes");
            let reserialized = serde_json::to_value(&decoded).expect("serializes");
            let back: LocalCheck = serde_json::from_value(reserialized).expect("re-decodes");
            assert_eq!(back.runnable(), decoded.runnable());
        }
    }

    /// Protects: `get_template` and `TemplateDetail` are otherwise never
    /// exercised -- the nested `template` row plus the same tri-state.
    #[tokio::test]
    async fn test_get_template_decodes_detail_and_check() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "template": {
                "name": "audio_ace_step1_5_xl_turbo",
                "title": "ACE-Step 1.5XL Turbo: Text to Music",
                "output_type": "audio",
                "tags": ["Music", "Text to Music"],
                "category_title": "Audio"
            },
            "local_check": {
                "checked": true,
                "runnable": true,
                "summary": "every node class and input option this template uses is present",
                "error_count": 0,
                "errors": []
            }
        }))])
        .await;

        let detail = client
            .get_template("audio_ace_step1_5_xl_turbo")
            .await
            .expect("detail decodes");
        assert_eq!(detail.template.name, "audio_ace_step1_5_xl_turbo");
        assert_eq!(detail.template.output_type.as_deref(), Some("audio"));
        // `api` is absent from a get_template row and must default to false,
        // not fail the decode.
        assert!(!detail.template.api);
        assert_eq!(
            detail.local_check.as_ref().and_then(LocalCheck::runnable),
            Some(true)
        );

        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log[0]["name"], json!("get_template"));
        assert_eq!(
            log[0]["arguments"]["name"],
            json!("audio_ace_step1_5_xl_turbo")
        );
    }

    #[tokio::test]
    async fn test_search_flags_a_widened_query() {
        let (client, _recorded) = client_and_log(vec![
            Reply::Json(json!({
                "total": 1,
                "shown": 1,
                "offset": 0,
                "rows": [{ "name": "widened", "api": false }],
                "match": "all-words"
            })),
            Reply::Json(json!({
                "total": 1,
                "shown": 1,
                "offset": 0,
                "rows": [{ "name": "exact", "api": false }]
            })),
        ])
        .await;
        let widened: TemplateSearch = client
            .search_templates("x", 10)
            .await
            .expect("widened search");
        assert!(widened.was_widened());
        let exact: TemplateSearch = client
            .search_templates("y", 10)
            .await
            .expect("exact search");
        assert!(!exact.was_widened());
    }

    #[tokio::test]
    async fn test_search_rows_carry_the_api_flag() {
        let row = json!({
            "name": "audio_ace_step1_5_xl_turbo",
            "title": "ACE-Step 1.5XL Turbo: Text to Music",
            "description": "Text-to-music generation",
            "output_type": "audio",
            "tags": ["Music", "Text to Music"],
            "category_title": "Audio",
            "api": false
        });
        let (client, _recorded) = client_and_log(vec![Reply::Json(json!({
            "total": 1,
            "shown": 1,
            "offset": 0,
            "rows": [row]
        }))])
        .await;
        let result: TemplateSearch = client.search_templates("ace", 10).await.expect("search");
        assert_eq!(result.rows.len(), 1);
        let first = &result.rows[0];
        assert_eq!(first.name, "audio_ace_step1_5_xl_turbo");
        assert!(!first.api);
        assert_eq!(first.output_type.as_deref(), Some("audio"));
        assert_eq!(first.tags, vec!["Music", "Text to Music"]);
    }

    #[tokio::test]
    async fn test_fetch_template_sends_name_and_out_path_verbatim() {
        let (client, recorded) = client_and_log(vec![Reply::Json(json!({
            "path": "C:/out/wf.json",
            "local_check": { "checked": true, "runnable": true, "errors": [] }
        }))])
        .await;
        let out = std::path::PathBuf::from("C:/out/wf.json");
        let _fetched = client
            .fetch_template("audio_ace_step1_5_xl_turbo", &out)
            .await
            .expect("fetch");
        let log = recorded.lock().expect("recorded calls");
        assert_eq!(log.len(), 1);
        assert_eq!(log[0]["name"], json!("fetch_template"));
        assert_eq!(
            log[0]["arguments"]["name"],
            json!("audio_ace_step1_5_xl_turbo")
        );
        assert_eq!(log[0]["arguments"]["out_path"], json!("C:/out/wf.json"));
    }

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
}
