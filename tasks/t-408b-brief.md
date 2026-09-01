# T-408b: many lyric documents per project, and delete a document

**Depends:** T-408a (`delete_version` and the reference-scan it now shares), T-201 (`library::lyrics`),
T-405 (`trash_to_os` and the delete-to-trash discipline)
**Crate/dir:** `crates/library` + `src-tauri` + `app`
**Phase scope:** T-408 part b (phase-4.md). Retire the Phase 2 one-document shortcut and add a
document delete. A project has only ever been able to hold one lyric document -- `lyrics_open`
returned `project.lyrics.first()` and there was no way to make a second -- which the phase file's
own decision-4 measurement surfaced.

**Lane split (WORKFLOW 1), two parts:**

- **T-408b-back -- backend. Architect-direct, landed.** `library::lyrics::delete_doc`, the
  `DocumentReferenced` error, a shared `tracks_referencing` helper (extracted from `delete_version`
  so the two refusals cannot drift), and the `lyrics_list` / `lyrics_create` / `lyrics_delete_doc`
  commands with `lyrics_open` taking an optional id. Pre-written, mutation-tested (six mutations by
  hand, all killed). `delete_doc` moves a file to the OS trash, so it does not go through an
  executor -- the T-405a/T-408a call for a destructive path.
- **T-408b-front -- frontend. The next run.** A document picker in Lyrics Studio (switch, New,
  Delete-document with an inline confirm and an inline refusal message), and the store's multi-doc
  model.

---

## Goal

A project holds any number of lyric documents. Lyrics Studio shows a picker to switch between them
and to create a new one; a **Delete document** control removes the whole file to the OS trash,
**refused when any track was generated from any of its versions**, naming the tracks. The refusal is
the same rule as T-408a's version delete, applied to the whole file.

---

## T-408b-back -- backend (DONE, architect-direct)

**Files:** `crates/library/src/lib.rs` (`DocumentReferenced`), `crates/library/src/lyrics.rs`
(`tracks_referencing` helper, `delete_doc`, `delete_version` refactored onto the helper, 8 tests),
`src-tauri/src/lyricdoc.rs` (`lyrics_list`, `lyrics_create`, `lyrics_delete_doc`, `lyrics_open(id?)`),
`src-tauri/src/lib.rs` (handlers).

### The design decisions

- **One reference-scan, two refusals.** `tracks_referencing(root, project, doc_id, version)` is the
  single definition of "what points at these lyrics": `Some(v)` narrows to one version (the version
  delete), `None` matches any version of the document (the document delete). Extracting it means the
  version refusal and the document refusal can never drift apart, and `delete_version` was refactored
  onto it (its own tests and the six T-408a mutations still pass).
- **`delete_doc` is the `delete_track` discipline for a lyric file.** File to OS trash via the
  **injected** trasher (production `trash_to_os`, tests a fake -- so a test never fills the real
  Recycle Bin); **order is file first, record last, a missing file tolerated**, so a crash mid-delete
  leaves a "Missing" document `list_docs` already renders and a retry self-heals; `next_lyric_seq` is
  untouched, so a deleted id is never reissued and a track's `LyricRef` can never come to mean a
  different document. Returns the project's remaining `LyricDocSet` for the picker to render.
- **`DocumentReferenced { doc_id, tracks }`** names the blocking tracks, the whole-file twin of
  `VersionReferenced`. A refused delete trashes nothing and unlists nothing.
- **`lyrics_open` gains an optional id, it is not replaced.** `Some(id)` opens that document; `None`
  stays the first-open default (first document, or create one). Keeping the no-id branch is what lets
  this backend land without breaking the current frontend, which still calls `lyrics_open` with no
  argument until T-408b-front lands. The picker passes the id it wants.

### Tests and mutations

`lyrics.rs` gained 8 tests (**library 92 -> 100**). Six mutations run by hand, all killed: the
helper's `doc_id` match dropped and its version-narrowing forced true (both shared by version and
document delete); `delete_doc`'s refusal guard skipped; its `retain` unlist inverted; its `exists()`
guard removed; its `NotFound` guard skipped. The three commands carry no new src-tauri test
(src-tauri stays 111), consistent with the existing lyric commands.

---

## T-408b-front -- frontend (the next run)

**Files to modify:**
- `app/src/bridge/lyricdoc.ts` (a `LyricDocSet` type + three functions; `openLyricDoc` gains an id)
- `app/src/state/lyrics.ts` (the multi-document model)
- `app/src/state/lyrics.test.ts` (store tests)
- `app/src/views/LyricsStudio.tsx` (a `DocumentPicker`)
- `app/src/theme.css` (the picker's styling)

### Spec

- **Bridge.** Add `listLyricDocs(): Promise<LyricDocSet>` (`lyrics_list`), `createLyricDoc(title?):
  Promise<LyricDoc>` (`lyrics_create`), `deleteLyricDoc(docId): Promise<LyricDocSet>`
  (`lyrics_delete_doc`); change `openLyricDoc` to take an optional `id` and pass it as `docId`. A
  `LyricDocSet` mirrors Rust: `{ docs: LyricDoc[]; warnings: LyricWarning[] }` (a minimal
  `LyricWarning` union is fine; the picker reads `docs`).
- **Store.** Add `docs: LyricDoc[]`, `selectedDocId: string | null`, and:
  - `loadDocs()` (replaces the single `loadDoc` on mount): list the project's docs; if none, create
    one; keep the current `selectedDocId` when it still exists, else select the first; then set `doc`
    and `draft` from the selected document.
  - `selectDoc(id)`: open that document, set `doc`, `selectedDocId` and `draft` (the `loadDoc` body).
  - `createDoc()`: create, then select the new document.
  - `deleteDoc(id)`: call the backend; on success set `docs` to the returned remainder and, **if the
    deleted document was selected**, select the first remaining one (or, if none remain, create one --
    a project with zero documents is not a state the studio can show). On a **refusal**, record the
    message in a dedicated `deleteDocError` (not the shared `error`, for the T-408a reason -- it
    feeds `generationPhase`) and leave `docs`/`doc` unchanged.
  - A `confirmingDocDelete: boolean` marker (the whole document is one thing, so a boolean, not an
    id) with `askDeleteDoc` / `cancelDeleteDoc`.
- **View.** A `DocumentPicker` above the brief form (or at the top of the output panel): a `<select>`
  over `docs` showing each document's title or, when untitled, its id; a **New** button; a **Delete
  document** button that arms an inline **Delete this document? / Delete / Cancel** confirm. The
  refusal message renders **inline at the picker** (the T-408a placement lesson -- next to the
  control), never at the top of a scrolled page.

### The traps to close

- **Switching documents must not leak the old draft.** `selectDoc` resets `draft` from the newly
  opened document (its latest version's text, the `loadDoc` rule), so an unsaved edit in one document
  does not bleed into another. Name this in a store test.
- **Deleting the selected document must land somewhere valid.** After deleting the open document, the
  studio shows another (or a fresh one), never an empty `doc: null` with a picker pointing at
  nothing. Test the "deleted the selected doc" path explicitly.
- **Happy paths named** (T-405b/T-408a lesson): `createDoc` selects the new doc; `deleteDoc` success
  replaces the list; `deleteDoc` refusal keeps the list and records the message; `selectDoc` swaps
  the draft.

### Acceptance criteria (T-408b-front)

- [ ] `npm run gate` green.
- [ ] No changes outside the five listed files.
- [ ] Every Tauri crossing is in `bridge/lyricdoc.ts`.
- [ ] The delete round-trips through `lyrics_delete_doc`; the store never edits `project.lyrics` or
      trashes anything itself.
- [ ] Every className has a rule in `theme.css`; no forked rules.
- [ ] The refusal message renders at the picker, and the doc-delete error is its own field (not the
      generation `error`).

### Aider launch (T-408b-front), if run through an executor

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/bridge/lyricdoc.ts --read app/src/state/lyrics.ts --file app/src/state/lyrics.ts --file app/src/state/lyrics.test.ts --file app/src/views/LyricsStudio.tsx --file app/src/theme.css --file app/src/bridge/lyricdoc.ts
```

---

## Out of scope (T-408b)

- **Persisting which document is selected across restarts.** Session-only for v1; the studio opens
  the first document. A per-project selection (the config-field shape T-401 used for projects) is a
  possible follow-up, not this task.
- **Deleting an album** (T-408c) and **deleting a project** (T-408d).
- **A title UI for a document** -- that is T-409 (`LyricDoc.title` gets an input); here a new document
  is untitled and shows by id until T-409.

## If unclear

Do not guess. Output a numbered list of questions and stop.

---

## Manual verify (producer click-through, after T-408b-front)

1. **Create a second document.** New -> the picker shows two; the editor is empty for the new one.
2. **Switch between them.** Selecting each shows its own versions and draft; an unsaved edit in one
   does not appear in the other.
3. **Delete an unreferenced document.** Delete document -> confirm -> it leaves the picker and its
   `lyrics/<id>.json` is in the OS Recycle Bin (not gone); the studio shows another document.
4. **The refusal names the track.** On `my-first-song` (its one document, `ld-0001`, has 19 tracks
   pointing at v31), Delete document is **refused** with a message naming the tracks, shown at the
   picker, and the document stays.
5. **A deleted document's id is not reused.** After deleting a document, New mints the next id, never
   the deleted one.
