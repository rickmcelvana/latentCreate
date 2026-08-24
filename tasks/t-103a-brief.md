# T-103a: templates — search, inspect, fetch, and the `local_check` tri-state
**Depends:** T-102 | **Dirs:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/templates.rs`

**Files to modify:** `crates/mcp-bridge/src/lib.rs`, `crates/mcp-bridge/src/local.rs`

> **T-103 is split.** This is the template half: find a template, inspect it, get it onto
> disk, and know whether this install can run it. **T-103b** takes slots, `set_workflow_slot`,
> `validate_workflow` and notes, and is briefed after this lands. Six tools in one run would
> blow the ~400-line limit (WORKFLOW §2).

## Goal
The wrappers the setup wizard and profile authoring both sit on. All three tools return
JSON-in-text like everything else on this surface, so they are ordinary `call` sites — the
work here is **the types**, and one of them is where this task's real risk lives.

## This surface is verified, not recalled
Captured live on 2026-08-24 against the owner's install; shapes and traps recorded in
**docs/MCP-SURFACE.md §9.4–9.5**. The reference code below compiles, is `cargo fmt`-clean,
passes `clippy -D warnings`, and its `LocalCheck` decoding was tested against all four real
input shapes.

⚠ **`local_check` is a tri-state, not a boolean.** `{"checked": false}` means the comparison
**could not be made** — usually ComfyUI is not running — and carries **no `runnable` key at
all**. The obvious modelling, `#[serde(default)] runnable: bool`, silently reads "unknown"
as "cannot run" and sends the user off to fix a problem they do not have. On a drifted
payload the `local_check` key is absent entirely, which is *also* unknown. Three distinct
inputs, one honest answer: `None`.

⚠ **`match: "all-words"` means the query was broadened.** `search_templates` runs an exact
phrase pass first; only if that finds nothing does it retry with all-words, flagging it in
the reply. Dropping that field presents a widened result as an exact match.

## Reference code

### `crates/mcp-bridge/src/templates.rs`
```rust
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

/// Wire shape of `local_check`, before it becomes the tri-state above.
#[derive(Debug, Clone, Deserialize)]
struct RawLocalCheck {
    #[serde(default)]
    checked: bool,
    #[serde(default)]
    runnable: Option<bool>,
    #[serde(default)]
    summary: Option<String>,
    #[serde(default)]
    errors: Vec<Value>,
}

impl From<RawLocalCheck> for LocalCheck {
    fn from(raw: RawLocalCheck) -> Self {
        match (raw.checked, raw.runnable) {
            (true, Some(runnable)) => LocalCheck::Checked {
                runnable,
                summary: raw.summary,
                errors: raw.errors,
            },
            // checked-but-no-verdict is drift, and is treated as unknown
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
```

### `crates/mcp-bridge/src/lib.rs`
```rust
mod templates;

pub use templates::{FetchedTemplate, LocalCheck, TemplateDetail, TemplateInfo, TemplateSearch};
```
Place `mod templates;` with the other module declarations and the `pub use` with the others.

## Tests
New `#[cfg(test)] mod tests` in `crates/mcp-bridge/src/templates.rs`. The first four need no
transport — build the value with `serde_json::from_value` directly. The last three use the
T-102 rig; make `client_and_log` reachable by moving it to a shared test helper, or
duplicate the four-line helper locally, whichever is smaller.

**Verified inputs — use these exactly, they are captured payloads:**

- `test_local_check_reads_a_real_runnable_verdict` — **protects:** the happy path stays
  readable. From `{"checked": true, "runnable": true, "summary": "...", "error_count": 0,
  "errors": []}`, assert `runnable() == Some(true)`.
- `test_local_check_reads_a_not_runnable_verdict` — `{"checked": true, "runnable": false,
  "errors": [{"x": 1}]}` → `Some(false)`, and the errors survive.
- `test_local_check_unknown_is_not_false` — **protects:** the trap this task exists for.
  `{"checked": false}` must give `runnable() == None`. Modelling `runnable` as a plain
  `bool` yields `Some(false)` here and the wizard tells the user their install cannot run a
  template it has never examined. **`None` and `Some(false)` are different answers and the
  UI shows different things for them.**
- `test_local_check_checked_without_a_verdict_is_unknown` — `{"checked": true}` with no
  `runnable` key is drift; it must be `None`, not a guess.
- `test_fetched_template_without_local_check_is_unknown` — `{"path": "C:/x/wf.json"}` alone
  decodes, with `local_check == None`.
- `test_search_flags_a_widened_query` — **protects:** honesty about match quality. A payload
  carrying `"match": "all-words"` gives `was_widened() == true`; one without the field gives
  `false`. Serve both through the mock.
- `test_search_rows_carry_the_api_flag` — **protects:** the paid/free distinction. Use the
  real captured row for `audio_ace_step1_5_xl_turbo` (`api: false`, tags `["Music",
  "Text to Music"]`, `output_type: "audio"`); assert `api` decodes and `tags` survive.
  Free and paid rows can share a title, so this flag is the only thing separating them.
- `test_fetch_template_sends_name_and_out_path_verbatim` — **protects:** argument naming on
  a surface that rejects a misspelling outright (docs/MCP-SURFACE.md §8.7). Using T-102's
  `RecordedCalls`, assert the outgoing `arguments` carry exactly `name` and `out_path`.
  Nothing about the *response* can catch this.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All eight named tests present and passing
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] `LocalCheck` has no method returning a bare `bool`
- [ ] No new dependencies

## Out of scope
`list_workflow_slots`, `set_workflow_slot`, `validate_workflow`, `list_workflow_notes` — all
T-103b. A `Slot` type or address parsing. Any Tauri command, any UI, any profile loading.
Caching template results. `search_templates`' other filters (`tag`/`type`/`model`/
`provider`/`exclude_api`) — add them when a caller needs them, not speculatively.

## Notes for the executor
- Do not "simplify" `LocalCheck` into a struct with a `bool`. The three-way distinction is
  the deliverable, and the `From<RawLocalCheck>` conversion is what enforces it.
- `#[serde(from = ...)]` and `#[serde(tag = ...)]` coexist deliberately: the wire shape comes
  in through `RawLocalCheck`, while the tag gives the frontend a clean discriminated union
  (`{"state":"checked",...}` / `{"state":"unknown"}`) instead of serde's default
  `{"Checked":{...}}` / `"Unknown"`, which is a string in one arm and an object in the other.
- `error_count` is deliberately not modelled; `errors.len()` is the same number and cannot
  disagree with itself.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/mock.rs --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/local.rs --file crates/mcp-bridge/src/templates.rs
```
