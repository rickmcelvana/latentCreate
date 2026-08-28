# T-310 — the queue panel

**Split by lane, the way T-308 and T-309 were.**

- **T-310a — architect-direct**: `state/queue.ts` (pure), the store's `register` signature, and
  the job identity that makes a row mean anything. Invariants and mutations.
- **T-310b — Aider**: `<JobQueue>` and its CSS. ~200 lines of JSX carrying no decisions.

**T-310a lands first.** The T-309d brief shipped a launch command while its architect-direct half
was unwritten, the executor stopped and asked for four files that did not exist, and it was right
to. The launch command belongs with the part that is actually runnable.

---

## The live read changed this task before it started

The phase file said: *"Read `job(action="error")` — the normalized view added since Phase 1 (16.6)
— rather than parsing `job(action="status")`."*

Measured against the live install with three real cancelled jobs (MCP-SURFACE §23), **that
instruction would have introduced the bug §21 was written to fix.**

| cancelled job | `action="queue"` | `action="status"` | `action="error"` |
|---|---|---|---|
| `488ce569` | `cancelled` | `cancelled` | **`error`** |
| `0e5b3b0c` | **`error`** | `cancelled` | `cancelled`, `error: null` |
| `63492516` | **`error`** | `cancelled` | `cancelled`, `error: null` |

`queue` and `error` swap on the same job in opposite directions. **`action="status"` is the only
surface that answers `cancelled` for all three**, stable across repeat calls — and it is what the
app already reads. `mcp_bridge::JobStatus::is_cancelled` is correct as written.

**So T-310 changes nothing about which surface the pump polls.** Write that down in the code, with
the section reference, or the phase file's instruction gets followed by whoever reads it next.

`action="error"` also returns **two shapes with mutually exclusive keys** — `{status, error: null}`
and `{status, error_code, exception_message, …}` — and a *cancelled* job returns `error: null`
despite the tool documenting that key as meaning healthy. Third time in this project that an absent
key was being read as a value (after `stale` in T-308c and `local_check` in T-306b). If anything
ever does consume this surface, discriminate on the shape, never on a key's nullness.

**The failure case has since been measured** (§24), by pointing an ACE-Step graph at MiniMax's VAE
-- a legitimate enum member and the wrong file, so it validates, runs, and throws at decode. That
changes the conclusion above in one direction: **T-310 needs both surfaces, for different
questions.**

- **Outcome** comes from `action="status"`. Only field that has never disagreed with itself.
- **Failure detail** comes from `action="error"`, the only surface with `node_id`, `node_type`,
  `exception_type` and `exception_message` as named fields.
- **`traceback_tail` is never rendered** -- twelve frames of absolute paths into the user's install.

`error_code` means different things on the two surfaces: on `action="queue"` it is the category
(`execution_error` / `cancelled` / `server_died`); on `action="error"` it is comfy-cli's
transport-level code, **null for an ordinary node failure**. Same key, two meanings.

## What is actually there today

`<JobQueue>` is **32 lines**, written incidentally across T-309d and the cancel fix:

```
<li>  {job.status}  |  {job.error}  |  [Cancel]  </li>
```

`Job` is `{ id, status, outputs, error }`. A row therefore cannot say **which** job it is. With two
jobs queued — which the producer has now done — both rows read `running`, and during the cancel
investigation a frozen row and a live row were indistinguishable *on screen*. That is not a styling
gap; it is the panel having no identity to render.

## T-310a — scope

### 1. A job knows what it was

`register(id)` takes only an id. `generatePanel.submit` already holds `profileId` and the
`Submission`, and throws all of it away.

```ts
export interface Job {
  id: string
  status: string
  outputs: string[]
  error: string | null
  /** What was generated, for the row's label. */
  profileId: string
  /** `Date.now()` at register. Local, deliberately -- see below. */
  submittedAt: number
}
```

`submittedAt` is timestamped **locally at register**, not read from the server. `submitted_at`
exists only on the `state_file` record and is absent from the other store (§23.3), so a
server-sourced timestamp would be present for some jobs and missing for others with nothing on
screen explaining why.

### 2. `state/queue.ts` — every decision the panel needs

Pure, because vitest cannot reach JSX and this is the module where a wrong answer is a wrong
screen:

- `queueRows(jobs, now)` — the ordered rows. **Running first, then newest-submitted first.** A
  panel sorted only by time buries the job the user is waiting on under the ones that finished.
- `statusLabel(job)` — the sentence. Today the raw string is rendered, so the user reads
  `completed` and `failed` as-is. This is the T-309b rule: **conditional wording belongs in the pure
  module.** `cancelled` must not render as a failure.
- `elapsed(job, now)` — `"1m 12s"`. Plain interpolation of a number stays in the view; choosing
  between `"12s"` and `"1m 12s"` is a conditional, so it lives here.
- `isDone(job)` / `canCancel(job)` — the Cancel button's condition, currently a three-way `!==`
  chain inline in JSX.
- `errorFor(job)` — `null` for a cancelled job. A cancel carries
  `"Job was interrupted/cancelled."` on one of the two shapes, and showing it would be the §21
  defect returning through the front door. For a real failure it composes
  `` `${node_type} failed: ${exception_message}` `` — measured fields, not guessed ones (§24.2) —
  and **never** `traceback_tail`.

  ⚠ The two `error` shapes **share no key**: `code` is absent on the failure shape,
  `exception_message` is absent on the cancel shape (§24.3). Classifying by reading `error.code`
  returns nothing for every real failure. Discriminate on `status`, then read the detail.

### 3. An empty state

`if (entries.length === 0) return null` — the panel vanishes entirely. After a first generation
that is fine; before one, the Audio view gives no indication that a queue exists. One sentence.

### 4. No progress bar, and the reason goes in a comment

`JobProgress` is `{id, status, outputs}`; `action="status"` carries no progress field either.
`action="watch"` is the only surface offering `{progress, total, nodes_done}`, the pump does not use
it, and comfy-cli is documented as sending no per-step events (§23.5). **A percentage cannot be
built from what the app reads**, so the row shows elapsed time. Say so in the code, or someone adds
a `<progress>` element wired to a field that does not exist.

## Invariants, each with a test

1. A running job sorts above a completed one submitted later.
2. Among jobs in the same state, newest-submitted first.
3. `cancelled` renders as its own label, and `errorFor` returns `null` for it.
4. `failed` renders its error text.
5. `canCancel` is true only while the job is live — and the set of live statuses is derived from
   the terminal ones, not listed twice.
6. `elapsed` crosses the minute boundary correctly at 59s → 60s.
7. An unknown status string does not crash the row and does not render as blank.

Invariant 7 matters more than it looks: `Job.status` is a `string` carrying whatever ComfyUI said,
and the comment in `jobs.ts` listing `'queued' | 'running' | 'completed' | 'failed'` is already
missing `cancelled`.

## Mutations

| # | mutation | must be killed by |
|---|---|---|
| M62 | sort by time only, dropping the running-first rule | invariant 1 |
| M63 | sort ascending | invariant 2 |
| M64 | `cancelled` falls through to the failure label | invariant 3 |
| M65 | `errorFor` returns the message for a cancel too | invariant 3 |
| M66 | `canCancel` hardcodes a status list instead of deriving it | invariant 5 — add a status |
| M67 | `elapsed` uses `>` instead of `>=` at 60s | invariant 6 |
| M68 | `register` drops `profileId` | a row with no identity |
| M69 (control) | rename a private helper | nothing |

M68 is the one to watch, and it is the same shape as M49 and M59: the rule is that data **survives**
a hop, and a test asserting the store's shape will pass while the value arrives `undefined`. Drive a
real submission through `submit` and read the row.

## Gate

`npm run gate`, `cargo test`, clippy, fmt. Frontend 245 → expect ~262. **No Rust change** — §23
vindicated the current polling surface, and `is_cancelled` stays as it is. (mcp-bridge went
94 → 96 while measuring §24: its `error`-shape docs were corrected, and the failure test's
hand-written fixture was replaced with the payload the server actually sends.)

## Click-through (after T-310b)

Queue two jobs and watch the second wait. Cancel the first and confirm the row settles as cancelled
**and does not read as failed**, with no error text. Confirm rows say which model they were.

## What this brief does not cover, deliberately

**`server_died` is still unobserved.** Producing it means actually killing ComfyUI, which is
T-314's kill-mid-job check rather than something to do while briefing a panel. So the row's
crash-versus-node-failure wording is the one provisional thing here, and should be labelled as
such in the code -- the same way `is_terminal`'s vocabulary was, and that comment's honesty is
what made the cancel bug findable.

**The `get_logs` fallback may not exist.** The phase file assumed `get_logs` "still reads across
the crash". On this install it returned a file a day stale, reporting v0.34.1 against a running
v0.34.2 -- while comfy-cli's own trust signals (`source: "explicit_port"`, `port_mismatch: false`)
both said it was fine. A server restarted by hand rather than through comfy-cli has no log for
`get_logs` to read, and nothing in the response says so (§24.5).

Also out of scope: `action="watch"` (§23.5), and which of comfy-cli's two stores a cancelled job
lands in (§23.3, interrupted-while-running versus deleted-while-queued is the likely discriminator
and would take two deliberate cancels to confirm).
