# T-505d-c — Conditioning polarity in role suggestion

**Lane: Aider.** Two files in `create-core`: role suggestion learns to tell a **positive** prompt
from a **negative** one by reading the graph, because on an image model their names are identical.
**Depends:** T-313c (`roles.rs`), T-505d-b (emit accepts image graphs, landed).
**Dir:** `crates/create-core`. **No UI, no click-through** — this is the last backend prerequisite
for T-505d-d (the adopt UI), which has the click-through.

**Files to modify:**

- `crates/create-core/src/audit.rs` — add `output_targets`: the input names a node's outputs drive.
- `crates/create-core/src/roles.rs` — use it so `Tags` never claims a negative encoder, and
  `Negative` finds one that no name would have matched.

**Fixtures are already committed — do not create, edit or read them:**
`testdata/workflows/flux2_klein_9b.json` and `testdata/mcp/list_workflow_slots.flux2-klein-9b.json`.
Every address this brief names is quoted from them, so you never need to open them.

---

## The defect this fixes

Adopting Flux.2 Klein 9B today produces a profile that **writes the user's prompt into the negative
conditioning**, silently. Verified live 2026-09-03 against the frozen fixture:

| slot | node | drives | today's suggestion |
|---|---|---|---|
| `75/74.text` | `CLIPTextEncode` | `CFGGuider.positive` (link 140) | `Tags`, **Strong**, pre-ticked |
| `75/67.text` | `CLIPTextEncode` | `CFGGuider.negative` (link 141) | `Tags`, **Strong**, pre-ticked |

Both inputs are named `text`, both are `STRING`, both sit on `CLIPTextEncode`. **Nothing in the slot
list distinguishes them.** `Role::Tags`'s name table contains `"text"`, so both match; both rank
`Strong`; and `initialSelection` (app/src/state/import.ts) pre-ticks every `Strong` candidate. The
user saves, the profile is written, generation works, and every image is rendered with the prompt in
*both* encoders. Nothing errors and nothing on screen says so.

That is precisely the silent-guess failure the "never pre-tick a `possible`" rule exists to prevent —
except here the wrong candidate is `Strong`, because name-and-type matching genuinely cannot tell
these two apart. **The graph can.** This module's own header already states the principle: name
matching alone produces an answer the pipeline refuses, which is why it reads the graph and not just
the slot list. The seed hop was that argument for `PrimitiveInt`; this is the same argument for
conditioning.

`Role::Negative` has the mirror of the problem: its name table is `["negative", "negative_prompt"]`
and **Klein has no slot named either**, so the negative prompt is currently unmappable — the row
reads "No input in this workflow looks like this" while a negative encoder sits right there.

This is not a Klein quirk. Positive/negative `CLIPTextEncode` pairs whose inputs are both named
`text` are the standard shape of essentially every SD/SDXL/Flux graph, so this lands on the first
image model anyone adopts.

## Why the audio models are unaffected (and must be proven so)

Both shipped audio graphs drive their negative side from `ConditioningZeroOut`, which has **no
`STRING` slot at all**:

- ACE-Step: `3.KSampler.positive` ← `94:TextEncodeAceStepAudio1.5`; `3.KSampler.negative` ←
  `47:ConditioningZeroOut`.
- MiniMax: `37/9.KSampler.positive` ← `13:MiniMaxMusic3TextEncode`; `.negative` ←
  `10:ConditioningZeroOut`.

So no audio text slot can ever resolve to `Negative`, and each one's tags encoder resolves to
`Positive`, which keeps it in `Tags` exactly as today. `roles.rs`'s existing claim that "ACE-Step has
no negative prompt at all" stays literally true: its negative side is a *zeroed* conditioning, not a
text encoder. **Both are regression-tested below.**

## Spec — `crates/create-core/src/audit.rs`

Add one public function beside `link_origin`. It is the same graph knowledge walked the other way, and
belongs here for the reason `link_origin`'s own doc gives — not in a second walker in `roles.rs`.

```rust
/// The input names that `instance`'s outputs drive.
///
/// The mirror of [`link_origin`]: that walks from a fed input back to its
/// origin, this walks from a node forward to what it feeds. Role suggestion
/// needs it because an image graph's two `CLIPTextEncode` nodes are
/// indistinguishable by name -- both expose `text` -- and the only thing that
/// separates the prompt from the negative prompt is which sampler input each
/// one lands on.
///
/// **One hop, and exact input names.** A deeper walk would follow ACE-Step's
/// encoder through `ConditioningZeroOut` and report that it drives the negative
/// side too; one hop reports `positive` and `conditioning`, which is the truth.
///
/// Unresolvable addresses return an empty list rather than an error: "no
/// opinion" is the safe answer here, and leaves the caller on its name table.
/// Nesting deeper than one subgraph level is unresolvable, matching
/// [`audit_slots`].
pub fn output_targets(workflow: &Value, instance: &str) -> Vec<String>
```

Behaviour:

- **Top level** (`"94"`): links are the positional six-element arrays — element 1 is the origin node
  id, element 3 the target node id, element 4 the target's input index. For every link whose origin
  is `instance`, read the target node's `inputs[target_slot].name`.
- **One subgraph deep** (`"75/67"`): resolve `outer` → its `type` → the matching
  `definitions.subgraphs` entry → its `nodes`, exactly as `resolve_in_subgraph` already does (reuse
  the existing helpers rather than re-deriving the hop). Interior links are **objects** keyed
  `origin_id` / `target_id` / `target_slot`, not positional arrays. Match `origin_id == inner`.
- Anything unresolvable — unknown node, missing subgraph, `A/B/C` nesting — yields `Vec::new()`.
- Order is not significant; duplicates are harmless.

Factoring note: `resolve_in_subgraph` currently resolves the interior *and* reads a link in one pass.
Pull the "instance → interior nodes + subgraph definition" part into a small private helper both it
and `output_targets` use, so the subgraph hop exists once. Do not change `resolve_in_subgraph`'s
behaviour.

### Tests (`audit.rs`)

- `test_output_targets_reads_a_top_level_consumer` — on the ACE-Step fixture, `94`'s targets contain
  `"positive"` and **not** `"negative"`.
- `test_output_targets_reads_a_subgraph_consumer` — on the Klein fixture, `75/74` → contains
  `"positive"`; `75/67` → contains `"negative"`.
- `test_output_targets_is_empty_for_an_unresolvable_instance` — a made-up id, and an `A/B/C` address,
  both give an empty list.

## Spec — `crates/create-core/src/roles.rs`

### 1. Polarity

Private to this module (it is a suggestion-ranking concept, not graph vocabulary):

```rust
/// Which side of a sampler's conditioning a text slot feeds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Polarity {
    Positive,
    Negative,
}

/// Which conditioning side `instance` feeds, when the graph says so
/// unambiguously.
///
/// `None` when it drives both or neither -- "no opinion", which leaves the
/// name table in charge and is why this can never make a suggestion worse than
/// it is today.
fn conditioning_polarity(workflow: &Value, instance: &str) -> Option<Polarity> {
    let targets = audit::output_targets(workflow, instance);
    let positive = targets.iter().any(|t| t == "positive");
    let negative = targets.iter().any(|t| t == "negative");
    match (positive, negative) {
        (true, false) => Some(Polarity::Positive),
        (false, true) => Some(Polarity::Negative),
        _ => None,
    }
}
```

### 2. Apply it in `suggest_roles`

Compute once per slot **before** the `for role in Role::ALL` loop — the walk is per node, not per
role, and doing it inside would repeat it seven times over:

```rust
    // Per slot, not per role: polarity is a fact about the node.
    let polarity: BTreeMap<&str, Polarity> = slots
        .iter()
        .filter_map(|s| {
            conditioning_polarity(workflow, &s.instance_id).map(|p| (s.instance_id.as_str(), p))
        })
        .collect();
```

Then, inside the existing `for slot in slots` loop, replace the bare name check with a rule that lets
the graph outrank the name for the two prompt roles:

```rust
            let named = role
                .names()
                .contains(&slot.name.to_ascii_lowercase().as_str());
            let side = polarity.get(slot.instance_id.as_str()).copied();
            // The graph outranks the name table for the prompt roles: an image
            // model names both encoders `text`, so the name says nothing about
            // which is which and the link says everything.
            let wanted = match role {
                Role::Tags => named && side != Some(Polarity::Negative),
                Role::Negative => named || side == Some(Polarity::Negative),
                _ => named,
            };
            if !wanted {
                continue;
            }
```

Everything after it — the inert check, the `blocked` hop, `Confidence::Strong`, the sort — is
unchanged. A polarity-derived negative stays **`Strong`**, and deliberately: it is pre-ticked because
the graph *proves* it, which is stronger evidence than a matching name, not weaker. That is the whole
point — the correct mapping must be the default, since the wrong one currently is.

Give a polarity-derived candidate (one that matched no name) a reason that says why it is there:

```rust
            let reason = if !named {
                format!(
                    "{} on {} -- drives the negative conditioning",
                    slot.name, slot.node_type
                )
            } else {
                format!("{} on {}", slot.name, slot.node_type)
            };
```

### 3. Doc the module header

`roles.rs`'s header table explains the seed hop as the reason this module reads the graph. Add a short
paragraph after it recording the second reason, in the same register: on an image graph the positive
and negative encoders are both `CLIPTextEncode` exposing `text`, so the name table alone maps the
prompt onto both, and the link into `positive` / `negative` is the only thing that separates them.
Keep `Role::Negative`'s existing comment accurate — its name table still matters for graphs that do
name the slot.

### Tests (`roles.rs`)

Add a `klein()` helper beside `ace()`, using the existing `workflow()` / `slots()` helpers with
`"flux2_klein_9b.json"` and `"list_workflow_slots.flux2-klein-9b.json"`.

- **`test_klein_maps_the_positive_encoder_to_tags_and_not_the_negative`** — the headline. `Tags`
  contains `75/74.text` and **does not contain** `75/67.text`.
- **`test_klein_finds_the_negative_prompt_no_name_would_have_matched`** — `Negative` contains
  `75/67.text`, `Strong`, and its reason mentions the negative conditioning.
- **`test_klein_suggests_the_subgraph_controls`** — `Seed` contains `75/73.noise_seed`, `Steps`
  contains `75/62.steps`, `Cfg` contains `75/63.cfg`. Proves subgraph-interior slots survive the whole
  pass (they are addressed `A/B.name`), which is what makes Klein adoptable at all.
- **`test_ace_step_still_maps_its_tags_encoder`** — regression: ACE-Step's `Tags` candidates are
  unchanged and `Negative` is still **absent** (`ConditioningZeroOut` exposes no `STRING` slot).
- **`test_minimax_still_maps_its_tags_encoder`** — the same for MiniMax.

The two audio regressions are the load-bearing ones: this change is only safe because it is inert for
every graph that names its slots honestly.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] On the Klein fixture: `Tags` → `75/74.text` only; `Negative` → `75/67.text`; `Seed`,
      `Steps`, `Cfg` populated as above.
- [ ] On both audio fixtures: suggestions unchanged, `Negative` still absent.
- [ ] `output_targets` returns an empty list rather than panicking on any unresolvable address.
- [ ] Only `crates/create-core/src/audit.rs` and `crates/create-core/src/roles.rs` change. **No
      fixture file is created or modified** — they are already committed.

## Out of scope (T-505d-d, T-506)

- **The adopt UI** — the "Bring in" button, the mapping screen wiring, `save_imported_profile`
  (T-505d-d). No frontend file changes here; `ROLES`/`LABELS` in `app/src/state/import.ts` already
  include `negative`, so the new candidate renders on the existing screen with no change.
- **A width/height role.** Klein exposes `75/62.width|height`, `75/66.width|height` and the
  `PrimitiveInt` pair `75/68.value`/`75/69.value` that drive them. There is no dimensions role and
  adding one is a T-506 decision; image profiles use the graph's own defaults for now.
- **Any change to `Role::ALL`.** The role set is ARCHITECTURE 5b's list and does not grow here.
- **`ConditioningZeroOut`-aware refinement**, multi-hop walking, or ranking by distance to the
  sampler. One hop, exact names, no opinion when ambiguous.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505d-c-brief.md --read WORKFLOW.md --read CONVENTIONS.md --file crates/create-core/src/audit.rs --file crates/create-core/src/roles.rs
```

The fixtures are deliberately **not** passed: they are large, already committed, and every address
this lane needs is quoted in the brief.
