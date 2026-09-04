# T-506d: the Cover Art view

**Depends:** T-506c-a/b/c (the config field, the two read commands, the panel factory, the submit
store, the listing store)
**Dir:** `app/src` | **Lane:** Aider — one view, two small extractions so it does not become a second
copy of the Audio Studio, one selector for the states it can be in, and the stylesheet.

**This lane carries the click-through.** It is where the two production lines with no unit test
behind them — the download-directory choice and the two-event dispatch, both in
`src-tauri/src/jobs.rs` `ingest_if_pending` — are finally proved by running the app.

**Files to create/modify (seven, plus one test file):**
- `app/src/state/paramPanel.ts` — export a `ParamPanelStore` type; nothing else changes
- `app/src/components/ParamPanel.tsx` — takes the store as a prop
- `app/src/components/ProfilePickerRow.tsx` — **new**, moved out of `AudioStudio.tsx` verbatim
- `app/src/components/GenerateArtBar.tsx` — **new**
- `app/src/views/AudioStudio.tsx` — passes the store, imports the moved row
- `app/src/views/CoverArt.tsx` — the view, replacing today's placeholder
- `app/src/state/profiles.ts` — `imageStudioState` and `imageStudioNote`
- `app/src/state/profiles.test.ts` — the five states
- `app/src/theme.css` — the gallery

## Goal

Pick an image model, set the prompt and seed, press Generate, watch the job in the shared queue, and
see the finished cover appear in a grid **without a reload** — in a view that keeps its own panel
state, so switching to Audio and back does not disturb either.

## Spec

### 1. `ParamPanel` takes its store as a prop

`ParamPanel.tsx` imports `useParamPanelStore` directly today. T-506c-b made the store a factory with
two instances precisely so the two studios could hold different profiles at once; the component is
the last thing pinning it to one.

In `paramPanel.ts`, add one line beside the two instances:

```ts
/** The type of one panel store, so a component can take either instance. */
export type ParamPanelStore = ReturnType<typeof createParamPanelStore>
```

`ParamPanel` becomes `export function ParamPanel({ store }: { store: ParamPanelStore })`, and every
`useParamPanelStore((s) => ...)` inside it becomes `store((s) => ...)`. **Nothing else in the file
changes** — not `ParamField`, not one line of JSX. `MAX_SAFE_SEED` keeps coming from `paramPanel`.
`AudioStudio.tsx` renders `<ParamPanel store={useParamPanelStore} />`; Cover Art passes
`useArtPanelStore`.

### 2. `ProfilePickerRow` moves to its own file

It is defined at the bottom of `AudioStudio.tsx` today. Cover Art needs the same row, and it is the
component that renders the **licence** — CONVENTIONS' rule that users ship this work commercially,
so the licence is never off-screen. A second copy is where that stops being true in one of the two
studios.

Move it to `app/src/components/ProfilePickerRow.tsx` **verbatim**, with one added prop:

```ts
export function ProfilePickerRow({
  row,
  selected,
  group,
  onSelect,
}: {
  row: ProfileRow
  selected: boolean
  /** The radio group's `name`. Two pickers exist; only one is mounted at a
   *  time, but a shared name would silently make them one group if that ever
   *  changed. */
  group: string
  onSelect: () => void
})
```

`name="profile"` becomes `name={group}`. `AudioStudio` passes `group="profile"` (the value it uses
today, so its DOM is unchanged); Cover Art passes `group="image-profile"`.

### 3. `state/profiles.ts` — which of five states Cover Art is in

Cover Art has more empty states than the Audio Studio, because **the app ships no image profile**:
a first visit can have nothing chosen *and* nothing to choose. Those read differently and need
different sentences, and a sentence assembled inside JSX is a sentence no test in this repo can
reach (vitest runs in `node`, no DOM — T-301b, and `profileRow`'s comment says the same). So the
decision is a value:

```ts
/**
 * What Cover Art can say for itself right now.
 *
 * `loading` -- the profile list has not come back.
 * `no-profiles` -- it came back with no image profiles at all; nothing to pick.
 * `none-chosen` -- image profiles exist and the user has not chosen one. There
 *   is no default to fall back on (see `effectiveImageProfileId`), so this is a
 *   real state rather than a moment before one.
 * `missing` -- an id is configured that no loaded profile answers to: a user
 *   profile deleted from disk, or renamed. Named rather than silently
 *   re-picked, the same rule the Audio Studio's fallback note follows.
 * `ready` -- a chosen profile is loaded.
 */
export type ImageStudioState = 'loading' | 'no-profiles' | 'none-chosen' | 'missing' | 'ready'

export function imageStudioState(view: ModelsView | null, config: Config | null): ImageStudioState

/**
 * The sentence for a state, or `null` when there is nothing to say.
 *
 * `id` is the configured id, needed only by `missing` -- naming it is what lets
 * a user find the profile they renamed.
 */
export function imageStudioNote(state: ImageStudioState, id: string | null): string | null
```

Sentences, exactly:

- `loading` → `null`
- `no-profiles` → `'No image model profile yet. Bring one in from the model catalog in Setup.'`
- `none-chosen` → `'Pick an image model to start.'`
- `missing` → ``The configured image profile `${id}` is not among the loaded profiles. Pick one below to continue.``
- `ready` → `null`

Build both on the existing `effectiveImageProfileId` / `selectedImageProfile` / `pickable` — do not
re-derive the id or re-filter the list. `no-profiles` is `pickable(view, 'image').length === 0`.

### 4. `components/GenerateArtBar.tsx`

A second component rather than a prop on `GenerateBar`, for the reason `artGenerate.ts` is a second
store: `GenerateBar` reads `useLyricsStore` and renders the approved-lyric offer, which Cover Art
has no document for. Threading a store, a doc and a placeholder through it would put three
permanently-dead props in the one component whose button decides what reaches Rust — and it would
save only JSX, because **every rule the bar enforces is already a shared pure function with a test**:
`blockers`, `canBatch`, `effectiveCount`, `queueingLabel`, `notesFor`, `BATCH_CHOICES`, `GENERATE`.
The duplication is in the untestable layer, and the logic stays single-sourced.

It is `GenerateBar` minus the lyric offer, reading `useGenerateArtStore` and `useArtPanelStore`:

- no `approvedOffer` block, no `useApprovedLyric`, no `doc?.title` fallback — the title input is
  `value={title ?? ''}` and its placeholder is `Untitled — names the artwork`
- `reasons = blockers(profileId, model, values)` from the **art** panel
- `notes = notesFor(last, lastProfileId, profileId, queued)`
- the Variations select, gated on `canBatch(model)`, and the button label
  `queueingLabel(queued, effectiveCount(model, count))`

Class names are the existing `generate-*` ones — this is the same bar, and a parallel set of
`art-generate-*` rules would drift from it in the stylesheet.

### 5. `views/CoverArt.tsx`

Replaces the placeholder. Modelled on `AudioStudio`, with the artwork grid where the Library's track
list would be.

Effects, all with the store function as the dependency the way both existing views do it:

```
startListening()          // useJobsStore  -- the shared queue
refresh()                 // useModelsStore
load()                    // useArtStore   -- the gallery
startListening()          // useArtStore   -- the art://saved subscription
```

and, when `chosenId !== null`, `panel.load(chosenId)` on `[chosenId, load]`. **Guard the null** —
`load` takes a `string`, and there is no default image profile, so `chosenId` really is `null` on a
first run.

Structure:

```
<h1 className="view-title">Cover Art</h1>
<p className="view-subtitle">Artwork for singles and albums, from the same ComfyUI.</p>

<section className="panel profile-picker">
  <h2 className="profile-picker-title">Image model</h2>
  {note !== null ? <p className="profile-picker-fallback">{note}</p> : null}
  {view !== null && !view.inventory_available ? (
    <p className="profile-picker-disclaimer">Readiness could not be checked because ComfyUI is not running.</p>
  ) : null}
  {state === 'no-profiles' ? <button className="profile-picker-setup" ...>Open Setup</button> : null}
  <ul className="profile-list"> ... ProfilePickerRow, group="image-profile",
        onSelect={() => void save({ default_image_profile_id: profile.id })} ... </ul>
</section>

{state === 'ready' ? <ParamPanel store={useArtPanelStore} /> : null}
{state === 'ready' ? <GenerateArtBar /> : null}

<JobQueue names={names} />

<section className="panel art-gallery"> ... </section>
```

- **No `ImportWorkflow`, no `LoraStack`.** Import-to-profile lives in the Audio Studio and Setup; an
  adopted image profile declares no LoRA block, so a stack panel would be an empty box promising a
  feature the profile does not have.
- **The Setup button** is `useNavStore.getState().setView('setup')` — the state's own sentence names
  the catalog, so the button must actually go there.
- **`names`** for `JobQueue` is `Object.fromEntries(rows.map((p) => [p.id, p.display_name]))` over
  the image rows, exactly as `AudioStudio` builds it over the music ones. The queue is shared and
  shows both studios' jobs; a name missing from the map is why an image job would otherwise read as
  its bare profile id.
- **No project picker.** Artwork follows the selected project, and the Library owns the one place
  that switches it; a second picker would be a second thing to keep in step. The gallery reloads on
  a switch because `state/projects.ts` calls `useArtStore.load()` (T-506c-c).

The gallery:

```
{error !== null ? <p className="library-error">{error}<button className="library-retry" ...>Retry</button></p> : null}
{warnings !== null ? <p className="library-warning">{warnings}</p> : null}
{art.length === 0 ? <p className="library-empty">{EMPTY_ART}</p>
                  : <ul className="art-grid">{art.map((row) => <ArtTile key={row.id} row={row} />)}</ul>}
```

`ArtTile` is local to this file:

```tsx
function ArtTile({ row }: { row: ArtRow }) {
  const [broken, setBroken] = useState(false)
  // `row.url` is a resolved path, not a promise the file is there: the backend
  // refuses an escape but does not stat (T-506c-c). A tile that cannot load
  // says so and keeps its facts -- the provenance is the point, and it is
  // still true when the image is gone.
  ...
}
```

Show the image when `row.url !== null && !broken`, else a `.art-missing` placeholder reading
`Image file not found.`. Below it the name, and the recipe facts as a `<dl>` — model, licence, size,
seed, created — reusing the Library's `track-fact` shape so the two read alike.

### 6. `theme.css`

Append a `/* --- Cover Art (T-506d) --- */` section at the end, using the existing tokens
(`--gap-*`, `--radius`, `--panel`, `--border`, `--text-muted`) and no new ones:

- `.art-grid` — `display: grid; grid-template-columns: repeat(auto-fill, minmax(200px, 1fr)); gap: var(--gap-md); list-style: none; margin: 0; padding: 0;`
- `.art-tile` — column flex, `gap: var(--gap-sm)`
- `.art-thumb` — `width: 100%; aspect-ratio: 1; object-fit: cover; border-radius: var(--radius); background: var(--panel);` *(covers are square today, and a fixed box keeps the grid from reflowing as images load)*
- `.art-missing` — the same box, centred muted text, `border: 1px dashed var(--border)`
- `.art-name`, `.art-facts` — following `.track-name` / `.track-recipe`
- `.profile-picker-setup` — the same button shape as `.library-retry`

## Tests — named by the invariant

`profiles.test.ts` only. The rest of this lane is JSX, which this repo cannot test in `node`; that is
the reason the sentences were pulled into `imageStudioNote` in the first place, and the reason the
click-through below is not optional.

- **a loaded list with no image profiles is `no-profiles`, whatever is configured** — including
  when a *music* id is configured, because `default_image_profile_id` and `default_profile_id` are
  separate fields and a music profile in the image slot must not read as `ready`.
- **profiles exist and none is chosen is `none-chosen`, not `no-profiles`** — the two produce
  different sentences and only one of them sends the user to Setup.
- **a configured id no loaded profile answers to is `missing`, and the sentence names it.**
- **a null view is `loading`, and its note is `null`** — an app that has not finished loading must
  not accuse the user of having no models.
- **a chosen, loaded profile is `ready` and says nothing.**

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the listed files
- [ ] `ParamPanel`'s body is unchanged apart from `useParamPanelStore` → `store`
- [ ] `ProfilePickerRow` is unchanged apart from `name="profile"` → `name={group}`
- [ ] `AudioStudio` renders the same DOM it does today
- [ ] no new CSS custom properties

## Out of scope
- **A provenance inspector disclosure on a tile.** The tile shows the recipe summary, as the Library
  card does. When the full inspector lands (T-506e) it must call the existing
  `provenanceView(artwork.provenance)` — it takes a `Provenance` since T-506c-c precisely so a
  second renderer is never written.
- **Attach-to-track, attach-to-album, delete, rename, export, reveal** — T-506e.
- **A size control.** Every size-shaped slot on the Klein profile is inert; the effective address is
  a `PrimitiveInt` the profile does not declare (MCP-SURFACE §35.2). That is a role-suggestion
  problem, not a view problem.
- **The project picker.** See above.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read app/src/components/GenerateBar.tsx --read app/src/views/Library.tsx --read app/src/state/generate.ts --read app/src/state/artGenerate.ts --read app/src/state/art.ts --read app/src/state/nav.ts --read app/src/bridge/models.ts --read app/src/bridge/config.ts --file app/src/state/paramPanel.ts --file app/src/components/ParamPanel.tsx --file app/src/components/ProfilePickerRow.tsx --file app/src/components/GenerateArtBar.tsx --file app/src/views/AudioStudio.tsx --file app/src/views/CoverArt.tsx --file app/src/state/profiles.ts --file app/src/state/profiles.test.ts --file app/src/theme.css
```

## Click-through (producer) — the lane's real acceptance

Run `npm run tauri dev` with ComfyUI available. **Read the files, not only the screen**: a tile on
screen proves the webview resolved a URL, not that the record on disk is right.

1. **Before choosing anything.** Open Cover Art on a config with no `default_image_profile_id`. It
   says either "No image model profile yet…" with a working Setup button, or "Pick an image model to
   start." — and **no param panel and no Generate button**, because there is no model to generate
   with and no default to invent.
2. **Pick Flux.2 Klein 9B.** The panel fills with `tags`, `negative`, `seed`, `steps`, `cfg`, the
   seed already rolled. Setup's model list is untouched by the choice.
3. **Cross-panel independence.** Type a distinctive prompt here, switch to Audio, confirm its tags
   and seed are exactly as left, switch back, confirm Cover Art's prompt and seed are exactly as
   left. *This is what T-506c-b's factory bought; the singleton would have wiped one of them.*
4. **Generate one.** The queue row shows **the image model's display name**, not a bare id. The seed
   in the panel after the click is the seed that ran.
5. **The tile appears with no reload.** *This proves the `Saved::Art` arm of `ingest_if_pending`
   emits `art://saved` — a line no unit test in `src-tauri` reaches, because it needs an `AppHandle`
   no test in that crate builds.*
6. **Check the disk.** In the project directory: the PNG is in `art/`, **not** `tracks/`, named
   `ar-0001.png`, with `ar-0001.json` beside it. *This proves the other untested line: `ModelKind::Image => art_dir`.*
7. **Read the sidecar.** `resolved_slots` names `75/74.text`, `75/67.text`, `75/73.noise_seed`,
   `75/62.steps`; the seed there is the seed on screen; `width`/`height` match the real PNG
   (768x768 on the stock Klein graph — the size slots are inert, so a 1536 typed anywhere is
   expected *not* to take effect).
8. **Batch of 2.** Two queue rows, two different seeds, two files, two tiles, two sidecars.
9. **Project switch.** In the Library, switch projects, return to Cover Art: the gallery is the new
   project's. *This is the reason T-506c-c refused to cache URLs by id — every project has an
   `ar-0001`.*
10. **A broken tile.** Rename one `art/ar-*.png` on disk and reload the view: that tile reads
    "Image file not found." and **keeps its facts**, and the others are unaffected. Rename it back.
11. **The Library is undisturbed.** Tracks, player, albums, the provenance disclosure and "re-use
    these settings" all still work — the `provenanceView` signature changed under them in T-506c-c.

Report which of the eleven passed. A bare "passed" is read as all eleven.
