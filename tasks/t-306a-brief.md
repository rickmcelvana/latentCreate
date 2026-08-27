# T-306a: the two things that make a slot write real

**Depends:** T-305b | **Crate/dir:** `crates/create-core` (pure) plus one profile JSON
**Files to modify:**
- `crates/create-core/src/generation.rs`
- `crates/create-core/src/audit.rs` *(new)*
- `crates/create-core/src/lib.rs` — two lines declaring the module
- `crates/create-core/src/profile.rs` — **tests and one doc comment only**, see §4
- `profiles/ace-step-1.5-turbo.json`

⚠ **`graph.rs` is not in this task.** An earlier draft put `audit_slots` there; see §2.

**T-306 is split.** This half is the pure seam between T-304's resolved slots and the wire, and
it lands a **bug fix to the shipped ACE-Step profile**. **T-306b** is the Tauri pipeline command
(fetch → set slots → graph edits → validate → run). Split because the pipeline command plus its
mock-transport tests would put the pair well past the ~400-line run limit, and because
everything here is testable with no ComfyUI at all.

## ⚠ The shipped ACE-Step profile writes the seed to two addresses that do nothing

Found while briefing this task, by resolving the real profile against the real template and then
checking the engine's own execution record.

The profile declares `"seed": { "type": "seed", "slots": ["94.seed", "3.seed"] }`. Both of those
inputs are **fed by a link** from `PrimitiveInt` node 109. `set_workflow_slot` accepts both
addresses and reports them `applied`. The executed prompt shows what actually happens:

```
3.seed  = ["109", 0]        94.seed = ["109", 0]        109.value = 12345
```

The widget values the app wrote are ignored; the sampler takes its seed from node 109. **Every
track would render with whatever 109 holds — 0 in the shipped template — no matter what the user
chose.** Seeds would appear to work, provenance would record the chosen value, batches (N seeds
of one spec, ARCHITECTURE §7) would produce N identical jobs, and nothing would report an error.

Nothing in the MCP surface says so: `list_workflow_slots` lists `3.seed` and `94.seed` with
current values and no hint that either is driven.

### But "link-fed" is not the same as "inert"

The same template has two more link-fed inputs — `94.duration` and `98.seconds`, both fed from
node 99 — and **those writes do land**. The difference is what is on the other end of the link:

| Source node | Class | In the executed prompt? | Consumer's widget value |
|---|---|---|---|
| 109 | `PrimitiveInt` — a real backend node | **yes**, and consumers link to it | **ignored** |
| 99 | `PrimitiveNode` — frontend-only | **no**, dropped at conversion | **used** |

Verified directly: node 99 holds `120`, both consumers were written `10`, and the executed
prompt carries `94.duration = 10.0` / `98.seconds = 10.0`. A frontend-only node's link does not
survive conversion, so the widget wins.

So the rule the guard has to encode is **"link-fed from a node that survives conversion"**, not
"link-fed". A check that flagged every link would condemn `duration_s`, which is fine, and the
false alarm would get the whole guard switched off.

## ⚠ `serde_json::to_value(input_value)` is the wrong conversion

T-304 produces `ResolvedSlots = BTreeMap<SlotAddress, InputValue>`. `SlotOverride` takes a
`serde_json::Value`. The obvious bridge is wrong: `InputValue` is **adjacently tagged**, so

```
serde_json::to_value(InputValue::Seed(42))  ->  {"type":"seed","value":42}
```

which is an object where the slot wants a number. Confirmed against the live install — and it
**fails closed**, for numbers and strings alike:

```
3.steps  = {"type":"int","value":12}       -> [workflow_slot_invalid] expected INT, got dict
94.tags  = {"type":"text","value":"..."}   -> [workflow_slot_invalid] expected STRING, got dict
```

So this mistake breaks every generation loudly rather than corrupting one quietly. It still has
to be got right, and the tag is not a wart to delete: it is what stops a seed deserialising as an
`Int` on the way back out of provenance (T-003).

## Seed range: the fix narrows it, and that is worth knowing

`KSampler.seed` declares `min: 0, max: 18446744073709551615` — the full `u64`, which is where
T-003's `Seed(u64)` came from and it is correct for the node. But `PrimitiveInt.value`, which is
where the seed now goes, declares `max: 9223372036854775807` (`i64::MAX`).

Writing above it is **a warning from `set_workflow_slot`** (`above_max`, and the value is applied
anyway) but **an error from `validate_workflow`** (`valid: false`). So the pipeline's validate
step catches it — do not add a range check here. Note it and move on; giving `InputSpec::Seed` a
declared range is a separate question, recorded in the backlog.

## Spec

### 1. `InputValue::to_slot_value` — `generation.rs`

Goes on the existing `impl InputValue`, above `kind`.

### 2. `audit_slots` — a new module, `audit.rs`

Pure, over the same workflow `Value` the T-305 edits take. It reports what it found **and what
it could not check**; a guard that silently ignores what it cannot see is how the seed bug
would survive a second time.

⚠ **Corrected after the second run stalled.** The first draft of this brief put `audit_slots`
in `graph.rs`. Two reasons it moves:

- **It does not belong there.** `graph.rs`'s own module doc says *"pure workflow graph **edits**
  that slots cannot express"*. `audit_slots` edits nothing; it answers a question about a graph.
- **`graph.rs` is 48 KB**, up from 18 KB before T-305b. Aider is running in `whole` edit format,
  so every `--file` is re-emitted in full — this task as originally scoped meant emitting
  ~102 KB across three large files before writing a line of new code, and the run did not get
  there. Splitting keeps every file in the executor lane small enough to rewrite.

`lib.rs` gains `pub mod audit;`, `pub use audit::*;` and one line in the module-list doc comment,
matching the existing entries.

`unchecked` covers subgraph-interior addresses (`37/6.unet_name`) and addresses naming a node
the top-level graph does not have. Resolving a subgraph interior means walking from the instance
node into its definition, a different id space — T-305b declined the same thing for the splice
and this declines it for the same reason. **MiniMax's one `slot_override` is such an address**,
so it lands in `unchecked` and is not a false pass.

### 3. Fix `profiles/ace-step-1.5-turbo.json`

```json
"seed": { "type": "seed", "slots": ["109.value"] }
```

was

```json
"seed": { "type": "seed", "slots": ["94.seed", "3.seed"] }
```

One address, not two: node 109 already feeds both consumers, so the fan-out the profile was
expressing is done by the graph. Verified — after the change the audit reports **no inert
slots**, and the two `PrimitiveNode`-fed addresses are still correctly reported as *not* inert.

⚠ **Do not "fix" `duration_s` as well.** `94.duration` and `98.seconds` are link-fed and land
correctly, per the table above. Changing them would be a regression driven by a
pattern-match on the word "link".

### 4. What the profile change breaks, and how each site changes

⚠ **Corrected after the first run stopped here.** The brief's original file list was wrong: it
named three files and the seed change reaches **five sites** — one in `generation.rs`, which was
listed, and four in `profile.rs`, which was not. Every one of them is enumerated below, so no
file has to be requested mid-run.

These are not string swaps. Four of the five encode a *claim* about the template, and the claim
is what changed — a site edited to make the compiler happy, without the reason, leaves the next
reader believing the old fan-out story.

**`crates/create-core/src/generation.rs`**

1. `test_seed_reaches_both_slots_and_u64_max_survives` asserts the seed lands in `94.seed` **and**
   `3.seed`. Rename it — the seed no longer fans out — and assert `109.value` instead. **Keep the
   `u64::MAX` half**: it is the T-003 guard and it is still exactly right here, because
   `resolve_slots` does not range-check and `InputSpec::Seed` declares no range. Add a line saying
   the live `109.value` maximum is `i64::MAX` and that `validate_workflow` is what rejects an
   over-range seed (MCP-SURFACE §18.3/§18.4), so the test does not read as a contradiction.
   `test_duration_reaches_both_slots...` is untouched: duration **is** still a fan-out, and it is
   now the only one, which makes it the load-bearing test for that behaviour.

**`crates/create-core/src/profile.rs`**

2. The `InputSpec` doc comment (~line 30) uses the seed as its fan-out example: *"separate
   planner/sampler seeds in `94.seed` and `3.seed`"*. That example is now false. Drop the seed
   clause and keep duration, which is still a genuine one-control-many-slots case.
   **This is the only doc comment in `profile.rs` that names the old addresses** — asked and
   answered on the second run; everything else at line 325+ is inside `mod tests`. The other
   four sites in this list are the whole of the rest.
3. `test_fixture_matches_verified_slot_addresses` — the `seed` arm expects two addresses. It
   becomes the single `109.value`. **Leave the `duration_s` arm exactly as it is**; it is the
   contrast case and it still passes.
4. `VERIFIED_ACE_STEP_SLOTS` — **add `"109.value"`. Do not remove `3.seed` or `94.seed`.** That
   constant records what `list_workflow_slots` returned for the template, not what the profile
   drives; all three addresses genuinely exist as slots. `109.value` is confirmed present in the
   live list. Without this addition,
   `test_shipped_ace_step_addresses_all_exist_in_the_verified_template` fails — a third breakage
   the first run did not spot.
5. `test_slot_addresses_walk_groups_and_skip_unsupported` — the exact expected set swaps
   `"3.seed"` and `"94.seed"` for `"109.value"`. This test is about walking groups; a minimal
   edit is right here.

Nothing outside these two files and the profile JSON references the old addresses — checked
across `crates/`, `src-tauri/`, `app/` and `profiles/`.

### 5. MiniMax has the same shape and is **out of scope**

For the record, so a green T-306a is not misread as "the profiles are verified": the MiniMax
profile's `seed` names `37/13.seed`, `37/9.seed` and `37/38.seed`, and **all three are link-fed**
— the first two from `SeedNode` 38, and `38.seed` itself from the subgraph's promoted input
proxy (`origin_id: -10`). Its subgraph also stores links as **objects**
(`{"id", "origin_id", "target_id", ...}`), not the top-level six-element arrays.

`audit_slots` reports all three as `unchecked`, which is correct and honest: this task does not
resolve subgraph interiors. **Do not change the MiniMax profile and do not extend the audit to
reach into subgraphs.** Settling it needs a MiniMax generation and a read of
`GET /history/<prompt_id>`, which is its own task.

## Reference implementation

Compiled and run against the real profile and the real fixture before this brief was written.
`rustfmt` clean.

### `generation.rs`

```rust
/// The bare JSON value a slot write carries.
///
/// **Not `serde_json::to_value(self)`.** `InputValue` is adjacently tagged, so
/// that yields `{"type":"seed","value":42}` -- an object where the slot wants a
/// number. comfy-mcp rejects it with `[workflow_slot_invalid]` (`expected INT,
/// got dict`), for STRING slots too, so the mistake fails closed rather than
/// corrupting a run; it still fails every generation.
///
/// The tag exists so a value survives the round trip through provenance
/// (T-003); it must be dropped on the way to the wire.
pub fn to_slot_value(&self) -> serde_json::Value {
    match self {
        InputValue::Text(s) | InputValue::Enum(s) => serde_json::Value::String(s.clone()),
        InputValue::Int(i) => serde_json::Value::from(*i),
        InputValue::Float(f) => serde_json::Value::from(*f),
        InputValue::Seed(s) => serde_json::Value::from(*s),
        InputValue::Bool(b) => serde_json::Value::Bool(*b),
    }
}
```

### `audit.rs`

New file. The module doc comment is part of the deliverable — it is where the reason this
module exists gets recorded, and `lib.rs`'s module list needs a one-line match for it.

```rust
//! Whether a resolved slot write can actually reach the engine.
//!
//! `set_workflow_slot` reports an address `applied` whenever it can write the
//! widget. Whether the widget is *read* depends on the graph, and the tool
//! never says: in the ACE-Step template `3.seed` and `94.seed` are driven by a
//! link from `PrimitiveInt` 109, so writing them is accepted, persisted, and
//! ignored (MCP-SURFACE 18.1). This module is the standing check.
//!
//! Separate from [`crate::graph`] on purpose: that module *edits* a workflow,
//! this one only asks questions about one.

use serde_json::Value;

/// One resolved slot address whose target input is driven by a link.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkFed {
    /// The address as the profile writes it, e.g. `"3.seed"`.
    pub address: String,
    /// `type` of the node driving the link, when it is in the top-level graph.
    ///
    /// This decides whether the write is inert. A link from a **frontend-only**
    /// node (`PrimitiveNode`) is dropped when the graph is converted for the
    /// engine and the consumer's own widget value is used, so the write lands.
    /// A link from a **real backend node** (`PrimitiveInt`) survives, and the
    /// consumer's widget is ignored -- so the write is inert.
    pub source_type: Option<String>,
}

/// What [`audit_slots`] could and could not determine.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SlotAudit {
    /// Addresses whose target input carries a link.
    pub link_fed: Vec<LinkFed>,
    /// Addresses this check could not resolve: subgraph interiors (`37/6.x`),
    /// and addresses naming a node the top-level graph does not have.
    ///
    /// Reported rather than skipped. A guard that quietly ignores what it
    /// cannot see is how an inert write survives review.
    pub unchecked: Vec<String>,
}

/// Node classes that exist only in the ComfyUI frontend.
///
/// Their links are dropped when the UI graph is converted to the API prompt,
/// which is why a write to an input they feed still lands. Verified on the
/// ACE-Step template: node 99 is a `PrimitiveNode` holding 120, it is **absent
/// from the executed prompt**, and its two consumers ran with the values
/// written into their own widgets.
pub const VIRTUAL_NODE_TYPES: [&str; 2] = ["PrimitiveNode", "Reroute"];

impl LinkFed {
    /// Whether writing this address changes nothing at execution time.
    ///
    /// An unknown source type is reported as inert. The link exists; if its
    /// source cannot be identified, the safe reading is that it survives
    /// conversion and overrides the widget.
    pub fn is_inert(&self) -> bool {
        match self.source_type.as_deref() {
            Some(t) => !VIRTUAL_NODE_TYPES.contains(&t),
            None => true,
        }
    }
}

/// Report which of `addresses` name an input that a link drives.
///
/// **Why this exists.** `set_workflow_slot` reports an address as `applied`
/// whether or not the value can reach the engine. In the ACE-Step template
/// `3.seed` and `94.seed` are both fed from `PrimitiveInt` 109, so writing them
/// is accepted, persisted, and ignored -- every track would render with node
/// 109's seed no matter what the user chose. Nothing in the MCP surface says
/// so: `list_workflow_slots` lists both addresses with a current value and no
/// hint that they are driven.
pub fn audit_slots(workflow: &Value, addresses: &[String]) -> SlotAudit {
    let mut audit = SlotAudit::default();
    let Some(nodes) = workflow.get("nodes").and_then(Value::as_array) else {
        audit.unchecked = addresses.to_vec();
        return audit;
    };

    for address in addresses {
        let Some((instance, field)) = split_address(address) else {
            audit.unchecked.push(address.clone());
            continue;
        };
        if instance.contains('/') {
            // Subgraph interior. Resolving it means walking from the instance
            // node to its definition, which is a different id space (T-305b
            // declined the same thing for the splice).
            audit.unchecked.push(address.clone());
            continue;
        }
        let Some(node) = nodes
            .iter()
            .find(|n| n.get("id").map(|v| v.to_string()).as_deref() == Some(instance))
        else {
            audit.unchecked.push(address.clone());
            continue;
        };
        let link = node
            .get("inputs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .find(|i| i.get("name").and_then(Value::as_str) == Some(field))
            .and_then(|i| i.get("link"))
            .and_then(Value::as_i64);
        let Some(link) = link else {
            continue;
        };
        audit.link_fed.push(LinkFed {
            address: address.clone(),
            source_type: source_type_of(workflow, nodes, link),
        });
    }
    audit
}

/// The `type` of the node that link `id` comes from.
fn source_type_of(workflow: &Value, nodes: &[Value], id: i64) -> Option<String> {
    let src = workflow
        .get("links")
        .and_then(Value::as_array)?
        .iter()
        .find(|l| l.get(0).and_then(Value::as_i64) == Some(id))
        .and_then(|l| l.get(1))
        .and_then(Value::as_i64)?;
    nodes
        .iter()
        .find(|n| n.get("id").and_then(Value::as_i64) == Some(src))
        .and_then(|n| n.get("type"))
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Split `"3.seed"` into `("3", "seed")`, or `"37/6.unet_name"` into
/// `("37/6", "unet_name")`. `None` when there is no field part.
fn split_address(address: &str) -> Option<(&str, &str)> {
    let (instance, field) = address.rsplit_once('.')?;
    if instance.is_empty() || field.is_empty() {
        return None;
    }
    Some((instance, field))
}
```

### Loading the fixtures in tests

`graph.rs`'s test helpers are private to its own `mod tests` and it is not in this run, so
`audit.rs` needs its own. This is the shape already used across the crate:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::generation::{GenerationSpec, InputValue};
    use crate::profile::ModelProfile;
    use std::path::PathBuf;

    /// The real captured template, read from disk at test time.
    fn fixture(name: &str) -> Value {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/workflows")
            .join(name);
        let text = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {}", path.display(), e));
        serde_json::from_str(&text).unwrap()
    }

    /// The shipped profile, compiled in the way `profile.rs`'s tests do.
    fn ace() -> ModelProfile {
        serde_json::from_str(include_str!("../../../profiles/ace-step-1.5-turbo.json")).unwrap()
    }
}
```

## Acceptance criteria

Fixtures are the real `testdata/workflows/ace_step_1_5_xl_turbo.json` and the real
`profiles/*.json`, loaded with the helpers above.

- [ ] `to_slot_value` tested per variant: `Text`/`Enum` → JSON string, `Int` → number,
      `Float` → number, `Bool` → bool, and **`Seed(u64::MAX)` → the exact integer**, not a float
      and not a string.
- [ ] ⚠ **`to_slot_value` is not `serde_json::to_value`.** Assert this directly: for at least
      one variant, `to_value` produces an object with a `"type"` key and `to_slot_value` does
      not. Without it the two can silently converge if someone "simplifies" the match arm into
      a serde call.
- [ ] ⚠ **The regression test for the seed bug**, and the reason this task exists: resolve the
      **shipped ACE-Step profile** against the **real template fixture** for a spec that sets
      every input, run `audit_slots` over the resolved addresses, and assert **no address is
      inert**. This test fails on the profile as it stands today and passes after §3. Write it
      before making the change and watch it fail.
- [ ] `audit_slots` distinguishes the two link kinds on the real fixture: `94.duration` and
      `98.seconds` come back `link_fed` with `source_type: Some("PrimitiveNode")` and
      `is_inert() == false`.
- [ ] An address whose input carries no link is not reported at all.
- [ ] A subgraph-interior address (`37/6.unet_name`) lands in `unchecked`, not in `link_fed`
      and not silently dropped. Use the MiniMax fixture and its profile's real
      `slot_overrides` key.
- [ ] An address naming a node that is not in the graph lands in `unchecked`.
- [ ] A link whose source node is missing from the graph yields `source_type: None` and
      `is_inert() == true` — unknown provenance is treated as inert, because the link exists.
- [ ] The five sites in §4 are updated, and the two doc/test sites that explained the seed
      fan-out now say what is true rather than merely compiling.
- [ ] `VERIFIED_ACE_STEP_SLOTS` still contains `3.seed` and `94.seed` **and** gains
      `109.value`. Removing the first two would be wrong: the constant records the template's
      slots, not the profile's choices.
- [ ] `audit.rs` carries a module doc comment saying what it is for, and `lib.rs`'s module list
      gains a matching line. A new module with no entry there is invisible to the next reader.
- [ ] `npm run gate` clean; **`graph.rs` is not modified at all**; no changes outside the five
      listed files.

**Mutation check before you call it done.** Each must turn the suite red:

1. `to_slot_value` for `Seed` returns `Value::from(*s as f64)`. (A `u64::MAX` seed becomes
      `1.8446744073709552e19` — this is the shape of the T-003 demotion, one layer down.)
2. `is_inert` returns `false` for an unknown `source_type`.
3. `audit_slots` treats every link as inert, ignoring `source_type`.
4. `audit_slots` pushes nothing to `unchecked` for a subgraph address — the silent skip.
5. Revert §3's profile change. The regression test must fail; if it does not, it is not
      testing the profile.
6. Remove `"109.value"` from `VERIFIED_ACE_STEP_SLOTS`.
      `test_shipped_ace_step_addresses_all_exist_in_the_verified_template` must fail — that test
      is the typo guard for every profile address and it has to still bite.

## Out of scope

- **No Tauri command, no MCP calls, no file I/O** — that is T-306b. Everything here takes a
  `Value` it did not load.
- Do not act on the audit. Deciding what a link-fed slot means for a run is the pipeline's job;
  this reports.
- Do not add a seed range check (see above — validation catches it).
- Do not resolve subgraph interiors.
- Do not touch `resolve_slots`, `ensure_lossless_output` or `splice_loras`, and **do not open
  `graph.rs`**. Nothing in this task needs it; the audit is self-contained.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --edit-format diff --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read tasks/t-306a-brief.md --read testdata/workflows/README.md --read profiles/minimax-music-3.json --file crates/create-core/src/generation.rs --file crates/create-core/src/audit.rs --file crates/create-core/src/lib.rs --file crates/create-core/src/profile.rs --file profiles/ace-step-1.5-turbo.json
```

`profile.rs` is `--file`, not `--read`: the seed change breaks four sites in it (§4). The
MiniMax profile is `--read` because one acceptance criterion uses its real `slot_overrides`
address and the executor should not invent one. `graph.rs` is **not passed at all** — the audit
needs nothing from it, and 48 KB of read context buys nothing here.

`--edit-format diff` is new. Every run so far has used Aider's default `whole` format, which
re-emits each `--file` in full; that was fine when these files were small and is what stalled
the second attempt at this task. If the diff format misbehaves with this model, fall back to
`whole` — the file list above is now small enough for it either way.
