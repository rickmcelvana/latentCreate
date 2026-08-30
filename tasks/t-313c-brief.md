# T-313c — role suggestion

**Lane: architect-direct.** One pure module over captured data. WORKFLOW section 1.

**Depends:** T-313b (landed). **Crate/dir:** `create-core`.

**Files to modify:**

- `crates/create-core/src/roles.rs` — **new**: `Role`, `Candidate`, `suggest_roles`, tests
- `crates/create-core/src/lib.rs` — `pub mod roles;`
- `testdata/mcp/list_workflow_slots.ace-step.json` — **new** captured fixture

## Why

T-313b hands back 33 slots for ACE-Step and 25 for MiniMax. Asking a user to read 33 rows and
decide which one receives lyrics is not an import flow, it is a punishment. ARCHITECTURE 5b's
answer is candidates pre-suggested by node class and input name, and both are already on every
slot (MCP-SURFACE 29.5).

## The finding that decides the design

**Name-matching alone produces an answer the pipeline then refuses.** Measured against this
project's own reference model, not imagined.

ACE-Step's graph exposes three slots that look like the seed:

| slot | driven by | node class | writing it |
|---|---|---|---|
| `3.seed` | node 109 | `PrimitiveInt` | **ignored** — accepted, persisted, never read |
| `94.seed` | node 109 | `PrimitiveInt` | **ignored**, same reason |
| `109.value` | — | `PrimitiveInt` | **this is the seed** |

The shipped profile maps `seed` to `109.value`, and that is not a stylistic choice: `audit_slots`
classifies a link from a real backend node as **inert**, and `build_and_submit` **refuses to
generate** when any resolved address is inert — *"writes N to inputs a node drives, so the engine
would ignore them"*. A suggester that offered `3.seed` would produce a profile that cannot run.

The trap is sharper than "avoid link-fed slots", because the *duration* role goes the other way:

| slot | driven by | node class | writing it |
|---|---|---|---|
| `94.duration` | node 99 | `PrimitiveNode` | **lands** |
| `98.seconds` | node 99 | `PrimitiveNode` | **lands** |

`PrimitiveNode` is a frontend-only node whose link is dropped when the graph is converted, so the
consumer's own widget is used. `PrimitiveInt` is a real backend node whose link survives. **Same
idea, opposite behaviour, one letter apart in the class name.** `create_core::audit` already
encodes this and is the only correct source for it — do not re-derive the rule here.

Two consequences, and they are the whole task:

1. **`suggest_roles` takes the graph as well as the slots**, and drops every candidate
   `audit_slots` calls inert. A suggestion the pipeline would refuse is worse than no suggestion.
2. **When a name-matching slot is inert, follow its link to the driver and offer the driver's own
   writable slot instead.** That is what turns `3.seed` into `109.value` — the correct answer, whose
   name (`value`) and class (`PrimitiveInt`) match nothing a name-based rule would ever look for.

## Spec

### 1. The roles

ARCHITECTURE 5b's list, and no more. Extra roles are T-313d/e's business if they want them.

```rust
/// A semantic input the app knows how to drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Tags,
    Lyrics,
    Negative,
    DurationSeconds,
    Seed,
    Steps,
    Cfg,
}
```

### 2. What a suggestion is

```rust
/// How sure the suggester is, which is the UI's pre-tick rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Confidence {
    /// Name and widget type both fit, and the class agrees or says nothing.
    /// The UI pre-selects these.
    Strong,
    /// The widget type fits and something else hints at it. Offered, not
    /// selected.
    Possible,
}

/// One slot offered for one role.
#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub address: String,
    pub node_type: String,
    pub confidence: Confidence,
    /// Why this was offered, in words a person can check. Shown in the UI --
    /// a suggestion nobody can verify is one nobody should accept.
    pub reason: String,
}
```

**`reason` is not decoration.** The user is being asked to trust a guess about their own graph; a
row reading "seed — `109.value` (PrimitiveInt drives 3.seed)" can be checked at a glance, and one
reading "seed — `109.value`" cannot.

### 3. The rule

For each role, over every slot:

- **Widget type must fit**, else the slot is not a candidate at all. `Tags`/`Lyrics`/`Negative`
  need `STRING`; `Seed`/`Steps` need `INT`; `DurationSeconds`/`Cfg` accept `FLOAT` or `INT`.
- **Name match** (case-insensitive, on the input name):
  - `Tags` — `tags`, `prompt`, `caption`, `text`, `positive`
  - `Lyrics` — `lyrics`
  - `Negative` — `negative`, `negative_prompt`
  - `DurationSeconds` — `duration`, `seconds`, `length`, `max_duration`
  - `Seed` — `seed`, `noise_seed`
  - `Steps` — `steps`
  - `Cfg` — `cfg`, `cfg_scale`, `guidance`
- **A name match with a fitting type is `Strong`.** No name match is not a candidate — a class hint
  alone is too weak to put in front of someone as a default.
- **Then the inert filter**, which is the part that matters: drop any candidate `audit_slots`
  reports as inert, and for each dropped one, try its driver. If the driving node has a writable
  slot of a fitting type, offer **that** as `Possible`, with a reason naming what it drives.

**Why the hop is `Possible` and not `Strong`:** its name matched nothing. `109.value` is the right
answer here because of the graph's shape, not because anything about it says "seed". Offering it as
a checked default would be claiming more than the evidence supports; offering it top of the list
with the reason "drives 3.seed, 94.seed" lets a person confirm in a second.

Order candidates `Strong` before `Possible`, then by address, so the output is stable.

### 4. The fixture

Capture ACE-Step's live slot list to `testdata/mcp/list_workflow_slots.ace-step.json`, beside the
MiniMax one. Both fixtures are then real captured payloads, which is what makes the tests below
evidence rather than assertion.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_ace_step_seed_resolves_to_the_primitive_not_the_sampler`** — the headline. Assert the
      `Seed` candidates **do not contain** `3.seed` or `94.seed`, and **do contain** `109.value`
      with a reason naming what it drives. The invariant: the suggester never offers a mapping
      `build_and_submit` would refuse.
- [ ] **`test_ace_step_duration_offers_both_slots_strongly`** — `94.duration` and `98.seconds` are
      both `Strong`. The invariant: `PrimitiveNode` is not `PrimitiveInt`, and the audit is what
      tells them apart. **This test and the one above fail together if the inert rule is
      re-derived here instead of delegated.**
- [ ] **`test_ace_step_tags_and_lyrics_land_on_the_encoder`** — `94.tags` and `94.lyrics`, `Strong`.
- [ ] **`test_minimax_maps_its_five_roles`** — against the MiniMax fixture: caption, lyrics,
      duration and seed all found. Confirms the rule works on a graph whose slots are all subgraph
      interiors (`37/13.*`), which is the case the address parser has broken on before.
- [ ] **`test_a_role_with_nothing_plausible_is_absent_rather_than_guessed`** — ACE-Step has no
      negative prompt (the profile records `Unsupported`, verified). Assert `Negative` has **no**
      candidates rather than a low-confidence guess at `94.tags`.
- [ ] **`test_a_slot_of_the_wrong_type_is_never_offered`** — a `STRING` slot named `seed` is not a
      `Seed` candidate.
- [ ] Mutation: removing the inert filter must fail the seed test; treating `PrimitiveInt` as
      virtual must fail it too; promoting the hop to `Strong` must fail a test.
- [ ] Pure: no I/O, no `LocalComfy`, no async. Everything above runs off two captured JSON files.

## Out of scope

- **Emitting a profile.** T-313d turns accepted candidates into `InputSpec`s.
- **Any UI.** T-313e.
- **Roles beyond 5b's seven.** `bpm`, `keyscale`, `language`, `timesignature` and `shift` are all in
  the shipped ACE-Step profile and could be suggested later; adding them now widens the name table
  without testing anything new about the mechanism.
- **Deciding the LoRA attach point.** `LoraSupport` needs `loader_node` and `attach_after`, which is
  graph-shape reasoning, not name matching. Its own task if it is wanted at all — a user with a
  LoRA already wired into their imported graph does not need the app to splice one.
- **Warning that a partially-mapped role will misbehave.** Mapping `94.duration` without
  `98.seconds` gives a 30-second encode inside a 120-second latent. Real, and a UI concern; the
  ranking putting both at `Strong` is what makes the right thing the default.

## Changed during implementation and review

**1. `audit::link_origin` was added** rather than doing the link walk in `roles.rs`. The audit
reports *what type* drives an inert address, not *which node*, and the hop needs the id. A second
graph walker beside the first is how the two drift, so the walk stays in `audit` and `roles` asks
it a question.

**2. `SlotInfo` restates `mcp_bridge::Slot`** rather than `create-core` depending on the bridge for
a pure ranking pass. Five fields, one mapping at the call site, and `create-core` stays free of the
transport crate.

**3. The hop's confidence was unasserted, and the brief's third mutation proved it.** Promoting the
`Possible` hop to `Strong` passed all 164 tests. That is not cosmetic: **confidence is the UI's
pre-tick rule**, so the mutation makes the app silently pre-select `109.value` as the seed on the
strength of graph shape alone — the exact claim the spec above says the evidence does not support.
The seed test now asserts `Confidence::Possible`.

Three mutations, three killed — the third after being made killable. That is two tasks running
(T-313b's staging, this one's confidence) where the brief's mutation list found the tests agreeing
with the code rather than checking it.

**Fixture note:** `list_workflow_slots.ace-step.json` is the live 33-slot payload with one
edit — the `94.lyrics` `current_value` is trimmed to a few lines. Role suggestion never reads
`current_value`, and the untrimmed value is a 900-character demo lyric. Recorded here so nobody
later reads it as byte-exact.
