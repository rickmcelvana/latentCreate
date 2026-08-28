# T-309b — `<LoraStack>`

**Lane: Aider** (WORKFLOW §1). About 300 lines of JSX and CSS with no logic in it, because
T-309a deliberately put every derivation in `state/loras.ts` and every piece of state in
`state/loraPanel.ts`. That is exactly the work the executor lane exists for, and the launch
command is at the bottom.

**Part 1 below landed first** (`183cace`), and it exists so that Part 2 can be a `map`.

---

## Part 1 — what landed, and why the component got smaller

Writing this brief turned up three lookups the component would otherwise have done itself. Each
is one line. Each is unreachable from any test the moment it lives in JSX, because vitest runs
in `node` with no DOM — which is how every panel this phase acquired the defect only a person
looking at a screen could find.

- **`entryFor(catalog, path)`** — a `<select>` hands back a string; the stack needs a label.
  The store therefore takes **`addPath(path)`**, not an entry. An unknown path is ignored
  rather than becoming a row that names a LoRA the install does not have.
- **`stackRows(stack, catalog)`** — carries `missing`, the sentence for it, `canMoveUp` and
  `canMoveDown`. `missingNote` is private now, so there is exactly one wording of it and a test
  can read it.
- **`ADD_PLACEHOLDER`, `EMPTY_STACK`, `fullNote(panel)`** — the three strings Part 2 would
  otherwise have had to invent. `fullNote` counts the profile's slots, so it is logic, so it
  is tested here rather than written into JSX: `All 4 slots are full` and a hardcoded `4` are
  indistinguishable on the only profile that has a `loras` block, which is the same vacuity
  the strength default had, so a one-slot profile is in the test.

Eight mutations, eight killed, `canMoveDown` off by one among them. Frontend 195 → **205
tests**.

---

## Part 2 — the Aider run

### Files

- **create** `app/src/components/LoraStack.tsx`
- **modify** `app/src/views/AudioStudio.tsx` (render it, load on profile change)
- **modify** `app/src/theme.css` (append one block; change no existing rule)

Nothing else. No new state, no new bridge call, no logic. **If a value needs deriving, it
belongs in `loras.ts` and this brief is wrong** — say so rather than deriving it in the
component.

### The store's surface

```ts
const panel = useLoraPanelStore((s) => s.panel)                  // LoraPanel | null
const stack = useLoraPanelStore((s) => s.stack)                  // StackRow[]
const showSuperseded = useLoraPanelStore((s) => s.showSuperseded)
const addPath = useLoraPanelStore((s) => s.addPath)              // (path: string) => void
const removeRow = useLoraPanelStore((s) => s.removeRow)          // (index: number) => void
const toggleRow = useLoraPanelStore((s) => s.toggleRow)
const setStrength = useLoraPanelStore((s) => s.setStrength)      // (index, strength) => void
const moveRow = useLoraPanelStore((s) => s.moveRow)              // (from, to) => void
const toggleSuperseded = useLoraPanelStore((s) => s.toggleSuperseded)
const refresh = useLoraPanelStore((s) => s.refresh)              // () => Promise<void>
const load = useLoraPanelStore((s) => s.load)                    // AudioStudio only
```

Subscribe with a selector, never the bare store (WORKFLOW §4.10).

From `../state/loras`: `pickerGroups`, `stackRows`, `catalogNote`, `excludedNote`,
`supersededCount`, `addable`, `fullNote`, `ADD_PLACEHOLDER`, `EMPTY_STACK`.

Call `pickerGroups` **once** into a local and use it for both the `disabled` test and the
options -- calling it twice per render is the only performance trap in this component, and it
reads worse besides.

### Structure

```
panel === null  → render nothing at all.   ← the whole component returns null

<section className="panel lora-stack">
  <h2 className="lora-stack-title">LoRAs</h2>

  catalogNote(panel.catalog) !== null →
    <p className="lora-stack-note">
      {note}
      <button type="button" className="lora-stack-retry" onClick={() => void refresh()}>Retry</button>
    </p>

  stackRows(stack, panel.catalog).map(row)          ← the stack, in apply order
  stack.length === 0 → <p className="lora-stack-empty">{EMPTY_STACK}</p>

  const offers = pickerGroups(panel.catalog, stack, showSuperseded)   ← once, at the top

  <div className="lora-stack-add">
    <select className="lora-picker" value="" onChange={e => addPath(e.target.value)}
            disabled={!addable(panel, stack) || offers.length === 0}>
      <option value="">{addable(panel, stack) ? ADD_PLACEHOLDER : fullNote(panel)}</option>
      { offers.map(group =>
          <optgroup key={group.label} label={group.label}>
            { group.entries.map(entry =>
                <option key={entry.path} value={entry.path}>
                  {entry.superseded ? `${entry.label} (epoch ${entry.epoch})` : entry.label}
                </option>) }
          </optgroup>) }
    </select>
    <span className="lora-stack-count">{stack.length} of {panel.max_stack}</span>
  </div>

  supersededCount(panel.catalog) > 0 →
    <button type="button" className="lora-checkpoints-toggle"
            aria-pressed={showSuperseded} onClick={toggleSuperseded}>
      Training checkpoints ({supersededCount(panel.catalog)})
    </button>

  excludedNote(panel.catalog) !== null →
    <p className="lora-stack-excluded">{excludedNote(panel.catalog)}</p>
</section>
```

**The `<select>` is a control that fires and resets.** Its `value` is the empty string always,
so it returns to the placeholder after each pick and can add the same-looking thing twice from
different groups. It never displays the current stack — the rows below it do that.

### One row, from a `StackRowView`

```
<li className={`lora-row ${view.missing ? 'lora-row-missing' : ''} ${view.row.enabled ? '' : 'lora-row-bypassed'}`}>
  <span className="lora-row-label">{view.row.label}</span>

  <input type="range" className="lora-row-strength"
         min={panel.strength.min} max={panel.strength.max}
         step={panel.strength.step ?? 0.01}
         value={view.row.strength}
         onChange={e => setStrength(view.index, Number(e.target.value))}
         disabled={!view.row.enabled}
         aria-label={`${view.row.label} strength`} />
  <span className="lora-row-value">{view.row.strength.toFixed(2)}</span>

  <button className="lora-row-move" disabled={!view.canMoveUp}
          onClick={() => moveRow(view.index, view.index - 1)} aria-label="Move up">↑</button>
  <button className="lora-row-move" disabled={!view.canMoveDown}
          onClick={() => moveRow(view.index, view.index + 1)} aria-label="Move down">↓</button>

  <label className="lora-row-bypass">
    <input type="checkbox" checked={view.row.enabled}
           onChange={() => toggleRow(view.index)} />
    On
  </label>

  <button className="lora-row-remove" onClick={() => removeRow(view.index)}
          aria-label={`Remove ${view.row.label}`}>Remove</button>

  view.note !== null → <p className="lora-row-note">{view.note}</p>
</li>
```

Rows live in a `<ul className="lora-rows">`. `key` is `view.row.path`.

**Every one of `missing`, `note`, `canMoveUp`, `canMoveDown` comes off the view.** Do not
recompute `index > 0` or compare paths against the catalog — that is the exact line T-309a
moved out, and putting it back makes it untestable again.

**The strength control is a `<input type="range">`, not a number box.** The profile's
`0.0–2.0 step 0.05` is 41 positions and every one of them is a value someone would choose. The
node's own range is `-100…100 step 0.01`, which is **not** what goes on screen — use
`panel.strength`, never anything read from a node.

### Copy — import these, do not write any

**Every user-visible sentence already exists in `state/loras.ts`.** There is nothing to word in
this run. Wording put in JSX is wording no test can read, and the param panel shipped a note
that was unreadable on a real screen while every test passed.

| From `loras.ts` | Where it goes |
|---|---|
| `catalogNote(panel.catalog)` | the cache / cannot-read sentence, above the rows |
| `excludedNote(panel.catalog)` | the count of non-adapters, at the bottom |
| `view.note` | the missing-LoRA sentence, under its own row |
| `ADD_PLACEHOLDER` | the picker's resting option |
| `fullNote(panel)` | the picker's resting option once the slots are gone |
| `EMPTY_STACK` | shown instead of rows when the stack is empty |

`EMPTY_STACK` deliberately does not read like an error — most generations use no LoRA at all,
so an empty stack is the normal case rather than something left undone.

If you find yourself typing a sentence a user will read, stop: it belongs in `loras.ts` and
this brief has missed one. Say so rather than inlining it.

### AudioStudio

Render `<LoraStack />` between `<ParamPanel />` and `<JobQueue />`, and load on the effective
profile, next to the existing param-panel load:

```tsx
const loadLoras = useLoraPanelStore((state) => state.load)
useEffect(() => {
  void loadLoras(effectiveId)
}, [effectiveId, loadLoras])
```

`effectiveId` already exists in that component. The store's own reload guard makes a repeat
call harmless — do not add a guard of your own.

### theme.css

Append a `/* --- LoRA stack (T-309b) --- */` block. Every class above needs a rule
(WORKFLOW §4.5). Use existing tokens only — `--panel`, `--panel-hover`, `--border`, `--accent`,
`--text-muted`, `--warning`, `--danger`, `--radius`, `--gap-xs/sm/md/lg`, `--transition` — and
follow the spacing the `.param-*` block already uses. **Change no existing rule.**

Three states need to look different at a glance and none may look like an error:

- `.lora-row-bypassed` — dimmed (`opacity` and `--text-muted`), still legible. It is a
  deliberate choice the user made, not a fault.
- `.lora-row-missing` — `--warning`, with `.lora-row-note` beneath it. This one *is* a problem
  to fix, but a mild one.
- `.lora-stack-note` — the cache/unavailable sentence, `--text-muted`, above the rows.

`.lora-row` is a grid: label, strength, value, the two move buttons, bypass, remove. Give the
label `min-width: 0` and `overflow-wrap: anywhere` — on the reference install four of the twelve
labels are full directory names up to 49 characters, and two are 40-character filenames. **They
will overflow if you do not plan for it**, and that is the single most likely visual defect in
this task.

## Acceptance criteria

1. `npm run gate` green, and **the test count does not change: 205**. This run adds no
   testable logic at all, so adding a test means something was put in the wrong file.
2. `oxlint` adds **no** warnings. The repo is at zero except one pre-existing in `llm.test.ts`.
3. Every new `className` has a rule in `theme.css`.
4. No `invoke` or `listen` outside `app/src/bridge/` (WORKFLOW §4.5).
5. **No user-visible sentence written inside the JSX.** Every one comes from `loras.ts` or from
   a `StackRowView`.
6. Nothing recomputed that a `StackRowView` already carries.

## Producer click-through (this is a UI task — the gate cannot see any of it)

Run with **ComfyUI up** unless a row says otherwise.

- [ ] Audio view shows a **LoRAs** panel below Settings, with an empty stack and no warning.
- [ ] The picker offers **12 entries in 6 groups**, one of them headed **Loose files**. No
      `training_state.pt` anywhere in it.
- [ ] Below it: `21 files in your loras folder are not adapters and are not offered.`
- [ ] **Training checkpoints (20)** reveals 20 more, each labelled with its epoch, and the
      `loragoth` group then shows `final` plus the checkpoints.
- [ ] Add one. It appears as a row at strength **1.00**, and the picker no longer offers it.
- [ ] The strength slider moves in **0.05** steps and stops at **0.00** and **2.00**.
- [ ] Add four. The picker disables and reads `All 4 slots are full`. There is no fifth.
- [ ] ↑ / ↓ reorder the rows; the first row's ↑ and the last row's ↓ are disabled.
- [ ] Unticking **On** dims a row without removing it, and its slider disables.
- [ ] **Remove** takes a row out and the picker offers it again.
- [ ] Switch the profile to **MiniMax Music 3**: the whole LoRAs panel disappears — not an
      empty panel, not a message.
- [ ] Switch to the Lyrics tab and back: the stack is still there, in the same order.
- [ ] **Quit ComfyUI**, restart the app: the panel is **visible** and says the LoRAs could not
      be read, with a Retry button. It must not look like the model has no LoRA support.
- [ ] Start ComfyUI, press **Retry**: the list fills.
- [ ] Long labels do not overflow the panel or push the buttons off the row.

### The label question — this is the one that needs your judgement

T-307 deferred cosmetic renaming to this task because MCP-SURFACE §12.2 says it needs the owner
looking at a panel. These are the twelve, mechanically derived:

- `ACE-Step-v1.5-acoustic-guitar-and-a-merge-LoRA`, `vocal_instrument_merge`
- `ACE-Step-v1.5-ambient_dream1-LoRA`
- `ACE-Step-v1.5-chinese-new-year-LoRA`
- `ACE-Step-v1.5-raspy-vocal-and-instrumental-5-LoRAs`, `instrumental`, `male_vocals`,
  `voc_06_inst_14`, `voc_14_inst_06`
- `final` — with 20 checkpoints behind the disclosure
- `minimax_h3_fl2v_turbo_4step_v1.0_768p_comfyui_bf16`,
  `minimax_h3_fl2v_turbo_8step_v1.0_comfyui_bf16`

Stripping `ACE-Step-v1.5-` and a trailing `-LoRA` is one line of code and reads far better on
your install. It is not written, because a rule that improves these twelve could mangle
somebody else's naming scheme and nobody has looked at a second install. **Look at the panel
and say which you want**: leave them mechanical, strip the prefixes, or let people rename
entries themselves (T-309c). `final` is worth a second look too — it is accurate and it is also
the least informative label on the screen.

## Out of scope

- **Favourites and user display names — T-309c**, including whatever you decide above.
- **Drag-and-drop reordering.** The buttons are the affordance; pointer reordering is polish a
  DOM-less vitest cannot reach.
- **Sending the stack anywhere.** `specLoras` is written and tested; nothing calls it until
  T-310 builds submit, exactly as `specInputs` has been sitting since T-308a.

## Aider launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-309b-brief.md --read CONVENTIONS.md --read app/src/state/loras.ts --read app/src/state/loraPanel.ts --read app/src/components/ParamPanel.tsx --file app/src/components/LoraStack.tsx --file app/src/views/AudioStudio.tsx --file app/src/theme.css
```

`loras.ts` and `loraPanel.ts` are `--read`: the component calls into both and must not edit
either. Everything it needs from them is already exported, copy included.
`ParamPanel.tsx` is `--read` as the house pattern for exactly this kind of panel — a closer
match than `JobQueue.tsx` was for T-308b. `theme.css` is **1196 lines**, which dominates the
working set; if the run struggles, split the CSS into its own follow-up rather than widening
the file list.
