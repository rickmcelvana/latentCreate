# T-308a — the param panel model *(pure TS)*

**Lane: architect-direct** (WORKFLOW §1). No Aider run, no launch command.

`app/src/state/params.ts` is a pure module over the profile's own declarations, exactly the
kind of task where writing the verified reference *is* writing the task. Sending it out would
be a round trip that cannot change the outcome, which is what the 2026-08-28 lane rule is
about.

**One consequence for this template:** an architect-direct brief does **not** restate the
reference implementation. The code is the code — read
[`app/src/state/params.ts`](../app/src/state/params.ts). What belongs here is the part that
does not survive in source: what was decided, what was rejected, and which invariant each
test exists to protect.

**Why T-308 is split.** The whole of T-308 is roughly 1100 lines: two Rust commands and their
view types, the bridge, this model, the component and its CSS. That is nearly three times the
≤400-line rule, and T-306a already stalled on brief size once. T-308a is the pure half, the
part testable without a running ComfyUI — the same ordering that made T-304/T-305/T-307
possible. **T-308b** is named at the bottom and gets its own brief, in the Aider lane, when
this lands.

---

## Findings

### 1. A `u64` seed cannot survive JavaScript, and rounding it would be silent

ACE-Step's seed runs to `u64::MAX`. `create-core` carries it as a `u64` deliberately —
`InputValue::Seed` exists *so that a seed cannot be demoted to another number type*, and
`generation.rs` pins `Seed(u64::MAX)` in its own tests.

JavaScript has no such integer. Above 2^53−1 the value changes the moment it becomes a
`number`, and `invoke` serialises through JSON, so a `BigInt` cannot cross the bridge either.
A seed typed as `18446744073709551615` reaches Rust as `18446744073709551616` — accepted,
generated with, and written into the provenance sidecar.

That is the exact failure `InputValue::Seed` was introduced to prevent, reappearing one layer
above the type that prevents it. The pattern is now familiar: MCP-SURFACE §17.1 in the
pipeline, the LoRA splice in T-306b, this in the panel. **A guard in one layer does not bind
the layer above it.**

**Decision: refuse, do not clamp.** `MAX_SAFE_SEED = Number.MAX_SAFE_INTEGER`, and `seedError`
returns a message naming the limit. A refused seed is on screen; a clamped one is a sidecar
that lies. This caps the app below the model's range, which is a real limitation and belongs
in T-308b's UI copy, not only in a comment.

### 2. `Unsupported` is evidence, and deleting it looks identical to a bug

ACE-Step declares `negative` as `unsupported` with the reason *"TextEncodeAceStepAudio1.5
exposes no negative input"* — someone read a live node schema and recorded it. A panel that
just filters those out throws the evidence away, and a missing negative-prompt box then looks
exactly like a forgotten one.

So the model returns `omitted: Omitted[]` alongside its controls, carrying the reason. Same
rule as `LoraCatalog::excluded` in T-307: nothing disappears without saying why.

### 3. Three of ACE-Step's most musical controls have no options at all offline

`keyscale`, `timesignature` and `language` are all `from_node_choices: true` with an **empty**
local list — 34 and 51 values that would rot on the first ComfyUI update (MCP-SURFACE §11).
Until something asks the node registry there is nothing to put in the dropdown.

The model therefore marks them `fromNode: true` with `choices: []`, and **T-308b must not
render that the same way it renders an unsupported input.** One means "ComfyUI is not
running"; the other means "this model does not have this". Collapsing them is how a user
concludes the app cannot see their install — the same failure the profile picker already
avoids with its "Readiness could not be checked" line.

### 4. The profile's own order is the wrong order

`inputs` is a `BTreeMap`, so it arrives alphabetically: `bpm, duration_s, keyscale, language,
lyrics, negative, planner, seed, shift, steps, tags, timesignature`. Rendering that puts bpm
above the style tags and buries lyrics in the middle.

`PRESENTATION_ORDER` is a constant in this module, **not** a new profile field. Ordering is a
property of this panel, not of the model, and a `display_order` on every profile is one more
thing a custom-imported workflow (ARCHITECTURE §5b) has to get right to look correct. Unknown
names sort alphabetically after the known ones, so an imported profile's inputs still render.

### 5. The planner group's members declare `advanced: false` inside an `advanced: true` group

Honour only the member's own flag and five LM-planner sampling controls — cfg_scale,
temperature, top_p, top_k, min_p — appear in the basic panel in front of someone who wanted
to type style tags, while the group meant to hide them is itself hidden. Members inherit.

---

## Spec

`panelModel(inputs)` → `{ basic, advanced, omitted }`.

| Rule | Behaviour |
|---|---|
| Groups | Flattened; members tagged with the group's label and **inheriting its `advanced`** |
| `unsupported` | Never a control; recorded in `omitted` with its reason |
| `from_node_choices` | `fromNode: true`, `choices: []` — flagged, never faked |
| Order | `PRESENTATION_ORDER`, then unknown names alphabetically |
| Bounds/defaults | From the profile only. `int` step 1, `float` step from the profile or `null` |
| Seed | `range` 0…`MAX_SAFE_SEED`, default 0 |

`defaults(model)` → the starting value of every control, and nothing else.

`seedError(raw)` → `null`, or why not: empty, non-digits, or above `MAX_SAFE_SEED`.

`specInputs(model, values)` → the `inputs` map of a `GenerationSpec`, **tagged by the
control's declared kind, never by `typeof value`**. `InputValue` is adjacently tagged for
precisely this reason: untagged, a JSON `3` deserialises as `Int`, `Float` or `Seed`. Typing
off the runtime shape here would hand Rust the guess its own encoding refuses to make. A
control with no value is skipped rather than sent — `resolve_slots` applies everything it is
given, so sending a default nobody chose is how a form quietly overrides the workflow.

**Types.** `InputSpec` and `ProfileInputs` were added to `app/src/bridge/profiles.ts`,
mirroring `create_core::profile::InputSpec`. That enum is `#[serde(tag = "type")]`, so it
serialises as exactly the shape already written in `profiles/*.json` — the webview gets the
profile's own shape, not a view type invented for it. T-308b adds the command that fetches it.

---

## Acceptance criteria

1. `npm run gate` green. Frontend 128 → **141 tests**.
2. `oxlint` adds **no** warnings. (The first draft added eight — `no-shadow` and
   `unicorn/no-array-sort`; both are fixed, and `toSorted` is available at this lib target.)
3. Tests run against the **real shipped profile**, imported directly:
   `import aceProfile from '../../../profiles/ace-step-1.5-turbo.json'`, the same pattern
   `config.test.ts` uses for its wire fixture.
4. 15 controls (8 basic, 7 advanced), 1 omission, from ACE-Step's 12 declared inputs.

**On the JSON cast.** TypeScript widens a JSON import's `type` fields to `string`, so the
file cannot satisfy the discriminated union on its own — `config.test.ts` solved this by
re-declaring its fixture, but re-declaring seventeen inputs would be a second copy of the
profile that drifts. So the cast stands, and `test_every_declared_input_is_accounted_for`
keeps it honest: it walks the real file's keys and fails if any of them vanished on the way
through, which is what a wrong cast would cause.

---

## Mutations — seven run, seven killed

| | Mutation | Killed by |
|---|---|---|
| MU1 | group members lose their inherited `advanced` | 3 tests |
| MU2 | `unsupported` dropped silently instead of recorded | 2 tests |
| MU3 | ordering falls back to alphabetical | 2 tests |
| MU4 | seed tagged `int` instead of `seed` | 1 test |
| MU5 | the `MAX_SAFE_SEED` ceiling removed | 1 test |
| MU6 | `fromNode` never set, so a node-backed enum reads as an empty fixed list | 1 test |
| MU7 | untouched controls sent anyway | 1 test |

MU4 and MU5 are the two that matter — between them they are the whole of finding 1.

---

## Out of scope — and what T-308b must therefore do

Nothing here reaches a screen. T-308b is the data path and the panel:

- **`profile_inputs` command** returning `profile.inputs` as-is (it already serialises to the
  right shape), plus the `getProfileInputs` call in `app/src/bridge/profiles.ts`.
- **A node-choices command** wrapping `mcp-bridge`'s existing `node_schema` / `choices_for`,
  so `keyscale`, `timesignature` and `language` get their options. Nothing exposes the node
  registry to the webview today.
- **`<ParamPanel>`** plus its `theme.css` rules, rendering `basic`, an advanced disclosure,
  and grouped fieldsets.
- **Three pieces of copy this brief has decided but not written:** why a node-backed enum is
  empty and what to do about it (start ComfyUI), why there is no negative-prompt box (the
  profile's recorded reason), and the seed ceiling.
- **A fresh random seed on mount, and a re-roll control.** `defaults()` returns 0 because it
  is pure and must stay testable; shipping every track on seed 0 is not the intent. Note that
  0 is a valid seed, not a sentinel.
- Zustand: subscribe with a selector, never the bare store (WORKFLOW §4.10). The values map
  is the store's; every derivation above stays here.
