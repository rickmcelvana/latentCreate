# T-408d — delete a project

Part **d** of T-408 (delete for every kind of created content), and the last.
Parts a (lyric version), b (lyric document) and c (album) landed and passed their
click-throughs. This part deletes the biggest thing there is — a **whole
project** — and, like c, it takes the pattern somewhere the others do not go.

## What a project is, and why this part is different

A project **is its directory**: `projects/<slug>/` holds `project.json`, the
`lyrics/` documents and the `tracks/` sidecars + audio. There is no outer record
of "which projects exist" — `list_projects` reads the filesystem. So deleting a
project is at once the **most destructive** delete (the tree it trashes holds
every track, sidecar, lyric and album in it) and the **simplest to get right**:

- **One trash call, one folder.** `trash::delete` moves a directory to the
  Recycle Bin whole, so the project lands there as a single restorable folder.
  There is no file-first/record-last ordering to protect — the discipline
  `delete_track` and `delete_doc` need because they trash files *and* edit an
  outer `project.json` — because here the record and the files are the same tree,
  trashed in one move. This is why T-408d is the shortest of the four cores.
- **Existence is checked on the directory, not the record.** `delete_track` and
  `delete_doc` `load_project` first, because they need the record to find files
  and references. `delete_project` must **not** — a project whose `project.json`
  is malformed has to stay deletable, or the one project a user most wants gone
  is the one they cannot remove. Check `project_dir(root, slug)?.exists()`
  instead. `project_dir` still refuses a slug that could escape the root
  (`UnusableName`), so the frontend-supplied slug is whitelisted exactly as
  before; only the *record's validity* is not a precondition.
- **The selection is left alone — on purpose.** Deleting the *selected* project
  leaves `config.default_project_slug` naming a slug that no longer exists.
  `delete_project` does **not** touch config, and neither does the command.
  Both `projectctx::selected_project` (backend) and `effectiveProjectSlug`
  (frontend) already resolve a configured-but-gone slug to the first remaining
  project — the same fallback a hand-edited config hits, and the arm the
  decisions log (2026-08-30) flagged. **T-408d is the first flow to reach that
  arm from a real deletion at runtime.** Reconciling config here would duplicate
  a resolution the app already trusts and couple a filesystem op to the config
  store. Leaning on the existing fallback is the design, not an omission — say so
  in the doc comment so a later reviewer does not "helpfully" add a config write.

`NotFound { kind: "project", .. }` on a slug with no directory, symmetric with
every other delete.

**Deleting the last project is allowed.** There is no refusal for "the only
one", consistent with a/b/c. After it, the next backend command resolves through
`selected_project`'s empty-list arm and recreates `My First Song` — first-run
behaviour, coherent, not an error. (The frontend picker shows nothing selected
until that recreation; acceptable, and out of scope to pre-empt.)

## d-back — `library::projects::delete_project`

Add to `crates/library/src/projects.rs`, next to `load_project` / `save_project`.
It returns the remaining projects (`ProjectSet`, already this module's type) so
the frontend refreshes from the filesystem truth after the tree is gone:

```rust
/// Delete a whole project — its entire `projects/<slug>/` directory to the OS
/// trash. Returns the projects that remain.
///
/// The most destructive delete in the app: the tree trashed holds every track,
/// sidecar, lyric document and the `project.json` itself. It is also the
/// simplest, because a project *is* its directory. There is no outer record to
/// keep in step (`list_projects` reads the filesystem) and no id counter to
/// preserve, so — unlike `delete_track` / `delete_doc` — there is no
/// file-first/record-last order to get wrong: one trash call moves the whole
/// tree, and the project lands in the Recycle Bin as a single restorable folder.
///
/// **Existence is checked on the directory, not the record.** This does not
/// `load_project`: a project whose `project.json` is malformed must still be
/// deletable — reading it first would make the one project the user most wants
/// gone the one they cannot remove. `project_dir` still refuses a slug that
/// could escape the root.
///
/// **The selection is not touched here.** Deleting the *selected* project leaves
/// `config.default_project_slug` naming a slug that no longer exists;
/// `projectctx::selected_project` and the frontend's `effectiveProjectSlug`
/// already resolve that to the first remaining project — the same fallback a
/// hand-edited config hits. Reconciling config here would duplicate a resolution
/// the app already trusts and couple a filesystem op to the config store.
///
/// `trash` is the injected trash operation — production passes
/// [`crate::tracks::trash_to_os`], tests a fake — the shape T-405 established for
/// the one destructive action a test must not really perform.
pub fn delete_project<F>(root: &Path, slug: &str, trash: F) -> Result<ProjectSet, LibraryError>
where
    F: Fn(&Path) -> Result<(), LibraryError>,
{
    let dir = project_dir(root, slug)?;
    if !dir.exists() {
        return Err(LibraryError::NotFound {
            kind: "project",
            id: slug.to_string(),
        });
    }
    trash(&dir)?;
    Ok(list_projects(root))
}
```

Tests (append to the module — reuse the existing `recording_trasher` shape from
`tracks.rs`, or a local fake that `fs::rename`s the dir into a graveyard so a
later `exists()` sees it gone without touching the real Recycle Bin):

1. `test_delete_project_trashes_the_whole_directory_and_hard_deletes_nothing` —
   **the headline, and the CONVENTIONS rule for the one destructive action:**
   create "A", delete it, assert the fake trasher was called **once with the
   project directory** (`projects/a`), and that the directory left via the
   trasher (it is now in the graveyard), **not** via `fs::remove_dir`. Asserts
   the trash call was made, not merely that the dir is gone.
2. `test_delete_project_returns_the_remaining_projects` — create "A" and "B",
   delete **"A"**, assert the returned `ProjectSet.projects` is just `["B"]` and
   a fresh `list_projects` agrees. (Deleting "A" not "B", and reading the list
   back, kills "return before removal" and "trash the wrong dir".)
3. `test_delete_project_leaves_the_other_projects_intact` — after deleting "A",
   assert "B"'s `project.json` still loads (`load_project(root, "b")` ok). Kills
   a mutation that trashes the parent `projects/` directory instead of the slug's.
4. `test_delete_project_of_a_malformed_project_still_deletes` — create a project
   directory by hand with `project.json` = `"{ not json"`, `delete_project` it,
   assert it succeeds and the dir is gone. **Proves existence is checked on the
   directory, not via `load_project`** — kills a mutation that swaps
   `project_dir(..).exists()` for a `load_project(..)` guard.
5. `test_delete_project_of_an_unknown_slug_is_not_found_and_trashes_nothing` —
   `delete_project(root, "nope", fake)` is `NotFound { kind: "project", .. }`
   and the trasher was never called.
6. `test_delete_project_refuses_a_slug_that_escapes_the_root` — `"../secrets"`,
   `"a/b"`, `""` each give `UnusableName` and the trasher is never called
   (the `project_dir` whitelist, reached before existence).

Mutation pass **by hand, against a file-copy backup — never `git checkout`**
(the T-408a trap that once wiped uncommitted work; copy the file to the
scratchpad, restore by copying back): the `dir.exists()` guard (→ `true`, killed
by 5), the `!` on it (killed by 5/1), `project_dir(..).exists()` → a
`load_project` guard (killed by 4), and the return value (`list_projects(root)`
→ an empty vec or the pre-delete list; killed by 2).

## d-cmd — `projects_delete`

Add to `src-tauri/src/projects.rs`, beside `projects_list` / `projects_create`.
It passes the real trasher, exactly as `delete_track` / `lyrics_delete_doc` do:

```rust
/// Delete a whole project — its `projects/<slug>/` tree to the OS trash — and
/// return the projects that remain.
///
/// Never a hard delete (CONVENTIONS): the real trasher is
/// `library::tracks::trash_to_os`. The selection is not written here; deleting
/// the *selected* project leaves `config.default_project_slug` pointing at a
/// gone slug, which `projectctx::selected_project` resolves to the first
/// remaining project — the frontend re-lists and lands on the same one.
#[tauri::command]
pub fn projects_delete(
    config_dir: State<'_, ConfigDir>,
    slug: String,
) -> Result<library::ProjectSet, String> {
    library::projects::delete_project(&config_dir.0, &slug, library::tracks::trash_to_os)
        .map_err(|e| e.to_string())
}
```

Register `projects::projects_delete` in the `invoke_handler!` list in
`src-tauri/src/lib.rs`, beside `projects::projects_create`.

## d-front — the delete affordance

**Bridge** (`app/src/bridge/projects.ts`):

```ts
/** Delete a whole project (its tree to the OS trash). Returns what remains. */
export async function deleteProject(slug: string): Promise<ProjectSet> {
  return await invoke<ProjectSet>('projects_delete', { slug })
}
```

**Store** (`app/src/state/projects.ts`) — mirror the store-held confirm a/b/c
use (one row confirms at a time). Alias the import to avoid the action/bridge
name clash, as albums did:

- import `deleteProject as deleteProjectRequest`
- `confirmingDelete: string | null` (the slug awaiting confirm; `null` when none),
  initialised `null`
- `askDelete(slug)` → `set({ confirmingDelete: slug, error: null })`
- `cancelDelete()` → `set({ confirmingDelete: null })`
- `deleteProject(slug)`:
  - `const projectSet = await deleteProjectRequest(slug)`
  - `set({ projects: projectSet.projects, warnings: projectWarningLine(projectSet.warnings), confirmingDelete: null, error: null })`
  - **reload the library** so tracks refresh for the now-effective project:
    `await useLibraryStore.getState().load()` (the album panel reloads on its own
    — the Library view's `albumsLoad` effect keys on `selected`, which recomputes
    when `projects` changes). Return `true`.
  - catch → `set({ error, confirmingDelete: null })`, return `false`

Do **not** call `select` / write config here: the selection resolves through the
fallback (see d-back). Return the refreshed list from the ProjectSet the command
gives back, not a hand-spliced array — the filesystem is the truth once the tree
is gone.

**View** (`app/src/views/Library.tsx`) — a Delete control in each `ProjectRow`,
in the `project-row-meta` area beside the date. Because this is the most
destructive delete, the confirm copy names the whole scope. New component beside
`ProjectRow`:

```tsx
/** Inline delete: a button that becomes a "Delete 'name'? … Delete / Cancel". */
function ProjectDelete({ slug, name }: { slug: string; name: string }) {
  const confirming = useProjectsStore((state) => state.confirmingDelete === slug)
  const askDelete = useProjectsStore((state) => state.askDelete)
  const cancelDelete = useProjectsStore((state) => state.cancelDelete)
  const deleteProject = useProjectsStore((state) => state.deleteProject)

  if (!confirming) {
    return (
      <button type="button" className="project-row-delete" onClick={() => askDelete(slug)}>
        Delete
      </button>
    )
  }
  return (
    <div className="project-delete-confirm">
      <span className="project-delete-prompt">
        Delete “{name}”? This trashes the whole project — every track, lyric and
        album in it — to the Recycle Bin.
      </span>
      <button type="button" className="project-delete-yes" onClick={() => void deleteProject(slug)}>
        Delete
      </button>
      <button type="button" className="project-delete-cancel" onClick={() => cancelDelete()}>
        Cancel
      </button>
    </div>
  )
}
```

Render `<ProjectDelete slug={row.slug} name={row.name} />` inside
`project-row-meta` (pass `row` through `ProjectRow`, which already has it). CSS:
mirror the album delete look — `.project-row-delete`, `.project-delete-confirm`,
`.project-delete-prompt`, `.project-delete-yes` (danger-toned via `--danger`),
`.project-delete-cancel` — reusing the same tokens as `.album-delete-*`.

**Store tests** (`app/src/state/projects.test.ts`): add `mockDeleteProject`, wire
it into the `vi.mock` for `../bridge/projects`, mock `useLibraryStore.load` (or
assert it is invoked), reset in `beforeEach`, add `confirmingDelete: null` to the
store's reset, and add:
- `askDelete` / `cancelDelete` toggle `confirmingDelete`
- delete adopts the refreshed list (returned ProjectSet), removing the slug, and
  clears the confirm
- delete triggers a library reload
- delete surfaces an error and clears the confirm

## Backend wiring test (`src-tauri/src/projectctx.rs`)

The unit arm `test_selected_project_falls_back_when_the_configured_one_is_gone`
already covers a *simulated* gone slug. Add one that produces the condition the
real way, proving the runtime wiring T-408d introduces:

`test_selected_project_after_deleting_the_configured_project_lands_on_a_sibling`
— create "Alpha" and "Beta", `write_config(Some("beta"))`, delete "beta" via
`library::projects::delete_project(root, "beta", <fake trasher>)`, then assert
`selected_project(root)` returns "alpha". (Use a fake trasher that removes the
dir so the deletion actually takes effect in the tempdir.)

## Click-through (producer, on the desktop app)

1. In the Library, create a project **"Delete Me"** (it becomes selected, its
   track list empty). Add an album to it so its `project.json` is non-trivial.
2. Select a **different** project (e.g. `My First Song`). Delete **"Delete Me"**
   from its picker row — confirm the warning names the whole project; after
   Delete it disappears from the picker, the selection is unchanged, and
   `My First Song`'s tracks are still listed.
3. Create **"Delete Me 2"**, select it (so the *selected* project is the one
   about to go). Delete it. **The app lands on another project with no restart** —
   the picker shows a real selection and that project's tracks load. *(This is
   the milestone: deleting the selected project resolves through the fallback.)*
4. Open the OS Recycle Bin / Trash — the deleted project **folders are there,
   whole and restorable** (`project.json`, `lyrics/`, any `tracks/`), not
   hard-deleted. This is the destructive-action check.
5. Restart the app. The selection is coherent — it lands on a real project with
   no error — confirming the stale `default_project_slug` degrades cleanly.

## Not this task

- Reconciling / clearing `config.default_project_slug` on delete — deliberately
  not done; the fallback chain owns it (see d-back).
- A trash/undo surface inside the app (the OS Recycle Bin is the undo).
- After T-408d, **T-408 closes**; the phase moves to **T-409** (song title
  carried from Lyrics Studio to the export filename), then **T-406** (provenance
  inspector, last).
