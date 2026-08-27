# T-305b: splice the LoRA stack into the MODEL chain

**Depends:** T-305a | **Crate/dir:** `crates/create-core` (pure; no I/O, no async)
**Files to modify:**
- `crates/create-core/src/graph.rs` — the only file. Tests go in its existing `mod tests`,
  beside T-305a's.

Second half of T-305. T-305a made the save node write lossless; this inserts
`LoraLoaderModelOnly` nodes between the profile's `attach_after` node and whatever consumes
its MODEL output. Slots cannot add nodes (MCP-SURFACE §4), so this is the other edit that has
to be graph surgery.

## ⚠ Read this section before anything else

**A LoRA splice that silently does nothing is a passing workflow.** This is not a theoretical
risk; it was produced and run on the live install while this brief was written.

Take the correct splice, then make one plausible mistake — insert the loader nodes and their
links, but leave the downstream consumer's link still sourced at the anchor. Then:

| | Correct splice | Dangling splice |
|---|---|---|
| `validate_workflow` | `valid: true`, 0 errors | **`valid: true`, 0 errors** |
| `converted_node_count` | 13 | **13** |
| `list_workflow_slots` sees the loaders | yes | **yes** |
| Job status | `success` | **`success`** |
| Audio written | yes | **yes** |
| LoRA actually applied | yes | **no** |

Every signal the app could plausibly check says the run was fine. The user picks two LoRAs,
gets a track, and not one byte of it was influenced by either.

The proof is ComfyUI's own execution record, `GET /history/<prompt_id>`, which returns the
API-format prompt as executed:

```
correct   3.model=["78",0]   78.model=["112",0]  112.model=["111",0]  111.model=["104",0]
dangling  3.model=["78",0]   78.model=["104",0]  112.model=["111",0]  111.model=["104",0]
```

In the dangling run nodes 111 and 112 are present, correctly configured and chained to each
other — and feed nothing. ComfyUI prunes unreachable nodes, reports success, and writes audio.

**Three consequences, and they set the shape of this task:**

1. **The tests must assert the chain, not the insertion.** "Two loader nodes were added with
   the right names and strengths" is true of the dangling graph. The invariant is: *following
   the MODEL edge out of the anchor reaches the original consumer, passing through every
   loader in stack order.* Write that as a traversal, not as a set of field checks.
2. **`validate_workflow` is not a safety net for this** and T-306 must not treat a clean
   validation as evidence the LoRA applied. Worth stating there when T-306 is briefed.
3. **You cannot check this by comparing audio.** Two runs of the *unmodified* template, same
   seed, greedy sampling (`temperature: 0`, `top_k: 1`), differ in **98.1% of their bytes**.
   ACE-Step is not reproducible run-to-run on this install. Do not write a test, or ask the
   producer for a check, that rests on two runs matching.

## What was verified live, and how

Method as always: run it, don't reason about it (`AGENTS.md`; the Phase 0 habit).

- The reference implementation below was **compiled and run against the real ACE-Step
  fixture**, and its output was submitted to the live ComfyUI v0.34.1.
- `validate_workflow` on the spliced graph: `valid: true`, `converted_node_count` **11 → 13**,
  the two loaders.
- A deliberately bogus `lora_name` on the spliced node fails with
  `unknown_enum_value: 'does_not_exist.safetensors' not in 53 known options` — so the
  converter really does read the spliced node and check it against the installed list. The
  splice is reaching the engine; validation just cannot tell reachable from orphaned.
- The run completed in ~19 s and wrote audio; the executed prompt above is from its history.
- After splicing, **the loaders' widgets become ordinary slots** — `111.lora_name`,
  `111.strength_model`, `112.*` all appear in `list_workflow_slots`. Useful for T-308: changing
  a strength later does not require re-splicing.

Two verification routes that **do not work here**, so nobody spends a session rediscovering it:

- **comfy-cli's captured log.** `get_logs` reads a file that is only written when comfy-cli
  launched the server in the background. The owner launches ComfyUI himself, so the file is
  stale — during this session it returned lines whose `mtime` predated every run by nine
  hours. It looked like real output. Check `mtime` before believing `get_logs`.
- **Audio comparison**, per the non-reproducibility above.

## The graph facts

Link entries are `[link_id, src_node, src_slot, dst_node, dst_slot, type]`.

In the ACE-Step fixture the MODEL chain is `104 --260--> 78 --175--> 3`, and the profile's
`attach_after` is `"104"`, so the stack goes between 104 and 78. `last_node_id` is 110 and
`last_link_id` 265, so a two-LoRA stack allocates nodes 111, 112 and links 266, 267.

⚠ **`attach_after` is a `String`, node `id` is a JSON number.** Compare with
`node.get("id").map(|v| v.to_string())`, exactly as T-305a does. This is why
`SaveNodeChange.nodes` is `Vec<String>` and why `SpliceChange.nodes` is too.

⚠ **`last_node_id` can exceed the highest top-level id, and could in principle lag it.** In the
MiniMax fixture `last_node_id` is 43 while the top-level maximum is 40 — the high-water mark
counts subgraph interiors too. So neither "declared value" nor "scan the nodes" is right on its
own: take the max of both. A collision does not error, it silently mis-wires the graph.

### Why the consumer links keep their ids

The splice re-sources each existing consumer link — link 260 keeps its id and its destination,
only its `src` moves from 104 to the last loader — and adds one fresh link per loader. Node 78
is **not touched at all**: its `inputs[0].link` is still 260.

The alternative (fresh links everywhere, rewriting each consumer's `inputs[].link`) also
produces a valid graph, but it edits every downstream node and renumbers links that provenance
may later reference. Fewer edits is a smaller surface for exactly the silent breakage above, so
this shape is required, not merely suggested.

It also falls out cleanly for fan-out: an anchor whose MODEL output feeds several consumers has
*all* of those links re-sourced to the last loader, and none of the consumers is edited.

### Errors

`GraphError` gains five variants beside T-305a's two. All of them are conditions the user can
be shown, so word them for someone looking at a LoRA picker:

- `TooManyLoras { max, got }` — more than the profile's `max_stack`.
- `StrengthOutOfRange { lora, min, max, value }` — outside the profile's declared range. Same
  rule as T-304's `in_range`, same reason: the UI is not the only caller, and a project saved
  under an older profile can carry a stack the current profile no longer allows.
- `NoAttachPoint { id }` — `attach_after` names a node that is not in the top-level graph.
- `NoModelOutput { id }` — the anchor has no MODEL output.
- `NoModelConsumer { id }` — nothing consumes the anchor's MODEL output, so a LoRA there could
  not affect anything. **An error, not a no-op**, for the reason the whole first section of
  this brief exists.

⚠ **Splicing is top-level only.** If `attach_after` names a node that exists only inside
`definitions.subgraphs`, return `NoAttachPoint` rather than attempting it. Subgraph interiors
have their own id and link space, and getting that wrong is another silent no-op. No shipped
profile needs it — MiniMax has no `loras` block at all — and when one does, it is its own task
with its own live proof.

## Reference implementation

**Compiled, run against the real fixture, and the result executed on the live ComfyUI**
(see above). `rustfmt` clean. Add to `graph.rs`; `use crate::profile::LoraSupport;` joins the
existing `OutputSpec` import.

```rust
/// One LoRA the user stacked, in the order it is applied.
#[derive(Debug, Clone, PartialEq)]
pub struct LoraChoice {
    /// Exactly as ComfyUI lists it -- backslashes and subdirectories included.
    pub name: String,
    pub strength: f64,
}

/// What `splice_loras` inserted.
#[derive(Debug, Clone, PartialEq)]
pub struct SpliceChange {
    /// Ids of the inserted loader nodes, in apply order. Empty for an empty stack.
    pub nodes: Vec<String>,
}

/// Insert the LoRA stack into the MODEL chain, after the profile's attach point.
pub fn splice_loras(
    workflow: &mut Value,
    loras: &LoraSupport,
    stack: &[LoraChoice],
) -> Result<SpliceChange, GraphError> {
    if stack.is_empty() {
        return Ok(SpliceChange { nodes: Vec::new() });
    }
    if stack.len() > loras.max_stack as usize {
        return Err(GraphError::TooManyLoras {
            max: loras.max_stack,
            got: stack.len(),
        });
    }
    for choice in stack {
        if choice.strength < loras.strength.min || choice.strength > loras.strength.max {
            return Err(GraphError::StrengthOutOfRange {
                lora: choice.name.clone(),
                min: loras.strength.min,
                max: loras.strength.max,
                value: choice.strength,
            });
        }
    }

    let attach = &loras.attach_after;
    let (src_slot, consumers, anchor_pos, anchor_order) = read_anchor(workflow, attach)?;

    let mut next_node = next_id(workflow, "last_node_id", max_node_id(workflow));
    let mut next_link = next_id(workflow, "last_link_id", max_link_id(workflow));

    let loader_ids: Vec<i64> = (0..stack.len()).map(|i| next_node + i as i64).collect();
    next_node += stack.len() as i64;

    let feed: Vec<i64> = (0..stack.len()).map(|i| next_link + i as i64).collect();
    next_link += stack.len() as i64;

    let mut new_links: Vec<Value> = Vec::new();
    new_links.push(Value::Array(vec![
        Value::from(feed[0]),
        Value::from(attach.parse::<i64>().unwrap_or_default()),
        Value::from(src_slot),
        Value::from(loader_ids[0]),
        Value::from(0),
        Value::from("MODEL"),
    ]));
    for i in 1..stack.len() {
        new_links.push(Value::Array(vec![
            Value::from(feed[i]),
            Value::from(loader_ids[i - 1]),
            Value::from(0),
            Value::from(loader_ids[i]),
            Value::from(0),
            Value::from("MODEL"),
        ]));
    }

    let last_loader = *loader_ids.last().expect("stack is non-empty");
    let mut made: Vec<Value> = Vec::new();
    for (i, choice) in stack.iter().enumerate() {
        let outgoing: Vec<Value> = if i + 1 < stack.len() {
            vec![Value::from(feed[i + 1])]
        } else {
            consumers.iter().map(|id| Value::from(*id)).collect()
        };
        made.push(serde_json::json!({
            "id": loader_ids[i],
            "type": loras.loader_node,
            "pos": [anchor_pos.0, anchor_pos.1 + 160.0 * (i as f64 + 1.0)],
            "size": [330, 82],
            "flags": {},
            "order": anchor_order,
            "mode": 0,
            "inputs": [{ "name": "model", "type": "MODEL", "link": feed[i] }],
            "outputs": [{ "name": "MODEL", "type": "MODEL", "links": outgoing }],
            "properties": { "Node name for S&R": loras.loader_node },
            "widgets_values": [choice.name, choice.strength],
        }));
    }

    let links = workflow
        .get_mut("links")
        .and_then(Value::as_array_mut)
        .ok_or_else(|| GraphError::Malformed {
            detail: "workflow has no links array".to_string(),
        })?;
    for link in links.iter_mut() {
        let id = link.get(0).and_then(Value::as_i64);
        if id.is_some_and(|id| consumers.contains(&id)) {
            let entry = link.as_array_mut().expect("link is an array");
            entry[1] = Value::from(last_loader);
            entry[2] = Value::from(0);
        }
    }
    links.extend(new_links);

    let all = workflow
        .get_mut("nodes")
        .and_then(Value::as_array_mut)
        .expect("checked in read_anchor");
    all.extend(made);
    for node in all.iter_mut() {
        if node.get("id").map(|v| v.to_string()).as_deref() == Some(attach.as_str()) {
            let out = node
                .pointer_mut(&format!("/outputs/{src_slot}/links"))
                .expect("checked in read_anchor");
            *out = Value::Array(vec![Value::from(feed[0])]);
        }
    }

    if let Some(map) = workflow.as_object_mut() {
        map.insert("last_node_id".to_string(), Value::from(next_node - 1));
        map.insert("last_link_id".to_string(), Value::from(next_link - 1));
    }

    Ok(SpliceChange {
        nodes: loader_ids.iter().map(|id| id.to_string()).collect(),
    })
}

type Anchor = (usize, Vec<i64>, (f64, f64), i64);

fn read_anchor(workflow: &Value, attach: &str) -> Result<Anchor, GraphError> {
    let nodes = workflow
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or_else(|| GraphError::Malformed {
            detail: "workflow has no top-level nodes array".to_string(),
        })?;
    let node = nodes
        .iter()
        .find(|n| n.get("id").map(|v| v.to_string()).as_deref() == Some(attach))
        .ok_or_else(|| GraphError::NoAttachPoint {
            id: attach.to_string(),
        })?;

    let outputs = node.get("outputs").and_then(Value::as_array);
    let (slot, output) = outputs
        .into_iter()
        .flatten()
        .enumerate()
        .find(|(_, o)| o.get("type").and_then(Value::as_str) == Some("MODEL"))
        .ok_or_else(|| GraphError::NoModelOutput {
            id: attach.to_string(),
        })?;

    let consumers: Vec<i64> = output
        .get("links")
        .and_then(Value::as_array)
        .map(|l| l.iter().filter_map(Value::as_i64).collect())
        .unwrap_or_default();
    if consumers.is_empty() {
        return Err(GraphError::NoModelConsumer {
            id: attach.to_string(),
        });
    }

    let pos = node.get("pos").and_then(Value::as_array);
    let x = pos
        .and_then(|p| p.first())
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let y = pos
        .and_then(|p| p.get(1))
        .and_then(Value::as_f64)
        .unwrap_or(0.0);
    let order = node.get("order").and_then(Value::as_i64).unwrap_or(0);

    Ok((slot, consumers, (x, y), order))
}

/// One past the document's high-water mark for `key`.
fn next_id(workflow: &Value, key: &str, present: i64) -> i64 {
    let declared = workflow.get(key).and_then(Value::as_i64).unwrap_or(0);
    declared.max(present) + 1
}

fn max_node_id(workflow: &Value) -> i64 {
    let mut max = 0;
    let mut arrays: Vec<&Value> = Vec::new();
    if let Some(nodes) = workflow.get("nodes") {
        arrays.push(nodes);
    }
    if let Some(subs) = workflow.pointer("/definitions/subgraphs").and_then(Value::as_array) {
        for sub in subs {
            if let Some(nodes) = sub.get("nodes") {
                arrays.push(nodes);
            }
        }
    }
    for array in arrays {
        for node in array.as_array().into_iter().flatten() {
            max = max.max(node.get("id").and_then(Value::as_i64).unwrap_or(0));
        }
    }
    max
}

fn max_link_id(workflow: &Value) -> i64 {
    workflow
        .get("links")
        .and_then(Value::as_array)
        .map(|links| {
            links
                .iter()
                .filter_map(|l| l.get(0).and_then(Value::as_i64))
                .max()
                .unwrap_or(0)
        })
        .unwrap_or(0)
}
```

## Acceptance criteria

Fixture is the real `testdata/workflows/ace_step_1_5_xl_turbo.json` and the real
`profiles/ace-step-1.5-turbo.json` `loras` block, loaded from disk as T-305a's tests do.

- [ ] ⚠ **The chain test.** After splicing a two-LoRA stack, walk the MODEL edge from node
      `104`: it reaches `111`, then `112`, then the original consumer `78`, in that order.
      **Write this as a traversal that follows links**, so it fails on the dangling graph.
      A test that only asserts "111 and 112 exist with the right `lora_name`" passes the bug
      this task is about.
- [ ] Node `78` is **untouched** — `inputs[0].link` is still `260` — and link `260` keeps its
      id and destination, with only its source moved to `112`.
- [ ] Widget order is `[lora_name, strength_model]`, verified against the live node schema.
      Both values land, and the `lora_name` keeps its backslashes verbatim
      (`ACE-Step-v1.5-ambient_dream1-LoRA\adapter_model.safetensors`) — these are paths into
      subdirectories, not decoration.
- [ ] `last_node_id` becomes 112 and `last_link_id` 267. No new node or link id collides with
      an existing one.
- [ ] A one-LoRA stack: anchor → loader → original consumer, and `last_node_id` 111.
- [ ] **An empty stack is a no-op**: `Ok` with `nodes: []`, and the workflow is unchanged —
      compare the whole document, including `last_node_id`. This is T-305a's
      `prefer_lossless: false` criterion, and it is the same trap: allocating an id for a stack
      of nothing quietly corrupts the high-water mark.
- [ ] **Fan-out**: a synthetic anchor whose MODEL output feeds two consumers has both links
      re-sourced to the last loader, both keep their ids, and neither consumer is edited.
- [ ] Each of the five errors, including `attach_after` naming a subgraph-only node.
- [ ] Everything outside the anchor, the new nodes and the consumer links is unchanged —
      whole-document comparison, the way T-305a's `assert_rest_of_workflow_unchanged` does it
      after review. Reuse that helper rather than writing a second one.
- [ ] `npm run gate` clean; no changes outside `graph.rs`.

**Mutation check before you call it done** (standing habit; it has now found real holes four
times — T-110, T-304, and twice in T-305a). Each must turn the suite red:

1. Leave the consumer link sourced at the anchor — the dangling splice. **If this one does not
   fail, the chain test is not a chain test** and everything else here is decoration.
2. Reverse the stack order, so the loaders apply back to front.
3. Allocate ids from `last_node_id` only, ignoring the ids actually present.
4. Splice on an empty stack anyway, bumping `last_node_id` by zero-length arithmetic.

## Out of scope

- **No UI.** The LoRA picker — filtering `training_state.pt`, collapsing the epoch-checkpoint
  series, the 53-entry list that is mostly training noise — is **T-309**, and MCP-SURFACE §4
  calls it a real design task rather than a combo box. This task inserts what it is handed.
- No enumeration of installed LoRAs, no `nodes(action="get")` call, no MCP at all. `create-core`
  does not talk to ComfyUI (ARCHITECTURE §2).
- Do not validate `lora_name` against anything. This crate cannot know what is installed, and
  the live validator already returns `unknown_enum_value` with suggestions.
- No subgraph splicing (above).
- Do not touch `ensure_lossless_output` or its tests, beyond reusing the unchanged-document
  helper.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read tasks/t-305b-brief.md --read crates/create-core/src/profile.rs --read testdata/workflows/README.md --read profiles/ace-step-1.5-turbo.json --file crates/create-core/src/graph.rs
```

One `--file` this time: everything lands in `graph.rs`, and T-305a already added `serde_json`
and the module declaration, so neither `Cargo.toml` nor `lib.rs` changes. `profile.rs` is
`--read` for `LoraSupport`/`StrengthRange`; the ACE-Step profile is `--read` because the tests
load its real `loras` block and the executor should not invent one.
