# T-405: track actions -- delete, rename, export, reveal

**Depends:** T-311 (the Library and its `TrackRow`), T-404 (the `TrackCard` action cluster this
extends, and the `failureFor`/`isSending` per-row-state pattern this copies)
**Crate/dir:** `crates/library` + `src-tauri` + `app`
**Phase scope:** the per-track actions the milestone line did not require but Phase 4 names
(phase-4.md). Delete is the one destructive action in the app.

**Lane split (WORKFLOW 1), three parts:**

- **T-405a -- backend. Architect-direct, landed.** The `library::tracks` functions
  (`delete_track`, `rename_track`, `export_track`, `trash_to_os`), the `trash` dependency, the
  `LibraryError::Trash` variant, and the four Tauri commands. Written, compiled, clippy- and
  fmt-clean, **10 new tests and 5 mutations all killed** before this brief was finished -- the
  destructive path is exactly where pre-writing and mutation-testing earn their keep, so it does
  not go through an executor.
- **T-405b -- frontend store. The first Aider run.** `bridge/tracks.ts`, `state/trackActions.ts`
  and its tests.
- **T-405c -- frontend UI. The second Aider run.** The `TrackCard` action controls (delete with an
  inline confirm, inline rename, export, reveal) and the CSS.

Split b from c so each run stays well under the ~400-line rule (the T-401/402/403 pattern), and
because c is the only part with a producer click-through -- the file actually reaching the OS
trash, the save dialog, and the reveal are things `npm run gate` cannot see.

---

## Goal

From a track's row in the Library: **Delete** it (to the OS trash, after an inline confirm),
**Rename** it (set its title), **Export** it (copy the audio to a chosen location), and **Reveal**
it (show the file in the OS file manager). No hard deletes anywhere; a deleted id is never reused.

---

## T-405a -- backend (DONE, architect-direct)

**Files:** `crates/library/Cargo.toml` (+`trash = "5.2"`, MIT), `crates/library/src/lib.rs`
(`LibraryError::Trash`), `crates/library/src/tracks.rs` (the three functions + `trash_to_os` +
`trash_if_present`), `src-tauri/src/tracks.rs` (four commands), `src-tauri/src/lib.rs` (handlers).

### What was verified before writing it, and what it changed

1. **`trash::delete` canonicalizes the path first and errors on a path that is not there** (read
   from `trash-5.2.6/src/lib.rs`). So a track whose audio file was already removed -- in Explorer,
   or by a half-completed prior delete -- would make a naive `trash::delete` fail. `delete_track`
   checks `path.exists()` before each trash call, and the "audio already gone" case has its own
   test.
2. **`trash` moves to the real Recycle Bin.** A test that calls it fills the developer's trash on
   every `cargo test`. So the trash operation is **injected**: `delete_track` takes a
   `Fn(&Path) -> Result<(), LibraryError>`, production passes `trash_to_os`, and the tests pass a
   fake that records the path and moves the file to a graveyard tempdir. This is the same shape
   `now_rfc3339` uses -- the side effect the test must not perform is a parameter, not a hardcoded
   call. It is also what lets the CONVENTIONS test ("assert the trash call was made, not that the
   file is gone") exist at all.
3. **`trash` 5.2.6 is MIT** and was not in `Cargo.lock` -- a genuine new dependency, permissive,
   so allowed (CONVENTIONS) and recorded here.

### The design decisions

- **Delete order: files first, record last, missing files tolerated.** A crash after trashing but
  before the `save_project` leaves the project listing a track whose files are gone -- the "Missing
  track" state T-403 already renders -- and a **retry completes cleanly** because `trash_if_present`
  skips the files already gone. The reverse order (record first) would leave orphan files nothing
  references and no id left to retry with. This is the opposite choice from `save_track` (which
  writes the record last so a failure is an orphan sidecar), and deliberately: for *create* a
  project listing a track with no file is a lie, but for *delete* it is a designed, recoverable,
  self-healing state.
- **The id leaves `Project::tracks` and every album that holds it.** T-403 renders a *live* id an
  album holds; a *deleted* id must drop out, or the album carries a permanent phantom. `next_track_seq`
  is untouched, so the freed id is never minted again -- an album's surviving ids can never come to
  mean a different song.
- **Rename writes only the sidecar** (ARCHITECTURE 8: the sidecar is the single source of truth for
  a title). An empty or whitespace title **clears** the title, and the Library falls back to the id,
  exactly as an untitled track already reads.
- **Export is a copy, not a move.** The track stays in the library. `dest` comes from the OS save
  dialog, so it is trusted -- unlike the id, which is whitelisted before it touches a path.
- **Reveal reuses `send_to`'s reveal**, on its own: resolve the id to an absolute path, hand it to
  the opener plugin.

### Tests

`crates/library/src/tracks.rs` gained 10 tests, and **library 74 -> 84** (83 + the ignored keychain
test). Every one names its invariant; the happy paths (`rename` sets and reads back, `export`
copies and leaves the original) are named deliberately, because T-404b's two surviving mutations
both sat in exactly the untested happy-path space. Five mutations were run by hand and all died:
dropping the album cleanup, dropping the `project.tracks` cleanup, removing the `exists()` guard,
inverting the blank-title clear, and turning the export copy into a move.

**The four Tauri commands carry no new src-tauri tests** (src-tauri stays 111), consistent with the
existing `library_tracks` / `track_audio_path`: they resolve the selected project and map errors,
and every behaviour they have lives in the `library::tracks` functions the tests already cover.

---

## T-405b -- frontend store (Aider run)

**Files to create:**
- `app/src/bridge/tracks.ts`
- `app/src/state/trackActions.ts`
- `app/src/state/trackActions.test.ts`

### Spec

- Every action tracks **which row** it belongs to: a busy marker, an error, a delete-confirm
  marker, and a rename marker are all `string | null` (a track id), read through pure helpers -- the
  same per-row discipline as `state/sendto.ts` `failureFor`/`isSending`, and for the same reason
  (one row's state must never leak onto another).
- **Delete is two-step.** `askDelete(id)` arms an inline confirm; `confirmDelete(id)` performs it;
  `cancelDelete()` disarms. No blocking modal (CONVENTIONS).
- **Rename is inline.** `startRename(id)` opens the editor; `submitRename(id, title)` saves;
  `cancelRename()` closes it.
- **Export cancellation is not an error.** `pickExportPath` returns `null` when the user dismisses
  the save dialog, and `exportTrack` then does nothing -- the same rule as `ImportWorkflow`'s file
  picker (reporting the user's own decision as a failure is the mistake).
- `confirmDelete` and `submitRename` **return a boolean**, so the view can re-load the Library on
  success. The reload is wiring in the view (the `ImportWorkflow` `refreshModels` precedent), not a
  cross-store call from here -- this store stays testable without mocking the library store.
- On a delete/rename **failure**, the confirm/rename marker stays set so the user can retry or
  cancel; only `busy` clears.

### Reference implementation

`app/src/bridge/tracks.ts`:

```ts
import { invoke } from '@tauri-apps/api/core'
import { save } from '@tauri-apps/plugin-dialog'

/** Move a track's files to the OS trash and unlist its id. */
export async function deleteTrack(id: string): Promise<void> {
  await invoke('delete_track', { id })
}

/** Set or clear a track's title. An empty title clears it. */
export async function renameTrack(id: string, title: string): Promise<void> {
  await invoke('rename_track', { id, title })
}

/** Copy a track's audio file to `dest`. */
export async function exportTrack(id: string, dest: string): Promise<void> {
  await invoke('export_track', { id, dest })
}

/** Reveal a track's audio file in the OS file manager. */
export async function revealTrack(id: string): Promise<void> {
  await invoke('reveal_track', { id })
}

/**
 * Show the OS save dialog for an export; `null` if the user cancelled.
 *
 * The dialog lives in the bridge, not the view -- the one Tauri surface
 * `ImportWorkflow` reaches for directly, kept here so every crossing is in
 * `bridge/` (CONVENTIONS).
 */
export async function pickExportPath(defaultName: string): Promise<string | null> {
  const chosen = await save({ defaultPath: defaultName })
  return typeof chosen === 'string' ? chosen : null
}
```

`app/src/state/trackActions.ts`:

```ts
import { create } from 'zustand'
import {
  deleteTrack,
  exportTrack,
  pickExportPath,
  renameTrack,
  revealTrack,
} from '../bridge/tracks'

/** An action failure, remembered with the track it belongs to. */
export interface ActionError {
  trackId: string
  message: string
}

/** The error to show under one row, or `null`. Twin of `sendto` `failureFor`. */
export function errorFor(error: ActionError | null, trackId: string): string | null {
  if (error === null) return null
  return error.trackId === trackId ? error.message : null
}

/** Whether `trackId` is the row named by a `string | null` marker. */
export function isRow(marker: string | null, trackId: string): boolean {
  return marker === trackId
}

interface TrackActionsState {
  /** A track id with an action in flight, or `null`. */
  busy: string | null
  error: ActionError | null
  /** A track id awaiting delete confirmation, or `null`. */
  confirming: string | null
  /** A track id whose title is being edited, or `null`. */
  renaming: string | null
  askDelete: (id: string) => void
  cancelDelete: () => void
  confirmDelete: (id: string) => Promise<boolean>
  startRename: (id: string) => void
  cancelRename: () => void
  submitRename: (id: string, title: string) => Promise<boolean>
  runExport: (id: string, defaultName: string) => Promise<void>
  reveal: (id: string) => Promise<void>
}

function message(err: unknown): string {
  // Tauri rejects a `Result<(), String>` with the bare string, not an Error.
  return err instanceof Error ? err.message : String(err)
}

export const useTrackActionsStore = create<TrackActionsState>((set) => ({
  busy: null,
  error: null,
  confirming: null,
  renaming: null,

  askDelete: (id) => set({ confirming: id, error: null }),
  cancelDelete: () => set({ confirming: null }),

  confirmDelete: async (id) => {
    set({ busy: id, error: null })
    try {
      await deleteTrack(id)
      set({ busy: null, confirming: null })
      return true
    } catch (err: unknown) {
      // Keep `confirming` set so the row can retry or cancel.
      set({ busy: null, error: { trackId: id, message: message(err) } })
      return false
    }
  },

  startRename: (id) => set({ renaming: id, error: null }),
  cancelRename: () => set({ renaming: null }),

  submitRename: async (id, title) => {
    set({ busy: id, error: null })
    try {
      await renameTrack(id, title)
      set({ busy: null, renaming: null })
      return true
    } catch (err: unknown) {
      set({ busy: null, error: { trackId: id, message: message(err) } })
      return false
    }
  },

  runExport: async (id, defaultName) => {
    let dest: string | null
    try {
      dest = await pickExportPath(defaultName)
    } catch (err: unknown) {
      set({ error: { trackId: id, message: message(err) } })
      return
    }
    // Cancelling the dialog is the user's decision, not a failure.
    if (dest === null) return
    set({ busy: id, error: null })
    try {
      await exportTrack(id, dest)
      set({ busy: null })
    } catch (err: unknown) {
      set({ busy: null, error: { trackId: id, message: message(err) } })
    }
  },

  reveal: async (id) => {
    set({ error: null })
    try {
      await revealTrack(id)
    } catch (err: unknown) {
      set({ error: { trackId: id, message: message(err) } })
    }
  },
}))
```

### Tests -- `app/src/state/trackActions.test.ts`

Mock `../bridge/tracks` wholesale (the `state/sendto.test.ts` / `state/albums.test.ts` header
shape). Name the invariant on each; **the happy paths are named on purpose** (T-404b's lesson --
both mutations that survived there lived in untested happy-path space):

1. `errorFor` returns the message for its own track / `null` for another / `null` when none.
2. `isRow` is true only for the marked id.
3. `askDelete` arms `confirming`; `cancelDelete` disarms it.
4. `confirmDelete` **success**: resolves the mock, returns `true`, clears `busy` and `confirming`,
   leaves no error.
5. `confirmDelete` **failure**: rejects with a bare string, returns `false`, records the error
   against the id, clears `busy`, **leaves `confirming` set** (retryable).
6. `startRename`/`cancelRename` toggle `renaming`.
7. `submitRename` **success**: returns `true`, clears `renaming`; passes the title through to the
   bridge (assert `renameTrack` was called with `(id, title)` -- the T-404b mutation was a dropped
   argument).
8. `submitRename` **failure**: returns `false`, error recorded.
9. `runExport` **cancelled**: `pickExportPath` resolves `null`, `exportTrack` is **never called**,
   no error set.
10. `runExport` **picked**: `pickExportPath` resolves a path, `exportTrack` is called with
    `(id, thatPath)`.
11. `reveal` calls `revealTrack(id)`; a rejection records the error against the id.

Expected: frontend 382 -> ~397.

### Acceptance criteria (T-405b)

- [ ] `npm run gate` green.
- [ ] No changes outside the three listed files.
- [ ] Every Tauri crossing (`invoke`, `save`) is in `bridge/tracks.ts`, none in the store.
- [ ] The store derives nothing in a component's place: `errorFor`/`isRow` are the only readers.

### Aider launch (T-405b)

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/sendto.ts --read app/src/state/sendto.test.ts --read app/src/components/ImportWorkflow.tsx --file app/src/bridge/tracks.ts --file app/src/state/trackActions.ts --file app/src/state/trackActions.test.ts
```

---

## T-405c -- frontend UI (Aider run, after T-405b lands)

**Files to modify:**
- `app/src/views/Library.tsx` (the `TrackCard` component only)
- `app/src/theme.css` (extend the T-404 `.track-*` block)

### Spec

Add an actions row to `TrackCard`, below the recipe `<dl>`. Four controls: **Rename**, **Export**,
**Reveal**, **Delete**. All four disabled while `busy` is this row.

- **Rename** opens an inline editor in place of the four buttons: a text input prefilled with the
  current name (`row.name`), plus **Save** and **Cancel**. Submitting calls `submitRename`; on
  success the view re-loads the Library (the `ImportWorkflow` `refreshModels` precedent -- wiring,
  not a derivation).
- **Delete** arms an inline confirm in place of the four buttons: the words **Move to Trash?** with
  **Trash it** and **Cancel**. **Trash it** calls `confirmDelete`; on success the view re-loads the
  Library. There is no modal (CONVENTIONS).
- **Export** calls `runExport(row.id, row.name + the extension)`. The default filename is the
  track's display name; derive the extension from `row.file` (its own `.flac`/`.wav`/`.mp3`).
- **Reveal** calls `reveal(row.id)`.
- A per-row error line (`errorFor`) shows under the row, reusing the `.track-send-error` treatment
  T-404 added (rename it to a shared `.track-action-error` if it now serves both, or add the new
  class alongside -- do **not** fork a second copy of the rule; PROJECT.md already tracks the
  three-identical-retry-buttons debt).

### Reference implementation (`TrackCard`)

The head block and recipe `<dl>` are unchanged from T-404. Add the store reads and the actions
region. Reload the Library on a successful delete or rename:

```tsx
function TrackCard({ row }: { row: TrackRow }) {
  const play = usePlayerStore((state) => state.play)
  const send = useSendToStore((state) => state.send)
  const sending = useSendToStore((state) => state.sending)
  const sendFailure = useSendToStore((state) => state.failure)

  const reloadLibrary = useLibraryStore((state) => state.load)
  const actions = useTrackActionsStore()

  const sendError = failureFor(sendFailure, row.id)
  const actionError = errorFor(actions.error, row.id)
  const busy = isSending(sending, row.id) || isRow(actions.busy, row.id)
  const confirming = isRow(actions.confirming, row.id)
  const renaming = isRow(actions.renaming, row.id)

  // ... existing <li> / track-head / track-recipe / track-file unchanged ...

  // Below the recipe dl and the track-file line, before the send error:
  //
  //   {renaming ? <RenameRow ... /> :
  //    confirming ? <ConfirmDeleteRow ... /> :
  //    <div className="track-actions"> Rename / Export / Reveal / Delete </div>}
  //
  //   {actionError !== null ? <p className="track-action-error">{actionError}</p> : null}
}
```

`RenameRow` holds local input state seeded from `row.name`, calls
`actions.submitRename(row.id, value)`, and on `true` calls `void reloadLibrary()` then relies on the
store having cleared `renaming`. `ConfirmDeleteRow`'s **Trash it** calls `actions.confirmDelete(row.id)`
and on `true` calls `void reloadLibrary()`. **Export**:

```tsx
const ext = row.file.split('.').pop() ?? 'flac'
void actions.runExport(row.id, `${row.name}.${ext}`)
```

Give `RenameRow` and `ConfirmDeleteRow` real inline components (not JSX buried in a ternary) so the
input's local `useState` has somewhere to live -- the same shape as the existing `ProjectCreate`.

### CSS (`theme.css`)

Add `.track-actions` (a flex row like `.track-head-actions`), buttons styled by joining the existing
`.track-send` selector list (**do not** fork the rule), a `.track-action-confirm` /
`.track-action-rename` pair for the inline regions, and `.track-action-error` sharing the
`.track-send-error` treatment. Tokens only.

### Acceptance criteria (T-405c)

- [ ] `npm run gate` green.
- [ ] No changes outside the two listed files.
- [ ] No forked CSS: action buttons join the `.track-send` selector list; the error line shares the
      `.track-send-error` rule.
- [ ] Every className used has a rule in `theme.css`.
- [ ] `TrackCard` derives nothing: `actionError`, `busy`, `confirming`, `renaming` all come from the
      pure helpers.

### Aider launch (T-405c)

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/state/trackActions.ts --read app/src/state/sendto.ts --read app/src/state/library.ts --file app/src/views/Library.tsx --file app/src/theme.css
```

---

## Out of scope (all of T-405)

- Deleting **lyric versions, lyric documents, albums or projects** -- that is T-408, which reuses
  `trash_to_os` and the delete-to-trash discipline this task establishes.
- Naming a track **at generation time** and carrying the title to the export filename by default --
  that is T-409. T-405's rename is how the 20 tracks that already exist get titles; T-409 does not
  backfill.
- Bulk actions (delete/export several at once).
- The provenance inspector's "re-use these settings" -- T-406.

## If unclear

Do not guess. Output a numbered list of questions and stop.

---

## Manual verify (producer click-through, after T-405c)

`npm run gate` runs `vite build`, never `tauri build`, and never touches the OS trash, the save
dialog or the file manager -- so all of this is click-through, the T-404 precedent.

1. **Delete.** Click **Delete** on a track -> the inline confirm appears. **Trash it** -> the row
   disappears from the Library, and the `.flac` **and** its `.json` sidecar are in the OS Recycle
   Bin / Trash (check it), not gone. **Cancel** on the confirm leaves the track untouched.
2. **Delete does not reuse the id.** After deleting a track, generate a new one: its id is the next
   number, never the deleted one. (Its filename in `projects/<slug>/tracks/` is the tell.)
3. **Rename.** Rename a track to `Midnight` -> the row shows `Midnight`, and reopening the app keeps
   it (the sidecar was written). Rename to blank -> the row falls back to the id.
4. **Export.** Export a track -> the save dialog opens with the track's name as the default
   filename; save it somewhere and confirm the file is a playable copy **and the original is still
   in the Library**. Cancel the dialog -> nothing happens, no error.
5. **Reveal.** Reveal a track -> the file manager opens with the `.flac` selected.
6. **The error belongs to its row.** Force a failure (e.g. reveal a track whose file you moved) and
   confirm the message shows under that row only, and moves/clears when you act on another.
