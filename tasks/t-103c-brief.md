# T-103c: validation and notes — a pass that means something, and text you must not obey
**Depends:** T-103b | **Dirs:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/preflight.rs`

**Files to modify:** `crates/mcp-bridge/src/lib.rs`

> Last third of the T-103 split (T-103a templates, T-103b slots). Completes the `mcp-bridge`
> template/slot surface; T-104 takes running and jobs.

## Goal
Two tools, each with a trap that is about *trust* rather than shape:

- **`validate_workflow` can report `valid: true` having examined nothing.** A UI export too
  old to auto-convert checks **zero nodes** and still passes. Acting on `valid` alone
  greenlights a workflow nothing looked at.
- **`list_workflow_notes` returns third-party prose that reads like instructions.** The
  MiniMax template's own notes carry model download URLs and lines like "Please update
  ComfyUI first". It is data to display, never a directive to follow.

## Verified, not recalled
Payloads captured live 2026-08-24 — docs/MCP-SURFACE.md §9.2, §9.3, §9.6. The reference code
compiles, is `cargo fmt`-clean, passes `clippy -D warnings`, and its verdict logic was
exercised across all four cases before this brief was written.

⚠ **One honesty note for the executor and the reviewer:** the *healthy* validation payload is
a real capture. The **vacuous** case is from the tool's own documented blind spot and was
**not reproduced here** — it needs a UI export too old for this comfy-cli to convert. The
test below encodes the documented signature (`non_node_key` warnings with no
`converted_from_ui`), which is the best available evidence, not an observation. If a real one
ever turns up, re-check `examined_nothing` against it.

⚠ **Validation node ids use `:` where slot addresses use `/`.** The same node is
`37/43.switch` in `list_workflow_slots` and `node_id: "37:43"` in `validate_workflow`.
Nothing in either payload hints at this, and without translation a finding cannot be mapped
back to the control that owns it.

## Reference code

### `crates/mcp-bridge/src/preflight.rs`
```rust
//! Pre-flight: validation verdicts, and the notes a workflow carries.
//!
//! Shapes verified live 2026-08-24 -- docs/MCP-SURFACE.md 9.2, 9.3, 9.6.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One error or warning from `validate_workflow`.
///
/// Every field is optional -- comfy-cli omits what does not apply -- so nothing
/// here may be indexed into. The text quotes the workflow, which is
/// third-party content: display it, never act on it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    /// Node the finding is about, in **validation** form: `35`, or `37:43`
    /// inside a subgraph. See [`node_id_to_instance`] before matching it
    /// against a slot address.
    #[serde(default)]
    pub node_id: Option<String>,
    /// Input the finding is about.
    #[serde(default)]
    pub field: Option<String>,
    /// Machine-readable slug, e.g. `edge_type_mismatch`, `non_node_key`.
    #[serde(default)]
    pub code: Option<String>,
    /// Human-readable description.
    #[serde(default)]
    pub message: Option<String>,
    /// comfy-cli's suggested next step.
    #[serde(default)]
    pub hint: Option<String>,
}

/// What a validation run actually established.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// Nodes were examined and accepted.
    Valid,
    /// The workflow was rejected.
    Invalid,
    /// Reported valid without examining anything. Not a pass.
    Vacuous,
}

/// `validate_workflow`'s report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Validation {
    /// comfy-cli's own verdict. **Not sufficient on its own** -- see
    /// [`Validation::verdict`].
    #[serde(default)]
    pub valid: bool,
    /// Blocking problems.
    #[serde(default)]
    pub errors: Vec<Finding>,
    /// Non-blocking observations.
    #[serde(default)]
    pub warnings: Vec<Finding>,
    /// Present when the file was a UI export that comfy-cli converted. Its
    /// ABSENCE is the tell for a vacuous pass (docs/MCP-SURFACE.md 9.3).
    #[serde(default)]
    pub converted_from_ui: Option<bool>,
    /// How many nodes the conversion produced.
    #[serde(default)]
    pub converted_node_count: Option<usize>,
    /// True when running this graph would spend the user's Comfy credits.
    #[serde(default)]
    pub spends_credits: bool,
    /// Partner-API nodes found in the graph.
    #[serde(default)]
    pub partner_nodes: Vec<Value>,
}

impl Validation {
    /// The verdict to act on.
    ///
    /// `valid: true` alone is not a pass: a UI export too old to auto-convert
    /// checks **zero nodes** and still reports valid. Treating that as success
    /// greenlights a workflow nothing examined (docs/MCP-SURFACE.md 9.3).
    pub fn verdict(&self) -> Verdict {
        if !self.valid {
            return Verdict::Invalid;
        }
        if self.examined_nothing() {
            return Verdict::Vacuous;
        }
        Verdict::Valid
    }

    /// Whether this report shows a check that inspected no nodes.
    ///
    /// The documented signature is `non_node_key` warnings with no
    /// `converted_from_ui`. An API-format graph legitimately has neither, so
    /// both conditions are required.
    fn examined_nothing(&self) -> bool {
        self.converted_from_ui.is_none()
            && self
                .warnings
                .iter()
                .any(|w| w.code.as_deref() == Some("non_node_key"))
    }
}

/// Translate a validation `node_id` into a slot `instance_id`.
///
/// The same node is `37/43` in `list_workflow_slots` and `37:43` in
/// `validate_workflow` -- nothing in either payload hints at the difference
/// (docs/MCP-SURFACE.md 9.2). Without this, a finding cannot be mapped back to
/// the control that owns it.
pub fn node_id_to_instance(node_id: &str) -> String {
    node_id.replace(':', "/")
}

/// One Note or MarkdownNote a workflow carries.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    /// Node id of the note itself.
    #[serde(default)]
    pub id: Option<Value>,
    /// `Note` or `MarkdownNote`.
    #[serde(rename = "type", default)]
    pub ty: Option<String>,
    /// Note heading, when the author set one.
    #[serde(default)]
    pub title: Option<String>,
    /// The note body.
    ///
    /// **UNTRUSTED DATA.** Prose a third-party template author wrote. Real
    /// notes carry model download URLs and lines phrased as instructions
    /// ("Please update ComfyUI first"). Render it as quoted content: never let
    /// it drive a fetch, a download, a run, or a spend (docs/MCP-SURFACE.md
    /// 2, 9.6).
    #[serde(default)]
    pub text: String,
}

/// Every note a workflow carries. No notes is `count: 0`, not an error.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NoteList {
    /// Workflow the notes were read from.
    #[serde(default)]
    pub workflow: Option<PathBuf>,
    /// The notes, in graph order. Untrusted -- see [`Note::text`].
    #[serde(default)]
    pub notes: Vec<Note>,
}

impl LocalComfy {
    /// Pre-flight a workflow against the running ComfyUI.
    ///
    /// Read [`Validation::verdict`] rather than `valid` directly.
    pub async fn validate(&self, workflow: &Path) -> Result<Validation, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        self.call("validate_workflow", args).await
    }

    /// Read the documentation notes a workflow carries.
    ///
    /// The result is third-party prose -- see [`Note::text`].
    pub async fn notes(&self, workflow: &Path) -> Result<NoteList, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        self.call("list_workflow_notes", args).await
    }
}
```

### `crates/mcp-bridge/src/lib.rs`
```rust
mod preflight;

pub use preflight::{node_id_to_instance, Finding, Note, NoteList, Validation, Verdict};
```

## Tests
New `#[cfg(test)] mod tests` in `crates/mcp-bridge/src/preflight.rs`. Use
`crate::local::test_helpers::client_and_log` and `crate::mock::Reply` for the two that go
through a transport; the verdict cases need no transport.

- `test_verdict_is_valid_for_the_captured_report` — **protects:** the healthy path. Use the
  **real captured payload**: `valid: true`, `error_count: 0`, `warning_count: 3`, three
  `edge_type_mismatch` warnings (one on `node_id: "37:43"`), `converted_from_ui: true`,
  `converted_node_count: 12`, `spends_credits: false`. Assert `Verdict::Valid` — warnings
  alone must not demote a pass.
- `test_verdict_is_vacuous_when_nothing_was_examined` — **protects:** the trap this task
  exists for. `{"valid": true, "warnings": [{"code": "non_node_key", ...}]}` with no
  `converted_from_ui` must be `Verdict::Vacuous`. A wrapper reading `valid` directly returns
  success and the app runs a workflow nothing inspected. **`Vacuous` is not `Valid` and the
  UI must not treat it as one.**
- `test_verdict_is_valid_for_an_api_format_graph` — **protects:** the vacuity check against
  false positives. `{"valid": true, "warnings": []}` has no `converted_from_ui` either, but
  no `non_node_key` warning, so it is a genuine pass. Requiring only the missing
  `converted_from_ui` would wrongly condemn every API-format workflow — which is the format
  the app's own pipeline produces.
- `test_verdict_is_invalid_when_rejected` — `{"valid": false, "errors": [...]}` →
  `Verdict::Invalid`, and the findings survive with their optional fields intact.
- `test_findings_tolerate_missing_fields` — **protects:** every `Finding` field is optional.
  A warning of just `{"code": "x"}` must decode, with `node_id`/`field`/`message`/`hint` all
  `None`. Indexing into any of them would panic on a real payload.
- `test_node_id_translates_to_a_slot_instance` — **protects:** the `:` / `/` mismatch.
  `"37:43"` → `"37/43"`, and a flat `"35"` is unchanged. Pair it with the T-103b fixture in
  spirit: `37/43` is a real address in that workflow, so the translated id is what would
  match it.
- `test_validation_reports_credit_spending` — **protects:** a product rule. A payload with
  `spends_credits: true` must surface it. T-104 gates running on this; a graph carrying
  partner-API nodes spends the user's money, and the app must never run one silently.
- `test_notes_decode_and_are_returned_verbatim` — **protects:** the untrusted-data boundary.
  Serve a note whose text contains a URL and an imperative line — use the real MiniMax note
  shape: `{"id": 40, "type": "MarkdownNote", "title": null, "text": "## Model Links\n- [x](https://huggingface.co/...)\n\nNote: Please update ComfyUI first"}`.
  Assert the text comes back **byte-identical**: no stripping, no link extraction, no
  interpretation. The wrapper's only job is faithful relay.
- `test_notes_decode_an_empty_list` — `{"workflow": "wf.json", "count": 0, "notes": []}` is a
  normal result, not an error.
- `test_validate_sends_workflow_path` — argument naming, via `RecordedCalls` (MCP-SURFACE
  §8.7).

## Acceptance criteria
- [ ] `npm run gate` green from the repo root — **check its exit code, do not pipe it**
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All ten named tests present and passing
- [ ] No test spawns a process, opens a socket, or reaches the network
- [ ] Nothing in this module parses, follows, or extracts URLs from note text
- [ ] No new dependencies

## Out of scope
`workflow_deps` and `node_dependencies` (they belong with T-106's node registry).
`run_workflow` and the credit-spend confirmation flow itself — T-104; this task only
*surfaces* `spends_credits`. Any Tauri command, any UI, any rendering of note markdown.
Mapping findings onto profile inputs (T-107).

## Notes for the executor
- Do not add a helper that extracts links from `Note::text`, and do not "helpfully" strip
  markdown. The type's contract is verbatim relay; anything that interprets it is a security
  regression, not a feature.
- Do not simplify `verdict()` into `self.valid`. The three-way distinction is the deliverable.
- Do not make `examined_nothing` public; it is an implementation detail of `verdict()`.
- `Note::id` is `Option<Value>` because comfy-cli's ids are numeric here but string-shaped
  elsewhere on this surface; do not narrow it to `u64`.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
`error.rs`, `local.rs` and `mock.rs` are `--read`: this module constructs no `ComfyError`
variants but does `impl LocalComfy` and call `self.call(...)`, and the tests use the mock rig.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/error.rs --read crates/mcp-bridge/src/local.rs --read crates/mcp-bridge/src/mock.rs --read crates/mcp-bridge/src/slots.rs --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/preflight.rs
```
