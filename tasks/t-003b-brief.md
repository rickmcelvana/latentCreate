# T-003b: create-core project, generation and provenance types
**Depends:** T-003 | **Crate:** `crates/create-core` | **Executor:** Aider

**Files to create/modify:**
`crates/create-core/src/generation.rs` (new),
`crates/create-core/src/project.rs` (new),
`crates/create-core/src/provenance.rs` (new),
`crates/create-core/src/lib.rs` (modify — add the three `pub mod` lines and re-exports)

> **Do not touch** `profile.rs` or `profiles/ace-step-1.5-turbo.json`; T-003 reviewed them.

## Goal
The runtime half of the domain: what the user asked for (`GenerationSpec`), what the
library stores (`Project`, `LyricDoc`, `Track`), and what makes a result reproducible
(`Provenance`). Still pure data plus a little pure logic — **no I/O, no async, no clock,
no id generation** (that is `library`'s job in Phase 1).

## Design decisions already made — implement, do not revisit

1. **`InputValue` is adjacently tagged**, i.e. `{"type": "seed", "value": 12345}`.
   Untagged would be shorter, but JSON `3` could deserialise as `Int`, `Float` *or*
   `Seed`, and serde takes the first match — so a seed of 3 would silently come back as
   an `Int`. Provenance exists to reproduce a track exactly; an ambiguous encoding
   defeats it.
2. **One source of truth per track** (ARCHITECTURE §8). `project.json` holds an ordered
   list of track *ids*; title, file, duration and provenance live only in the track's own
   sidecar. Storing a title in both would guarantee drift on the first rename.
3. **Provenance keeps both levels**: the `GenerationSpec` (semantic, `duration_s = 120`)
   and `resolved_slots` (what the graph received, `94.duration = 120` *and*
   `98.seconds = 120`). The first powers "re-use these settings"; the second is the only
   record of what actually ran.
4. **Timestamps are RFC 3339 strings**, not a date type. It keeps `create-core`
   dependency-free; `library` supplies real times. Document the format on each field.
5. **Ids are opaque newtypes over `String`** — filesystem-safe and sortable is the
   contract, but generating them is out of scope here.

## Spec — reference implementation
Integrate essentially verbatim. Every public item needs a `///` doc comment.

### `generation.rs`
```rust
use crate::profile::SlotAddress;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A concrete value for one input, tagged so it survives a JSON round trip.
///
/// Adjacently tagged on purpose: untagged, a JSON `3` could deserialise as `Int`,
/// `Float` or `Seed`, and a seed silently demoted to an `Int` would make a track
/// unreproducible — the one thing provenance must never allow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum InputValue {
    Text(String),
    Int(i64),
    Float(f64),
    /// Generation seed. `u64` because ACE-Step's range reaches `u64::MAX` (T-003).
    Seed(u64),
    /// A choice from a fixed set (key/scale, language, time signature).
    Enum(String),
    Bool(bool),
}

/// One LoRA in the stack. Order is this value's position in `GenerationSpec::loras`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoraRef {
    /// Exactly as ComfyUI lists it in `lora_name`, including any sub-directory —
    /// e.g. `"ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors"`.
    /// Never normalise the separators; this string is passed back verbatim.
    pub file: String,
    /// Applied strength; UI range is usually 0.0..=2.0 (profile decides).
    pub strength: f64,
    /// Bypassed entries stay in the list so the user can toggle without losing them.
    #[serde(default = "default_true")]
    pub enabled: bool,
}

fn default_true() -> bool {
    true
}

/// Which lyric document and version a generation used.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricRef {
    pub doc_id: LyricDocId,
    pub version: u32,
}

/// Opaque id for a lyric document. Filesystem-safe and sortable; `library` mints it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LyricDocId(pub String);

/// Everything needed to run one generation: the semantic choices, before they are
/// fanned out to slot addresses.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GenerationSpec {
    /// `ModelProfile::id` this spec was built against.
    pub profile_id: String,
    /// Semantic input name (`"tags"`, `"duration_s"`, `"seed"`) -> chosen value.
    pub inputs: BTreeMap<String, InputValue>,
    #[serde(default)]
    pub loras: Vec<LoraRef>,
    #[serde(default)]
    pub lyrics: Option<LyricRef>,
}

impl GenerationSpec {
    /// Conventional key under which the seed is stored in `inputs`.
    pub const SEED_KEY: &'static str = "seed";

    /// The seed, if one is set. `None` when the profile has no seed input.
    pub fn seed(&self) -> Option<u64> {
        match self.inputs.get(Self::SEED_KEY) {
            Some(InputValue::Seed(s)) => Some(*s),
            _ => None,
        }
    }

    /// This spec with a different seed, leaving every other input untouched.
    ///
    /// This is how a batch works: one spec, N seeds (ARCHITECTURE §7).
    pub fn with_seed(&self, seed: u64) -> Self {
        let mut next = self.clone();
        next.inputs
            .insert(Self::SEED_KEY.to_string(), InputValue::Seed(seed));
        next
    }

    /// The LoRAs that will actually be applied, in order, skipping bypassed entries.
    pub fn active_loras(&self) -> impl Iterator<Item = &LoraRef> {
        self.loras.iter().filter(|l| l.enabled)
    }
}

/// The slot values actually submitted to ComfyUI, after fan-out.
pub type ResolvedSlots = BTreeMap<SlotAddress, InputValue>;
```

### `project.rs`
```rust
use crate::generation::LyricDocId;
use serde::{Deserialize, Serialize};

/// Opaque id for a track. Filesystem-safe and sortable; `library` mints it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TrackId(pub String);

/// Where a lyric version came from. Records whether the user accepted an optimised
/// prompt, which must never happen silently (ARCHITECTURE §6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LyricSource {
    /// Typed or pasted by the user.
    Human,
    /// Generated by an LLM.
    Llm {
        model: String,
        /// True only if the user accepted an optimised prompt for this generation.
        #[serde(default)]
        prompt_optimized: bool,
    },
    /// Hand-edited from an earlier version.
    Edited { from_version: u32 },
}

/// One immutable revision of a lyric document. Versions are whole copies: cheap at
/// this size, and it means an edit can never corrupt the text it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricVersion {
    /// 1-based, monotonically increasing within a document.
    pub number: u32,
    pub text: String,
    /// RFC 3339, e.g. `"2026-08-23T18:31:24Z"`.
    pub created_at: String,
    pub source: LyricSource,
}

/// A set of lyric revisions, one of which may be approved for audio generation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricDoc {
    pub id: LyricDocId,
    #[serde(default)]
    pub title: Option<String>,
    pub versions: Vec<LyricVersion>,
    /// `LyricVersion::number` the user approved, if any. Only an approved version can
    /// be sent to AudioStudio.
    #[serde(default)]
    pub approved: Option<u32>,
}

impl LyricDoc {
    /// The approved version, if one has been chosen.
    pub fn approved_version(&self) -> Option<&LyricVersion> {
        let n = self.approved?;
        self.versions.iter().find(|v| v.number == n)
    }

    /// The most recent version by number, if the document has any.
    pub fn latest(&self) -> Option<&LyricVersion> {
        self.versions.iter().max_by_key(|v| v.number)
    }
}

/// A named ordering of tracks — a single, an EP, an album.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AlbumList {
    pub name: String,
    #[serde(default)]
    pub tracks: Vec<TrackId>,
}

/// A working project. Holds *ids only* for tracks: every fact about a track lives in
/// its sidecar, so a rename cannot leave two files disagreeing (ARCHITECTURE §8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Directory name under `library/projects/`; filesystem-safe.
    pub slug: String,
    pub name: String,
    /// RFC 3339.
    pub created_at: String,
    /// Ordered; newest last.
    #[serde(default)]
    pub tracks: Vec<TrackId>,
    #[serde(default)]
    pub lyrics: Vec<LyricDocId>,
    #[serde(default)]
    pub albums: Vec<AlbumList>,
}
```

### `provenance.rs`
```rust
use crate::generation::{GenerationSpec, ResolvedSlots};
use crate::project::TrackId;
use serde::{Deserialize, Serialize};

/// Which ComfyUI produced a track, for when a result cannot be reproduced later.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComfyServerInfo {
    #[serde(default)]
    pub comfyui_version: Option<String>,
    #[serde(default)]
    pub comfy_cli_version: Option<String>,
    /// Endpoint the job was submitted to, e.g. `"http://127.0.0.1:8188"`.
    #[serde(default)]
    pub url: Option<String>,
}

/// The full recipe for one generated asset.
///
/// Complete enough to reproduce the result — including the LoRA stack, which lives in
/// `spec`. A LoRA-generated track that cannot be recreated from its sidecar is a bug
/// (CONVENTIONS.md).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Provenance {
    /// `ModelProfile::id`.
    pub profile_id: String,
    pub profile_display_name: String,
    /// The model's licence, copied at generation time — some weights are
    /// open-with-conditions, and the user may need it long after generating.
    pub model_license: String,
    /// Gallery template name, when one was used.
    #[serde(default)]
    pub template: Option<String>,
    /// What the user chose, in semantic terms.
    pub spec: GenerationSpec,
    /// What ComfyUI actually received, after one control fanned out to its slots.
    #[serde(default)]
    pub resolved_slots: ResolvedSlots,
    #[serde(default)]
    pub comfy: Option<ComfyServerInfo>,
    /// RFC 3339, when generation finished.
    pub created_at: String,
}

/// One generated audio file: the contents of `tracks/<id>.json`, the sidecar that is
/// the single source of truth for this track (ARCHITECTURE §8).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    pub id: TrackId,
    #[serde(default)]
    pub title: Option<String>,
    /// Path relative to the project directory, e.g. `"tracks/abc123.flac"`.
    pub file: String,
    /// Length in seconds, when known.
    #[serde(default)]
    pub duration_s: Option<f64>,
    pub provenance: Provenance,
}
```

### `lib.rs`
Add `pub mod generation;`, `pub mod project;`, `pub mod provenance;` with matching
`pub use ...::*;` re-exports, alongside the existing `profile` ones. Update the
crate-level doc comment: the T-003b types now exist, so the sentence pointing at T-003b
should go.

## Tests
Put each module's tests in that module's own `#[cfg(test)] mod tests`.

**`generation.rs`**
- `test_seed_value_does_not_collapse_to_int` — serialise `InputValue::Seed(3)`, re-parse,
  and assert it is still `Seed(3)` and not `Int(3)`. This is the whole reason for the
  adjacent tagging; it must fail if someone switches to `untagged`.
- `test_lora_path_with_backslashes_roundtrips` — a `LoraRef` whose `file` is
  `"ACE-Step-v1.5-ambient_dream1-LoRA\\adapter_model.safetensors"` survives JSON
  unchanged. Real ComfyUI LoRA names are sub-paths with backslashes.
- `test_with_seed_replaces_only_the_seed` — build a spec with tags + duration + seed,
  call `with_seed(99)`, and assert the seed changed while every other entry is identical.
- `test_seed_returns_none_when_absent`.
- `test_active_loras_skips_disabled` — three LoRAs, middle one disabled, order preserved.

**`project.rs`**
- `test_approved_version_returns_that_version` and
  `test_approved_version_is_none_when_unapproved`.
- `test_latest_returns_highest_numbered_version` — include versions out of order in the
  `Vec` to prove it sorts by `number`, not by position.
- `test_lyric_source_llm_records_optimized_flag` — round-trip
  `LyricSource::Llm { prompt_optimized: true, .. }`.

**`provenance.rs`**
- `test_track_sidecar_roundtrips` — a `Track` with provenance, two LoRAs and populated
  `resolved_slots` survives serialise/parse unchanged.
- `test_resolved_slots_records_fan_out` — build a `Provenance` whose `spec` has one
  `duration_s` input while `resolved_slots` carries both `94.duration` and `98.seconds`,
  and assert both are present after a round trip. This is the reproducibility guarantee
  in ARCHITECTURE §7 made testable.

## Acceptance criteria
- [ ] `cargo test -p create-core` passes, all named tests present
- [ ] `cargo clippy --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean
- [ ] Every public item has a `///` doc comment
- [ ] `npm run gate` green from the repo root
- [ ] No new dependencies (`serde` and dev-only `serde_json` already present)
- [ ] No changes outside the listed files

## Out of scope
Loading or writing any file. Generating ids or timestamps. Validating a `GenerationSpec`
against a `ModelProfile` (Phase 1, needs the profile loader). Album/track mutation
helpers beyond those specified. Any UI.

## Notes for the executor
- `BTreeMap`, never `HashMap` — sidecars must diff cleanly in git.
- Do not add `deny_unknown_fields`; forward compatibility is deliberate.
- `LoraRef::file` is passed to ComfyUI verbatim. Do not normalise, trim or re-case it.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/create-core/src/profile.rs --file crates/create-core/src/generation.rs --file crates/create-core/src/project.rs --file crates/create-core/src/provenance.rs --file crates/create-core/src/lib.rs
```
