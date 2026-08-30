# T-313d — profile emission

**Lane: architect-direct.** A pure builder plus a thin command around it.

**Depends:** T-313a, T-313b, T-313c (all landed). **Crate/dir:** `create-core`, `src-tauri`.

**Files to modify:**

- `crates/create-core/src/emit.rs` — **new**: `build_profile`, `MappedSlot`, `EmitError`, tests
- `crates/create-core/src/lib.rs` — `pub mod emit;`
- `src-tauri/src/import.rs` — the `save_imported_profile` command and its tests
- `src-tauri/src/lib.rs` — register the command

## Why

T-313b stores a workflow; T-313c says which slot does what. Nothing turns that into something the
app can run. 5b's bar is a user profile **indistinguishable from a shipped one**, which means a real
`ModelProfile` in `config_dir/profiles/` — the same directory T-313a's hand-written test profile
proved already loads and already reaches the picker.

## What the shipped profiles settle

Read rather than assumed, from `profiles/ace-step-1.5-turbo.json`:

| | shipped | emitted |
|---|---|---|
| `tags` | `default` is hand-written | the graph's own current text |
| `lyrics` | **no default**, plus `structure_tags` | no default, no tags |
| `steps` | `min 1, max 100` | the node's real `1..10000` |
| `duration_s` | `min 10, max 300, step 1` | the node's real bounds |
| `output` | `SaveAudioAdvanced`, `prefer_lossless: true` | the same |

**Two of those rows are honest limits, and the brief states them rather than hiding them.**

1. **Bounds come out wider.** The shipped profile's `steps: 1..100` is a human narrowing a node that
   really accepts `1..10000` (verified live). Emission uses the node's real bounds, because the
   alternative is inventing a range for a graph nobody here has seen. A user who wants 8 steps can
   still type 8.
2. **Lyrics never get a default**, copying the shipped profile's own stated reason: prefilled lyrics
   are words the app put in the user's mouth. Tags *do* get one, and the difference is not
   inconsistency — the tags in the user's own graph are their prompt, and MCP-SURFACE 20.2 is about
   a *template's* demo text running invisibly under an empty box, which is the opposite situation.

## The contract that makes the key names free

`app/src/state/generate.ts` finds the lyrics control **by `kind`, not by the name `"lyrics"`** — the
comment there says so explicitly, and a profile names its own inputs. So the binding contract is the
`InputSpec` *variant*, not the map key. Emission still uses the shipped names (`tags`, `lyrics`,
`negative`, `duration_s`, `seed`, `steps`, `cfg`) because a person reads this file, but nothing
breaks if a later task changes them.

## Spec

### 1. `create-core/src/emit.rs`

```rust
/// Numeric bounds for one input, from the live node registry.
pub struct Bounds {
    pub min: f64,
    pub max: f64,
    pub step: Option<f64>,
}

/// One slot a role was mapped to, with everything needed to declare it.
pub struct MappedSlot {
    pub address: String,
    /// `STRING` / `INT` / `FLOAT`, as `list_workflow_slots` reports it.
    pub widget_type: String,
    /// What the user's graph currently holds. The emitted default, because it
    /// is the value they already chose.
    pub current_value: Value,
    /// `None` when the registry had no bounds for this input.
    pub bounds: Option<Bounds>,
}

pub enum EmitError {
    /// A numeric role whose bounds the registry did not report. Refused rather
    /// than filled with a guess -- a slider with invented limits is worse than
    /// an absent control.
    NoBounds { role: Role, address: String },
    /// A role mapped to no slots at all.
    NoSlots { role: Role },
    /// The graph has no audio save node, so `ensure_lossless_output` would fail
    /// at generate time (`GraphError::NoSaveNode`). Caught here, where it can
    /// still be explained.
    NoSaveNode,
}

pub fn build_profile(
    id: &str,
    display_name: &str,
    workflow_path: &str,
    mappings: &[(Role, Vec<MappedSlot>)],
) -> Result<ModelProfile, EmitError>;
```

Per role:

- `Tags`, `Negative` → `InputSpec::Text { slots, default: current_value as string }`
- `Lyrics` → `InputSpec::Lyrics { slots, default: None, structure_tags: vec![] }`
- `Seed` → `InputSpec::Seed { slots }` — no bounds needed, which is why the T-313c hop to a
  `PrimitiveInt` costs nothing here
- `Steps` → `InputSpec::Int { slots, min, max, default }`
- `DurationSeconds`, `Cfg` → `Int` or `Float` **by the slot's widget type**, not by the role. ACE-Step's
  duration is `FLOAT` and MiniMax's `max_duration` may not be; the graph decides.

`ComfySpec`: `template: None`, `workflow: Some(path)`, `output: SaveAudioAdvanced` +
`prefer_lossless: true`, everything else default. **Never both `template` and `workflow`** — T-313a
refuses that and its test names this task as the thing that must not produce one.

`license`: `"Not declared (imported workflow)"`. The app has no idea what the user's graph is
licensed under and must not imply one; the field is required and shown wherever a model is chosen.

### 2. The command

`save_imported_profile(workflow_id, display_name, mappings)` in `import.rs`:

1. Resolve `config_dir/workflows/<workflow_id>.json`; refuse if absent.
2. Read the graph, check for a save node before anything else.
3. `list_slots` on the stored file for widget types and current values.
4. One `nodes(action="get")` **per distinct node class** for bounds — not per slot.
5. `build_profile`, then write `config_dir/profiles/<id>.json`.
6. `id` is `slugify(display_name)`, uniquified against the profiles directory the same way T-313b
   uniquifies workflow ids.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_an_emitted_profile_round_trips_as_a_model_profile`** — serialize, deserialize with the
      real `ModelProfile` deserializer, and compare. The invariant: emitted profiles load through
      exactly the path shipped ones do. A profile the app cannot read back is the whole failure mode.
- [ ] **`test_an_emitted_profile_never_declares_both_sources`** — `template` is `None` and
      `workflow` is set. Pairs with T-313a's `test_a_profile_declaring_both_is_refused`.
- [ ] **`test_lyrics_get_no_default_but_tags_do`** — the one deliberate asymmetry.
- [ ] **`test_duration_follows_the_widget_type_not_the_role`** — a `FLOAT` slot gives `Float`, an
      `INT` slot gives `Int`.
- [ ] **`test_a_numeric_role_without_bounds_is_refused_rather_than_guessed`** — `EmitError::NoBounds`
      naming the role and address. The invariant: no invented slider limits.
- [ ] **`test_a_graph_with_no_audio_save_node_is_refused`** — with a message a person can act on,
      caught here rather than at generate time.
- [ ] **`test_the_emitted_seed_needs_no_bounds`** — a `Seed` mapping with `bounds: None` succeeds.
      This is what makes T-313c's `PrimitiveInt` hop free.
- [ ] Mutation: emitting a `template` alongside the workflow must fail a test; giving lyrics a
      default must fail a test; defaulting absent bounds to `0..100` must fail a test.

## Out of scope

- **The UI.** T-313e collects the mappings and calls this.
- **LoRA support on an imported profile.** `LoraSupport` needs `loader_node` and `attach_after`,
  which is graph-shape reasoning; a user whose graph already has LoRAs wired does not need it.
- **`prompt_guide`, `lyrics_contract`, `vram_gb_min`, `models`.** All optional, none derivable from
  a graph. Absent is honest; invented is not.
- **Editing or deleting an emitted profile.** Its own task.
- **Narrowing bounds to something musically sensible.** Stated above as a known limit.

## Changed during implementation and review

**1. `build_profile` takes the graph**, not just a path, so the save-node check happens inside the
pure builder where it is testable rather than in the command where it is not.

**2. `has_audio_save_node` scans subgraph interiors too.** A top-level-only scan would have refused
MiniMax, whose save node lives inside a subgraph and which `ensure_lossless_output` rewrites at both
levels. Caught by writing the test, not by the compiler.

**3. `Role` gained `Deserialize`** so a mapping can arrive from the frontend.

**4. Added `test_an_emitted_profile_loads_through_the_real_loader`**, which is the test that
actually means something. The brief asked for a serde round trip, and that proves the *struct*.
5b's bar is a profile indistinguishable from a shipped one, so the emitted **file** is loaded back
through `library::profiles::load` -- the same call five commands make -- from the directory the
picker really reads. A round trip inside `create-core` would pass even if the profile landed
somewhere nothing looks.

**5. `bounds_of` treats a half-open range as no range.** `emit` refuses a numeric control it cannot
bound, and filling in a missing end here would have defeated that by the back door.

Three mutations, three killed. One had to be re-run: the first attempt inserted a duplicate struct
field and did not compile, which is a *broken* mutation rather than a surviving one -- worth
distinguishing, since "no test failed" looks the same in both cases.
