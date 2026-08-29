# T-311e — `<Library>`: the tracks on screen, with the recipe that made them

**Lane: Aider.** JSX and CSS carrying no decisions. Every word a row shows is already computed by
`app/src/state/library.ts` (T-311c, landed) and arrives on a `TrackRow`. **The component must not
derive anything** — no formatting, no fallbacks, no wording, no sorting.

**Depends:** T-311c (landed). T-311c is on `master`, so this is runnable now.

**Files to modify — two only:**

- `app/src/views/Library.tsx`
- `app/src/theme.css`

---

## What exists

`Library.tsx` is 13 lines and hard-codes a sentence that is now wrong -- it says "Nothing saved
yet" whether or not anything is saved:

```tsx
<div className="panel muted">Nothing saved yet. Generated tracks land here.</div>
```

The producer has **one real track on disk** (`tr-0001`: two LoRAs, 2:00, `promptId: null` because
it predates T-311d). It is the first thing this view will ever render.

**`app/src/theme.css` is 1545 lines** and is the working-set risk in this run; `--edit-format diff`
is what keeps it viable. **Append only.** No existing rule may change.

## The data

```ts
import { EMPTY_LIBRARY, useLibraryStore, type TrackRow } from '../state/library'

interface TrackRow {
  id: string
  name: string        // the title, else the id -- never empty
  model: string       // display name, else the id, else 'Unknown model'
  license: string
  duration: string    // '2:00', or '--'
  created: string     // '2026-08-29'
  loras: string       // 'stem x1.0, stem x1.0', or '' for none
  seed: string        // the seed, or '--'
  promptId: string | null   // null for a track written before T-311d
  file: string
}
```

Store: `tracks: TrackRow[]`, `warnings: string | null`, `loading: boolean`, `error: string | null`,
`load()`, `startListening()`. **Subscribe with a selector per field**, never the bare store
(CONVENTIONS; the queue panel does this correctly and is the model).

## Mount

Two effects, matching `AudioStudio.tsx` exactly:

```tsx
useEffect(() => {
  void load()
}, [load])

useEffect(() => {
  void startListening()
}, [startListening])
```

`startListening` already guards double subscription and no-ops outside Tauri, so this needs no
condition of its own.

## The component

```
<>
  <h1 className="view-title">Library</h1>
  <p className="view-subtitle">Everything you have made, with the recipe that made it.</p>

  error !== null    -> <p className="library-error">{error}<button …>Retry</button></p>
  warnings !== null -> <p className="library-warning">{warnings}</p>

  tracks.length === 0
    ? <p className="library-empty">{EMPTY_LIBRARY}</p>
    : <ul className="track-list"> one <TrackCard> per row </ul>
</>
```

Keep the existing `view-title` and `view-subtitle` lines exactly as they are. **Delete the
`panel muted` div** -- its sentence is the bug this task fixes.

Each row:

```
<li className="panel track-row" key={row.id}>
  <div className="track-head">
    <span className="track-name">{row.name}</span>
    <span className="track-duration">{row.duration}</span>
  </div>

  <dl className="track-recipe">
    <div className="track-fact"><dt>Model</dt><dd>{row.model}</dd></div>
    <div className="track-fact"><dt>Licence</dt><dd>{row.license}</dd></div>
    <div className="track-fact"><dt>Created</dt><dd>{row.created}</dd></div>
    <div className="track-fact"><dt>Seed</dt><dd>{row.seed}</dd></div>
    {row.loras !== '' ? <div className="track-fact"><dt>LoRAs</dt><dd>{row.loras}</dd></div> : null}
    {row.promptId !== null ? <div className="track-fact"><dt>Run</dt><dd>{row.promptId}</dd></div> : null}
  </dl>

  <p className="track-file">{row.file}</p>
</li>
```

**A field with nothing in it is omitted, never given a word.** `row.loras` of `''` means no LoRAs
and `row.promptId` of `null` means the track predates the field -- and "None" or "Not recorded"
would be *wording*, which is exactly what `state/library.ts` exists to own and test. Showing or
hiding a row is a rendering choice; naming an absence is not. If a placeholder seems necessary, it
belongs in `state/library.ts` and is not this task's to add -- say so and stop.

`key={row.id}`. Never the array index; the list reorders as tracks are added.

`type="button"` on Retry.

The six labels (`Model`, `Licence`, `Created`, `Seed`, `LoRAs`, `Run`) are **static JSX text, not
data** -- they are the same for every row and never vary, so they are markup rather than a decision.
Use `Licence` (this repo's spelling, as in `profile-row-license-name`'s copy).

`loading` deliberately renders nothing extra. The first load resolves from local disk in
milliseconds, and a spinner that flashes once is worse than no spinner. Do not add one.

## CSS — append only

New classes: `.track-list`, `.track-row`, `.track-head`, `.track-name`, `.track-duration`,
`.track-recipe`, `.track-fact`, `.track-file`, `.library-empty`, `.library-warning`,
`.library-error`, `.library-retry`.

- `.track-list` — `list-style: none`, no padding, `display: flex; flex-direction: column;`
  `gap: var(--gap-md)`.
- `.track-row` — already carries `panel`, so add only what `panel` lacks: `display: flex`,
  `flex-direction: column`, `gap: var(--gap-sm)`.
- `.track-head` — `display: flex`, `justify-content: space-between`, `align-items: baseline`.
- `.track-name` — `var(--text)`, 15px, `font-weight: 600`. It is the row's identity.
- `.track-duration` — `var(--text-muted)`, 13px, `font-variant-numeric: tabular-nums`.
- `.track-recipe` — `display: grid`, `grid-template-columns: repeat(auto-fit, minmax(180px, 1fr))`,
  `gap: var(--gap-xs) var(--gap-md)`, `margin: 0`. The facts wrap on a narrow window instead of
  overflowing.
- `.track-fact dt` — `var(--text-muted)`, 11px, uppercase, `letter-spacing: 0.4px`.
- `.track-fact dd` — `var(--text)`, 13px, `margin: 0`, and **`overflow-wrap: anywhere`**: a LoRA
  summary and a prompt id are both long unbroken strings, and without it they push the grid wide.
- `.track-file` — `var(--text-muted)`, 12px, `margin: 0`, monospace is fine if a token exists;
  otherwise leave the family alone.
- `.library-empty` — `var(--text-muted)`, 13px, matching `.lora-stack-empty`.
- `.library-warning` — `var(--warning)`, 13px.
- `.library-error` — `var(--danger)`, 13px.
- `.library-retry` — copy `.lora-stack-retry`'s shape (theme.css 1255) rather than inventing one.

**Tokens only.** No literal colours, no literal spacing. If a needed token does not exist, say so
rather than inventing a hex.

## Constraints

- **Two files only**: `app/src/views/Library.tsx`, `app/src/theme.css`.
- **No new dependencies.**
- **No changes to `app/src/state/` or `app/src/bridge/`.** If a value or a word seems to be
  missing, it belongs in `state/library.ts` and is not this task's to add -- say so and stop.
- **No existing `theme.css` rule may change.** Append.
- **The test count must stay at 285.** This task adds no logic, so it adds no tests. A changed
  count means something was derived in the component that should not have been.

## Acceptance

`npm run gate` green -- `tsc -b`, `oxlint src`, 285 tests, `vite build`. One pre-existing oxlint
warning in `src/state/llm.test.ts` is expected and is not yours.

## Click-through (producer, after the run)

1. Open Library with the existing track. It shows **one card** reading `tr-0001`, `2:00`,
   ACE-Step 1.5 XL Turbo, Apache-2.0, `2026-08-29`, its seed, and both LoRA stems with strengths.
2. **No "Run" line on that card** -- it predates `prompt_id`, and its absence must be silent rather
   than an error or an empty label.
3. Generate a new track. It appears **without a reload**, and this one **does** have a Run line.
4. Narrow the window. The recipe grid wraps; nothing scrolls sideways.
5. A long LoRA name wraps inside its cell rather than widening the card.

Step 3 is the one that matters: it proves `track://saved` reaches the store, which nothing has ever
exercised.

---

## Launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-311e-brief.md --read CONVENTIONS.md --read app/src/state/library.ts --read app/src/components/LoraStack.tsx --read app/src/components/JobQueue.tsx --read app/src/views/AudioStudio.tsx --file app/src/views/Library.tsx --file app/src/theme.css
```

`LoraStack.tsx` is the house style for a list component with rows; `JobQueue.tsx` is the closest
precedent for a component that renders a pure module's rows and decides nothing; `AudioStudio.tsx`
is where the mount effects are copied from. `theme.css` is 1545 lines and is the working-set risk,
so do not change the edit format.
