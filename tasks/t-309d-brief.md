# T-309d — Generate: the panel becomes a job

**Two parts in two lanes** (WORKFLOW §1).

**Part 1 — architect-direct. LANDED.** (If the executor reports these files missing, Part 1 has not been run yet — it must land first.) The spec assembly, the blockers, the
submit store, and one addition to the jobs store. Small, and every line of it is a rule about
what reaches ComfyUI — this is the first task in the phase whose defects are *wrong tracks*
rather than wrong screens, so none of it goes out to an executor.

**Part 2 — the Aider run. LANDED.** `<GenerateBar>`, its `theme.css` block, the AudioStudio wiring.

---

## The finding that reorders this task

`generate_audio` submits and then starts the job pump itself (`generate.rs:96` →
`jobs.rs:70`), keyed on `submission.prompt_id`. The frontend jobs store learns about a job in
exactly one place — `useJobsStore.run()`, which is what the *other* submit path calls — and
`applyJobEvent` opens with:

```ts
const job = jobs[event.payload.id]
if (!job) return jobs
```

That early return is correct: the store must not invent entries for jobs it never started. But
it means **a job submitted through `generate_audio` has every one of its progress, done and
failed events silently discarded.** Press Generate, and the queue panel stays empty; the
generation runs to completion on the GPU and the app never hears about it. No error, nothing in
the console, nothing in the log.

Fifth instance this phase of the same shape — *a guard in one layer does not bind the layer
above it* — and the first where the two layers are correct in isolation and wrong together.

So Part 1 adds **`useJobsStore.register(id)`**, and submitting without registering is the
defect the tests are written against.

There is a race worth naming rather than fixing: the pump starts inside the command, so an
event can be emitted before `register` runs and be dropped. Terminal `done` / `failed` always
arrive afterwards, so a job cannot end up stuck — the worst case is a missed early status. A
fix means the backend emitting a `job://queued` the frontend registers from, which is a change
to the pump for a cosmetic gain; not now, and written down so the next person does not read the
gap as an oversight.

## What is deliberately *not* a blocker

**ComfyUI being disconnected.** `generate_audio` calls `ensure_connected`, which spawns
`comfy-mcp` when nothing is connected yet (`comfy.rs:152`). Disabling Generate on
`jobs.connected` would refuse to start a generation the backend would have handled — and
`connected` is false until something else happens to connect, so the button would be dead on a
cold start. If ComfyUI itself is down, the command's own error is what says so.

Same reasoning for the LoRA catalog being `unavailable`: it is evidence ComfyUI is not
answering, but it is not this button's job to infer that.

---

## Part 1 — what landed

### `app/src/bridge/generate.ts`

```ts
/** Mirrors Rust `src-tauri/src/generate.rs` `Submission`. */
export interface Submission {
  prompt_id: string
  workflow_path: string
  unchecked_slots: string[]
  lora_nodes: string[]
  output_format: string | null
}

export async function generateAudio(spec: GenerationSpec): Promise<Submission>
```

`GenerationSpec` mirrors the Rust struct: `profile_id`, `inputs` (the tagged `InputValue` map
`specInputs` already produces), `loras` (`specLoras`), `lyrics` (`LyricRef | null`).

### `app/src/state/generate.ts` — pure

As landed — these are the exact signatures, and they differ from this brief's first draft
because the lyrics and seed controls are found by **kind** rather than by the names `"lyrics"`
and `"seed"`, which needs the model:

```ts
specFor(profileId: string, model: PanelModel, values: Record<string, ControlValue>,
        stack: StackRow[], doc: LyricDoc | null): GenerationSpec
blockers(profileId: string | null, model: PanelModel | null,
         values: Record<string, ControlValue>): string[]
lyricRefFor(doc: LyricDoc | null, model: PanelModel,
            values: Record<string, ControlValue>): LyricRef | null
approvedOffer(doc: LyricDoc | null, model: PanelModel | null,
              values: Record<string, ControlValue>): string | null
submissionNotes(submission: Submission): string[]
export const QUEUED: string
export const USE_APPROVED = 'Use it'   // the button label
```

A profile names its own inputs, and a custom-imported workflow (ARCHITECTURE 5b) may well call
its lyrics field something else; matching a hardcoded key would silently stop attaching lyric
provenance for exactly those users.

### The invariants

1. **A submitted job is registered with the jobs store.** Without it the pump's events are
   dropped and the app is deaf to its own generation. The test submits and asserts the id is in
   `useJobsStore.getState().jobs` — and a second test drives a `job://progress` through
   `applyJobEvent` for that id and asserts the status changes, because "the id is in the map" and
   "events now land" are different claims and only the second one is the point.

2. **The `LyricRef` is attached only when the text being sent *is* the approved version's
   text.** `GenerationSpec` carries the lyric text in `inputs` and a `LyricRef` beside it, and
   nothing downstream reconciles them — PROJECT.md (2026-08-27) records that gap and defers
   closing it to T-311's sidecar. It closes cheaply here instead: compare `values.lyrics` to
   `approvedText(doc)` and attach the ref only on an exact match. A ref that names v2 beside v3's
   words is a provenance record that is wrong in the one way provenance must never be wrong, and
   the acceptance bar for T-311 is that a run reproduces **from the sidecar alone**.

   Byte-exact, deliberately: a user who pastes the approved lyric and then fixes one word has a
   different lyric, and the ref should drop. *Vacuity trap:* a test where the panel is empty and
   the doc has no approval passes with the rule deleted — one test must have a real approved doc
   and matching text, and another the same doc with one character changed.

3. **An invalid seed blocks Generate; it is never sent.** `seedError` exists and the panel
   already shows its message. This is the third layer of the same rule (T-308a refuses, T-308b's
   text input keeps the DOM from rounding, this refuses to submit) and it is the layer that
   decides what actually reaches Rust.

4. **An unknown profile blocks Generate.** `effectiveProfileId` returns the configured id
   whether or not a profile answers to it, so this is reachable by deleting a user profile;
   `generate_audio` would answer `no profile named X`, which is true and useless.

5. **`unchecked_slots` is reported, never swallowed.** These are addresses the audit could not
   resolve — MiniMax's seed is one (MCP-SURFACE 18.5), *unverified rather than known-working*.
   Empty on ACE-Step, so the note fires only where there is something to say.

6. **The submission says what the graph edits did.** `lora_nodes.length` and `output_format`
   are the only confirmation a user gets that their LoRAs were spliced and that the lossy save
   node was swapped — the two edits this pipeline makes that validation cannot vouch for
   (17.1, 16.3). The owner swaps that save node by habit; the app doing it silently is worth
   one line on screen.

7. **Submitting twice in a row is prevented while one submit is in flight**, with a `busy`
   flag. Two clicks would queue two jobs with the same seed, and ACE-Step is not reproducible
   run-to-run (17.3), so they would not even be the same track.

### Copy — all of it in `generate.ts`, none in the component

Conditional wording goes in the pure module; plain interpolation stays in the view. That line
was drawn at T-309b, where an `<option>` composing `${label} (epoch ${n})` was unreadable by any
test.

| Where | Text |
|---|---|
| Blocked, bad seed | the existing `seedError` message, unchanged |
| Blocked, unknown profile | `No profile answers to {id}. Pick a model profile above.` |
| Approved-lyric offer | `The Lyrics Studio has v{n} approved. Use it` |
| Queued | `Queued. Watch it in the queue below.` |
| Unchecked slots | `{n} settings could not be checked against this workflow: {names}. They may not reach the model.` |
| LoRAs applied | `{n} LoRAs applied.` / `1 LoRA applied.` |
| Output format | `Saving lossless {format}.` |
| Failed | the command's own error string, on its own, **unspliced** |

That last row is not a formality. The param panel shipped a note with comfy-cli's raw transport
error spliced into the middle of a sentence, and it took a person reading the screen to find it
while every test passed. An error from `generate_audio` gets its own element and no sentence
wrapped around it.

---

## Part 2 — the Aider run

### Files

- **create** `app/src/components/GenerateBar.tsx`
- **modify** `app/src/views/AudioStudio.tsx` (render it between `<LoraStack />` and `<JobQueue />`)
- **modify** `app/src/theme.css` (append one block; change no existing rule)

No logic. Every string comes from `generate.ts`; every decision comes off the store or a pure
function. **If a value needs deriving, this brief is wrong** — say so rather than deriving it.

### The stores' surface

```ts
// from ../state/generatePanel
const busy = useGenerateStore((s) => s.busy)                 // boolean
const error = useGenerateStore((s) => s.error)               // string | null
const last = useGenerateStore((s) => s.last)                 // Submission | null
const submit = useGenerateStore((s) => s.submit)             // () => Promise<void>
const useApprovedLyric = useGenerateStore((s) => s.useApprovedLyric)

// read for the pure functions above
const profileId = useParamPanelStore((s) => s.profileId)
const model = useParamPanelStore((s) => s.model)
const values = useParamPanelStore((s) => s.values)
const doc = useLyricsStore((s) => s.doc)
```

Selectors, never the bare store (WORKFLOW §4.10). The component calls
`blockers(profileId, model, values)`, `approvedOffer(doc, model, values)` and, when `last` is
not null, `submissionNotes(last)` — each once, into a local.

### Structure

```
<section className="panel generate-bar">
  approvedOffer(doc, values) !== null →
    <p className="generate-lyric-offer">
      {offer}
      <button type="button" className="generate-use-lyric" onClick={useApprovedLyric}>{USE_APPROVED}</button>
    </p>

  blockers.map(reason => <p className="generate-blocked" key={reason}>{reason}</p>)

  <button type="button" className="generate-button"
          disabled={blockers.length > 0 || busy}
          onClick={() => void submit()}>
    {busy ? 'Queueing…' : 'Generate'}
  </button>

  error !== null → <p className="generate-error">{error}</p>

  notes.map(note => <p className="generate-note" key={note}>{note}</p>)
</section>
```

Every `<button>` carries `type="button"` — three of them were missed in T-309b and defaulted to
submit.

### theme.css

Append `/* --- Generate (T-309d) --- */`. Existing tokens only; every class needs a rule; change
no existing rule. `.generate-button` is the first primary action on this view — use `--accent`
with `--accent-hover`, and make the disabled state visibly disabled rather than merely dimmed,
because a user who cannot tell it is disabled reads a broken app.

`.generate-error` uses `--danger`, `.generate-blocked` uses `--warning`, `.generate-note` uses
`--text-muted`.

## Acceptance criteria

1. `npm run gate` green, and **the test count does not change: 236**. Part 2 adds no testable
   logic; adding a test means something went in the wrong file. *(Met. Review then added
   `notesFor` and its three tests, taking the total to 239 -- a rule the brief had missed, not
   logic the run misplaced.)*
2. `oxlint` adds no warnings.
3. Every new `className` has a rule in `theme.css`.
4. No `invoke` or `listen` outside `app/src/bridge/`.
5. No user-visible sentence written inside the JSX.
6. Every `<button>` has an explicit `type`.

## Producer click-through

- [ ] **Generate with ComfyUI never connected this session.** It must work — the backend
      connects itself. A dead button here is the bug this brief's "not a blocker" section exists
      to prevent.
- [ ] The job **appears in the queue panel** and its status changes as it runs. This is the
      whole point of the task; an empty queue means `register` is not wired.
- [ ] It reaches `completed`, and a file exists where the submission says.
- [ ] `Saving lossless flac.` appears. **This is the first end-to-end proof of the T-305a swap**
      against a real run.
- [ ] Stack two LoRAs and generate: `2 LoRAs applied.`
- [ ] Paste `18446744073709551615` into the seed: Generate is **disabled** with the seed message.
- [ ] **Quit ComfyUI and press Generate**: an error appears in one piece, readable, with no URL
      or `WinError` spliced into a sentence.
- [ ] With a lyric approved in the Lyrics Studio, the offer line appears; **Use it** fills the
      lyrics field; generate, and confirm the run carries it.

**This is the first generation this project has ever run through its own pipeline.** Everything
so far has been proven against the mock transport. Expect the click-through to find things, and
treat anything it finds as evidence about the pipeline rather than about this task.

## Out of scope

- **Output ingestion, the library write and the provenance sidecar — T-311.** This task queues a
  job and reports what it queued; nothing collects the audio yet.
- **The queue panel's own improvements — T-310.**
- **Batch by seeds — T-312.**

## Aider launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-309d-brief.md --read CONVENTIONS.md --read app/src/state/generate.ts --read app/src/state/generatePanel.ts --read app/src/components/LoraStack.tsx --file app/src/components/GenerateBar.tsx --file app/src/views/AudioStudio.tsx --file app/src/theme.css
```

`LoraStack.tsx` is `--read` as the closest house pattern — same shape, same store discipline,
written against the same conventions one task ago.
