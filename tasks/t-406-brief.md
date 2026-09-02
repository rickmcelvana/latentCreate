# T-406 — provenance inspector — "re-use these settings"

The **last task in Phase 4**. The recipe that made a track, shown in full and
reusable. Two halves, each its own lane: **a** shows the whole sidecar (today the
card shows a summary); **b** loads a past track's spec back into the Audio Studio.

## What already exists, and what this adds

The Library card already renders a **summary** recipe (T-311e): model, licence,
created, seed, LoRAs, run (`prompt_id`) — see the `track-recipe` `<dl>` in
[Library.tsx](../app/src/views/Library.tsx). T-406 adds the **rest of the
sidecar** and a **re-use** action.

**T-406 is entirely frontend.** `listTracks()` already returns the full
`TrackSet` with each raw `Track` — `provenance.spec` (all semantic inputs, the
LoRA stack, the lyric ref, the title), `resolved_slots` (what ComfyUI actually
received), `comfy` (server versions + url), `template`. The library store then
**throws it away**: `trackRows(set)` flattens each `Track` to a display-only
`TrackRow` of strings ([library.ts](../app/src/state/library.ts)). So lane a's
first job is to stop discarding it; no Rust, no bridge change.

Verified surfaces (2026-09-02, before briefing):
- `create_core::provenance::{Track, Provenance, ComfyServerInfo}` — the sidecar
  shape ([provenance.rs](../crates/create-core/src/provenance.rs)); mirrored in
  `bridge/library.ts`.
- `GenerationSpec` is **already the shape `specFor` builds** — so re-use is a
  store handoff, not a new type. The reverse of `specInputs` is a one-liner
  (each `InputValue.value` is already a `ControlValue`); the reverse of
  `specLoras` maps `{file,strength,enabled}` back to a `StackRow`.
- `paramPanel.load(profileId)` fetches the model and sets
  `values: initialValues(model), seedPinned: false` — re-use needs the same
  fetch but with the **sidecar's** values and **`seedPinned: true`**.
- `useNavStore.setView('audio')` switches to the Audio Studio.

## Lane a — the inspector (read-only)

**Keep the raw tracks in the store.** In `state/library.ts`, alongside
`tracks: TrackRow[]`, add `byId: Record<string, Track>` built from the same
`TrackSet` (`Object.fromEntries(set.tracks.map((t) => [t.id, t]))`). The event
path (`subscribeTracks`) and `load` both set it. This is the only state change.

**A pure view builder.** Add `provenanceView(track: Track): ProvenanceSection[]`
where a section is `{ title: string; facts: { label: string; value: string }[] }`.
Sections, in order — and only those with content:

- **Inputs** — every entry in `spec.inputs`, `name` → formatted value. A `seed`
  reads as its number; text/enum/int/float as their value. This is the semantic
  recipe the summary omits (tags, duration, key/scale, language, …).
- **Lyrics** — when `spec.lyrics` is set: `doc_id` and `version` (e.g.
  "ld-0001, v2"). It is a *reference*, not the words; the words, if the profile
  took them, are an input above.
- **Resolved slots** — every `resolved_slots` entry, `address` → value: what
  ComfyUI actually received after fan-out (MCP-SURFACE 17.2's "what really ran").
  Empty for older sidecars (`#[serde(default)]`).
- **Server** — `comfy.comfyui_version`, `comfy.comfy_cli_version`, `comfy.url`;
  and `template` when present. Each line only when its value is `Some`.

Formatting the tagged `InputValue`/slot value to a string is a small helper
(`formatValue`) — reuse the seed/number/text rendering the summary already does
rather than JSON-stringifying a `{type,value}` object onto the screen.

**The UI.** A **Details** disclosure on each `TrackCard`, below the summary
`track-recipe`. Local `useState` for open/closed (a read-only toggle needs no
cross-row exclusivity — unlike the delete/rename confirms, which are store-held
because only one may be active). When open, render the `provenanceView` sections
as labelled `<dl>` groups, inside an `overflow-x: auto` container (slot addresses
and paths can be long — the page body must never scroll sideways, per CONVENTIONS
/ the T-407 scrollbar rule). The **"Re-use these settings"** button lives here
(lane b wires it).

**Tests (`state/library.test.ts`):** `byId` carries the raw track for a row;
`provenanceView` produces the Inputs/Lyrics/Resolved/Server sections from a full
sidecar, omits the empty ones (no lyric ref, no comfy, no resolved slots), and
renders a seed as its number, not `[object Object]`.

## Lane b — "re-use these settings"

The inverse of `specFor`: a `GenerationSpec` → the Audio Studio panels.

**Pure reversers** (in `state/generate.ts`, beside `specInputs`/`specLoras`):

```ts
/** A spec's tagged inputs back to raw panel values (the reverse of specInputs). */
export function controlValues(inputs: Record<string, InputValue>): Record<string, ControlValue> {
  const values: Record<string, ControlValue> = {}
  for (const [name, input] of Object.entries(inputs)) values[name] = input.value
  return values
}

/** A spec's LoRA refs back to stack rows (the reverse of specLoras). */
export function stackFromLoras(loras: LoraRef[]): StackRow[] {
  return loras.map((lora) => ({
    path: lora.file,
    label: (lora.file.split(/[\\/]/).pop() ?? lora.file).replace(/\.safetensors$/i, ''),
    strength: lora.strength,
    enabled: lora.enabled,
  }))
}
```

**Panel hydration actions:**

- `paramPanel.hydrate(profileId, values)` — like `load`, but sets the given
  `values` and **`seedPinned: true`** (the trap, below). It must still fetch the
  model (`getProfileInputs` → `panelModel`) so the controls render; a profile the
  app no longer has yields the same "No profile answers to …" error `load` sets,
  not a broken panel. Do **not** early-return on a same-profile match the way
  `load` does — re-use must overwrite the on-screen values even when the profile
  is already selected.
- `loraPanel.hydrate(profileId, stack)` — load the panel's catalog for
  `profileId` (so labels/`missingFrom` still work), then set `stack` from the
  spec. A re-used LoRA the current catalog no longer offers stays in the stack
  and is reported by the existing `missingFrom`, exactly as a deleted LoRA is.

**The orchestrator.** Add `reuse(spec: GenerationSpec)` to the **generatePanel**
store (it already reads these panels to build a spec in `submit`; the inverse
belongs beside it):

```
reuse: async (spec) => {
  await useParamPanelStore.getState().hydrate(spec.profile_id, controlValues(spec.inputs))
  await useLoraPanelStore.getState().hydrate(spec.profile_id, stackFromLoras(spec.loras))
  set({ title: spec.title ?? '' })   // the T-409 title override; '' shows untitled
  useNavStore.setView('audio')
}
```

`title: spec.title ?? ''` sets the Audio Studio title override to what the track
was made with (T-409 trap 5 — re-use carries the title). `''` when the spec was
untitled, which the field shows as empty.

**The button.** In the inspector's Details (lane a), a "Re-use these settings"
button → `reuse(byId[row.id].provenance.spec)`. After it, the user is on the
Audio Studio with the profile, inputs, LoRA stack, title and **the exact seed**
loaded, ready to regenerate the same track.

**Not carried (say so):** the source **lyric document is not re-opened** in
Lyrics Studio. `spec.lyrics` is a `(doc_id, version)` ref that may have been
edited or deleted, and the scope is "the param panel and LoRA stack". The lyric
*text*, if the model took it, is an `inputs` entry and rides along; the ref is
shown in the inspector for reference. The **audio file is never touched** — re-use
prepares a *new* generation, it does not alter the past track.

### The trap — the seed is pinned, not re-rolled

`reuse` must land with **`seedPinned: true`**. A fresh Generate re-rolls an
unpinned seed (T-316, to avoid `execution_cached` duplicates), but re-use is the
opposite intent: the user is reproducing **this** track, so the seed on screen
must be the sidecar's, and `submit` must not replace it. **The test that matters:
after `reuse`, `paramPanel.values[seed]` equals the sidecar's seed verbatim and
`seedPinned` is `true`** — and, downstream, a `submit` right after `reuse` sends
that same seed (kills a mutation that hydrates with `seedPinned: false`).

**Tests:** `controlValues` inverts `specInputs` (round-trip a spec's inputs);
`stackFromLoras` inverts `specLoras`; `reuse` hydrates paramPanel with the seed
pinned, loraPanel with the stack, the title override, and navigates to `audio`;
a `reuse` of a spec whose profile is gone surfaces the param panel's error rather
than a blank panel.

## Traps (summary)

1. **Seed pinned on re-use** (lane b) — the one behavioural trap; the test asserts
   the seed is verbatim and pinned.
2. **Stop discarding the raw sidecar** (lane a) — the data is already on the wire;
   the store just needs to keep it. No Rust.
3. **Long values scroll their own container** — slot addresses and LoRA paths are
   long; the inspector body scrolls inside `overflow-x: auto`, never the page.
4. **Re-use prepares, never mutates** — no file is touched, the past track is
   unchanged; the lyric document is not re-opened.

## Click-through (producer, on the desktop app)

1. In the Library, open a track's **Details** — it shows the full recipe: every
   input (tags, duration, seed, …), the resolved slots, the lyric ref if any, and
   the ComfyUI version/url. An older track with no `prompt_id`/slots still opens,
   just with fewer lines.
2. A **titled** track (T-409) shows its title; the resolved slots read as
   `address: value`, not `[object Object]`.
3. Click **Re-use these settings** — the app switches to the Audio Studio with
   the same profile selected, the same inputs and LoRA stack, the same title in
   the Title field, and **the same seed**, not a fresh one.
4. Generate — the queued job's seed matches the original track's seed (re-use
   reproduces; it does not re-roll).
5. Re-use a track whose profile has since been removed — the panel shows the
   "no profile answers to …" note, not a blank or broken form.

## Lane order and close-out

`a` (inspector) → `b` (re-use). Each commits on a green `npm run gate`. When
lane b's click-through passes, **T-406 is done and Phase 4 closes** — update
PROJECT.md's Snapshot and session log and `tasks/phase-4.md` to mark the phase
complete, and note the milestone/what-remains for whatever phase comes next.

## Not this task

- Re-opening the source lyric document (see lane b).
- Editing a past track's provenance — the sidecar is a record, read-only here.
- The two noted-not-scheduled items (Library player below-the-fold layout,
  docs/CSS-TODO.md; persisting the selected lyric document across restarts).
