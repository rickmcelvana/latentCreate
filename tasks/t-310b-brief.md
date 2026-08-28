# T-310b — `<JobQueue>`: the panel

**Lane: Aider.** JSX and CSS carrying no decisions. Every value a row shows is already computed by
`app/src/state/queue.ts` (T-310a, landed) and arrives on a `QueueRow`. **The component must not
derive anything** — no status mapping, no sorting, no time arithmetic, no conditional wording.

T-310a is landed and on `master`, so this is runnable now.

---

## What exists

`app/src/components/JobQueue.tsx` is 32 lines and renders the raw status string:

```tsx
<li className={`job-item job-item-${job.status}`}>
  <span className="job-status">{job.status}</span>
  {job.error !== null ? <span className="job-error">{job.error}</span> : null}
  {running ? <button className="job-cancel" …>Cancel</button> : null}
</li>
```

It computes `running` inline from a three-way `!==` chain, shows `completed` and `error` as bare
tokens, has no ordering, and cannot say which model a row is. It also returns `null` for an empty
queue, so the Audio view gives no sign that a queue exists until the first generation.

**`app/src/theme.css` is 1514 lines** and already carries a `.job-*` block at **lines 205–264**:
`.job-queue`, `.job-item`, `.job-status`, `.job-item-completed .job-status`,
`.job-item-failed .job-status`, `.job-item-cancelled .job-status`, `.job-error`, `.job-cancel`,
`.job-cancel:hover`. **Do not rewrite that block.** Add to it and add after it.

⚠ **One existing rule is dead and this task fixes it.** `.job-item-failed .job-status` colours a
failure red — but a real failure's status is **`error`**, not `failed`, so the class is
`job-item-error` and that rule has never once applied. Nobody noticed because no generation had
ever failed until 2026-08-28 (MCP-SURFACE 24). The new CSS must colour **both**.

## The data

```ts
import { EMPTY_QUEUE, queueRows, type QueueRow } from '../state/queue'

interface QueueRow {
  id: string
  label: string        // 'Queued' | 'Running' | 'Done' | 'Cancelled' | 'Failed'
  model: string        // display name, or the id, or 'Unknown model' -- never empty
  elapsed: string      // '12s', '1m 12s'
  error: string | null // already null for a cancelled job
  canCancel: boolean
  status: string       // raw, for the CSS class only
  profileId: string
}

queueRows(jobs, now, names): QueueRow[]
```

`jobs` and `cancel` come from `useJobsStore` as they do today. **`names`** is
`{ [profileId]: display_name }`, built in `AudioStudio.tsx` from the models list it already has:

```tsx
const rows = pickable(view, 'music')   // already in AudioStudio
const names = Object.fromEntries(rows.map((p) => [p.id, p.display_name]))
```

Pass `names` into `<JobQueue names={names} />`. `queueRows`'s third argument defaults to `{}`, so
a missing map degrades to showing the profile id rather than blanking the column.

## `now`, and the one piece of state the component owns

`elapsed` needs a clock. The store has no ticker and should not grow one — a store that re-renders
every consumer once a second to move a text label is the wrong trade.

```tsx
const [now, setNow] = useState(() => Date.now())
useEffect(() => {
  const id = setInterval(() => setNow(Date.now()), 1000)
  return () => clearInterval(id)
}, [])
```

**Unconditional, and cleared on unmount.** Do not try to stop the interval when nothing is running:
that is a derived condition, it belongs in the pure module if anywhere, and a stray one-second timer
is cheaper than the bug where the clock stops for a job that started after it.

## The component

```
<section className="panel job-panel">
  <h2 className="panel-title">Queue</h2>

  rows.length === 0
    ? <p className="job-empty">{EMPTY_QUEUE}</p>
    : <ul className="job-queue"> one <JobRow> per row </ul>
</section>
```

Each row:

```
<li className={`job-item job-item-${row.status}`}>
  <span className="job-status">{row.label}</span>
  <span className="job-model">{row.model}</span>
  <span className="job-elapsed">{row.elapsed}</span>
  {row.error !== null ? <span className="job-error">{row.error}</span> : null}
  {row.canCancel ? <button type="button" className="job-cancel" onClick={() => void cancel(row.id)}>Cancel</button> : null}
</li>
```

`type="button"` on the Cancel button. Three buttons shipped without it in T-309b; harmless with no
`<form>` in the tree and a submit the moment one appears.

`key={row.id}`. Never the array index — rows reorder as jobs finish, which is the exact case an
index key gets wrong.

## CSS — append only

New classes: `.job-panel`, `.job-empty`, `.job-model`, `.job-elapsed`, plus
`.job-item-error .job-status` (the dead-rule fix above).

- `.job-model` — `var(--text)`, 13px. It is the row's identity, so it should not be the faintest
  thing in it.
- `.job-elapsed` — `var(--text-muted)`, 12px, tabular numerals if a token exists for it; the label
  changes every second and digits of different widths make the row twitch.
- `.job-empty` — `var(--text-muted)`, 13px, matching `.generate-note` (theme.css, end of file).
- `.job-item-error .job-status` — `var(--danger)`, alongside the existing `.job-item-failed` rule.
  Write them as one selector list so they cannot drift apart again.

**Tokens only.** No literal colours, no literal spacing. Every value comes from a `var(--…)` that
already exists in `theme.css`; if a needed token does not exist, say so rather than inventing a hex.

`.job-cancel` already sets `margin-left: auto` to push itself right. With three new spans in the
row, check that still reads correctly — the model name should sit next to the status, and the
elapsed time should not be flush against the Cancel button.

## Constraints

- **Three files only**: `app/src/components/JobQueue.tsx`, `app/src/views/AudioStudio.tsx`,
  `app/src/theme.css`.
- **No new dependencies.**
- **No changes to `app/src/state/`.** If a value seems to be missing, it belongs in `queue.ts` and
  is not this task's to add — say so and stop.
- **No existing `theme.css` rule may change.** Append, and add to the `.job-*` block only where
  this brief names it.
- **The test count must stay at 268.** This task adds no logic, so it adds no tests. A changed
  count means something was derived in the component that should not have been.

## Acceptance

`npm run gate` green — `tsc -b`, `oxlint src`, 268 tests, `vite build`. One pre-existing oxlint
warning in `src/state/llm.test.ts` is expected and is not yours.

## Click-through (producer, after the run)

1. Empty Audio view before any generation shows the Queue heading and "Generations you start will
   appear here."
2. Generate once — the row appears with **the model's display name**, not `ace-step-1.5-turbo`.
3. The elapsed time counts up once a second.
4. Queue a second job while the first runs. **The running one stays on top**, and the finished one
   drops below it.
5. Cancel a running job — the row reads **Cancelled**, shows **no error text**, and loses its
   Cancel button. It must not read "Failed".
6. A finished job shows Done in green and no Cancel button.

Step 5 is the one that matters most. It is the third time this project has had to prove that a
cancel does not present itself as a failure.

---

## Launch

```
aider --model ollama_chat/kimi-k2.7-code:cloud --no-auto-commits --read tasks/t-310b-brief.md --read CONVENTIONS.md --read app/src/state/queue.ts --read app/src/state/jobs.ts --read app/src/state/profiles.ts --read app/src/components/LoraStack.tsx --file app/src/components/JobQueue.tsx --file app/src/views/AudioStudio.tsx --file app/src/theme.css
```

`LoraStack.tsx` is `--read` as the house style for a list component with rows and per-row buttons.
`theme.css` is 1514 lines and is the working-set risk in this run; `--edit-format diff` is what
keeps it viable, so do not change it.
