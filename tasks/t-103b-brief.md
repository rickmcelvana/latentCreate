# T-103b: slots — reading parameters, and writing them without losing the write
**Depends:** T-103a | **Dirs:** `crates/mcp-bridge/` | **Executor:** Aider

**Files to create:** `crates/mcp-bridge/src/slots.rs`

**Files to modify:** `crates/mcp-bridge/src/lib.rs`

> **Second half of the T-103 split.** T-103a took templates. This is the parameter
> mechanism: `list_workflow_slots` and `set_workflow_slot`. **T-103c** takes
> `validate_workflow` and `list_workflow_notes` — four tools here would have run ~470 lines,
> over the ~400 limit (WORKFLOW §2). Everything T-103c needs is already captured in
> MCP-SURFACE §9.2, §9.3 and §9.6; it needs no further live work.

## Goal
The mechanism the whole parameter panel is built on (ARCHITECTURE §3a): read every tweakable
widget as a stable address, and write a whole parameter set back in one call. **The app never
parses or rewrites graph JSON to change a parameter.**

## The write path has a trap that fails silently
Verified live 2026-08-24 — docs/MCP-SURFACE.md §9.1. Reference code below compiles, is
`cargo fmt`-clean, passes `clippy -D warnings`, and all three guards were exercised against
the T-102 mock before this brief was written.

1. ⚠ **`set_workflow_slot` does not write by default.** `stdout` defaults to **`true`**,
   which is *non-destructive*: it **returns** the modified workflow instead of saving it.
   The response still lists the addresses it applied. A wrapper built on the defaults looks
   successful, reports every address, and changes nothing on disk. **Always send
   `stdout: false`.**
2. ⚠ **Only the structured override form is safe.** `{"address": ..., "value": ...}`
   preserves type exactly. The string form `"address=value"` is **parsed as JSON**, so
   `"x.y=true"` writes a boolean and `"x.y=123"` an integer. A user's lyric or caption that
   happens to read as JSON would be silently retyped — and CONVENTIONS forbids modifying
   user text without an explicit accept step.
3. ✅ **A bad address fails the whole batch atomically.** Verified by inspecting the file
   after a mixed valid/invalid call: it wrote nothing, and the previously-applied values were
   untouched. So send a complete parameter set in one call; there is no partial-write
   recovery to write.

Because 1 and 2 are invisible in a passing response, `set_slots` **verifies its own write**:
a reply with no `wrote` path, or an address missing from `applied`, is an error rather than a
success. comfy-mcp reports the latter with an empty `warnings`, so silence is not
confirmation.

## Reference code

### `crates/mcp-bridge/src/slots.rs`
```rust
//! Slots: the parameter mechanism, read and write.
//!
//! Shapes and traps verified live 2026-08-24 -- docs/MCP-SURFACE.md 9.1.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::{ComfyError, LocalComfy};

/// One agent-tweakable widget on a workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slot {
    /// Stable address: flat `35.filename_prefix`, or subgraph
    /// `37/6.unet_name`. This is what `set_workflow_slot` takes.
    pub address: String,
    /// Input name -- the part after the last `.`.
    pub name: String,
    /// ComfyUI input type: `STRING`, `INT`, `FLOAT`, `COMBO`, `BOOLEAN`.
    #[serde(rename = "type")]
    pub ty: String,
    /// The value currently baked into the graph.
    pub current_value: Value,
    /// Node instance: `35`, or `37/6` inside a subgraph.
    pub instance_id: String,
    /// Node class, e.g. `UNETLoader`.
    pub node_type: String,
}

/// Every slot a workflow exposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotList {
    /// Workflow the slots were read from.
    #[serde(default)]
    pub workflow: Option<PathBuf>,
    /// comfy-cli's id for the workflow, e.g. `minimax_music3_int8`.
    #[serde(default)]
    pub id: Option<String>,
    /// The slots themselves.
    #[serde(default)]
    pub slots: Vec<Slot>,
}

impl SlotList {
    /// Find one slot by its exact address.
    pub fn get(&self, address: &str) -> Option<&Slot> {
        self.slots.iter().find(|s| s.address == address)
    }

    /// Addresses in `wanted` that this workflow does not expose.
    ///
    /// A profile naming a slot the template no longer has is the drift T-107
    /// has to report; the gallery is cached with a 24 h TTL and does move.
    pub fn missing<'a>(&self, wanted: &[&'a str]) -> Vec<&'a str> {
        wanted
            .iter()
            .filter(|a| self.get(a).is_none())
            .copied()
            .collect()
    }
}

/// Split a slot address into `(instance_id, input_name)`.
///
/// Splits on the LAST `.`, because subgraph instance ids contain `/` but never
/// `.` -- 24 of the 25 slots in the MiniMax fixture are subgraph-form, so a
/// parser that splits on the first separator mishandles almost all of a real
/// workflow.
pub fn split_address(address: &str) -> Option<(&str, &str)> {
    let idx = address.rfind('.')?;
    let (instance, name) = (&address[..idx], &address[idx + 1..]);
    if instance.is_empty() || name.is_empty() {
        None
    } else {
        Some((instance, name))
    }
}

/// One parameter write.
///
/// Always the structured form. comfy-mcp also accepts `"addr=value"` strings,
/// but parses those as JSON and therefore COERCES -- a lyric or caption that
/// happens to read as `true` or `123` would be silently retyped
/// (docs/MCP-SURFACE.md 9.1).
#[derive(Debug, Clone, Serialize)]
pub struct SlotOverride {
    /// Target address, from [`Slot::address`].
    pub address: String,
    /// Value to write. Type is preserved exactly as given.
    pub value: Value,
}

impl SlotOverride {
    /// Build one override for `address`.
    pub fn new(address: impl Into<String>, value: Value) -> Self {
        Self {
            address: address.into(),
            value,
        }
    }
}

/// Result of a successful parameter write.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlotWrite {
    /// Addresses comfy-mcp confirms it applied.
    #[serde(default)]
    pub applied: Vec<String>,
    /// Non-fatal notes. Third-party content.
    #[serde(default)]
    pub warnings: Vec<Value>,
    /// File that was written. Absent when the call did not persist -- which
    /// for this wrapper means something is wrong, since it always sends
    /// `stdout: false`.
    #[serde(default)]
    pub wrote: Option<PathBuf>,
}

impl LocalComfy {
    /// Read every slot a workflow exposes.
    pub async fn list_slots(&self, workflow: &Path) -> Result<SlotList, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        self.call("list_workflow_slots", args).await
    }

    /// Write parameter values into a workflow file, in one atomic call.
    ///
    /// Sends `stdout: false`, without which comfy-mcp **returns** the modified
    /// workflow instead of saving it, reporting the addresses it applied while
    /// changing nothing on disk (docs/MCP-SURFACE.md 9.1).
    ///
    /// A bad address fails the whole batch and writes nothing, so the caller
    /// may send a complete parameter set and needs no partial-write recovery.
    pub async fn set_slots(
        &self,
        workflow: &Path,
        overrides: &[SlotOverride],
    ) -> Result<SlotWrite, ComfyError> {
        let mut args = Map::new();
        args.insert(
            "workflow_path".into(),
            Value::String(workflow.display().to_string()),
        );
        args.insert(
            "overrides".into(),
            serde_json::to_value(overrides).map_err(|e| ComfyError::Payload {
                tool: "set_workflow_slot".to_string(),
                detail: e.to_string(),
            })?,
        );
        args.insert("stdout".into(), Value::Bool(false));

        let write: SlotWrite = self.call("set_workflow_slot", args).await?;
        confirm_persisted(&write, overrides)?;
        Ok(write)
    }
}

/// Reject a write that did not actually land.
///
/// Two ways it can fail quietly: no `wrote` path at all (the call did not
/// persist), or an address that never appears in `applied`. comfy-mcp reports
/// the latter with an empty `warnings`, so silence is not confirmation.
fn confirm_persisted(write: &SlotWrite, overrides: &[SlotOverride]) -> Result<(), ComfyError> {
    if write.wrote.is_none() {
        return Err(ComfyError::Tool {
            tool: "set_workflow_slot".to_string(),
            code: Some("not_persisted".to_string()),
            message: "the workflow was not written to disk".to_string(),
        });
    }
    let unapplied: Vec<&str> = overrides
        .iter()
        .map(|o| o.address.as_str())
        .filter(|a| !write.applied.iter().any(|done| done == a))
        .collect();
    if !unapplied.is_empty() {
        return Err(ComfyError::Tool {
            tool: "set_workflow_slot".to_string(),
            code: Some("not_applied".to_string()),
            message: format!("these addresses were not applied: {}", unapplied.join(", ")),
        });
    }
    Ok(())
}
```

### `crates/mcp-bridge/src/lib.rs`
```rust
mod slots;

pub use slots::{split_address, Slot, SlotList, SlotOverride, SlotWrite};
```

## Tests
New `#[cfg(test)] mod tests` in `crates/mcp-bridge/src/slots.rs`, using
`crate::local::test_helpers::client_and_log` and `crate::mock::Reply` as T-103a does.

- `test_slots_decode_from_the_captured_fixture` — **protects:** the shape the whole parameter
  panel reads. Serve `testdata/mcp/list_workflow_slots.minimax.json` (via `include_str!`,
  parsed to a `Value`) as `Reply::Json`; assert 25 slots decode, that
  `get("37/6.unet_name")` has `ty == "COMBO"` and its `current_value` is the int8
  filename, and that `get("35.filename_prefix")` has `ty == "STRING"`. This is a
  live-captured payload — do not hand-trim it.
- `test_split_address_handles_both_forms` — **protects:** the case 24 of the 25 real slots
  are in. `"35.filename_prefix"` → `("35", "filename_prefix")`; `"37/6.unet_name"` →
  `("37/6", "unet_name")`; `"nodot"` → `None`. A parser splitting on the FIRST separator
  returns `("37", "6.unet_name")` and passes nothing here.
- `test_slot_list_reports_missing_addresses` — **protects:** T-107's drift check. Against the
  fixture, `missing(&["37/6.unet_name", "94.tags"])` returns exactly `["94.tags"]` — a real
  ACE-Step address that this MiniMax workflow does not have.
- `test_set_slots_sends_stdout_false` — **protects:** the whole write path. Assert the
  outgoing `arguments["stdout"]` is `false`. Without it comfy-mcp returns the modified
  workflow and saves nothing, while still reporting every address as applied — the wrapper
  would report success and the user's parameters would never reach the graph.
- `test_set_slots_sends_structured_overrides_preserving_type` — **protects:** user text
  against silent retyping. Send `SlotOverride::new("37/13.caption", json!("true"))` and
  assert the outgoing `arguments["overrides"][0]` is
  `{"address": "37/13.caption", "value": "true"}` with `value` still a **JSON string**. The
  string form comfy-mcp also accepts would turn that caption into a boolean.
- `test_set_slots_rejects_a_reply_that_did_not_persist` — **protects:** trap 1 at runtime, not
  just at the call site. Serve exactly what `stdout: true` returns — `applied` populated,
  **no `wrote` key** — and assert `ComfyError::Tool` with code `not_persisted`. A wrapper
  that trusted `applied` alone would call this a success.
- `test_set_slots_rejects_an_unapplied_address` — **protects:** "silence is not
  confirmation". Serve `{"applied": [], "warnings": [], "wrote": "wf.json"}` for a
  single-override write and assert `ComfyError::Tool` with code `not_applied`.
- `test_list_slots_sends_workflow_path` — **protects:** argument naming on a surface that
  rejects a misspelling outright (MCP-SURFACE §8.7). Assert the outgoing argument key is
  exactly `workflow_path`.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root
- [ ] `cargo clippy -p mcp-bridge --all-targets -- -D warnings` clean
- [ ] All eight named tests present and passing
- [ ] No test spawns a process, opens a socket, or writes to `testdata/`
- [ ] `set_slots` sends `stdout: false` on every path
- [ ] No new dependencies

## Out of scope
`validate_workflow` and `list_workflow_notes` — T-103c. `vary_workflow`. Any Tauri command,
any UI, any profile loading or slot-address validation against a profile (T-107). Mapping a
validation `node_id` back to a slot address — that translation belongs with the validation
types in T-103c (MCP-SURFACE §9.2).

## Notes for the executor
- Never send the string override form, and do not add a convenience that builds one.
- Do not drop `confirm_persisted`. It is the runtime half of traps 1 and 2; the wrapper
  sending `stdout: false` is the other half, and both are needed because a silent no-op is
  indistinguishable from success in the payload.
- `SlotOverride` derives `Serialize` only — it travels outbound. Do not add `Deserialize`
  "for symmetry"; T-103a's round-trip finding is what happens when in and out disagree.
- Do not modify `testdata/mcp/list_workflow_slots.minimax.json`. It is a live capture, and
  the 24-of-25 subgraph ratio is load-bearing.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read crates/mcp-bridge/src/mock.rs --read crates/mcp-bridge/src/templates.rs --file crates/mcp-bridge/src/lib.rs --file crates/mcp-bridge/src/slots.rs
```
