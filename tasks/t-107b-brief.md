# T-107b: profile slot addresses, checked against the template
**Depends:** T-107a (not code-wise -- ordering only), T-106b (`slot_overrides`) | **Crate/dir:** `crates/create-core`
**Files to create/modify:**
- `crates/create-core/src/profile.rs` (modify: one import, one `impl InputSpec`, one
  `impl ModelProfile`, four tests)

## Goal
`ModelProfile::slot_addresses()` returns every slot address a profile names, so a profile
can be checked against the template it targets before anything is generated. A profile
whose address is wrong does not fail loudly -- ComfyUI happily runs the template with its
own default prompt and the user gets a track that ignores what they typed. This is the
primitive that turns that silent failure into a message.

## Spec
Exactly the reference implementation below.

**Three sources, all of which must exist in the template:**
1. every `InputSpec`'s `slots`, descending into `Group::members` (the LM-planner group
   holds five real addresses);
2. `comfy.slot_overrides` keys -- MiniMax's `37/6.unet_name` appears in no input, and an
   override the template lacks is exactly the checkpoint-pinning bug T-106b introduced the
   field to prevent (MCP-SURFACE 6);
3. `lyrics_contract.languages_from` -- a *read* address, but if it does not exist the
   language picker is silently empty.

`InputSpec::Unsupported` contributes nothing, by definition.

Returns `BTreeSet<SlotAddress>`: de-duplicated (ACE-Step's `94.language` is both an enum
slot and `languages_from`) and sorted, so a "missing addresses" message is stable.

**No comparison function is added, deliberately.** `mcp-bridge`'s landed
`SlotList::missing(&[&str]) -> Vec<&str>` (T-103b) already answers "which of these are
absent from the fetched template". The two compose at the `src-tauri` seam:

```rust
// T-110/T-111 wiring, NOT part of this brief:
let wanted: Vec<&str> = addresses.iter().map(|a| a.0.as_str()).collect();
let missing = slot_list.missing(&wanted);
```

Duplicating that comparison inside `create-core` would give the app two answers to one
question. `create-core` also stays I/O-free and gains no dependency on `mcp-bridge`
(ARCHITECTURE 2) -- the fetch and the compare belong at the seam that already owns both.

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, and its 4 tests pass.

### 1. Import (line 9 of `profile.rs`)
```rust
use std::collections::{BTreeMap, BTreeSet};
```
Replaces `use std::collections::BTreeMap;`. Nothing else in the import block changes.

### 2. The two `impl` blocks
Insert **between the `ModelProfile` struct and `#[cfg(test)] mod tests`**, with one blank
line either side.

```rust
impl InputSpec {
    /// Adds every slot address this control writes to `into`, descending into
    /// group members. [`InputSpec::Unsupported`] writes nothing by definition.
    fn collect_slots(&self, into: &mut BTreeSet<SlotAddress>) {
        match self {
            InputSpec::Text { slots, .. }
            | InputSpec::Lyrics { slots, .. }
            | InputSpec::Int { slots, .. }
            | InputSpec::Float { slots, .. }
            | InputSpec::Seed { slots }
            | InputSpec::Enum { slots, .. } => {
                into.extend(slots.iter().cloned());
            }
            InputSpec::Group { members, .. } => {
                for member in members.values() {
                    member.collect_slots(into);
                }
            }
            InputSpec::Unsupported { .. } => {}
        }
    }
}

impl ModelProfile {
    /// Every slot address this profile names, de-duplicated and sorted.
    ///
    /// Three sources, because all three must exist in the template or the
    /// profile is broken in a way the user sees only at generation time: the
    /// `inputs` it writes (group members included), the `slot_overrides` it
    /// pins, and the `lyrics_contract.languages_from` address it reads the
    /// live language list from.
    ///
    /// Pair with `SlotList::missing` in `mcp-bridge` to check a profile
    /// against a fetched template; nothing here touches ComfyUI.
    pub fn slot_addresses(&self) -> BTreeSet<SlotAddress> {
        let mut addresses = BTreeSet::new();
        for input in self.inputs.values() {
            input.collect_slots(&mut addresses);
        }
        addresses.extend(self.comfy.slot_overrides.keys().cloned());
        if let Some(contract) = &self.lyrics_contract {
            if let Some(languages_from) = &contract.languages_from {
                addresses.insert(languages_from.clone());
            }
        }
        addresses
    }
}
```

### 3. Four tests, appended inside the existing `mod tests`
Append after `test_minimax_fixture_surfaces_the_conditional_license`, before the module's
closing brace. `BTreeSet` is already in scope there via `use super::*`.

```rust
    /// Every slot address MCP-SURFACE 3 records from the live
    /// `audio_ace_step1_5_xl_turbo` template (verified 2026-08-23,
    /// `local_check: runnable: true`). Not the full 33 -- the documented
    /// subset, which covers everything the shipped profile drives.
    const VERIFIED_ACE_STEP_SLOTS: &[&str] = &[
        "107.filename_prefix",
        "107.quality",
        "3.cfg",
        "3.denoise",
        "3.sampler_name",
        "3.scheduler",
        "3.seed",
        "3.steps",
        "78.shift",
        "94.bpm",
        "94.cfg_scale",
        "94.duration",
        "94.generate_audio_codes",
        "94.keyscale",
        "94.language",
        "94.lyrics",
        "94.min_p",
        "94.seed",
        "94.tags",
        "94.temperature",
        "94.timesignature",
        "94.top_k",
        "94.top_p",
        "98.seconds",
    ];

    /// Protects: the collector descends into groups and skips `Unsupported`.
    /// Asserted as an exact set, so a group whose members stop being walked
    /// (the LM-planner's five controls) fails, and so does an `Unsupported`
    /// input that starts contributing a phantom address.
    #[test]
    fn test_slot_addresses_walk_groups_and_skip_unsupported() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let addresses: BTreeSet<String> =
            profile.slot_addresses().into_iter().map(|a| a.0).collect();

        let expected: BTreeSet<String> = [
            "3.seed",
            "3.steps",
            "78.shift",
            "94.bpm",
            "94.cfg_scale",
            "94.duration",
            "94.keyscale",
            "94.language",
            "94.lyrics",
            "94.min_p",
            "94.seed",
            "94.tags",
            "94.temperature",
            "94.timesignature",
            "94.top_k",
            "94.top_p",
            "98.seconds",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        assert_eq!(addresses, expected);
    }

    /// Protects: a pinned checkpoint variant is checked like any other
    /// address. `37/6.unet_name` appears in no input -- only in
    /// `slot_overrides` -- and an override the template lacks fails at
    /// generation time, which is exactly what this collector exists to
    /// prevent.
    #[test]
    fn test_slot_addresses_include_slot_overrides() {
        let profile: ModelProfile = serde_json::from_str(MINIMAX_FIXTURE).unwrap();
        let addresses = profile.slot_addresses();
        assert!(addresses.contains(&SlotAddress("37/6.unet_name".to_string())));
        assert!(profile
            .inputs
            .values()
            .all(|input| !format!("{input:?}").contains("37/6.unet_name")));
    }

    /// Protects: `lyrics_contract.languages_from` is collected even when no
    /// input writes it. The app reads the live language list from that
    /// address; if it does not exist the language picker is empty, silently.
    #[test]
    fn test_slot_addresses_include_languages_from() {
        let json = r#"
        {
            "id": "reads-languages",
            "display_name": "Reads Languages",
            "kind": "music",
            "license": "MIT",
            "comfy": { "output": { "save_node": "SaveAudioAdvanced" } },
            "inputs": {
                "tags": { "type": "text", "slots": ["94.tags"] }
            },
            "lyrics_contract": {
                "format": "plain",
                "languages_from": "94.language"
            }
        }
        "#;
        let profile: ModelProfile = serde_json::from_str(json).unwrap();
        let addresses = profile.slot_addresses();
        assert!(addresses.contains(&SlotAddress("94.language".to_string())));
        assert_eq!(addresses.len(), 2);
    }

    /// Protects: every address the shipped ACE-Step profile names exists in
    /// the live-captured slot list. A typo in the profile JSON -- `94.tag`
    /// for `94.tags` -- fails here instead of producing a track generated
    /// from the template's default prompt.
    #[test]
    fn test_shipped_ace_step_addresses_all_exist_in_the_verified_template() {
        let profile: ModelProfile = serde_json::from_str(ACE_STEP_FIXTURE).unwrap();
        let known: BTreeSet<&str> = VERIFIED_ACE_STEP_SLOTS.iter().copied().collect();
        let missing: Vec<String> = profile
            .slot_addresses()
            .into_iter()
            .filter(|a| !known.contains(a.0.as_str()))
            .map(|a| a.0)
            .collect();
        assert!(
            missing.is_empty(),
            "addresses not in the template: {missing:?}"
        );
    }
```

## Acceptance criteria
- [ ] `cargo test -p create-core` passes; `create-core` goes from 24 to **28 tests**
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green
- [ ] no changes outside `crates/create-core/src/profile.rs`
- [ ] no new dependencies; `create-core` still depends only on `serde` (+ dev-dep
      `serde_json`)

## Out of scope
- Fetching a template or listing its slots (that is `mcp-bridge`, landed in T-103a/b).
- Wiring the check into a Tauri command or the wizard (T-110/T-111).
- Reporting *unused* template slots. Most of a template's 33 slots are untouched by any
  profile; that is normal, not a defect, and a warning for it would be noise.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/MCP-SURFACE.md --read profiles/ace-step-1.5-turbo.json --read profiles/minimax-music-3.json --file crates/create-core/src/profile.rs
```
The two profile JSONs are `--read` because the tests assert against them as fixtures
(`include_str!`), and MCP-SURFACE because `VERIFIED_ACE_STEP_SLOTS` is copied from its
section 3 table. None of the three may be edited (WORKFLOW 3).
