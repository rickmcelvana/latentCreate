# T-505c — Model catalog: curated one-click install on a row

**Lane: Aider.** A thin cross-language lane: one new `ProfileStatus` field (Rust + its TS mirror),
one pure join helper (TS), a one-line store guard (TS), and the catalog row wiring (TS). **Depends:**
T-505b landed (`ModelCatalog.tsx`, `state/catalog.ts`). **Dirs:** `src-tauri/src`, `app/src`.
**This lane has a producer click-through.**

**Files to create/modify:**

- `src-tauri/src/models.rs` — add `template: Option<String>` to `ProfileStatus`, populate it in
  `row(...)`, one test.
- `app/src/bridge/models.ts` — mirror the field: `template: string | null` on `ProfileStatus`.
- `app/src/state/catalog.ts` — add the pure `curatedIndex(view)` join helper.
- `app/src/state/catalog.test.ts` — tests for `curatedIndex`.
- `app/src/state/models.ts` — one-line guard on `refresh` so a duplicate mount-time refresh collapses.
- `app/src/state/models.test.ts` — one test for the guard.
- `app/src/components/ModelCatalog.tsx` — render the curated treatment (pill + license + Install +
  progress) on a row whose gallery `name` matches a shipped profile.

---

## Goal

A **curated** catalog row — a gallery template the app ships a verified profile for — gains a
one-click **Install** and shows real readiness ("Installed" / "Not installed" + Install), reusing the
Models step's existing install path unchanged. A **bare** gallery row (no shipped profile) keeps the
T-505b behaviour exactly: lazy `local_check` readiness, no button. Nothing new is downloaded from
prose and no new backend command is added — the install seam already exists.

## The join, verified live (this is the whole lane)

A shipped profile already carries the gallery template name it rides:
`ComfySpec.template` (e.g. `profiles/ace-step-1.5-turbo.json` → `"audio_ace_step1_5_xl_turbo"`,
`profiles/minimax-music-3.json` → `"audio_minimax_music_3"`). Those strings are the **exact** `name`
field the catalog's browse returns — verified live 2026-09-03 against the running gallery:

- `search_templates(query="ace step", type="audio")` → a row with `"name": "audio_ace_step1_5_xl_turbo"`.
- `search_templates(query="minimax music", type="audio")` → `"name": "audio_minimax_music_3"`.

So the curated join is: **a catalog row is curated iff some loaded profile's `comfy.template`
equals the row's `name`.** That profile carries the `id` the installer needs. The one gap is that
`ProfileStatus` (the Tauri view row) does not currently surface `template`, so the frontend can't
make the match. This lane adds that one field and joins on it.

In the **Audio** kind there are 9 ACE-family gallery rows but only **two** curated rows in total
(`audio_ace_step1_5_xl_turbo` + `audio_minimax_music_3`); every other audio row and **every** image
row is bare. That 2-of-many split is the click-through's visible proof.

## The one correctness rule (do not break it)

**A curated row's readiness comes from the profile (`ProfileStatus.readiness`), NEVER from
`catalog_readiness` / `local_check`.** `local_check` answers "can this exact template run here", and
it reports a fully-installed MiniMax as `runnable: false` over a filename the profile's
`slot_overrides` already corrects (the MiniMax lesson, `MCP-SURFACE §6`; it is why `models.rs`
refuses `local_check` in the first place). So when a row is curated the catalog must show the
**profile-derived** pill via `rowFor(profile.readiness)` and **must not** fire the
`IntersectionObserver` `local_check` for it. Bare rows are unchanged and still use `local_check`.

## Reuse, don't rebuild

The Models step's `ModelRow` (`Setup.tsx`) already does exactly the install UX a curated row needs,
all from the **singleton** `useModelsStore`:

- pill from `rowFor(profile.readiness)`,
- Install button → `install(profile.id)`, `disabled={installing !== null}`,
- live progress from `installView(progress)` while `installing === profile.id`.

Reuse those verbatim. Because the store is a singleton shared with the Models step, an install
started from the catalog **is** the same install the Models step shows — one transfer in flight, both
surfaces reflect it, and `installing !== null` blocks a second concurrent install across both. When
the install finishes, `install`'s own `finally` calls `refresh()`, so both surfaces flip to
"Installed" automatically. **No new store, no new Tauri command, no new install code.**

Do **not** extract a shared component out of `ModelRow`. The curated catalog row lives inside the
gallery `<li>` (which already renders title/description/tags) and shows a compact pill + license +
next-step + Install + progress — not the per-file missing list, which stays in the Models step. All
the *logic* is already shared through the store's pure helpers (`rowFor`, `installView`) and the
`install` action; only a little wiring JSX differs, and it sits in a different container. The
non-extraction is deliberate.

## Spec

### 1. `src-tauri/src/models.rs` — surface the template name

Add the field to `ProfileStatus` (near `source` / `vram_gb_min`), documented as the join key:

```rust
    /// The gallery template this profile rides, when it rides one
    /// (`ComfySpec.template`). The model catalog joins on it: a gallery row whose
    /// `name` equals this is the same model, so the catalog shows this profile's
    /// readiness and install instead of the row's own `local_check`. `None` for a
    /// profile that uses an imported workflow rather than a gallery template.
    pub template: Option<String>,
```

Populate it in `row(...)` — read it before the struct moves the other `profile` fields:

```rust
    ProfileStatus {
        id: profile.id,
        display_name: profile.display_name,
        kind: profile.kind,
        license: profile.license,
        license_notes: profile.license_notes,
        source,
        vram_gb_min: profile.comfy.vram_gb_min,
        template: profile.comfy.template,
        readiness,
    }
```

Add one test beside the others:

```rust
    /// Protects: the row carries its gallery template name, the key the model
    /// catalog joins a gallery row to a shipped profile on. A drift here (or a
    /// dropped field) silently un-curates every catalog row -- it falls back to
    /// `local_check`, resurrecting the MiniMax `runnable: false` bug.
    #[test]
    fn test_a_row_carries_its_gallery_template() {
        let ace = row(profile(ACE_STEP), ProfileSource::Shipped, None);
        assert_eq!(ace.template.as_deref(), Some("audio_ace_step1_5_xl_turbo"));

        let minimax = row(profile(MINIMAX), ProfileSource::Shipped, None);
        assert_eq!(minimax.template.as_deref(), Some("audio_minimax_music_3"));
    }
```

Nothing else in `models.rs` changes. Do not touch `local_check`, `take_inventory`, or the readiness
derivation — this is one added field.

### 2. `app/src/bridge/models.ts` — mirror the field

On the `ProfileStatus` interface, add:

```ts
  /**
   * The gallery template this profile rides (Rust `ComfySpec.template`), or null
   * for an imported-workflow profile. The model catalog joins a gallery row to a
   * profile on this: a row whose `name` equals it is the same model.
   */
  template: string | null
```

### 3. `app/src/state/catalog.ts` — the pure join helper

Add (import the two types at the top from `../bridge/models`):

```ts
import type { ModelsView, ProfileStatus } from '../bridge/models'
```

```ts
/**
 * Index the shipped/user profiles by the gallery template they ride, so the
 * catalog can find a profile for a gallery row by its `name`.
 *
 * A profile with no `template` (an imported-workflow profile) is not in the
 * gallery and is skipped. A curated row is one whose `name` this map has: it
 * shows the profile's readiness and install, never the row's own `local_check`
 * (the MiniMax lesson -- MCP-SURFACE 6). Pure so the join is testable without a
 * bridge or a running ComfyUI.
 */
export function curatedIndex(view: ModelsView | null): Map<string, ProfileStatus> {
  const index = new Map<string, ProfileStatus>()
  if (view === null) return index
  for (const profile of view.profiles) {
    if (profile.template !== null) index.set(profile.template, profile)
  }
  return index
}
```

Leave the rest of `state/catalog.ts` (the browse/readiness store, `verdictFor`, `rowViewFor`)
untouched.

### 4. `app/src/state/catalog.test.ts` — test the join

Add a `describe('curatedIndex', ...)` block. No mocks needed — it is pure over a plain `ModelsView`
literal. Cover:

- Two profiles with distinct templates → the map has both, keyed by template, valued by the profile.
- A profile with `template: null` is skipped.
- `null` view → empty map.

Build minimal `ProfileStatus` literals inline (a `readiness: { state: 'ready' }` is enough — the
helper only reads `template`). Keep to the existing file's style.

### 5. `app/src/state/models.ts` — collapse the duplicate refresh

`ModelCatalog` now also needs the models view, so it will call `refresh()` on mount like `ModelsStep`
does. Two mount-time refreshes are two `models_status` round-trips. Guard the store's `refresh` so
the second collapses:

```ts
  refresh: async () => {
    if (!isTauri() || get().busy) return
    set({ busy: true })
    try {
      set({ view: await modelsStatus() })
    } finally {
      set({ busy: false })
    }
  },
```

(`get` is already in scope — the store is `create<ModelsState>((set, get) => ...)`.) The Retry button
is already `disabled={busy}`, so this changes no user-visible behaviour; it only dedupes the two
components' mount effects.

Add one test to `state/models.test.ts`:

- Assert a `refresh()` while `busy` is already true is a no-op (does not call the bridge a second
  time). Follow the file's existing `vi.mock('../bridge/models')` setup; if the file mocks
  `modelsStatus`, assert its call count stays 1 across two overlapping refreshes. Keep it minimal and
  in the existing style — if the setup makes the busy-window hard to hold open, a single assertion
  that `refresh` returns early when `busy` is pre-set (via the store's `setState`) is enough.

### 6. `app/src/components/ModelCatalog.tsx` — render the curated treatment

Additions (keep everything else from T-505b exactly as it is):

- New imports:
  ```ts
  import { useMemo } from 'react'  // add to the existing 'react' import
  import type { ProfileStatus } from '../bridge/models'
  import { curatedIndex } from '../state/catalog'   // add to the existing catalog import
  import { installView, rowFor, useModelsStore } from '../state/models'
  ```
- In `ModelCatalog`, drive the models view and build the join:
  ```ts
  const modelsView = useModelsStore((s) => s.view)
  const refreshModels = useModelsStore((s) => s.refresh)
  useEffect(() => {
    void refreshModels()
  }, [refreshModels])
  const curated = useMemo(() => curatedIndex(modelsView), [modelsView])
  ```
- When mapping rows, pass the matched profile (or `undefined`) down:
  ```tsx
  {page.rows.map((row) => (
    <CatalogRow key={row.name} row={row} profile={curated.get(row.name)} />
  ))}
  ```
- `CatalogRow` takes `profile?: ProfileStatus`. **When `profile` is defined, render the curated
  branch and do NOT set up the IntersectionObserver / `local_check` at all** (skip that effect and
  the `verdict`/`rowViewFor` bare-row block). When `profile` is undefined, the existing T-505b
  bare-row body is unchanged.

  Curated branch content, inside the same `<li className="catalog-row">` (title/description/tags from
  the gallery row stay as they are; the pill in `catalog-row-head` comes from the profile, and the
  license + next-step + Install + progress are added below the tags):

  ```tsx
  function CatalogRow({ row, profile }: { row: TemplateInfo; profile?: ProfileStatus }) {
    if (profile !== undefined) return <CuratedRow row={row} profile={profile} />
    // ...unchanged T-505b bare-row body (observer + verdict + reasons)...
  }

  /** A gallery row the app ships a profile for: readiness and one-click install
   *  come from the profile via the shared models store, never local_check. */
  function CuratedRow({ row, profile }: { row: TemplateInfo; profile: ProfileStatus }) {
    const install = useModelsStore((s) => s.install)
    const installing = useModelsStore((s) => s.installing)
    const progress = useModelsStore((s) => s.progress)

    const view = rowFor(profile.readiness)
    const active = installing === profile.id
    const live = active ? installView(progress) : null

    return (
      <li className="catalog-row">
        <div className="catalog-row-head">
          <span className="catalog-row-title">{row.title || row.name}</span>
          <span className={`status-pill status-pill-${view.tone}`}>{view.label}</span>
        </div>

        {row.description !== '' ? <p className="catalog-row-desc">{row.description}</p> : null}

        {row.tags.length > 0 ? (
          <div className="catalog-tags">
            {row.tags.map((tag) => (
              <span key={tag} className="catalog-tag">{tag}</span>
            ))}
          </div>
        ) : null}

        {/* Shown wherever a model is installed -- some weights are open with
            conditions the user takes on by generating (CONVENTIONS). */}
        <p className="model-row-license">
          <span className="model-row-license-name">{profile.license}</span>
          {profile.license_notes !== null ? ` -- ${profile.license_notes}` : null}
        </p>

        {view.nextStep !== null && !active ? (
          <p className="setup-next-step">{view.nextStep}</p>
        ) : null}

        {live !== null ? (
          <p className="setup-next-step">
            Downloading {live.done} of {live.total} files
            {live.percent === null ? '' : ` -- ${live.percent}%`}
            {live.failed.length > 0 ? ` -- ${live.failed.length} failed` : ''}
          </p>
        ) : null}

        {profile.readiness.state === 'missing' && profile.readiness.installable ? (
          <div className="setup-actions">
            <button
              type="button"
              className="setup-button setup-button-primary"
              onClick={() => void install(profile.id)}
              disabled={installing !== null}
            >
              {active ? 'Downloading...' : 'Install'}
            </button>
          </div>
        ) : null}
      </li>
    )
  }
  ```

No new CSS — every class above (`catalog-*`, `status-pill*`, `model-row-license*`, `setup-next-step`,
`setup-actions`, `setup-button*`) already exists from T-505b / the Models step.

## Acceptance criteria

- [ ] `npm run gate` green (Rust field + TS).
- [ ] In **Audio**, the `audio_ace_step1_5_xl_turbo` and `audio_minimax_music_3` rows show a
      profile-derived pill matching the Models step (on the dev machine: MiniMax "Installed",
      ACE-Step "Not installed" + an **Install** button). Every other audio row and every image row is
      a bare row with the T-505b `local_check` pill and no button.
- [ ] A curated row's pill comes from the profile, never `local_check`: MiniMax reads **Installed**,
      not "Not installed", despite its template failing `local_check` on the fp16/int8 pin.
- [ ] Clicking **Install** on the ACE-Step curated row starts the same download the Models step's
      Install starts, both surfaces show the same progress, and both flip to "Installed" when it
      finishes. (Do not run the full 18.5 GiB install in the click-through — start it, confirm both
      surfaces go to "Downloading..." with progress, then cancel/stop is fine.)
- [ ] A curated row with an Install button shows the model's licence line (CONVENTIONS).
- [ ] Only the seven listed files change.

## Producer click-through (after the gate)

1. Setup → **Model catalog**, Audio. The two curated rows show profile pills matching the **Models**
   step above; the other audio rows keep the plain `local_check` pill. No console errors.
2. The MiniMax row reads **Installed** (not "Not installed") — the profile verdict, not `local_check`.
3. Click **Install** on the ACE-Step curated row → it and the Models step's ACE-Step row both read
   "Downloading..." with the same file count/percent. (Stop it before the full 18.5 GiB lands.)
4. Toggle **Image** → every row is bare (no image profile ships yet), pills resolve on scroll as in
   T-505b, no Install buttons.
5. Stop ComfyUI, Retry the Models step → curated catalog rows read "Cannot check" (profile `Unknown`),
   never "Not installed".

## Out of scope (T-505d, T-506)

- **Adopt an installed *bare* gallery row into a profile** — the T-313 import path (T-505d). This
  lane only installs models the app already ships a profile for.
- **An image curated entry** — none ships yet; T-506 adds the first image profile, which then appears
  here curated for free.
- **Paging past the first page** of gallery rows.
- **Extracting a shared install component** out of `ModelRow` — deliberately not done (see Reuse).

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505c-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read app/src/views/Setup.tsx --file src-tauri/src/models.rs --file app/src/bridge/models.ts --file app/src/state/catalog.ts --file app/src/state/catalog.test.ts --file app/src/state/models.ts --file app/src/state/models.test.ts --file app/src/components/ModelCatalog.tsx
```

`Setup.tsx` is `--read`: it is the reference for the exact install UX to reuse (`ModelRow`), and it
must not change — the catalog reuses the store, it does not move the Models step. All seven files
this lane edits are `--file`.
```
