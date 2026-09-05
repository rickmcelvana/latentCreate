# T-512: strip the model catalog to the curated installable list

**Follows:** T-511 (five curated image profiles landed). The 2026-09-05 catalog pivot (PROJECT.md
decisions log): comfy-mcp cannot auto-install an arbitrary gallery model, so the browse-the-whole-
gallery UI is removed and the catalog becomes the **curated installable list** already rendered by
the Setup **Models step**.
**Dir:** `app/src` + `src-tauri` | **Lane:** Aider — mostly deletion, plus two surgical edits to
shared files and one small grouping change. Architect updates ARCHITECTURE/README at commit.

## The one hazard: two flows share code — cut only the gallery one

The gallery **adopt** ("Bring in", T-505d) and **import-your-own-workflow** (T-313) both flow through
`state/import.ts`, `components/RoleMapping.tsx`, and the backend `import::save_imported_profile`.
**Import-your-own-workflow stays** — it is ARCHITECTURE 5b's bring-your-own valve. So this is not a
blanket delete of "import" code; it removes the **gallery** entry points and leaves the file-import
path whole. The map, verified:

| Piece | Fate |
|---|---|
| `components/ModelCatalog.tsx` | **delete** (gallery browse — only Setup renders it) |
| `state/catalog.ts` + `state/catalog.test.ts` | **delete** (browse store; used only by ModelCatalog) |
| `bridge/catalog.ts` | **delete** (`catalogBrowse`/`catalogReadiness`/`catalogAdoptBegin` + gallery-only types) |
| `src-tauri/src/catalog.rs` | **delete** (whole module: `catalog_browse`/`catalog_readiness`/`catalog_adopt_begin` + `adopt_from_fetched` + its tests) |
| `components/RoleMapping.tsx` | **keep** (ImportWorkflow uses it) |
| `bridge/import.ts`, `import::import_workflow`, `import::save_imported_profile`, `emit_profile` | **keep** |
| `components/ImportWorkflow.tsx`, `state/import.ts` | **keep, minus the adopt pieces** (below) |
| mcp-bridge `search_templates`/`get_template`/`fetch_template` wrappers | **keep** (generation still fetches `template` profiles via `fetch_template`; the browse *command* is what goes, not the bridge surface) |

## Files to change

### Delete outright (five)
- `app/src/components/ModelCatalog.tsx`
- `app/src/state/catalog.ts`
- `app/src/state/catalog.test.ts`
- `app/src/bridge/catalog.ts`
- `src-tauri/src/catalog.rs`

### `src-tauri/src/lib.rs` — drop the module and its commands
- Remove `mod catalog;` (line ~12).
- Remove the three handler registrations: `catalog::catalog_browse`, `catalog::catalog_readiness`,
  `catalog::catalog_adopt_begin` (lines ~73–75). Leave `import::import_workflow` and
  `import::save_imported_profile` registered.

### `app/src/state/import.ts` — remove the adopt flow (surgical)
Cut exactly these, leave everything else (`begin`, `toggle`, `setName`, `save`, `reset`, the pure
`roleRows`/`canSave`/`mappingsOf`/`initialSelection`):
- the import: `import { catalogAdoptBegin } from '../bridge/catalog'`
- the interface field `adopting: string | null` and its doc comment
- the interface method `adopt: (name: string, title: string) => Promise<void>` and its doc comment
- in the store body: the `adopting: null` initialiser; `adopting: null` inside `begin`'s first `set`
  (leave `set({ phase: { kind: 'importing' } })`); `adopting: null` inside `reset`; and the entire
  `adopt: async (name, title) => { … }` method.

### `app/src/components/ImportWorkflow.tsx` — remove the adopt-in-progress branch
- Remove `const adopting = useImportStore((s) => s.adopting)`.
- Remove the whole `if (adopting !== null) { return ( … "Bringing in a model on the Setup screen.
  Finish there first." … ) }` block. The remaining `phase.kind === 'idle'` / mapping render is the
  file-import UI and is unchanged.

### `app/src/state/import.test.ts` — drop the catalog mock and adopt tests
- Remove `import { catalogAdoptBegin } from '../bridge/catalog'` and the
  `vi.mock('../bridge/catalog', …)` block.
- Remove the two adopt tests ("seeds the name from the gallery title…", "falls back to the row name
  when the gallery has no title…") and the `emptyReport()` helper if it is used only by them.
- Keep every file-import / `roleRows` / `canSave` / `mappingsOf` / seed-warning test. If a shared
  `beforeEach` reset references `adopting`, drop that key too.

### `app/src/views/Setup.tsx` — remove the catalog, split the Models step by kind
1. Remove `import { ModelCatalog }` and the `<ModelCatalog />` render (it sits between `<ModelsStep />`
   and `<LlmStep />`).
2. In `ModelsStep`, the profiles list is currently `curatedFirst(view.profiles)` rendered flat. With
   two audio and five image profiles it now needs an **Audio | Image split**. Group by
   `profile.kind` into two labelled sections, each `curatedFirst`-sorted, each rendering the existing
   `<ModelRow>`. Only render a section when it has rows. Reference:

   ```tsx
   const profiles = view === null ? [] : curatedFirst(view.profiles)
   const music = profiles.filter((p) => p.kind === 'music')
   const image = profiles.filter((p) => p.kind === 'image')
   // …inside the panel, after the offline note:
   {music.length > 0 ? (
     <div className="model-group">
       <h3 className="model-group-title">Music models</h3>
       {music.map((p) => <ModelRow key={p.id} profile={p} />)}
     </div>
   ) : null}
   {image.length > 0 ? (
     <div className="model-group">
       <h3 className="model-group-title">Image models</h3>
       {image.map((p) => <ModelRow key={p.id} profile={p} />)}
     </div>
   ) : null}
   ```
   `curatedFirst` already sorts within each kind (shipped-then-ready), so filtering its output keeps
   that order. Add minimal `theme.css` for `.model-group` / `.model-group-title` (a section heading;
   match the existing `setup-step` type scale — no new colour tokens).

## Keep working (the acceptance spine)
- **Import a workflow** on the Audio Studio still opens the file picker, maps roles, and saves a
  profile (the T-313 flow, untouched but for the dead adopt branch).
- **Generation** of a `template`-based profile still fetches its template — do not touch the
  generation pipeline or the mcp-bridge template wrappers.
- The **Models step** installs any shipped profile one-click, now under Music / Image headings.

## Docs (architect handles at commit, not Aider)
ARCHITECTURE §10 step 3, §10a (the whole gallery-catalog design), the §5b "second entry point /
adopt" extension, and the §264 "Undeclared" caveat (resolved by T-507b) are rewritten to describe
the curated-installable Models step; README's Model-catalog bullet and the "one image profile"
line (§235, now five) are corrected. Left out of the Aider lane because they are prose the executor
should not invent.

## Gate & acceptance
- `npm run gate` green. Expect the frontend test count to **drop** (catalog.test.ts removed and the
  two adopt tests gone) — that is correct, not a regression; note the new number in the commit.
- oxlint/tsc find no dangling reference to `bridge/catalog`, `state/catalog`, `ModelCatalog`,
  `catalogAdoptBegin`, or `adopting`.
- `cargo` builds with `catalog.rs` gone and no unused-import warnings in `lib.rs`.
- Producer click-through (dev build): (1) Setup shows **no gallery browser**; the Models step lists
  Music and Image models under headings, each with Install/Installed. (2) Installing an image model
  still works (T-511 proven). (3) Audio Studio → **Import a workflow…** still imports a `.json`,
  maps roles, and saves a usable profile. (4) Nothing on Setup references "browse the gallery" or a
  "Bring in" button any more.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-512-brief.md --read app/src/components/ImportWorkflow.tsx --read app/src/components/RoleMapping.tsx --read app/src/bridge/import.ts --file app/src/views/Setup.tsx --file app/src/state/import.ts --file app/src/state/import.test.ts --file app/src/components/ImportWorkflow.tsx --file app/src/components/ModelCatalog.tsx --file app/src/state/catalog.ts --file app/src/state/catalog.test.ts --file app/src/bridge/catalog.ts --file app/src/theme.css --file src-tauri/src/lib.rs --file src-tauri/src/catalog.rs
```

(The deletions: Aider empties/removes `ModelCatalog.tsx`, `catalog.ts`, `catalog.test.ts`,
`bridge/catalog.ts`, `catalog.rs`. If the executor cannot delete a file, empty it and the architect
removes it before committing.)
