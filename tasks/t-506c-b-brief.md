# T-506c-b: the param-panel store factory and the art generation store

**Depends:** T-506c-a (`generate_image` is registered; `default_image_profile_id` exists)
**Dir:** `app/src` | **Lane:** Aider — a store to write against an existing twin, a singleton to
turn into a factory across its users, and the tests for both.

**Files to create/modify (five, plus two test files):**
- `app/src/state/paramPanel.ts` — the store becomes a factory; two instances exported
- `app/src/bridge/generate.ts` — `generateImage`
- `app/src/state/artGenerate.ts` — **new**, the Cover Art submit store
- `app/src/state/profiles.ts` — `effectiveImageProfileId`, `selectedImageProfile`
- `app/src/state/paramPanel.test.ts` — the independence test
- `app/src/state/artGenerate.test.ts` — **new**
- `app/src/state/profiles.test.ts` — the two new selectors

## Goal

Pressing Generate in Cover Art (T-506d) has something to call: a panel store of its own, a submit
store that assembles specs and queues them through `generate_image`, and a selector that says which
image profile is chosen — or that none is. No UI in this lane.

## Spec

### 1. `paramPanel.ts` becomes a factory

Today `useParamPanelStore` is a module-level singleton. A CoverArt view calling `load(imageId)` on
it would **reset the Audio Studio's panel on every view switch** — discarding tags someone typed
and re-rolling a seed they had already seen, which the store's own `load` doc comment says must
never happen. Same class of defect as T-505d-d's shared import store, one store over.

Wrap the existing `create<ParamPanelState>(...)` call in

```ts
/**
 * A panel store. Called once per studio rather than shared, because the two
 * studios hold different profiles at the same time: one singleton would reset
 * whichever panel was not on screen every time the view changed -- discarding
 * typed values and re-rolling a seed the user had already seen, which is
 * exactly what `load`'s same-profile early return exists to prevent.
 */
export function createParamPanelStore() { /* the existing body, unchanged */ }

/** The Audio Studio's panel. */
export const useParamPanelStore = createParamPanelStore()

/** Cover Art's panel -- an independent instance, not a view of the same state. */
export const useArtPanelStore = createParamPanelStore()
```

**Nothing else in the file changes** — not `initialValues`, not `freshSeed`, not one line of the
store body. Every existing importer of `useParamPanelStore` keeps working untouched, which is what
makes this lane safe to land before the view exists.

### 2. `bridge/generate.ts`

```ts
/**
 * Queue one cover-art generation.
 *
 * Same contract as `generateAudio` -- including that **the caller must register
 * the returned `prompt_id` with the jobs store**, because this command starts
 * the pump itself and `applyJobEvent` drops events for ids the store does not
 * know. The backend refuses a music profile here, and says where it belongs.
 */
export async function generateImage(spec: GenerationSpec): Promise<Submission> {
  return await invoke<Submission>('generate_image', { spec })
}
```

### 3. `state/artGenerate.ts` — the submit store

Model it on `state/generatePanel.ts`, which it deliberately does **not** share: that store reads a
LoRA panel, a lyric document and a nav store that Cover Art has none of, and threading a "kind"
through it would put four unused branches in the one place a wrong branch means a wrong track.

```ts
interface ArtGenerateState {
  /** A submit is in flight. Two clicks would queue two jobs. */
  busy: boolean
  /** The command's own error, verbatim and unspliced. */
  error: string | null
  /** What was queued last, or `null` before anything was. */
  last: Submission | null
  /** The profile `last` was generated for -- see `notesFor`. */
  lastProfileId: string | null
  /** How many variations this click will queue. Not a profile input. */
  count: number
  /** How many of the current batch ComfyUI has accepted so far. */
  queued: number
  /** The artwork title. Free text; `cleanTitle` normalises empty to untitled. */
  title: string | null
  submit: () => Promise<void>
  setCount: (n: number) => void
  setTitle: (title: string) => void
}
```

`submit` is `generatePanel.ts`'s `submit` with four differences, each of which has a reason worth
keeping in a comment:

1. It reads **`useArtPanelStore`**, not `useParamPanelStore`.
2. It calls `specsFor(profileId, model, values, [], null, get().title, count, freshSeed, seedPinned)`
   — an **empty LoRA stack and no lyric document**. An adopted image profile declares no `loras`
   block and no `lyrics_contract`, so both are genuinely absent rather than omitted; passing them
   empty reuses the one spec assembler instead of forking it. `specLoras([])` is `[]` and
   `lyricRefFor(null, ..)` is `null`, so nothing about the resulting spec is invented.
3. The title is **the field alone**. The Audio Studio falls back to the selected lyric document's
   title; Cover Art has no document, so `get().title` goes straight to `specsFor`, which normalises
   it through `cleanTitle` (empty or whitespace-only becomes an untitled artwork).
4. It calls **`generateImage`**.

Everything else is carried over verbatim and must be, because each line is a rule this project paid
for:

- **`useJobsStore.getState().register(submission.prompt_id, profileId)` inside the loop.** Without
  it the job runs to completion on the GPU and every progress, done and failed event is discarded —
  an empty queue and no error anywhere (T-309d, ARCHITECTURE §7).
- **Sequential `await`, never `Promise.all`.** One stdio transport, and `register` stamps
  `submittedAt`, which `queueRows` sorts by; parallel submits list the queue in an order ComfyUI
  will not run it in.
- **Write the first spec's seed back with `setSeed`** so the field shows the seed that actually
  ran (`setSeed`, not `setValue` — it must not pin).
- **`blockers(...)` is re-checked here** even though the button will be disabled on it: the button
  is a view, and this is the layer that decides what reaches Rust.
- **The error is stored verbatim**, and `last` is *not* cleared on failure — a partial batch really
  is running.

### 4. `state/profiles.ts` — two selectors

```ts
/**
 * The image profile Cover Art is working against, or `null` when none is chosen.
 *
 * **No default, deliberately.** `effectiveProfileId` can fall back to
 * `DEFAULT_PROFILE_ID` because `ace-step-1.5-turbo` ships; the app ships no image
 * profile at all, so there is nothing to fall back to and inventing one would
 * generate with a model the user never picked. `null` is the view's cue to say
 * so and point at the Setup catalog.
 */
export function effectiveImageProfileId(config: Config | null): string | null

/**
 * The chosen image profile, when it is still one of the loaded ones.
 *
 * `null` while the list has not loaded, when nothing is chosen, and when the
 * configured id names a profile that is no longer there -- a deleted or renamed
 * user profile. The caller says which, rather than substituting another model.
 */
export function selectedImageProfile(
  view: ModelsView | null,
  config: Config | null,
): ProfileStatus | null
```

An empty or whitespace-only stored id is treated as unchosen, matching `effectiveProfileId`'s own
`trim()` check.

## Tests — named by the invariant

`paramPanel.test.ts`:

- **the two panels do not share state** — load a profile into each, `setValue` on one, and assert
  the other's values are untouched and its `seedPinned` is unchanged. *The whole point of the lane:
  before the factory, this is the same object.*
- **`beforeEach` must reset both stores.** The existing hook resets only `useParamPanelStore`;
  vitest does not clear module state between tests, so a value left in `useArtPanelStore` leaks
  into the next test. (T-505d-d's review found this exact class of thing with accumulating mock
  call counts, and a guard that reads earlier tests' state is a guard that proves nothing.)

`artGenerate.test.ts` — **copy the harness from `app/src/state/generatePanel.test.ts`**, which
already mocks `../bridge/comfy`, `../bridge/profiles` and `../bridge/generate` and counts
concurrent calls to prove submission is sequential. Two adjustments: the `../bridge/generate` mock
exports **`generateImage`** (its own module registry, so `generatePanel.test.ts`'s
`generateAudio`-only mock is unaffected), and the profile is the image one --
`import kleinProfile from '../../../testdata/profiles/flux2-klein-9b-image.json'`, which works the
same way `generatePanel.test.ts` imports `../../../profiles/ace-step-1.5-turbo.json` from outside
`app/`. That fixture declares `tags`, `negative`, `seed`, `steps` and `cfg`, and **no lyrics and no
LoRA block**, which is what makes it the right input here rather than the ACE profile:

- **submit queues one job per variation, and registers every prompt id** — count 4 gives four
  `generateImage` calls and four `register` calls. *The register line is the one whose absence is
  invisible: the GPU runs and the queue stays empty.*
- **the specs carry no LoRAs and no lyric ref** — assert on what was passed to `generateImage`,
  because an image spec that claimed a lyric version would be a sidecar that lies.
- **each variation gets a different seed** — the values differ only in the seed control.
- **an unpinned seed is re-rolled on submit, a pinned one is kept** — the T-316 rule, which is
  worth its own assertion here because Cover Art re-runs the same prompt far more often than the
  Audio Studio does.
- **the title reaches the spec, and an empty title becomes `null`** — no lyric-document fallback
  exists on this side, so the field is the only source.
- **a failing `generateImage` leaves the error verbatim and `busy` false** — and does not clear
  `last`.
- **a second click while busy queues nothing.**

`profiles.test.ts`:

- **no image profile chosen returns `null`, not a default** — including for `''` and `'   '`.
  *Without this a copy of `effectiveProfileId` that kept its fallback would hand Cover Art
  `ace-step-1.5-turbo`, and the backend would refuse it with a message about the wrong studio.*
- **a chosen id that no loaded profile answers to returns `null` from `selectedImageProfile`**,
  rather than the first image profile in the list.

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the listed files
- [ ] `paramPanel.ts`'s store body is **byte-identical** to what it is now — only the wrapper and
      the two exported instances are new
- [ ] no component imports change; `useParamPanelStore` still resolves for every existing user

## Out of scope
- **Any view or component.** No `CoverArt.tsx`, no `ParamPanel` prop, no `theme.css` — T-506d.
- **The artwork listing store** (`bridge/art.ts`, `state/art.ts`, the `art://saved` subscription) —
  T-506c-c, briefed after this lands.
- **Writing `default_image_profile_id`.** The field exists (T-506c-a) and these selectors read it;
  the picker that writes it is part of the view.
- **`notesFor` changes.** `lastProfileId` is carried so the view can call the existing helper
  unchanged.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/generatePanel.ts --read app/src/state/generate.ts --read app/src/state/params.ts --read app/src/state/jobs.ts --read app/src/bridge/config.ts --read app/src/bridge/models.ts --file app/src/state/paramPanel.ts --file app/src/state/paramPanel.test.ts --file app/src/bridge/generate.ts --file app/src/state/artGenerate.ts --file app/src/state/artGenerate.test.ts --file app/src/state/profiles.ts --file app/src/state/profiles.test.ts
```
