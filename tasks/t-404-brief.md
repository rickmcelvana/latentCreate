# T-404: Send-to -- open the sibling app and reveal the file

**Depends:** T-311 (the Library and its track rows), T-402a (`resolve_track_file`, the id ->
absolute path discipline this reuses verbatim)
**Crate/dir:** `src-tauri` + `app`
**Milestone:** the third and last Phase 4 milestone line -- *"send-to opens site with file
revealed"*.

**Lane split (WORKFLOW 1), decided before writing:**

- **T-404a -- the backend. Architect-direct, already in the working tree.** One module, one
  command, four tests; written, compiled, clippy-clean, fmt-clean and passing before this brief
  was finished. Sending it to an executor is a round trip that cannot change the outcome.
- **T-404b -- the frontend. The Aider run.** Bridge wrapper, store, the Library affordance and
  the CSS: five files of wiring, which is exactly what the executor exists for.

---

## Goal

A finished track can be handed to latentMixing or latentMastering in one click: the sibling app
opens in the browser and the OS file manager opens with the track's audio file selected, ready to
drag in. A track whose audio file is missing says so and opens nothing.

**This is a link-out, not a file handoff.** Re-verified 2026-09-01: neither sibling repo has an
import surface to implement against. The real protocol is owned by those repos (ARCHITECTURE 8,
phase-4 decision 3); when it lands it opens as its own task here rather than as a change to this
one.

---

## What was verified before writing this, and what it changed

Each of these is a claim the brief would otherwise have rested on from memory.

1. **`tauri-plugin-opener` 2.5.4's Rust API**, read from the vendored source
   (`~/.cargo/registry/.../tauri-plugin-opener-2.5.4/src/lib.rs`): `OpenerExt::opener()` yields
   `Opener<R>`, with `open_url(url, with: Option<impl Into<String>>)` and
   `reveal_item_in_dir<P: AsRef<Path>>(p)`, both returning the crate's `Result<()>`. The `with`
   argument needs a turbofish at the call site -- `None::<&str>` -- because nothing else fixes the
   generic.
2. **No capability change is needed.** `src-tauri/capabilities/default.json` already grants
   `opener:default`, and that set is `allow-open-url` + `allow-reveal-item-in-dir` +
   `allow-default-urls` (the plugin's `permissions/default.toml`). It is moot anyway: the
   capability system gates the plugin's **JS** commands, and this task calls the **Rust** API,
   which never consults the scope. Recorded so nobody spends an afternoon on a permission that was
   never the problem.
3. **`reveal_item_in_dir` canonicalizes the path first**, so a deleted file is an `io` error rather
   than a silent no-op. That is why the command checks `is_file()` itself and gives the missing
   file its own sentence instead of surfacing a canonicalize error.
4. **The plugin's own commands are `async`.** `send_to` follows, so a shell-out and a COM call do
   not run on the webview's thread.
5. **The two URLs, which are the whole feature.** `../latent-mixing`'s docs mention
   `latentmixing.com` 59 times and `latentmixer.com` 17, and **the raw count points the wrong
   way.** That repo's decisions log (2026-08-08) records the app as deployed at
   **`app.latentmixer.com`**, taking alpha traffic, with every `latentmixing.com` reference stale
   and a doc sweep owed; `../website/latentbeats.com/index.html` -- the branding source of truth
   (ARCHITECTURE 9) -- links `https://app.latentmixer.com` and `https://app.latentmastering.com`.
   **latentCreate's ARCHITECTURE 8 is right.** This is the "prefer the most recently dated number
   over the oldest" rule (AGENTS) doing real work: a majority vote here ships a dead link.

---

## T-404a -- backend (DONE, architect-direct)

**Files:** `src-tauri/src/sendto.rs` (new), `src-tauri/src/lib.rs` (`mod sendto;` + one handler
entry).

`SendTarget` is a two-variant enum deserialized from `"mixing"` / `"mastering"`. `target_url` is
the single place the app names the siblings' addresses. `send_to` resolves the id through the same
three calls as `track_audio_path` (`selected_project` -> `load_track` -> `resolve_track_file`),
**refuses a file that is not on disk**, reveals, then opens the URL.

**The ordering is the design.** Reveal comes first and a failure returns early, so a missing file
never leaves the user with a browser tab and nothing to drag into it. A failure in the other
direction is benign: an already-open file manager and a message naming the site.

**Error copy follows T-315's rule** -- one sentence, ending in something to do. The missing-file
case gets its own constant rather than being folded into a generic reveal failure, because
claiming a cause the code has not established is exactly what T-315's review took back out of
`prompt_not_found`.

Tests (4, all passing): the two URLs are asserted literally (nothing else in the app mentions those
hosts, so no other test can catch a typo); the wire words round-trip through serde; a sidecar whose
audio is gone yields `MISSING_FILE`; an unknown id is refused before any path is joined.
`track_path` is split out of the command so all of that is reachable without an `AppHandle`.

**src-tauri 107 -> 111 tests.** `cargo clippy --all-targets -- -D warnings` clean,
`cargo fmt --all --check` clean.

---

## T-404b -- frontend (the Aider run)

**Files to create:**
- `app/src/bridge/sendto.ts`
- `app/src/state/sendto.ts`
- `app/src/state/sendto.test.ts`

**Files to modify:**
- `app/src/views/Library.tsx` (the `TrackCard` component only)
- `app/src/theme.css` (the existing track block from T-311/T-402)

### Spec

- The affordance lives in `.track-head-actions`, beside the existing Play button: a muted
  `Send to` label followed by a `Mixing` button and a `Mastering` button.
- While a send is in flight, **that row's** two buttons are disabled. No other row is affected.
- A failure renders as one line under the row that produced it, in `--danger`, carrying the
  backend's sentence verbatim.
- Starting a new send clears the previous failure.
- **`SEND_TARGET_NAMES` lives in `state/sendto.ts`, not in the bridge.** The test mocks
  `../bridge/sendto` wholesale (the `state/albums.test.ts` pattern), and a display constant sitting
  inside a mocked module is a constant the test cannot see.

### Reference implementation

`app/src/bridge/sendto.ts`:

```ts
import { invoke } from '@tauri-apps/api/core'

/** Mirrors Rust `sendto::SendTarget`. These are the wire words, not labels. */
export type SendTarget = 'mixing' | 'mastering'

/**
 * Open the sibling app and reveal this track's audio file for drag-in.
 *
 * Rejects with the backend's own sentence -- the copy lives in Rust
 * (`src-tauri/src/sendto.rs`) so every surface says the same thing.
 */
export async function sendTo(id: string, target: SendTarget): Promise<void> {
  await invoke('send_to', { id, target })
}
```

`app/src/state/sendto.ts`:

```ts
import { create } from 'zustand'
import { sendTo, type SendTarget } from '../bridge/sendto'

/** The order the two destinations are offered in. */
export const SEND_TARGETS: readonly SendTarget[] = ['mixing', 'mastering']

/** What each destination is called on screen. */
export const SEND_TARGET_NAMES: Record<SendTarget, string> = {
  mixing: 'Mixing',
  mastering: 'Mastering',
}

/** The last send failure, remembered with the track it belongs to. */
export interface SendFailure {
  trackId: string
  message: string
}

/**
 * The message to show under one track's row, or `null`.
 *
 * A failure belongs to the row that produced it. Showing the last error under
 * every row is the absent-versus-empty confusion this repo has paid for four
 * times, landing in the one place a user is about to click something that
 * touches their files.
 */
export function failureFor(failure: SendFailure | null, trackId: string): string | null {
  if (failure === null) return null
  return failure.trackId === trackId ? failure.message : null
}

/** True only for the row whose send is in flight. */
export function isSending(sending: string | null, trackId: string): boolean {
  return sending === trackId
}

interface SendToState {
  /** The track currently being sent, or `null`. */
  sending: string | null
  failure: SendFailure | null
  send: (trackId: string, target: SendTarget) => Promise<void>
}

export const useSendToStore = create<SendToState>((set) => ({
  sending: null,
  failure: null,

  send: async (trackId, target) => {
    set({ sending: trackId, failure: null })
    try {
      await sendTo(trackId, target)
      set({ sending: null })
    } catch (err: unknown) {
      // Tauri rejects a `Result<(), String>` with the bare string, not an
      // `Error`; `state/player.ts` narrows the same way for the same reason.
      const message = err instanceof Error ? err.message : String(err)
      set({ sending: null, failure: { trackId, message } })
    }
  },
}))
```

`app/src/views/Library.tsx` -- `TrackCard` only. Add one import, and replace the component's head
block:

```tsx
import {
  failureFor,
  isSending,
  SEND_TARGET_NAMES,
  SEND_TARGETS,
  useSendToStore,
} from '../state/sendto'
```

```tsx
function TrackCard({ row }: { row: TrackRow }) {
  const play = usePlayerStore((state) => state.play)
  const send = useSendToStore((state) => state.send)
  const sending = useSendToStore((state) => state.sending)
  const failure = useSendToStore((state) => state.failure)

  const sendError = failureFor(failure, row.id)
  const busy = isSending(sending, row.id)

  return (
    <li className="panel track-row">
      <div className="track-head">
        <span className="track-name">{row.name}</span>
        <div className="track-head-actions">
          <button
            type="button"
            className="track-play"
            onClick={() => void play(row.id, row.name)}
          >
            Play
          </button>
          <span className="track-send-label">Send to</span>
          {SEND_TARGETS.map((target) => (
            <button
              key={target}
              type="button"
              className="track-send"
              disabled={busy}
              onClick={() => void send(row.id, target)}
            >
              {SEND_TARGET_NAMES[target]}
            </button>
          ))}
          <span className="track-duration">{row.duration}</span>
        </div>
      </div>
```

and, immediately after the existing `<p className="track-file">{row.file}</p>` line, before the
closing `</li>`:

```tsx
      {sendError !== null ? <p className="track-send-error">{sendError}</p> : null}
```

The rest of `TrackCard` -- the whole `<dl className="track-recipe">` block and the `track-file`
line -- is unchanged.

`app/src/theme.css` -- **extend the existing selectors, do not write a second copy.** `.track-send`
is styled by joining `.track-play`'s two existing rules:

```css
.track-play,
.track-send {
  /* ...existing .track-play body, unchanged... */
}

.track-play:hover,
.track-send:hover {
  /* ...existing body, unchanged... */
}
```

then append:

```css
.track-send:disabled {
  color: var(--text-muted);
  border-color: var(--border);
  cursor: default;
}

.track-send-label {
  color: var(--text-muted);
  font-size: 12px;
}

.track-send-error {
  margin: var(--gap-sm) 0 0;
  color: var(--danger);
  font-size: 13px;
}
```

PROJECT.md's backlog already carries an entry for three identical retry buttons written three
times. Copying `.track-play`'s body into a fourth rule is precisely how that entry got written; the
selector list is the fix, and it costs nothing here because the two buttons are meant to look
alike. All four tokens used above already exist in `theme.css`.

### Tests -- `app/src/state/sendto.test.ts`

Mock `../bridge/sendto` with the `state/albums.test.ts` header shape. Each test names the invariant
it protects:

1. `test_failure_for_returns_the_message_for_its_own_track` -- **protects:** the error appears
   under the row that produced it.
2. `test_failure_for_returns_null_for_another_track` -- **protects:** one row's failure never leaks
   onto every other row. This is the mutation that matters: delete the `trackId` comparison and
   test 1 still passes.
3. `test_failure_for_returns_null_when_nothing_failed` -- **protects:** absent is not empty.
4. `test_is_sending_is_true_only_for_the_track_in_flight` -- **protects:** clicking one row's
   button does not disable every other row's.
5. `test_send_records_the_failure_against_the_track_that_failed` -- reject the mock with a bare
   string (**not** an `Error`: that is how Tauri actually rejects) and assert the store holds
   `{ trackId, message }` with the string intact.
6. `test_send_clears_a_previous_failure_before_trying_again` -- **protects:** a stale error does not
   sit under a row whose retry is still running.
7. `test_send_leaves_no_row_marked_sending_after_a_failure` -- **protects:** the buttons come back.
   A `try` with no reset on the `catch` side is the obvious way to write this wrong.

Expected: frontend 373 -> ~380.

### Acceptance criteria

- [ ] `npm run gate` green from the repo root.
- [ ] No changes outside the five listed frontend files.
- [ ] `theme.css` gains **no** duplicate of `.track-play`'s body; `.track-send` is styled through
      the shared selector list.
- [ ] Every `className` used in the TSX has a rule in `theme.css` (CONVENTIONS).
- [ ] `TrackCard` derives nothing: `sendError` and `busy` both come from the pure helpers.

### Out of scope

- Any change to `src-tauri` -- T-404a is done.
- Delete, rename, export, and reveal as an action of its own: those are T-405.
- Bulk send, or sending a whole album -- backlog, and it needs bulk import on the mastering side.
- Changing the URLs, or adding a third destination.

### If unclear

Do not guess. Output a numbered list of questions and stop.

### Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read src-tauri/src/sendto.rs --read app/src/state/player.ts --read app/src/state/albums.test.ts --read app/src/bridge/library.ts --file app/src/bridge/sendto.ts --file app/src/state/sendto.ts --file app/src/state/sendto.test.ts --file app/src/views/Library.tsx --file app/src/theme.css
```

---

## Manual verify (producer click-through)

**The gate cannot check any of steps 1-4.** `npm run gate` never launches a browser or a file
manager. T-402 is the precedent: the whole feature lived in the part CI could not see.

1. **Mixing.** On a track whose file is present, click **Mixing**. Explorer opens **with the
   `.flac` selected**, not merely the folder open, and the browser opens
   `https://app.latentmixer.com`.
2. **Mastering.** Same track, click **Mastering**: `https://app.latentmastering.com`.
3. **Missing file.** Rename or move that track's `.flac` in `projects/<slug>/tracks/`, then click
   **Mixing**: the row shows the missing-file sentence and **no browser tab opens**. Put the file
   back and click again -- it works.
4. **Window order -- an observation, not a pass/fail.** Note which window ends up on top. Reveal
   runs first for failure-ordering reasons, so the browser may cover Explorer; if dragging is
   awkward in practice that is a CSS-TODO/polish entry, not a defect in this task.
5. **The error belongs to its row.** With one row showing a failure, click Send on a *different*
   track: the message must move, not multiply.
