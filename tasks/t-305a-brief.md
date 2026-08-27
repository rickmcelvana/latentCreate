# T-305a: make the save node write lossless

**Depends:** T-304 | **Crate/dir:** `crates/create-core` (pure; no I/O, no async)
**Files to create/modify:**
- `crates/create-core/src/graph.rs` *(new)*
- `crates/create-core/src/lib.rs`
- `crates/create-core/Cargo.toml`

**T-305 is split.** This half is the save node; **T-305b** is the LoRA splice. They are
independent transforms over the same file, and together they exceed the ~400-line run limit —
the splice needs its own reference code for link rewiring.

## Goal

`ensure_lossless_output(&mut Value, &OutputSpec)`: rewrite every audio save node in a working
copy of a workflow so the generated audio is FLAC, not MP3.

This is a **correctness requirement, not a preference** (PROJECT.md, 2026-08-23): the owner
swaps this node out of every workflow by habit, and an app feeding a mastering chain must not
hand it lossy audio. It is also the one edit that cannot be expressed as a slot.

## The evidence, and the trap

Read [MCP-SURFACE §16.1 and §16.3](../docs/MCP-SURFACE.md) first. Three facts shape the whole
function:

1. **`format` is not a slot.** It is a `COMFY_DYNAMICCOMBO_V3`; `list_workflow_slots` does not
   surface it and `set_workflow_slot` rejects it with `[workflow_slot_invalid]`. It lives as a
   **positional entry in `widgets_values`**.
2. **The array length varies by format.** `flac` has no sub-widget; `mp3` and `opus` carry a
   `quality`. So the array is **rebuilt to exactly two entries**, never patched in place, or a
   stale `"V0"` survives the format that owned it.
3. ⚠ **The test is the format value, not the node class.** ACE-Step's template ships
   `SaveAudioMP3`. MiniMax Music 3's ships **`SaveAudioAdvanced` already set to `mp3`/`V0`**.
   A check that asks "is this the modern node" passes MiniMax and ships MP3 — the exact
   outcome this function exists to prevent. Both fixtures are in `testdata/workflows/` and
   **both must be tested**; a test that uses only ACE-Step passes while the bug ships.

`flac` is the only lossless format the node offers — there is no WAV — and it writes
16-bit/48 kHz with no bit-depth control.

## Spec

### 1. Where it lives, and the third dependency

New module `crates/create-core/src/graph.rs`, declared in `lib.rs` beside the others. Add
**`serde_json = "1.0"`** to `[dependencies]` — it is currently a dev-dependency only.

This is `create-core`'s third dependency, after T-304 added `thiserror` to a crate that had
one. It belongs here rather than in `mcp-bridge`: the transform is pure and is *about the
domain* (a profile's output policy applied to a graph), while `mcp-bridge`'s role is typed
wrappers per verified MCP tool (ARCHITECTURE §2), and it does not depend on `create-core` —
moving the edit there would invert that. ARCHITECTURE §2's `create-core` line gains "graph
edits" in the same commit.

### 2. Reference implementation

**Compiled and run against both fixtures before this brief was written.** `rustfmt` clean.
The `save_nodes`-returning-`Vec<&mut Value>` shape you might reach for first does **not**
compile — two live mutable borrows of the workflow — hence the two sequential passes.

```rust
use crate::profile::OutputSpec;
use serde_json::Value;

/// The audio save nodes ComfyUI ships.
///
/// `SaveAudio` (FLAC), `SaveAudioMP3` and `SaveAudioOpus` are all marked
/// DEPRECATED in the install this was verified against; `SaveAudioAdvanced` is
/// the current one (MCP-SURFACE 5). All four are recognised because a template
/// may ship any of them, and a save node this list misses is a graph the app
/// silently leaves writing MP3.
const SAVE_NODE_TYPES: [&str; 4] = [
    "SaveAudio",
    "SaveAudioMP3",
    "SaveAudioOpus",
    "SaveAudioAdvanced",
];

/// The only lossless format `SaveAudioAdvanced` offers.
///
/// Verified against the live node schema: the options are exactly `flac`,
/// `mp3` and `opus`, **there is no WAV**, and `flac` alone has no `quality`
/// sub-widget (MCP-SURFACE 16.1).
pub const LOSSLESS_FORMAT: &str = "flac";

/// Why a workflow could not be edited.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum GraphError {
    /// The file is not a workflow shaped the way every template is.
    #[error("workflow is malformed: {detail}")]
    Malformed { detail: String },
    /// No audio save node anywhere in the graph.
    ///
    /// An error, never a silent pass: a workflow the app cannot make lossless
    /// is one it must refuse to run, because the alternative is handing MP3 to
    /// the mastering stage without saying so.
    #[error("no audio save node found")]
    NoSaveNode,
}

/// What `ensure_lossless_output` did, for provenance and for the caller's logs.
#[derive(Debug, Clone, PartialEq)]
pub struct SaveNodeChange {
    /// Ids of the nodes rewritten, in the order found.
    pub nodes: Vec<String>,
    /// The node class every one of them now has.
    pub node_type: String,
    /// The format value written, or `None` when the profile opted out.
    pub format: Option<String>,
}

/// Make every audio save node in `workflow` write the profile's format.
///
/// **The test is the format value, not the node class** (MCP-SURFACE 16.3).
/// ACE-Step's template ships `SaveAudioMP3`; MiniMax Music 3's ships
/// `SaveAudioAdvanced` **already set to `mp3`**, so a check that only asks
/// "is this the modern node" passes MiniMax and hands lossy audio to the
/// mastering stage.
///
/// `format` is a `COMFY_DYNAMICCOMBO_V3`: not a slot, unreachable by
/// `set_workflow_slot`, and a positional entry in `widgets_values`
/// (MCP-SURFACE 16.1). The array length varies by format, so it is rebuilt to
/// exactly two entries rather than patched, or a stale `"V0"` survives.
///
/// `filename_prefix` is preserved: it is the part of this node the user
/// legitimately owns, and `107.filename_prefix` remains an ordinary slot.
pub fn ensure_lossless_output(
    workflow: &mut Value,
    output: &OutputSpec,
) -> Result<SaveNodeChange, GraphError> {
    if !output.prefer_lossless {
        // Opting out leaves the graph exactly as the template shipped it.
        // Both shipped profiles set this true; a profile that does not is
        // making a deliberate choice the app should not quietly override.
        return Ok(SaveNodeChange {
            nodes: Vec::new(),
            node_type: output.save_node.clone(),
            format: None,
        });
    }

    let mut changed = Vec::new();

    // Two sequential passes, not one collected list: each nodes array needs
    // its own mutable borrow of the workflow, and holding both at once does
    // not compile.
    if let Some(subgraphs) = workflow
        .pointer_mut("/definitions/subgraphs")
        .and_then(Value::as_array_mut)
    {
        for sub in subgraphs.iter_mut() {
            if let Some(nodes) = sub.get_mut("nodes").and_then(Value::as_array_mut) {
                rewrite_save_nodes(nodes, output, &mut changed)?;
            }
        }
    }
    if let Some(nodes) = workflow.get_mut("nodes").and_then(Value::as_array_mut) {
        rewrite_save_nodes(nodes, output, &mut changed)?;
    }

    if changed.is_empty() {
        return Err(GraphError::NoSaveNode);
    }

    Ok(SaveNodeChange {
        nodes: changed,
        node_type: output.save_node.clone(),
        format: Some(LOSSLESS_FORMAT.to_string()),
    })
}

/// Rewrite every save node in one nodes array.
///
/// Subgraph interiors are searched as well as the top level because MiniMax
/// Music 3 puts most of its graph inside a subgraph (its `UNETLoader` is at
/// `definitions.subgraphs[0].nodes[0]`). Its save node happens to be
/// top-level, but a template that nested one would otherwise be silently left
/// writing MP3 -- and silence is the failure this must not have.
fn rewrite_save_nodes(
    nodes: &mut [Value],
    output: &OutputSpec,
    changed: &mut Vec<String>,
) -> Result<(), GraphError> {
    for node in nodes.iter_mut() {
        let is_save = node
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| SAVE_NODE_TYPES.contains(&t));
        if !is_save {
            continue;
        }

        let prefix = node
            .get("widgets_values")
            .and_then(Value::as_array)
            .and_then(|w| w.first())
            .and_then(Value::as_str)
            .unwrap_or("audio/latentCreate")
            .to_string();
        let id = node
            .get("id")
            .map(|id| id.to_string())
            .ok_or_else(|| GraphError::Malformed {
                detail: "a save node has no id".to_string(),
            })?;

        let map = node.as_object_mut().ok_or_else(|| GraphError::Malformed {
            detail: format!("node {id} is not an object"),
        })?;
        map.insert("type".to_string(), Value::String(output.save_node.clone()));
        map.insert(
            "widgets_values".to_string(),
            Value::Array(vec![
                Value::String(prefix),
                Value::String(LOSSLESS_FORMAT.to_string()),
            ]),
        );
        // Kept in step with `type`: the frontend uses it for search-and-replace
        // and a stale value makes the node look like the class it no longer is.
        if let Some(props) = map.get_mut("properties").and_then(Value::as_object_mut) {
            if props.contains_key("Node name for S&R") {
                props.insert(
                    "Node name for S&R".to_string(),
                    Value::String(output.save_node.clone()),
                );
            }
        }
        changed.push(id);
    }
    Ok(())
}
```

### 3. Observed behaviour, for the tests to assert

Run against the checked-in fixtures and the shipped profiles' `comfy.output`:

| Fixture | Before | After |
|---|---|---|
| `ace_step_1_5_xl_turbo.json` node `107` | `SaveAudioMP3` / `["audio/ACE_Step1.5_xl_turbo", "V0"]` | `SaveAudioAdvanced` / `["audio/ACE_Step1.5_xl_turbo", "flac"]` |
| `minimax_music3_int8.json` node `35` | `SaveAudioAdvanced` / `["audio/audio_minimax_music3", "mp3", "V0"]` | `SaveAudioAdvanced` / `["audio/audio_minimax_music3", "flac"]` |

Both return `format: Some("flac")`; a graph with no save node returns `Err(NoSaveNode)`.

## Acceptance criteria

Fixtures are the **real templates** in `testdata/workflows/`, loaded from disk the way
`profile.rs` loads the profile fixtures. Do not hand-write a workflow: a rule about template
JSON has to run against template JSON.

- [ ] ACE-Step: node `107` becomes `SaveAudioAdvanced` with **exactly two** widget values, the
      second `"flac"`, and the original `filename_prefix` preserved.
- [ ] ⚠ MiniMax: node `35` **still changes**, `mp3` -> `flac`, and its widgets go from **three
      entries to two** — the stale `"V0"` is gone. This is the §3 trap; without this test the
      bug ships.
- [ ] `Node name for S&R` follows `type` where the node has one, and its absence is not an
      error.
- [ ] A workflow with no save node -> `Err(GraphError::NoSaveNode)`.
- [ ] `prefer_lossless: false` leaves the graph **byte-identical** and returns
      `format: None`. Assert the whole workflow is unchanged, not just the save node.
- [ ] A save node nested in `definitions.subgraphs[].nodes` is found and rewritten.
- [ ] Every other node in both fixtures is untouched — compare the full JSON before and after,
      excluding the save node. The edit must not reformat or reorder anything else.
- [ ] `npm run gate` clean; no changes outside the three listed files.

**Mutation check before you call it done** (this is now a standing habit — T-304's seed guard
was found missing this way, and T-110's before it). Each of these must turn the suite red:
1. Make the node-type test the condition instead of always rewriting, so MiniMax is skipped.
2. Patch `widgets_values[1]` in place instead of rebuilding the array, leaving `"V0"`.
3. Return `Ok` instead of `Err` when no save node is found.

## Out of scope

- **No LoRA splicing** — T-305b, and it needs the link rewiring this task does not touch.
- No slot application, no `set_workflow_slot`, no template fetch, no file I/O — T-306. This
  function takes a `Value` it did not load and does not write it back.
- Do not make `create-core` depend on `mcp-bridge`. That inversion is what ARCHITECTURE §2
  exists to prevent.
- Do not add a format choice to `OutputSpec` or the UI. `flac` is the only lossless option;
  offering a picker would be offering the user a way to defeat the requirement.
- Do not touch `filename_prefix` beyond preserving it — it is still a slot, written by T-306.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read tasks/t-305a-brief.md --read crates/create-core/src/profile.rs --read testdata/workflows/README.md --file crates/create-core/src/graph.rs --file crates/create-core/src/lib.rs --file crates/create-core/Cargo.toml
```

`profile.rs` is `--read` for `OutputSpec`; it does not change. `docs/MCP-SURFACE.md` is
`--read` because §16.1 and §16.3 are the whole reason this function is shaped as it is, and an
executor that has not seen them will "simplify" the two-entry rebuild back into a patch.
`testdata/workflows/README.md` documents both fixtures and their node ids.
