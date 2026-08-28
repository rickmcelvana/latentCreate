# T-309a — the LoRA catalog reaches the panel

**Lane: architect-direct** (WORKFLOW §1). Two Rust types, one command, one pure TypeScript
module and a store — small, test-bearing, and every interesting line is an invariant that has
to be written and then mutated. Writing the reference *is* writing the task, so there is
nothing here for an executor to save.

**T-309b is the Aider run**: `<LoraStack>`, its `theme.css` block and the AudioStudio wiring —
about 300 lines of JSX and CSS with no logic in it, the same shape as T-308b Part 2.

---

## What was verified before writing this

ComfyUI was running, so both of these are live reads rather than recollections.

**1. The list has not moved.** `nodes(action="get", name="LoraLoaderModelOnly")` returns the
same 53 `lora_name` choices as the T-307 fixture, and the live response carries **no `stale`
key and no warning** — a second independent confirmation of the shape T-308c's click-through
established (MCP-SURFACE §19.1). `strength_model` is `min: -100, max: 100, step: 0.01,
default: 1.0`; the ACE-Step profile offers `0.0–2.0, step 0.05`, and **the panel follows the
profile**. That narrowing is deliberate and is not a mistake to correct.

**2. A doc claim I wrote yesterday was wrong, and the correction changes this panel's copy.**
§19.1 said a cached list "offers files the user has deleted", and that picking one completes
silently with no LoRA applied (17.6). Measured instead: two spliced copies of the turbo
template, one loader node each, differing only in `lora_name`.

| `lora_name` | in the live list | `validate_workflow` |
|---|---|---|
| a path no longer installed | no | **`valid: false`** — `unknown_enum_value` on `lora_name` |
| `loragoth/final/training_state.pt` | yes | **`valid: true`**, zero errors |

So the two failure modes are not one failure mode:

- **A deleted LoRA is loud, and loud early.** The pipeline already validates the edited copy
  (T-306b), so a stale picker produces a rejected job — not a track quietly missing its LoRA.
- **A non-adapter is silent**, because it is a legitimate member of the enum and validation
  cannot help. `create-core::loras` excluding those is the only thing standing between the user
  and a no-op run.

**Consequence for this brief:** a stale LoRA list is a **short** list, not a wrong one. The
cache note must say what is *missing*, not caution about what is shown. Recorded as
MCP-SURFACE §19.3, with §19.1 corrected and the matching claim in `params.ts` fixed.

---

## What lands

### Rust — `create-core`

Derive `Serialize` (not `Deserialize` — nothing reads these back) on `LoraCatalog`,
`LoraGroup`, `LoraEntry`, `Excluded`, and `ExclusionReason` with
`#[serde(rename_all = "snake_case")]`. No view type: the catalog is a computed value with one
consumer, and a field-for-field copy in `src-tauri` would be free to drift from it — the same
reasoning `profile_inputs` already follows.

### Rust — `src-tauri/src/loras.rs` (new)

```rust
pub struct LoraPanel {
    pub strength: StrengthRange,   // the profile's range, never the node's
    pub max_stack: u8,
    pub catalog: CatalogState,
}

#[serde(tag = "state", rename_all = "snake_case")]
pub enum CatalogState {
    Loaded { groups: Vec<LoraGroup>, excluded: Vec<Excluded>, cached: bool },
    Unavailable { detail: String },
}

#[tauri::command]
pub async fn lora_panel(..., profile_id: String) -> Result<Option<LoraPanel>, String>
```

**`None` hides the panel; `Unavailable` shows it empty with a sentence.** That distinction is
the whole point of the tagged enum, and it is the same rule T-308b's copy table keeps between
"ComfyUI is off" and "this model has no such input". A panel that vanishes when ComfyUI is down
tells the user their model does not support LoRAs. `None` is returned for an unknown profile
**and** for a profile with no `loras` block, because both genuinely mean *render nothing* — and
an unknown profile is already reported by the param panel directly above it.

`cached` is `schema.is_cached()`, the classification T-308c landed. Backend classifies, view
renders.

### TypeScript — `app/src/state/loras.ts` (pure) and `loraPanel.ts` (store)

The same split as `params.ts` / `paramPanel.ts`, for the same reason: vitest runs in `node`
with no DOM, so a rule that lives in JSX is a rule no test can reach.

The pure module carries `StackRow { path, label, strength, enabled }`, `pickerGroups`,
`addable`, `add`, `removeAt`, `toggleAt`, `setStrengthAt`, `move`, `missingFrom`, `specLoras`,
and the copy constants.

`StackRow` carries its own `label` rather than looking one up: a row must still render after a
Retry whose catalog no longer lists it, which is invariant 6.

### The invariants, each with the test that protects it

1. **A path already in the stack is not offered again.** Two loader nodes for one file is a
   strength the user could have typed once, applied twice with nothing on screen saying so.
   *Vacuity trap:* a test that stacks two different LoRAs passes with the rule deleted — the
   test has to try to add the same one twice.
2. **Bypassed rows survive `specLoras`.** `GenerationSpec.loras` records the stack and
   `active_loras()` is what filters it (generation.rs says so in its own doc). Dropping disabled
   rows here would make a bypass indistinguishable from a delete in the provenance sidecar.
3. **Order is the apply order, end to end.** `splice_loras` chains loaders in list order, so a
   reorder changes the audio. The test reorders and asserts `specLoras` follows — a stack of one
   proves nothing.
4. **The cap is the profile's `max_stack`, not a constant.** `splice_loras` already errors on
   overflow; a UI that lets someone add a fifth turns that guard into a failed job. This is the
   phase's recurring shape — *a guard in one layer does not bind the layer above it* — for the
   fourth time, so it gets its own test rather than a comment.
5. **Strength comes from the profile's range.** 0.0–2.0 step 0.05, never the node's
   -100…100 step 0.01.
6. **A row whose path the catalog no longer offers is named, not blanked.** Reachable by
   deleting a LoRA and pressing Retry. Per §19.3 this is the case validation would reject at
   submit, so saying it in the panel first is strictly earlier feedback.
7. **Switching profiles clears the stack; reloading the same profile does not.** A LoRA chosen
   for ACE-Step is meaningless on MiniMax, which has no `loras` block at all — and the reload
   guard is the one `paramPanel` already needed, for the same tab-switch reason.

### The fixture, and how it is stopped from drifting

`loras.test.ts` cannot call Rust, so it needs the catalog in the shape the bridge delivers:
commit `testdata/mcp/lora_catalog.ace-step.json`, **generated from `catalog(installed())` over
the real 53-entry capture**, carrying a `_derived_from` field naming its source.

Hand-writing that fixture is the trap this repo has already named twice: a fixture written to
agree with the code agrees with the code. So a Rust test asserts the committed JSON equals
`serde_json::to_value(catalog(installed()))`. There is no generator between the two languages,
so drift is otherwise silent — exactly the gap `profile_inputs`' wire-shape tests exist to
close.

### The copy

| Where | Text |
|---|---|
| Cached list | `This list came from ComfyUI's cache. A LoRA added since ComfyUI last ran will not be here. Start ComfyUI, then Retry.` |
| Unavailable | `Your installed LoRAs could not be read because ComfyUI is not running. Start it, then Retry.` |
| Loose-file group | `Loose files` |
| Row no longer listed | `{label} is no longer in your loras folder.` |
| Excluded count | `{n} files in your loras folder are not adapters and are not offered.` |

The cached sentence is the one §19.3 rewrote. It would previously have warned about ghosts;
what a stale list actually costs the user is a LoRA they finished training an hour ago being
absent.

`Loose files` is the wording T-307 explicitly refused to choose (`LoraGroup::name` is empty for
files in the `loras` root, and that module's doc says the panel owns the word). On this install
it holds the two `minimax_h3_fl2v_turbo_*` video LoRAs — which, per MCP-SURFACE §4, cannot be
filtered out by filename and are deliberately still offered.

## Acceptance criteria

1. `npm run gate` green, `cargo fmt` clean, `oxlint` adds no warnings.
2. Every one of the seven invariants above has a test naming what it protects, and a mutation
   of the rule it describes is killed.
3. No `invoke` outside `app/src/bridge/` (WORKFLOW §4.5).
4. Nothing derived in JSX — this task adds no JSX at all.
5. The Rust fixture-equality test fails if `catalog` changes and the JSON does not.

## Out of scope

- **`<LoraStack>` and its CSS — T-309b**, the Aider run.
- **Favourites and user display names — T-309c.** Deferred here from T-307, and deferred again
  on purpose: both are persisted user state keyed on the entry path, which is a config-schema
  change, and neither can be designed before the owner has seen the real labels.
- **Wiring the stack into `GenerationSpec` — T-310.** `specLoras` is written and tested here;
  nothing calls it until submit exists, exactly as `specInputs` has been sitting since T-308a.
- Drag-and-drop reordering. `move(from, to)` is the pure function; T-309b gives it buttons, and
  pointer-driven reordering is polish a DOM-less vitest cannot reach.

## The question T-309b's click-through has to answer

T-307 deferred cosmetic renaming to this task with a reason: it is the rule MCP-SURFACE §12.2
says needs the owner looking at a panel. Here is what the mechanical labels produce on the real
install, so the question can be asked against the actual screen rather than in the abstract —
four of the twelve are the full directory name, prefix included, and two are 40-character
filenames:

- `ACE-Step-v1.5-acoustic-guitar-and-a-merge-LoRA`, `vocal_instrument_merge`
- `ACE-Step-v1.5-ambient_dream1-LoRA`
- `ACE-Step-v1.5-chinese-new-year-LoRA`
- `ACE-Step-v1.5-raspy-vocal-and-instrumental-5-LoRAs`, `instrumental`, `male_vocals`,
  `voc_06_inst_14`, `voc_14_inst_06`
- `final` — one representative, with 20 checkpoints behind the expander
- `minimax_h3_fl2v_turbo_4step_v1.0_768p_comfyui_bf16`,
  `minimax_h3_fl2v_turbo_8step_v1.0_comfyui_bf16`

Stripping `ACE-Step-v1.5-` and a trailing `-LoRA` reads much better and is one line of code. It
is not written here because a rule that improves these twelve could mangle someone else's
naming scheme, and nobody has looked at a second install.
