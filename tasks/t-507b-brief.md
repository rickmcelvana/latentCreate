# T-507b: an adopted profile declares its model files, so its Setup row reads Ready

**Follows:** T-507a/T-507a-2 (the frontend half of T-507, landed). This is the backend carry-over
from T-505d-d's click-through, and the **last piece of T-507**.
**Dir:** `crates/create-core` + `src-tauri` | **Lane:** Aider — one pure selector + a tiny inventory
query in create-core (both unit-tested), then wire them into the existing `emit_profile` seam.
**No mcp-bridge change** — every transport call this needs already exists.

## The problem, verified

`emit::build_profile` sets `comfy.models = Vec::new()` (emit.rs:195). An adopted/imported profile
therefore declares no model files, and `ModelProfile::readiness` returns **`Undeclared`** the moment
the list is empty (readiness.rs:102) — the Setup Models row reads **"Cannot check / This profile
does not list the model files it needs."** (models.ts `rowFor`, the `undeclared` arm). This is
*correct* for a hand-authored graph the app cannot vouch for, but wrong for a gallery row the app
adopted **because `local_check` said it was runnable** — every file is present, we just never wrote
the list down. Nothing gates on readiness (verified T-505d-d), so the profile is fully usable; this
is presentation only. The fix makes the row read **Ready**.

## Why this is the pure/live split again

`build_profile` is pure and offline, and stays that way — it cannot know which *folder* a filename
lives in without asking a running ComfyUI, and readiness itself needs a live inventory for exactly
this reason (readiness.rs module docs). So:

- **create-core (pure, tested):** given the candidate filenames and a `ModelInventory`, produce the
  `ModelFileSpec` list. The inventory is the filter — a value in some model folder is a model file;
  a COMBO value that is in no folder (`euler`, `en`, `normal`) is not.
- **src-tauri `emit_profile` (has the live `&LocalComfy`):** enumerate the model folders, build the
  inventory, pull the graph's COMBO values from the slots it already fetched, call the pure function,
  and assign the result to `profile.comfy.models`.

**Do not** identify loaders by node class. The inventory match is the whole filter — it needs no
list of loader names, handles Klein's three `*_name` COMBOs and any custom loader alike, and cannot
mistake a sampler name for a weight file.

## Files to modify (three)

- `crates/create-core/src/readiness.rs` — `ModelInventory::folder_of` + its test
- `crates/create-core/src/emit.rs` — `resolve_model_files` + its tests
- `src-tauri/src/import.rs` — wire both into `emit_profile`; extend the `emit_replies` mock + assert

---

## §1 — `ModelInventory::folder_of` (`readiness.rs`)

Add to the `impl ModelInventory` block, beside `has`:

```rust
/// The folder holding `file`, or `None` when no listed folder has it.
///
/// The inverse of `has`: readiness asks "is this file in *that* folder?"; the
/// adopt path (T-507b) has a filename and must discover which folder it lives
/// in. First match wins -- a file of the same name in two folders is not a
/// case any real model install produces, and either answer is correct for
/// readiness (both folders have it).
pub fn folder_of(&self, file: &str) -> Option<&str> {
    self.by_folder
        .iter()
        .find(|(_, files)| files.contains(file))
        .map(|(folder, _)| folder.as_str())
}
```

**Test** (`readiness.rs` tests): *Protects: a filename resolves to the folder that holds it, and an
unknown filename resolves to nothing.* Build a `ModelInventory::new` with two folders
(`diffusion_models` → `{klein.safetensors}`, `vae` → `{ae.safetensors}`); assert
`folder_of("klein.safetensors") == Some("diffusion_models")`, `folder_of("ae.safetensors") ==
Some("vae")`, and `folder_of("euler") == None`.

## §2 — `resolve_model_files` (`emit.rs`)

Add near `build_profile`. It needs `ModelInventory` (same crate) and `ModelFileSpec`,
`std::collections::BTreeSet`:

```rust
use crate::readiness::ModelInventory;
// ModelFileSpec is already reachable via crate::profile in this module's use list.

/// The model files a graph's COMBO slot values name, resolved to their folders.
///
/// `candidates` are the string values of the graph's COMBO slots. Most are model
/// filenames (`flux_klein.safetensors`); a COMBO also carries choices like a
/// sampler name or a language code. `inventory` is the filter and the folder
/// source at once: a value present in some model folder becomes a
/// `ModelFileSpec`, one in no folder is dropped.
///
/// `source_url`/`size_bytes` stay `None`. The app resolved where a file *is*,
/// not where to *fetch* it, and must never imply it can download someone else's
/// weights (the T-505d rule). Duplicates -- the same file named by two slots --
/// collapse; order follows first appearance so the emitted list is stable.
pub fn resolve_model_files(candidates: &[String], inventory: &ModelInventory) -> Vec<ModelFileSpec> {
    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for file in candidates {
        if !seen.insert(file.as_str()) {
            continue;
        }
        if let Some(folder) = inventory.folder_of(file) {
            out.push(ModelFileSpec {
                file: file.clone(),
                folder: folder.to_string(),
                // The app resolved where the file *is*, not where to fetch it or
                // under what terms -- all three stay None. `license` is per-file
                // and only set when it differs from the profile's own; an adopted
                // graph declares none.
                source_url: None,
                size_bytes: None,
                license: None,
            });
        }
    }
    out
}
```

**Tests** (`emit.rs` tests), each naming the invariant. Build a shared inventory with
`diffusion_models → {klein.safetensors}`, `text_encoders → {clip.safetensors}`,
`vae → {ae.safetensors}`:

- **the three Klein-shaped filenames resolve to their folders** — `["klein.safetensors",
  "clip.safetensors", "ae.safetensors"]` → three specs, each `folder` correct, each `source_url` and
  `size_bytes` `None` (and `license` `None`). *Protects the headline: an adopted graph's loader
  values become a declared, checkable file list — without ever claiming a fetch URL.*
- **a non-model COMBO value is dropped** — `["euler", "klein.safetensors"]` → one spec
  (`klein.safetensors`). *Protects: the inventory is the filter; a sampler/scheduler/language choice
  is not a weight.*
- **a duplicate filename collapses** — `["klein.safetensors", "klein.safetensors"]` → one spec.
  *Protects: two slots naming the same checkpoint declare it once.*
- **all-unknown candidates give an empty list** — `["euler", "en"]` → `[]`, i.e. still
  `Undeclared`, which is correct: nothing here is a model file. *Protects: the change never invents
  a declaration.*

## §3 — wire into `emit_profile` (`src-tauri/src/import.rs`)

`emit_profile` already has `comfy: &LocalComfy` and the `slots: SlotList` it read for
`resolve_mappings`. Add a best-effort inventory build and the enrichment. Imports needed:
`create_core::emit::resolve_model_files`, `create_core::readiness::ModelInventory`,
`std::collections::{BTreeMap, BTreeSet}` (BTreeMap may already be imported).

A small helper, near `resolve_mappings`:

```rust
/// The model files ComfyUI currently has, across every folder it knows.
///
/// Unlike `models::take_inventory` (which lists only the folders a set of
/// profiles already name), an adopted profile names no folders yet, so this
/// walks them all -- `list_model_folders` then one listing per folder. It is a
/// one-shot cost on a deliberate adopt, not a hot path.
///
/// **Best-effort:** any transport failure yields an empty inventory, and the
/// profile then saves with `comfy.models = []` -- exactly today's behaviour
/// (`Undeclared`), never a failed adopt. The profile is usable regardless;
/// nothing gates on readiness.
async fn full_inventory(comfy: &LocalComfy) -> ModelInventory {
    let folders = match comfy.list_model_folders().await {
        Ok(f) => f.folders,
        Err(_) => return ModelInventory::default(),
    };
    let mut listed: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for folder in folders {
        if let Ok(contents) = comfy.list_models_in(&folder.name).await {
            listed.insert(
                folder.name.clone(),
                contents.files.into_iter().map(|f| f.name).collect(),
            );
        }
    }
    ModelInventory::new(listed)
}
```

Then in `emit_profile`, change `let profile = build_profile(...)` to `let mut profile`, and after it
(before writing to disk), enrich:

```rust
    let mut profile = build_profile(
        &profile_id,
        display_name,
        &graph,
        &stored.display().to_string(),
        &resolved,
    )
    .map_err(|e| e.to_string())?;

    // Declare the model files the graph's COMBO slots name, so an adopted row
    // reads Ready instead of "Cannot check". Best-effort: an inventory we could
    // not take leaves the list empty, which is today's Undeclared state.
    let candidates: Vec<String> = slots
        .slots
        .iter()
        .filter(|s| s.ty == "COMBO")
        .filter_map(|s| s.current_value.as_str().map(str::to_string))
        .collect();
    let inventory = full_inventory(comfy).await;
    profile.comfy.models = resolve_model_files(&candidates, &inventory);
```

The COMBO slots include subgraph loaders: `list_slots` flattens Klein's `75/70.unet_name` etc. into
the same `slots` vec with `ty: "COMBO"` and the filename in `current_value`, so no subgraph walk is
needed here.

### The mock, and the assertion it earns

`emit_profile` now makes two more calls after the node-schema reads: `list_model_folders`, then one
`list_models_in` per folder returned. The sequential `emit_replies()` **must** answer them or the
existing `test_an_emitted_profile_loads_through_the_real_loader` fails on a starved mock. Extend it,
and make the extension prove the feature:

1. Add a COMBO model slot to the `list_slots` reply (reply[0]) — e.g.
   `{ "address": "5.ckpt_name", "name": "ckpt_name", "type": "COMBO",
   "current_value": "ace_step.safetensors", "instance_id": "5",
   "node_type": "CheckpointLoaderSimple" }`. (It needs no role mapping; it is read only for the
   inventory match.)
2. Append, **after** the two node-schema replies, in this order:
   - the folder list: `Reply::Json(json!({ "count": 1, "folders": [{ "name": "checkpoints" }] }))`
   - the folder listing: `Reply::Json(json!({ "folder": "checkpoints", "count": 1,
     "files": [{ "name": "ace_step.safetensors" }] }))`

   (Confirm the field names against `ModelFolders`/`ModelFolder` in `crates/mcp-bridge/src/models.rs`
   — `folders[].name`, `files[].name` — and match the shape the deserializer expects.)
3. In that test, add: `assert_eq!(loaded.profile.comfy.models.len(), 1);` and that the one spec is
   `ace_step.safetensors` in `checkpoints` with `source_url`/`size_bytes`/`license` all `None`.

If matching the exact live JSON shape for the two replies proves fiddly, fall back to a **separate**
test with its own reply vec rather than bending the existing one — but the existing test's replies
must still be extended enough not to starve, since the two new calls now always run.

## Out of scope

- No change to `build_profile`'s signature or its pure `Vec::new()` default — the enrichment lives
  at the live seam, not in the pure builder.
- No curated-row behaviour change: curated rows already carry a hand-verified file list and never
  reach `emit_profile` (the T-505d "bare rows adopt, curated rows install" rule).
- No mcp-bridge change — `list_model_folders`/`list_models_in` already exist.
- No frontend change: `rowFor` already renders `Ready` for a populated list; this just stops the
  list being empty.

## Gate & acceptance

- `npm run gate` green (this is a Rust-side change; the create-core and src-tauri suites carry it).
- New unit tests: `folder_of` (3 cases), `resolve_model_files` (4 cases), and the enriched
  `emit_profile` assertion.
- Producer click-through (needs a running ComfyUI with an image model installed):
  1. Adopt a **bare, ready** gallery image row (e.g. Flux.2 Klein 9B) from the Setup catalog — the
     T-505d "Bring in" path.
  2. On the Setup **Models** step, the adopted profile's row now reads **Ready** (a green
     "Installed" pill), **not** "Cannot check / Undeclared".
  3. Open the saved profile JSON under `profiles/`: `comfy.models` lists the graph's model files
     (Klein: three — the unet, clip and vae filenames) each with its resolved `folder` and
     `source_url: null`, `size_bytes: null`.
  4. (Regression) A **hand-imported** workflow whose model file is genuinely *not* installed still
     reads sensibly — the file it does have resolves; a missing one is simply absent from the list,
     never invented.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-507b-brief.md --read crates/create-core/src/profile.rs --read crates/mcp-bridge/src/models.rs --read crates/mcp-bridge/src/slots.rs --read src-tauri/src/models.rs --file crates/create-core/src/readiness.rs --file crates/create-core/src/emit.rs --file src-tauri/src/import.rs
```
