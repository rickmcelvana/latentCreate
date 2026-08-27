# T-304: `resolve_slots` — semantic choices to the values actually submitted

**Depends:** T-303 | **Crate/dir:** `crates/create-core` (pure; no I/O, no async)
**Files to modify:**
- `crates/create-core/src/generation.rs`
- `crates/create-core/Cargo.toml`

## Goal

`ModelProfile::resolve_slots(&GenerationSpec) -> Result<ResolvedSlots, ResolveError>`: the
fan-out that turns what the user chose into the addresses ComfyUI is actually sent.

**`GenerationSpec`, `InputValue`, `LoraRef`, `LyricRef` and the `ResolvedSlots` alias already
exist** (T-003). What has never existed is the function that maps one to the other. This is
the last pure piece before the pipeline touches a workflow file, and everything after it —
T-305's graph edits, T-306's command, T-311's provenance — is downstream of its output.

## Why this is the task that hides the traps

The two things a profile exists to hide are both fan-outs, and both are in the shipped
ACE-Step profile as data:

```
duration_s -> ["94.duration", "98.seconds"]     two slots, must stay in sync
seed       -> ["94.seed",     "3.seed"]         planner seed and sampler seed
```

The UI shows one duration and one seed (MCP-SURFACE §3). If the fan-out is wrong, a
generation runs at the wrong length or is unreproducible, and **neither failure is visible
until a track exists**. A test that sets duration and asserts one address is vacuous — name
the invariant, which is that *every* address the input declares carries the value
(WORKFLOW §4.2).

## Spec

### 1. Only what the spec sets is written

An input the spec omits is **left alone**. `fetch_template` already carries the template's own
defaults, and the profile's `default` fields exist to seed the *form*, not to restate the
template. Writing every declared slot on every run would have the app silently asserting
values it has no opinion about — and would make a profile's `default` a second source of truth
against the template's.

### 2. Overrides first, inputs second

`comfy.slot_overrides` seeds the map, then the spec's inputs are applied. That is how MiniMax
Music 3 pins the int8 DiT its template gets wrong (MCP-SURFACE §6). The two sets are not
expected to intersect; a profile where they do is an authoring mistake, and the acceptance
criteria test both shipped profiles for it.

### 3. Types are matched exactly, never widened

`set_workflow_slot`'s structured form **preserves the type it is given** (MCP-SURFACE §9.1
trap 2). So an `Int` accepted for a `Float` control is an integer landing in a FLOAT slot, and
a `Seed` demoted to an `Int` is a track that cannot be reproduced — which is the exact hazard
`InputValue`'s adjacent tagging was introduced for. Mismatch is an error naming the input.

### 4. Group members are dotted

`planner` is an `InputSpec::Group` whose members are ordinary specs. A spec addresses them as
`"planner.temperature"`, so a member can never collide with a top-level name and an error
message says which group the control is in. `flat_inputs()` is the walk, and is worth being
`pub` — T-308's param panel needs the same flattening to render.

### 5. What resolution does **not** do

- ⚠ **It does not fetch lyric text.** `spec.lyrics` is a `LyricRef` — a provenance pointer to
  a document and version. The *text* arrives as `inputs["lyrics"]`, an `InputValue::Text`,
  because `lyrics` is an ordinary input with a slot (`94.lyrics`). This crate has no store
  access and never will (ARCHITECTURE §2). A caller expecting `resolve_slots` to look up the
  document gets a workflow with an empty lyric field and no error.
- **It does not touch LoRAs.** They need node insertion, not slot values (MCP-SURFACE §4);
  that is T-305.
- **It does not validate against a template.** `SlotList::missing` in `mcp-bridge` already
  does that comparison (2026-08-24 decision), and duplicating it here would be a second answer
  to one question.

### 6. `Cargo.toml`

Add `thiserror = "2.0"`. Note what this means: `create-core` is **serde-only today and has no
error type at all**, so `ResolveError` is its first and this is its second dependency. It is
the same crate the other three already use, it is already in `Cargo.lock` (so this adds an
edge, not a package), and hand-rolling `Display` for five variants is boilerplate that drifts.
Recorded here rather than done quietly, because "create-core has one dependency" was a
property worth noticing before it stopped being true.

## Reference implementation

`rustfmt --check` clean as written. Integrate verbatim, adapting only the `use` lines.

```rust
/// Why a spec could not be turned into slot values.
///
/// Every variant names the offending input, because these are read by a user
/// who is looking at a form with that label on it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ResolveError {
    /// The spec carries an input this profile does not declare.
    ///
    /// An error rather than a shrug: silently dropping an unknown name is how
    /// a user's duration fails to apply with nothing on screen to explain it.
    #[error("{profile_id} has no input named {input}")]
    UnknownInput { profile_id: String, input: String },
    /// The spec carries a value for an input this model does not accept.
    #[error("{profile_id} does not support {input}: {reason}")]
    Unsupported {
        profile_id: String,
        input: String,
        reason: String,
    },
    /// The value's type does not match what the profile declares.
    #[error("{input} expects {expected}, got {actual}")]
    TypeMismatch {
        input: String,
        expected: &'static str,
        actual: &'static str,
    },
    /// A number outside the profile's declared range.
    #[error("{input} must be between {min} and {max}, got {value}")]
    OutOfRange {
        input: String,
        min: f64,
        max: f64,
        value: f64,
    },
    /// A choice the profile does not list.
    #[error("{input} does not accept {value}")]
    NotAChoice { input: String, value: String },
}

impl InputValue {
    /// The variant name, for error messages.
    fn kind(&self) -> &'static str {
        match self {
            InputValue::Text(_) => "text",
            InputValue::Int(_) => "int",
            InputValue::Float(_) => "float",
            InputValue::Seed(_) => "seed",
            InputValue::Enum(_) => "enum",
            InputValue::Bool(_) => "bool",
        }
    }
}

impl ModelProfile {
    /// Every input this profile declares, keyed by the name a spec uses.
    ///
    /// Group members are dotted (`"planner.temperature"`) so a member can never
    /// collide with a top-level name, and so an error message says which group
    /// the control lives in.
    pub fn flat_inputs(&self) -> BTreeMap<String, &InputSpec> {
        let mut flat = BTreeMap::new();
        for (name, spec) in &self.inputs {
            flatten_into(name, spec, &mut flat);
        }
        flat
    }

    /// Turn the semantic choices into the slot values actually submitted.
    ///
    /// **Only what the spec sets is written.** An input the spec omits is left
    /// alone, because `fetch_template` already carries the template's own
    /// defaults and the profile's `default` fields exist to seed the *form*,
    /// not to restate the template. Writing every declared slot on every run
    /// would have the app silently asserting values it has no opinion about.
    ///
    /// `slot_overrides` go in first and the spec's inputs after, so a profile
    /// that pins a checkpoint variant (MCP-SURFACE 6) still gets it. The two
    /// sets are not expected to intersect; a profile where they do is an
    /// authoring mistake, and the shipped ones are tested for it.
    ///
    /// Fan-out lives here: one semantic value reaches every address the input
    /// names, which is what hides ACE-Step's two durations and two seeds.
    pub fn resolve_slots(&self, spec: &GenerationSpec) -> Result<ResolvedSlots, ResolveError> {
        let declared = self.flat_inputs();
        let mut resolved: ResolvedSlots = self.comfy.slot_overrides.clone();

        for (name, value) in &spec.inputs {
            let input = declared
                .get(name.as_str())
                .ok_or_else(|| ResolveError::UnknownInput {
                    profile_id: self.id.clone(),
                    input: name.clone(),
                })?;
            check(name, input, value, &self.id)?;
            for address in slots_of(input) {
                resolved.insert(address.clone(), value.clone());
            }
        }

        Ok(resolved)
    }
}

/// Add `spec` under `name`, descending into groups with a dotted prefix.
fn flatten_into<'a>(name: &str, spec: &'a InputSpec, into: &mut BTreeMap<String, &'a InputSpec>) {
    if let InputSpec::Group { members, .. } = spec {
        for (member, inner) in members {
            flatten_into(&format!("{name}.{member}"), inner, into);
        }
    } else {
        into.insert(name.to_string(), spec);
    }
}

/// The addresses one control writes. A group writes none of its own.
fn slots_of(spec: &InputSpec) -> &[SlotAddress] {
    match spec {
        InputSpec::Text { slots, .. }
        | InputSpec::Lyrics { slots, .. }
        | InputSpec::Int { slots, .. }
        | InputSpec::Float { slots, .. }
        | InputSpec::Seed { slots }
        | InputSpec::Enum { slots, .. } => slots,
        InputSpec::Group { .. } | InputSpec::Unsupported { .. } => &[],
    }
}

/// Whether `value` is acceptable for `spec`.
///
/// Types are matched exactly rather than widened. `set_workflow_slot`'s
/// structured form preserves the type it is given (MCP-SURFACE 9.1), so an
/// `Int` accepted for a `Float` control is an integer landing in a FLOAT slot,
/// and a seed demoted to an `Int` is a track that cannot be reproduced.
fn check(
    name: &str,
    spec: &InputSpec,
    value: &InputValue,
    profile_id: &str,
) -> Result<(), ResolveError> {
    let mismatch = |expected| {
        Err(ResolveError::TypeMismatch {
            input: name.to_string(),
            expected,
            actual: value.kind(),
        })
    };

    match (spec, value) {
        (InputSpec::Text { .. }, InputValue::Text(_)) => Ok(()),
        (InputSpec::Text { .. }, _) => mismatch("text"),
        (InputSpec::Lyrics { .. }, InputValue::Text(_)) => Ok(()),
        (InputSpec::Lyrics { .. }, _) => mismatch("text"),
        (InputSpec::Seed { .. }, InputValue::Seed(_)) => Ok(()),
        (InputSpec::Seed { .. }, _) => mismatch("seed"),
        (InputSpec::Int { min, max, .. }, InputValue::Int(v)) => {
            in_range(name, *v as f64, *min as f64, *max as f64)
        }
        (InputSpec::Int { .. }, _) => mismatch("int"),
        (InputSpec::Float { min, max, .. }, InputValue::Float(v)) => in_range(name, *v, *min, *max),
        (InputSpec::Float { .. }, _) => mismatch("float"),
        (
            InputSpec::Enum {
                from_node_choices,
                choices,
                ..
            },
            InputValue::Enum(v),
        ) => {
            // A live-read list cannot be checked here: the choices come from
            // the node schema at render time, and this crate never talks to
            // ComfyUI (ARCHITECTURE 2).
            if *from_node_choices || choices.iter().any(|c| c == v) {
                Ok(())
            } else {
                Err(ResolveError::NotAChoice {
                    input: name.to_string(),
                    value: v.clone(),
                })
            }
        }
        (InputSpec::Enum { .. }, _) => mismatch("enum"),
        (InputSpec::Unsupported { reason }, _) => Err(ResolveError::Unsupported {
            profile_id: profile_id.to_string(),
            input: name.to_string(),
            reason: reason
                .clone()
                .unwrap_or_else(|| "this model has no such input".to_string()),
        }),
        (InputSpec::Group { .. }, _) => Err(ResolveError::UnknownInput {
            profile_id: profile_id.to_string(),
            input: name.to_string(),
        }),
    }
}

fn in_range(name: &str, value: f64, min: f64, max: f64) -> Result<(), ResolveError> {
    if value < min || value > max {
        return Err(ResolveError::OutOfRange {
            input: name.to_string(),
            min,
            max,
            value,
        });
    }
    Ok(())
}
```

## Acceptance criteria

Tests run against the **shipped profile fixtures**, which `profile.rs` already loads with
`include_str!("../../../profiles/ace-step-1.5-turbo.json")`. Use the real profiles, not
hand-written ones: a rule about a profile has to run against a profile, the same reason the
lyric fixtures in `testdata/lyrics/` are unedited model output.

- [ ] **Duration reaches both `94.duration` and `98.seconds`**, from one `duration_s` value.
      Assert both addresses; asserting one is the vacuous version.
- [ ] **One seed reaches both `94.seed` and `3.seed`**, and arrives as `InputValue::Seed`,
      not demoted.
- [ ] `u64::MAX` as a seed survives resolution unchanged.
- [ ] A group member resolves under its dotted name (`planner.temperature` -> `94.temperature`).
- [ ] `flat_inputs()` on the ACE-Step profile contains every group member and no bare
      `planner` key.
- [ ] MiniMax's `slot_overrides` appear in the output **with no inputs set at all**, and
      survive a resolution that also sets inputs.
- [ ] `negative` on ACE-Step is `Unsupported` and returns `ResolveError::Unsupported` carrying
      the profile's own reason.
- [ ] Unknown input name -> `UnknownInput`. Wrong type -> `TypeMismatch`. Out-of-range int and
      float -> `OutOfRange`. An enum value not in a static `choices` list -> `NotAChoice`,
      while a `from_node_choices` enum accepts anything.
- [ ] **Neither shipped profile's `slot_overrides` intersects its inputs' addresses** — the
      authoring mistake §2 describes.
- [ ] An empty `spec.inputs` against a profile with no overrides resolves to an empty map, not
      to defaults.
- [ ] `npm run gate` clean.
- [ ] No changes outside the two listed files.

## Out of scope

- No graph edits, no LoRA splicing, no save-node swap — **T-305**.
- No MCP calls, no `set_slots`, no template fetch — **T-306**. Nothing in this task may make
  `create-core` depend on `mcp-bridge`; that inversion is what ARCHITECTURE §2 exists to
  prevent.
- No template validation. `SlotList::missing` already answers that question.
- Do not add defaulting behaviour beyond §1, and do not "helpfully" coerce a type.
- Do not change `GenerationSpec`, `InputValue`, `LoraRef` or `LyricRef` — they are T-003's and
  are already serialised into lyric documents.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-304-brief.md --read crates/create-core/src/profile.rs --read profiles/ace-step-1.5-turbo.json --read profiles/minimax-music-3.json --file crates/create-core/src/generation.rs --file crates/create-core/Cargo.toml
```

`profile.rs` is `--read` because the new code opens an `impl ModelProfile` block and matches
every `InputSpec` variant, so its definitions must be in view — but it does not change. Both
profile JSONs are `--read` so the tests are written against the real declared addresses rather
than remembered ones.
