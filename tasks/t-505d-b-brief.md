# T-505d-b — Generalize profile emission for image models

**Lane: Aider.** A single-file `create-core` change: `build_profile` learns that a workflow can be an
**image** model, not only audio. **Depends:** T-313 (`emit.rs`), T-505d-a (adopt seam, landed).
**Dir:** `crates/create-core`. **No UI, no click-through** — this unblocks T-505d-c (the adopt UI),
which has the click-through, and T-506 (cover art), which generates over the image profile this makes
possible.

**File to modify:**

- `crates/create-core/src/emit.rs` — detect the output kind (audio vs image), stop refusing image
  graphs, set `kind` and an image-appropriate `OutputSpec`; update/extend the tests.

**No other file changes.** `import.rs`'s `emit_profile` calls `build_profile` and passes the graph
already; the frontend and bridge are untouched.

---

## Goal

`build_profile` currently **refuses any workflow without an *audio* save node** and hardcodes
`kind: ModelKind::Music` ([emit.rs:137](../crates/create-core/src/emit.rs), :152). So adopting an
image model (Flux.2 Klein 9B, whose graph has a `SaveImage` node) works right up to Save and then
fails at emit, and even if it didn't, it would produce a `music` profile CoverArt cannot use. This
lane teaches emission to recognise an image graph, emit `kind: image`, and give it an image
`OutputSpec` — so an adopted image workflow becomes a first-class profile indistinguishable from a
shipped one, exactly as 5b requires for audio.

## Verified live (2026-09-03), so the detection is grounded

- Flux.2 Klein **9B** (`image_flux2_text_to_image_9b`) is installed and `local_check runnable: true`.
- Its workflow has **`SaveImage` at the top level** (the sampler/CLIP/seed nodes live inside a
  subgraph, MiniMax-style — irrelevant to this lane; `build_profile` only scans for the save node and
  reads the `mappings` it is handed).
- So the top-level node scan `emit.rs` already does will find `SaveImage` — the detection just has to
  *look* for image save types, then map that to `ModelKind::Image`.

## The one design rule

`graph::ensure_lossless_output` (the audio save-node swap) runs **only** in the `generate_audio`
path and never sees an image profile — T-506 owns the image pipeline. So this lane does **not** touch
`graph.rs`. The audio `SAVE_NODE`/`SAVE_NODE_TYPES` and their "kept in step with
`ensure_lossless_output`" contract stay exactly as they are. Image save-node knowledge lives only in
`emit.rs`, because there is no image lossless-swap to keep in step with — the image `OutputSpec`
records what the graph already uses, and T-506 decides what to do with it.

## Spec — `crates/create-core/src/emit.rs`

### 1. Image save-node constants

Beside the existing `SAVE_NODE` / `SAVE_NODE_TYPES`:

```rust
/// Node classes that write an image (core ComfyUI). Unlike the audio list,
/// these have no lossless-swap behind them -- `graph::ensure_lossless_output`
/// is audio-only and never sees an image profile (T-506 owns image output). So
/// this list lives only here.
const IMAGE_SAVE_NODE_TYPES: [&str; 2] = ["SaveImage", "SaveImageWebP"];

/// The save node an emitted image profile records. Just what the graph already
/// uses -- there is no swap to a canonical node the way audio swaps to
/// `SaveAudioAdvanced`. T-506 refines how the image pipeline reads this.
const IMAGE_SAVE_NODE: &str = "SaveImage";
```

### 2. Factor the scan, add kind detection

Refactor the existing `has_audio_save_node`'s node/subgraph scan into a private helper both it and the
new detector use (so the scan logic is not duplicated), and add `detect_output_kind`:

```rust
/// Whether `workflow` -- top-level nodes or any subgraph interior -- has a node
/// whose `type` is in `types`.
fn graph_has_save_node(workflow: &Value, types: &[&str]) -> bool {
    fn scan(nodes: Option<&Vec<Value>>, types: &[&str]) -> bool {
        nodes.is_some_and(|nodes| {
            nodes.iter().any(|n| {
                n.get("type")
                    .and_then(Value::as_str)
                    .is_some_and(|t| types.contains(&t))
            })
        })
    }
    if scan(workflow.get("nodes").and_then(Value::as_array), types) {
        return true;
    }
    // Subgraph interiors count: MiniMax's save node lives inside one, and
    // Klein's controls do too.
    workflow
        .pointer("/definitions/subgraphs")
        .and_then(Value::as_array)
        .is_some_and(|subs| {
            subs.iter()
                .any(|s| scan(s.get("nodes").and_then(Value::as_array), types))
        })
}

/// Whether `workflow` has a node the pipeline can save audio through.
pub fn has_audio_save_node(workflow: &Value) -> bool {
    graph_has_save_node(workflow, &SAVE_NODE_TYPES)
}

/// The output kind an emitted profile should declare, decided by its save node.
///
/// Audio wins when a graph somehow has both -- this is a music app first, and a
/// graph with both save kinds is not a real case worth a knob. `None` when
/// neither is present, which `build_profile` refuses.
fn detect_output_kind(workflow: &Value) -> Option<ModelKind> {
    if graph_has_save_node(workflow, &SAVE_NODE_TYPES) {
        Some(ModelKind::Music)
    } else if graph_has_save_node(workflow, &IMAGE_SAVE_NODE_TYPES) {
        Some(ModelKind::Image)
    } else {
        None
    }
}
```

`has_audio_save_node` stays `pub` and behaves identically (its existing test is unchanged).

### 3. `build_profile`: use the detected kind

Replace the audio-only guard and the hardcoded `kind`/`output`:

```rust
    // Was: if !has_audio_save_node(workflow) { return Err(EmitError::NoSaveNode); }
    let kind = detect_output_kind(workflow).ok_or(EmitError::NoSaveNode)?;
```

Then in the returned `ModelProfile`:

```rust
        kind,
```

and the `OutputSpec` becomes kind-aware:

```rust
            output: match kind {
                ModelKind::Music => OutputSpec {
                    save_node: SAVE_NODE.to_string(),
                    prefer_lossless: true,
                },
                // No lossless swap for images (PNG is already lossless); record
                // what the graph uses and let T-506's pipeline decide.
                ModelKind::Image => OutputSpec {
                    save_node: IMAGE_SAVE_NODE.to_string(),
                    prefer_lossless: false,
                },
            },
```

Everything else in `build_profile` (the `inputs` loop, `license`, `template: None` +
`workflow: Some`, `loras: None`, `lyrics_contract: None`) is unchanged. An image profile simply has no
lyrics/duration inputs mapped, because its graph exposes none.

### 4. Generalize `EmitError::NoSaveNode`'s message

It currently names only audio. Reword so it fits either kind:

```rust
    /// The graph has no save node this app recognises, of either kind.
    #[error("This workflow has no save node latentCreate recognises. Add a Save Image or Save Audio node in ComfyUI, then import it again.")]
    NoSaveNode,
```

### 5. Tests

- **Update** `test_a_graph_with_no_audio_save_node_is_refused` → the intent generalises to "a graph
  with *no* save node of either kind is refused". Its current fixture (a graph with no save node)
  still triggers `NoSaveNode`; adjust the name/doc, keep the assertion.
- **Add** `test_an_image_graph_emits_an_image_profile`. `build_profile` reads `workflow` only for the
  save-node scan and takes the inputs from `mappings`, so a minimal inline graph suffices:

  ```rust
  #[test]
  fn test_an_image_graph_emits_an_image_profile() {
      let workflow = serde_json::json!({ "nodes": [{ "type": "SaveImage" }] });
      let mappings = [(
          Role::Tags,
          vec![MappedSlot {
              address: "6.text".to_string(),
              widget_type: "STRING".to_string(),
              current_value: serde_json::json!("a neon album cover"),
              bounds: None,
          }],
      )];
      let profile = build_profile("klein", "Klein 9B", &workflow, "/x/klein.json", &mappings)
          .expect("an image graph emits");
      assert_eq!(profile.kind, ModelKind::Image);
      assert_eq!(profile.comfy.output.save_node, "SaveImage");
      assert!(!profile.comfy.output.prefer_lossless);
      assert!(profile.inputs.contains_key("tags"));
      // Adopted, so workflow-backed, never a gallery template.
      assert_eq!(profile.comfy.template, None);
      assert!(profile.comfy.workflow.is_some());
  }
  ```

- **Add** a one-line guard that the audio path still emits `Music` (a `SaveImage` node must not turn an
  audio graph into an image profile). Either extend an existing audio emit test with
  `assert_eq!(profile.kind, ModelKind::Music)`, or add a tiny test using a `{"type":"SaveAudio"}`
  node. Keep the existing subgraph audio test (`test_a_save_node_inside_a_subgraph_counts`) as is.

## Acceptance criteria

- [ ] `npm run gate` green (`create-core` tests included).
- [ ] `build_profile` on a graph with a `SaveImage` node returns `kind: Image`, `output.save_node:
      "SaveImage"`, `prefer_lossless: false`; on an audio graph it still returns `kind: Music` with the
      audio `OutputSpec`; on a graph with neither, `EmitError::NoSaveNode`.
- [ ] `graph.rs`, `import.rs`, the bridge, and the frontend are unchanged.
- [ ] Only `crates/create-core/src/emit.rs` changes.

## Out of scope (T-505d-c, T-506)

- **The adopt UI** — the "Bring in" button, the mapping screen, calling `save_imported_profile`
  (T-505d-c). That lane also owns whether `suggest_roles` finds Klein's prompt/seed/steps inside its
  **subgraph** (MiniMax-shaped) — the real risk there, not here.
- **A width/height role** — the current role set has none, so adopted image profiles use the graph's
  own dimension defaults for now. Adding a dimensions control is a T-506 (cover art) decision.
- **How the catalog reflects an adopted row** — an adopted profile is `workflow`-backed with
  `template: None`, so it does **not** join the T-505c curated index (which keys on `template`); the
  gallery row stays "bare" after adoption and the new profile shows in the Models step. Whether/how to
  surface "adopted" on the row is a T-505d-c question, noted so it is not a surprise.
- **The image generation pipeline** — reading this `OutputSpec` to actually produce a PNG is T-506.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505d-b-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read crates/create-core/src/profile.rs --file crates/create-core/src/emit.rs
```

`profile.rs` is `--read` for the `ModelKind` / `OutputSpec` / `ModelProfile` definitions the change
relies on; it does not change.
