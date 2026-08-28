# T-308b — the data path, the panel store, and `<ParamPanel>`

**This brief has two parts in two lanes** (WORKFLOW §1).

**Part 1 — landed, architect-direct.** The `profile_inputs` command, the bridge call and the
panel store. Small, test-bearing, and already written and verified here, so there is nothing
for an executor to save. Recorded below; the code is in the tree.

**Part 2 — the Aider run.** `<ParamPanel>`, its `theme.css` rules and the AudioStudio wiring.
About 300 lines of JSX and CSS with no logic in it, because T-308a deliberately put every
derivation in `state/params.ts` and Part 1 put every piece of state in
`state/paramPanel.ts`. That is exactly the work the executor lane exists for, and the
launch command is at the bottom.

---

## Part 1 — what landed

### The finding: the profile knows the node *instance*, not the node *class*

Verified live while writing this. ACE-Step declares:

```json
"keyscale": { "type": "enum", "slots": ["94.keyscale"], "from_node_choices": true, "label": "Key" }
```

`94` is a node **instance id inside that profile's template**, not a class name. Reading the
live choices means `94` → `TextEncodeAceStepAudio1.5` → `nodes(action="get", …)` →
`choices_for("keyscale")`, and **the only place that first hop exists is the workflow file**.
Nothing in the profile schema names the class.

Confirmed against the live registry (served from comfy-cli's cache, ComfyUI down):
`TextEncodeAceStepAudio1.5` carries `keyscale` (34 choices), `language` (51, default `"en"`)
and `timesignature` (**4 choices: `"2" "3" "4" "6"`** — numerators, not `"4/4"`). Two things
worth writing down while looking at it:

- That node's `seed` input has `max: 18446744073709551615`. The node itself corroborates
  T-308a's ceiling finding — this is `u64::MAX`, live, from ComfyUI.
- Its `duration` runs to **2000.0** while the profile caps `duration_s` at 300. The profile is
  deliberately narrower (2000 s is a 33-minute song). Nobody should "fix" the profile to match
  the node.

**So live enum choices are deferred to T-308c**, with the fix named: `InputSpec::Enum` gains
an optional `node` field carrying the class, which is consistent with a schema that already
names `save_node` and `loras.loader_node`. The input name comes from the slot address's field
part, so only the class is missing. Chasing it through a template fetch instead would put an
MCP round trip and a file write behind opening a panel.

This is why T-308a made `fromNode` a flag with empty `choices` rather than pretending: the
panel ships now, renders those three controls in an honest "not loaded" state, and T-308c
fills them in.

### `profile_inputs` returns the profile's own shape

No view type. `InputSpec` is `#[serde(tag = "type")]`, so it serialises to exactly what is
already written in `profiles/*.json`; a second projection would be a copy of the schema, free
to drift from it. Two Rust tests pin the wire shape the hand-written TypeScript union in
`app/src/bridge/profiles.ts` is built against — there is no generator, so drift is silent
otherwise — and one of them asserts that an `unsupported` input crosses **with its reason**.

### The store holds only what a person did

`app/src/state/paramPanel.ts`. Three rules that earned tests:

- **`freshSeed()` is 53 bits from `crypto`, assembled as `(hi % 2^21) * 2^32 + lo`** — exactly
  `Number.MAX_SAFE_INTEGER` at the top. Using a single 32-bit draw passes "is a safe integer"
  and "seeds vary" while confining every track this app ever makes to the first 0.05% of the
  seed space, so there is a test for the range actually being used. `crypto` rather than
  `Math.random` because this number is written into a sidecar as a track's identity.
- **A fresh panel rolls its seed.** Opening on 0 every time makes every first track of every
  session the same one, and 0 is a real seed rather than a sentinel.
- **Reloading the same profile is a no-op.** A view re-mounts on every tab switch; re-running
  defaults there wipes the tags someone typed, and re-rolling their seed puts a seed in the
  sidecar they never saw.

Six mutations, six killed. Frontend 141 → **151 tests**.

---

## Part 2 — the Aider run

### Files

- **create** `app/src/components/ParamPanel.tsx`
- **modify** `app/src/views/AudioStudio.tsx` (render it, load on profile change)
- **modify** `app/src/theme.css` (append one block; change no existing rule)

Nothing else. No new state, no new bridge call, no logic — if a value needs deriving, it
belongs in `params.ts` and this brief is wrong.

### The store's surface

```ts
const model = useParamPanelStore((s) => s.model)              // PanelModel | null
const values = useParamPanelStore((s) => s.values)            // Record<string, ControlValue>
const showAdvanced = useParamPanelStore((s) => s.showAdvanced)
const error = useParamPanelStore((s) => s.error)              // string | null
const setValue = useParamPanelStore((s) => s.setValue)
const rerollSeed = useParamPanelStore((s) => s.rerollSeed)
const toggleAdvanced = useParamPanelStore((s) => s.toggleAdvanced)
const load = useParamPanelStore((s) => s.load)
```

Subscribe with a selector, never the bare store (WORKFLOW §4.10). `seedError` and
`MAX_SAFE_SEED` come from `../state/params` and `../state/paramPanel`.

### Structure

```
<section className="panel param-panel">
  <h2 className="param-panel-title">Settings</h2>

  error !== null   → <p className="param-panel-empty">{error}</p>   and nothing else
  model === null   → render nothing

  model.basic.map(field)

  model.advanced.length > 0 →
    <button className="param-advanced-toggle" aria-expanded={showAdvanced} onClick={toggleAdvanced}>
      Advanced settings ({model.advanced.length})
    </button>
    showAdvanced → <div className="param-advanced">  grouped: controls with group === null
                    first, then one <fieldset className="param-group"> per distinct group with
                    a <legend className="param-group-title">{group}</legend>  </div>

  model.omitted.length > 0 →
    <div className="param-omitted">
      <h3 className="param-omitted-title">Not offered by this model</h3>
      <ul>{ one <li className="param-omitted-item"> per omission }</ul>
    </div>
</section>
```

### One field, by `control.kind`

Every field is `<div className="param-field">` with
`<label className="param-field-label" htmlFor={control.name}>{control.label}</label>`.

| kind | control |
|---|---|
| `text` | `<textarea rows={2}>` |
| `lyrics` | `<textarea rows={10}>` |
| `int` | `<input type="number" min max step={1}>` |
| `float` | `<input type="number" min max>`, `step` when the range declares one |
| `enum` with choices | `<select>` over `control.choices` |
| `enum` with `fromNode` and none | `<select disabled>` with a single placeholder option |
| `seed` | **`<input type="text" inputMode="numeric">`** — see below |

**The seed field must not be `<input type="number">`.** A number input coerces its value
through a JS number, which is the precise rounding T-308a refuses. Text plus
`seedError(raw)` is what keeps the refusal real; render the message in
`<p className="param-field-error">` and leave the value in the store as the user typed it.
Next to it, `<button className="param-seed-reroll" onClick={rerollSeed}>` labelled `Reroll`,
both inside `<div className="param-seed">`.

Numeric fields carry `<span className="param-field-hint">{min}–{max}</span>`.

### Copy — use these exact strings

| Where | Text |
|---|---|
| Node-backed enum, no options | `Options come from your ComfyUI. Start it to choose a value.` |
| Node-backed enum placeholder | `Not loaded` |
| Seed hint | `Any whole number up to 9007199254740991.` |
| Omitted item, reason present | `{name} — {reason}` |
| Omitted item, no reason | `{name} — the profile records that this model does not accept it.` |

The first and the fourth must not read alike. One means ComfyUI is off; the other means the
model has no such input, checked against a live node schema and recorded. Collapsing them is
how a user concludes the app cannot see their install — the same distinction the profile
picker already keeps with its "Readiness could not be checked" line.

The seed hint states a limit lower than the model's own range. That is deliberate (T-308a) and
the number belongs on screen, not only in a comment.

### AudioStudio

Render `<ParamPanel />` between the profile-picker `<section>` and `<JobQueue />`, and load on
the effective profile:

```tsx
const load = useParamPanelStore((state) => state.load)
useEffect(() => {
  void load(effectiveId)
}, [effectiveId, load])
```

`effectiveId` already exists in that component. The store's own reload guard makes a repeat
call harmless.

### theme.css

Append a `/* --- Param panel (T-308b) --- */` block. Every class above needs a rule
(WORKFLOW §4.5). Use existing tokens only — `--panel`, `--panel-hover`, `--accent`, `--radius`
— and follow the spacing already used by `.profile-list` and `.lyrics-field`. Change no
existing rule.

### Acceptance criteria

1. `npm run gate` green. **Test counts do not change** (151) — this run adds no testable
   logic, and adding some means it was put in the wrong file.
2. `oxlint` adds **no** warnings. The repo is at zero except one pre-existing in
   `llm.test.ts`; keep it that way.
3. Every new `className` has a rule in `theme.css`.
4. No `invoke` or `listen` outside `app/src/bridge/` (WORKFLOW §4.5).
5. No value derived in JSX. Reading `control.range?.min` to fill an attribute is rendering;
   computing which controls to show, in what order, or what a value should be, is not.

### Producer click-through (this is a UI task — the gate cannot see any of it)

- [ ] Audio view shows **tags, lyrics, duration, bpm, key, time signature, language, seed**, in that order.
- [ ] **No negative-prompt box**, and "Not offered by this model" explains why.
- [ ] Key, time signature and language are **disabled** and say to start ComfyUI. They must not look like the negative-prompt case.
- [ ] Advanced is collapsed; opening it shows steps, shift and a **Planner** fieldset of five.
- [ ] Seed is filled on open, differs from last session, and **Reroll** changes only it.
- [ ] Paste `18446744073709551615` into the seed: refused with a message, nothing rounded.
- [ ] Switch to Lyrics and back: typed tags and the seed survive.

### Out of scope

- Live enum choices — **T-308c**, with the `InputSpec::Enum { node }` fix above.
- Submitting anything. `specInputs` exists and is tested; nothing calls it until T-310.
- The LoRA stack panel (T-309) and sliders/polish.

### Aider launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-308b-brief.md --read CONVENTIONS.md --read app/src/state/params.ts --read app/src/state/paramPanel.ts --read app/src/components/JobQueue.tsx --file app/src/components/ParamPanel.tsx --file app/src/views/AudioStudio.tsx --file app/src/theme.css
```

`params.ts` and `paramPanel.ts` are `--read`: the component calls into both and must not edit
either. `JobQueue.tsx` is `--read` as the house pattern for a small panel component.
`theme.css` is 998 lines, which dominates the working set — if the run struggles, split the
CSS into its own follow-up rather than widening the file list.
