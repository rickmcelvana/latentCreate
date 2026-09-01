# T-408a: delete a lyric version

**Depends:** T-201 (`library::lyrics`, the document store), T-311 (track sidecars carry
`provenance.spec.lyrics`, the `LyricRef` this refusal reads), T-405 (the delete-to-trash discipline
and the "one destructive action, pre-write it" pattern this follows)
**Crate/dir:** `crates/library` + `src-tauri` + `app`
**Phase scope:** T-408 part a (phase-4.md). Delete for created content, starting with the smallest
unit: a single lyric version. The producer's `my-first-song` holds **31 versions in one document**
with no way to remove one, which is what opened T-408.

**Lane split (WORKFLOW 1), two parts:**

- **T-408a-back -- backend. Architect-direct, landed.** `library::lyrics::delete_version`, the
  `LibraryError::VersionReferenced` variant, and the `lyrics_delete_version` Tauri command.
  Pre-written, mutation-tested (six mutations by hand, all killed) and committed with this brief.
  The refusal rule is the safety-critical core of the whole T-408 family, so it does not go through
  an executor -- the same call T-405a made for the trasher.
- **T-408a-front -- frontend. The Aider run.** `bridge/lyricdoc.ts` (one function), the lyrics
  store's `deleteVersion`, the `VersionRow` delete affordance with an inline confirm, and the CSS.

---

## Goal

From a version's row in Lyrics Studio's **Versions** list: **Delete** it, after an inline confirm.
The delete is **refused when any track in the project was generated from that version**, and the
refusal **names the tracks** so the user knows what to remove first. No version is ever renumbered.

---

## T-408a-back -- backend (DONE, architect-direct)

**Files:** `crates/library/src/lib.rs` (`LibraryError::VersionReferenced`),
`crates/library/src/lyrics.rs` (`delete_version` + 8 tests), `src-tauri/src/lyricdoc.rs`
(`lyrics_delete_version` command), `src-tauri/src/lib.rs` (handler registration).

### What the refusal reads, and why it is the feature

A track's sidecar records the lyrics it used as `provenance.spec.lyrics: Option<LyricRef>`, and a
`LyricRef` is `{ doc_id, version }`. Deleting a version a track points at would leave that track's
recipe resolving to lyrics nobody could show -- provenance pointing at a hole. So `delete_version`
scans `list_tracks(root, project)` for any track whose `spec.lyrics` matches `(doc_id, version)`
exactly, and if any do, returns `VersionReferenced { doc_id, version, tracks }` naming them. This is
the owner's rule from the 2026-09-01 decisions log ("refuse and say why", not delete-and-render-
missing): 19 of the producer's 20 sidecars point at `ld-0001` version 31, so a delete-and-strand
policy would break the provenance of nearly the whole library. The error names the tracks the way
T-403 names a dangling id "Missing track" -- a refusal with no subject is a dead end (T-408 trap 3).

### The design decisions

- **No OS trash here.** A version is an element inside the document's JSON, not a file of its own,
  so the delete is an in-file edit written through `save_doc` (the same atomic write every version
  edit already uses). T-408 trap 1 ("OS trash, never `fs::remove_file`") applies to parts b/c/d,
  which remove whole files/trees; it does not apply to a version.
- **Versions are never renumbered.** The chosen version is removed and every other keeps its
  `number`; a hole is legal, which is exactly what `LyricDoc::push_version` already assumes (it
  counts from the highest present). Renumbering would silently repoint every surviving sidecar's
  `LyricRef` -- the hazard the refusal exists to stop.
- **Top-number reuse is possible and is safe.** Deleting the *highest* version frees its number, and
  a later `push_version` mints it again. Safe precisely *because* the refusal guaranteed no sidecar
  referenced the deleted version, so the reissued number cannot collide with any track's recipe. A
  per-document version counter (the `next_*_seq` shape ids use) is deliberately **not** added: it
  would protect nothing the refusal does not already protect. A test documents this rather than
  guards against it.
- **Deleting the approved version clears `approved`** rather than being refused. Approval is the
  user's current working pointer for AudioStudio, not provenance; a document with none is an
  ordinary state (a fresh one has none). The track-reference rule is the *only* bar to deletion.
- **A missing version number is `NotFound { kind: "lyric version" }`,** not a silent no-op -- the
  caller named something that is not there.
- **The command returns the updated `LyricDoc`,** so the frontend replaces its `doc` with the
  server's result rather than editing its own copy and re-saving -- a frontend that removed the
  version locally and called `lyrics_save` would **bypass the refusal check entirely.** The delete
  must round-trip through the backend.

### Tests and mutations

`crates/library/src/lyrics.rs` gained 8 tests (**library 84 -> 92**, 91 + the ignored keychain
test). Each names its invariant, happy paths included (T-404b's lesson). Six mutations were run by
hand and all died:

1. refusal guard skipped (`if false`) -> the referenced-version test fails.
2. the `doc_id` half of the reference match dropped -> the cross-document test fails (a track
   referencing doc B's v1 wrongly blocks deleting doc A's v1).
3. the `version` half of the match dropped -> the "another version does not block" test fails.
4. the approval-clear removed -> the approved-version test fails.
5. the `retain` predicate inverted (`!=` -> `==`) -> five tests fail.
6. the `NotFound` guard skipped -> the missing-version test fails.

The `lyrics_delete_version` command carries no new src-tauri test (src-tauri stays 111), consistent
with `lyrics_open`/`lyrics_save`: it resolves the selected project and maps errors, and every
behaviour lives in `library::lyrics::delete_version`, which the tests cover.

---

## T-408a-front -- frontend (Aider run)

**Files to modify:**
- `app/src/bridge/lyricdoc.ts` (add one function)
- `app/src/state/lyrics.ts` (add `deleteVersion`, and a per-row confirm/error marker)
- `app/src/views/LyricsStudio.tsx` (`VersionRow` + `VersionList`)
- `app/src/theme.css` (extend the existing `.lyrics-version-*` block)

### Spec

- **`bridge/lyricdoc.ts`** gains `deleteLyricVersion(docId, number)` invoking `lyrics_delete_version`
  and returning the updated `LyricDoc`. It is the only new Tauri crossing; keep it in the bridge
  (CONVENTIONS -- every crossing in `bridge/`).
- **`state/lyrics.ts`** gains `deleteVersion(number: number): Promise<boolean>`. It reads the current
  `doc`, calls the bridge, and on success `set({ doc: updated })` -- **replacing** the doc with the
  backend's result, never editing `versions` locally (that would skip the refusal). On failure it
  records the backend's message (which names the blocking tracks) in the existing `error` field and
  returns `false`, leaving the doc unchanged. Follows the shape of `approve` (optimistic set + save)
  but inverted: the backend is the authority here, so the store waits for its answer.
- **The delete is two-step,** matching T-405's track delete: an inline **Delete** on a `VersionRow`
  arms a confirm in place of the row's action buttons (the words **Delete this version?** with
  **Delete** and **Cancel**); no modal (CONVENTIONS). Track which version is confirming with a
  `confirmingVersion: number | null` in the store (the T-405 `confirming` shape, keyed by version
  number since that is a version's identity in a document).
- On a **successful** delete the store has already replaced `doc`, so the list re-renders with the
  version gone -- no reload call needed (unlike T-405c, the lyrics store owns `doc` directly).
- On a **refusal** the confirm disarms and the error line shows the backend message under the
  version list (reuse the existing `error` rendering path in `LyricsStudio` -- do not add a second
  error surface).

### The trap to close (T-404b/T-405b lesson)

Name the happy-path invariants, not only the failure ones. The two mutations that survived T-404b
and the vacuous tests caught in T-405b both lived in untested happy-path space. So the store tests
must assert: `deleteVersion` **success** replaces `doc` with the returned value and returns `true`;
**failure** records the error, returns `false`, and **leaves `doc` unchanged**; and the confirm
marker arms and disarms. Assert `deleteLyricVersion` was called with `(docId, number)` -- a dropped
argument was the T-404b mutation.

### Reference implementation

`app/src/bridge/lyricdoc.ts` -- add beside `saveLyricDoc`:

```ts
/** Delete one version, refusing (with a message) when a track references it. */
export async function deleteLyricVersion(docId: string, number: number): Promise<LyricDoc> {
  return await invoke<LyricDoc>('lyrics_delete_version', { docId, version: number })
}
```

`app/src/state/lyrics.ts` -- add to the store interface and implementation (mirror `approve`'s
error handling; note the doc-replace, not a local edit):

```ts
  // in the state interface:
  confirmingVersion: number | null
  askDeleteVersion: (number: number) => void
  cancelDeleteVersion: () => void
  deleteVersion: (number: number) => Promise<boolean>

  // in the store body:
  confirmingVersion: null,
  askDeleteVersion: (number) => set({ confirmingVersion: number, error: null }),
  cancelDeleteVersion: () => set({ confirmingVersion: null }),

  deleteVersion: async (number) => {
    const doc = get().doc
    if (doc === null) return false
    try {
      const updated = await deleteLyricVersion(doc.id, number)
      set({ doc: updated, confirmingVersion: null })
      return true
    } catch (err: unknown) {
      // The message names the tracks holding the version -- show it as-is.
      set({ error: String(err), confirmingVersion: null })
      return false
    }
  },
```

`VersionRow` gains a **Delete** button beside Restore/Approve, and when
`confirmingVersion === version.number` it shows the inline confirm instead of the action buttons.
`VersionList` reads `confirmingVersion`, `askDeleteVersion`, `cancelDeleteVersion`, `deleteVersion`
from the store and threads them down, the same way it already threads `restore`/`approve`.

### CSS (`theme.css`)

Add a `.lyrics-version-confirm` region (a flex row like `.lyrics-version-actions`) and, if the
Delete button needs a destructive tint, a `.setup-button-danger` **only if one does not already
exist** -- grep first; otherwise reuse the existing button classes. Tokens only. Do not fork an
existing rule.

### Acceptance criteria (T-408a-front)

- [ ] `npm run gate` green.
- [ ] No changes outside the four listed files.
- [ ] The delete round-trips through `lyrics_delete_version`; the store never edits `doc.versions`
      locally and never calls `lyrics_save` to perform a delete.
- [ ] Every Tauri crossing is in `bridge/lyricdoc.ts`.
- [ ] Every className used has a rule in `theme.css`; no forked rules.
- [ ] Store tests name the happy path as well as the failure (T-405b lesson).

### Aider launch (T-408a-front)

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/bridge/lyricdoc.ts --read app/src/state/trackActions.ts --file app/src/state/lyrics.ts --file app/src/state/lyrics.test.ts --file app/src/views/LyricsStudio.tsx --file app/src/theme.css --file app/src/bridge/lyricdoc.ts
```

---

## Out of scope (T-408a)

- **Many lyric documents per project, and deleting a whole document** -- T-408b (retires the
  `lyrics_open` -> `first()` shortcut, adds `lyrics_create`/`lyrics_list`/`lyrics_open(id)` and a
  document picker; a document is deletable only when none of its versions is referenced).
- **Deleting an album** -- T-408c (`library::albums::delete_album` + the panel affordance).
- **Deleting a project** -- T-408d (the whole `projects/<slug>/` tree to OS trash).
- **Naming a track / carrying a title to export** -- T-409.

## If unclear

Do not guess. Output a numbered list of questions and stop.

---

## Manual verify (producer click-through, after T-408a-front)

`npm run gate` runs `vite build`, never `tauri build`, and cannot exercise the real project on disk
-- so the refusal against the producer's actual 31 versions is click-through.

1. **Delete an unreferenced version.** In Lyrics Studio, Delete a version no track was generated
   from -> inline confirm -> **Delete** -> the row disappears and the remaining versions **keep
   their numbers** (a hole, not a renumber). Reopen the app: the deletion persisted.
2. **The refusal names the track.** Delete the approved version `my-first-song` generated its tracks
   from (version 31) -> the delete is **refused** with a message naming the track(s) that hold it,
   and the version stays. This is the case that matters: 19 sidecars point at it.
3. **Delete an unreferenced older version, then the approved one is still fine.** Confirms the scan
   matches the exact version, not the whole document.
4. **Deleting the approved version (once nothing references it) clears the approval.** If you delete
   whatever version is approved and no track uses it, the "vN is approved" line clears rather than
   pointing at a gone version.
5. **Cancel leaves everything untouched.**
