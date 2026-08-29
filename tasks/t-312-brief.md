# T-312 — batch by seeds: N variations from one click

**Lane: Aider.** Frontend only. Every decision this task makes is a pure function in
`app/src/state/generate.ts` with a test; the store loops and the bar renders.

**Depends:** nothing unlanded. T-306b (`generate_audio`), T-309d (the Generate button), T-310d
(the queue's execution order) and T-311b (ingestion) are all on `master`.

**Files to modify — four only:**

- `app/src/state/generate.ts`
- `app/src/state/generatePanel.ts`
- `app/src/components/GenerateBar.tsx`
- `app/src/theme.css`

Plus the two test files that already exist for the first two: `generate.test.ts`,
`generatePanel.test.ts`.

---

## What the phase file said, and what the code says

The phase file's entry is two sentences and **one of them is stale**:

> N jobs from one spec, differing only in seed, sharing the queue and the ingestion path. Small,
> and it is the feature that makes the two-seed trap (T-304) visible if it was got wrong.

**"Sharing the queue and the ingestion path" is already true, and needs no work.** Read before
briefing:

- `generate_audio` (`src-tauri/src/generate.rs`) mints a **per-job** working copy — `mint_job_id`
  is epoch-millis plus an atomic counter, and its test pins that two calls in the same millisecond
  differ. N calls are N independent graphs.
- Each call ends in `state.pump(app, comfy, prompt_id, Some(pending), root)`, and `pump` stores
  that job's own `PendingTrack` under its own prompt id. N jobs are N provenance records.
- `ingest_outputs` mints one track per audio file per job, advancing and persisting the counter
  before the write (T-311a).
- `queueRows` sorts live jobs **oldest-first** (T-310d, verified live), so a batch reads down the
  panel in the order ComfyUI will run it.

So the Rust side needs no change. This task is the frontend loop that was never written.

**"The two-seed trap, visible if it was got wrong" is no longer true, in two ways.** The seed
fans out to *one* address in both shipped profiles — `109.value` on ACE-Step (T-306a's
`PrimitiveInt` redirect) and `37/38.seed` on MiniMax (settled live, MCP-SURFACE 18.5). The
fan-out that survives is `duration_s -> ["94.duration", "98.seconds"]`, which a batch does not
vary. Do not write acceptance criteria around it.

**And the obvious check does not work either.** "Four variations that sound different" proves
nothing: ACE-Step is not reproducible run-to-run even on a fixed seed — two identical runs differ
in 98.1% of bytes (MCP-SURFACE 17.3). The batch is verified by **four sidecars carrying four
different seeds**, not by the audio.

## What exists

`state/generate.ts` (pure, 23 tests) already has `specFor`, `blockers`, `submissionNotes`,
`notesFor`, and two private helpers `seedControl(model)` and `lyricsControl(model)`.

`state/generatePanel.ts` holds `busy`, `error`, `last`, `lastProfileId`, `submit`.

`paramPanel.ts` exports **`freshSeed()`** — 53 bits from `crypto.getRandomValues`, capped at
`MAX_SAFE_SEED` because anything wider cannot cross `invoke` exactly. It is already the panel's
reroll and its initial seed. **This task reuses it; it does not write a second generator.**

## The decisions

### 1. The count is not a profile input

It lives in `generatePanel`, not in the param panel's `values`. A value in `values` flows into
`specInputs` and then into `GenerationSpec.inputs`, where `resolve_slots` would reject it as
`UnknownInput` — a hard error on every generation. The count reaches Rust never, and reaches the
sidecar never, because it describes the click and not the track.

### 2. A select, not a number field

`export const BATCH_CHOICES = [1, 2, 4, 8] as const` and `export const MAX_BATCH = 8`.

A bounded number input would need a third refusal layer and its own error sentence, the way the
seed has one. The seed earns that because a clamped seed is a sidecar that lies; a clamped count
lies about nothing — the queue shows exactly what was queued. A fixed set has no invalid state and
needs no copy. *(If the producer wants 3 and 5, that is a number input and one more brief; say so
rather than adding one here.)*

### 3. Vary the **values**, not the spec

The first job uses the seed on screen. Jobs 2..N use `freshSeed()`.

Build each spec by calling the existing `specFor` with a modified `values`, never by editing a
`GenerationSpec` after the fact:

```ts
specFor(profileId, model, { ...values, [name]: nextSeed() }, stack, doc)
```

`specInputs` owns the `{ type: 'seed', value }` tagging, and `InputValue` is adjacently tagged
precisely so a seed cannot be demoted to an `Int` (generation.rs). A second place that hand-builds
that object is a second place that can get it wrong, silently, in the provenance record.

This also gives every spec its own `inputs` record and its own `loras` array for free. **N specs
sharing one `inputs` object is the defect this shape exists to prevent** — all N jobs would carry
the last seed, four tracks would claim one identity, and nothing on screen would say so.

Not sequential (`first + i`): `freshSeed` can return exactly `MAX_SAFE_SEED`, so `+ i` would
overflow the range the app can record honestly, and would need a refusal for an edge nobody wants.
No dedupe: a 53-bit collision is not a real event, and a retry loop would be code no test can
reach honestly.

### 4. A model with no seed control cannot batch

`specsFor` returns **one** spec whatever the count, and `canBatch(model)` is false so the control
is not on screen. N identical specs are N identical jobs whose sidecars could not be told apart —
the app would have queued four tracks and be unable to say why they differ. Both shipped profiles
declare a seed; a custom-imported workflow (T-313) may not.

### 5. Submit sequentially, register as you go, stop on the first failure

- **Sequential `await`, never `Promise.all`.** One stdio transport; and `register` stamps
  `submittedAt = Date.now()`, which is the key `queueRows` sorts the live half by. Parallel
  submits would interleave and list the queue in an order ComfyUI will not run it in.
- **`register(prompt_id, profileId)` immediately after each success**, inside the loop. Without it
  the pump runs and every event is discarded (jobs.ts) — the reason it is the load-bearing line in
  the single-job path is unchanged, it just now happens N times.
- **Stop at the first failure.** A failure here is systemic almost by construction — ComfyUI down,
  an inert slot refusal, an unknown enum — so seven more attempts are seven more identical errors
  and seven more `fetch_template` round trips.
- **Do not clear `last` on failure.** Today's `catch` sets `last: null`, which is right for one
  job and wrong for a batch: it would wipe "Queued 2" off the screen while two jobs really are
  running on the GPU. **This is the most likely regression in the task.**

## The shape

In `state/generate.ts`:

```ts
export const MAX_BATCH = 8
export const BATCH_CHOICES = [1, 2, 4, 8] as const

/** Whether this model can be batched at all: only a seed makes two jobs differ. */
export function canBatch(model: PanelModel | null): boolean

/**
 * The specs for one click: the first exactly as `specFor` builds it, the rest
 * identical but for a fresh seed.
 *
 * `nextSeed` is a parameter with no default. `freshSeed` lives in `paramPanel.ts`,
 * which imports zustand, and this module is pure -- importing a store here to get a
 * random number would pull the store graph into the one file that has none.
 */
export function specsFor(
  profileId: string,
  model: PanelModel,
  values: Record<string, ControlValue>,
  stack: StackRow[],
  doc: LyricDoc | null,
  count: number,
  nextSeed: () => number,
): GenerationSpec[]

/** The button's label while a batch is in flight. */
export function queueingLabel(queued: number, total: number): string
```

`specsFor`, in full, because the guard order is the whole of it:

```ts
const name = seedControl(model)
const first = specFor(profileId, model, values, stack, doc)
if (name === null || count <= 1) return [first]

const total = Math.min(Math.trunc(count), MAX_BATCH)
const specs = [first]
for (let i = 1; i < total; i++) {
  specs.push(specFor(profileId, model, { ...values, [name]: nextSeed() }, stack, doc))
}
return specs
```

`queueingLabel(queued, total)`: `total <= 1` gives the existing `QUEUEING`; otherwise
`Queueing ${queued + 1} of ${total}…` — `queued` is how many have landed, so the label names the
one being submitted now.

Notes gain a count, by **optional parameter**, so the existing tests keep passing unchanged:

```ts
export function submissionNotes(submission: Submission, queued: number = 1): string[]
export function notesFor(
  last: Submission | null,
  lastProfileId: string | null,
  profileId: string | null,
  queued: number = 1,
): string[]
```

Only the first line changes: `queued <= 1` keeps `QUEUED` verbatim (`'Queued. Watch it in the
queue below.'`), otherwise `Queued ${queued}. Watch them in the queue below.` The LoRA, format and
unchecked lines stay singular — they describe the recipe, which every job in the batch shares.

In `state/generatePanel.ts`, the store gains:

```ts
count: number        // 1, the chosen batch size
queued: number       // 0, how many of the current batch ComfyUI has accepted
setCount: (n: number) => void
```

and `submit` becomes the loop described in decision 5: reset `queued: 0` and `error: null` before
it, `set({ last, lastProfileId, queued: get().queued + 1 })` after each success, `set({ error })`
and `break` on a throw, `busy: false` in `finally`.

The `blockers` check and the `isTauri`/`busy` guard at the top stay exactly as they are.

In `GenerateBar.tsx`, left of the button, and only when `canBatch(model)`:

```tsx
<label className="generate-count">
  Variations
  <select value={count} disabled={busy} onChange={(e) => setCount(Number(e.target.value))}>
    {BATCH_CHOICES.map((n) => <option key={n} value={n}>{n}</option>)}
  </select>
</label>
```

Button label: `busy ? queueingLabel(queued, count) : GENERATE`. Notes:
`notesFor(last, lastProfileId, profileId, queued)`.

**The component derives nothing else.** No count arithmetic, no pluralising, no conditional
sentence — if a word is missing it belongs in `generate.ts`.

## CSS

`app/src/theme.css` is **1646 lines** and is the working-set risk in this run. **Append only**, one
block at the end, no existing rule touched:

- `.generate-count` — `display: flex`, `align-items: center`, `gap: var(--gap-xs)`,
  `var(--text-muted)`, 12px. It sits in the same row as the button.
- `.generate-count select` — match the existing select styling in the param panel rather than
  inventing one; tokens only, no literal colours.

## The tests

Named, because these are the rules:

`generate.test.ts`
- `test_a_count_of_one_is_exactly_the_single_spec` — `specsFor(..., 1, nextSeed)` deep-equals
  `[specFor(...)]`, **and `nextSeed` was not called**. The single-job path must not change.
- `test_each_spec_carries_its_own_seed` — four specs, four distinct `inputs.seed` values, and the
  first is the seed in `values`.
- `test_only_the_seed_differs` — every other key of `inputs`, plus `loras` and `lyrics`, is equal
  across all four.
- `test_the_specs_are_separate_objects` — mutate `specs[0].inputs`; `specs[1]` is unaffected.
- `test_a_model_with_no_seed_cannot_batch` — a `PanelModel` with the seed control removed yields
  one spec for a count of 4, and `canBatch` is false.
- `test_the_count_is_capped` — a count of 99 yields `MAX_BATCH` specs.
- `test_the_queued_line_counts` — `submissionNotes(s, 4)[0]` names 4; `submissionNotes(s)[0]` is
  still `QUEUED`.

`generatePanel.test.ts`
- `test_a_batch_submits_every_spec_and_registers_every_job` — four `generateAudio` calls, four
  distinct prompt ids registered in `useJobsStore`. *(The existing mock returns one fixed
  `Submission`; it needs to return a distinct `prompt_id` per call — a counter in the mock.)*
- `test_a_failure_midway_keeps_what_was_queued` — the mock succeeds twice then throws:
  `queued === 2`, `error` is the thrown text, **`last` is not null**, and only two jobs are
  registered.
- `test_a_batch_submits_one_at_a_time` — the mock records that no call starts before the previous
  resolves.

## Constraints

- **Four source files.** No changes to `src-tauri/`, to `state/params.ts`, `state/paramPanel.ts`,
  `state/jobs.ts` or `state/queue.ts`. If something seems to be missing there, say so and stop.
- **No new dependencies.**
- **`freshSeed` is imported from `paramPanel.ts` by `generatePanel.ts` only.** `generate.ts` stays
  free of store imports.
- **No existing `theme.css` rule may change.**
- The existing 23 `generate.test.ts` tests and every `generatePanel.test.ts` test must pass
  **unedited**, except the one mock change named above.

## Acceptance

`npm run gate` green — `cargo fmt`/`clippy`/`cargo test --workspace` untouched, then `tsc -b`,
`oxlint src`, `vitest run`, `vite build`. Frontend goes from **285** tests to roughly 295-300. One
pre-existing oxlint warning in `src/state/llm.test.ts` is expected and is not yours.

## Click-through (producer, after the run)

1. ACE-Step, **Variations: 4**, Generate. The button counts `Queueing 1 of 4…` through
   `4 of 4…`, then the note reads **Queued 4**.
2. The queue shows **four rows in the order they will run** — the running one at the top of the
   live group, three `pending` below it, oldest first.
3. Let them finish. The Library gains **four tracks**, and their four recipe cards show **four
   different seeds**. That is the check — not whether they sound different, which proves nothing
   on ACE-Step (17.3).
4. Switch to MiniMax Music 3. The Variations control is still there (it has a seed) and a batch of
   2 queues two.
5. Set Variations back to 1 and Generate. Identical to before this task: one row, one track, the
   note reads `Queued. Watch it in the queue below.`

**One thing to watch and report, for T-312b:** whether two jobs' ingests ever overlap — i.e. a
second `track://saved` arriving while the first track was still being written. `ingest_outputs`
does an unguarded read-modify-write of `project.json` (load the project, mint the id, save), one
tokio task per job and no lock between them, so two overlapping completions could mint the same
`tr-NNNN`. ComfyUI runs one job at a time and each ingest is seconds against a minutes-long
generation, so the window is narrow and **has never been observed**. The batch is the first thing
that makes back-to-back completions ordinary, so this run is the first honest evidence about it.
Do not treat it as a known bug; report what the timing actually looks like.

---

## Launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-312-brief.md --read CONVENTIONS.md --read app/src/state/params.ts --read app/src/state/paramPanel.ts --read app/src/state/jobs.ts --file app/src/state/generate.ts --file app/src/state/generate.test.ts --file app/src/state/generatePanel.ts --file app/src/state/generatePanel.test.ts --file app/src/components/GenerateBar.tsx --file app/src/theme.css
```

`params.ts` is where `specInputs` and the seed's typing live; `paramPanel.ts` is where `freshSeed`
comes from; `jobs.ts` is what `register` writes into and why it is load-bearing. `theme.css` is
1646 lines — do not change the edit format.
