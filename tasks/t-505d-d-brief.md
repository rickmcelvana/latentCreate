# T-505d-d — "Bring in": adopt a gallery row into a profile

**Lane: Aider.** The frontend half of adopt: a ready **bare** catalog row gets a "Bring in" button
that drives `catalog_adopt_begin` and shows the existing role-mapping screen inline, ending in
`save_imported_profile`. **Depends:** T-313 (import UI), T-505d-a (the seam), T-505d-b (emit accepts
image graphs), T-505d-c (the prompt maps to the right encoder) — all landed.
**Dir:** `app`. **This lane has the click-through**, the first in the chain: Flux.2 Klein 9B is
installed and every backend piece is in place.

**Files to modify:**

- `app/src/state/import.ts` — an `adopt` action and the `adopting` field that says which row owns
  the flow.
- `app/src/components/RoleMapping.tsx` — **new.** The in-progress import screen, lifted verbatim out
  of `ImportWorkflow` so two surfaces can render it.
- `app/src/components/ImportWorkflow.tsx` — keeps the file picker, renders `RoleMapping`.
- `app/src/components/ModelCatalog.tsx` — the "Bring in" button and the inline mapping screen.
- `app/src/state/import.test.ts` — store tests for the new action.

**No backend, no bridge, and no CSS changes.** `catalogAdoptBegin` already exists in
`app/src/bridge/catalog.ts`; every class name used here already exists.

---

## Goal

A gallery row the user can already run, but which the app ships no profile for, currently offers
nothing at all — the readiness pill says "Installed" and the row stops there. This lane makes that row
adoptable in place: **Bring in** → the same mapping screen a file import shows → Save → a profile in
the Models step. The whole flow is the T-313 one; nothing new is being designed except where it
appears and which row owns it.

## The two decisions this lane makes

### 1. Bare rows adopt; curated rows install

`CuratedRow` already has the one-click **Install** (T-505c) and a shipped profile. Adopting one would
emit a *second*, worse profile for a model the app already describes. So **"Bring in" belongs on
`BareRow` only**, and only when its verdict is `ready` — the row is `not_ready` or `unknown`
otherwise, and `catalog_adopt_begin` would be refused at validation anyway (that refusal is the
backstop; the button is the affordance).

### 2. The row that started the flow owns the screen

`ImportWorkflow` is mounted in **AudioStudio**; `ModelCatalog` is in **Setup** — different views over
one singleton store. Without a marker, a file import started in the studio would make a catalog row
sprout a mapping screen for an unrelated workflow, and vice versa. So the store records **which row
started it**, and each surface renders the screen only for its own flow.

## Spec — `app/src/state/import.ts`

Add to `ImportState`:

```ts
  /**
   * The gallery row `name` whose "Bring in" started this flow, or `null` for a
   * file import.
   *
   * `ImportWorkflow` (Audio Studio) and the catalog step (Setup) render the
   * same singleton store from different views. Without this, a file import
   * would draw a mapping screen under a catalog row it has nothing to do with.
   */
  adopting: string | null
  /** Adopt a gallery row: fetch it through the import path and map its inputs. */
  adopt: (name: string, title: string) => Promise<void>
```

Initial state `adopting: null`. Then:

```ts
  adopt: async (name: string, title: string) => {
    // One import flow at a time. A second adopt would replace the report the
    // user is still mapping, losing their ticks with nothing on screen saying
    // so. The buttons are disabled too; this is the guard that matters.
    if (get().phase.kind !== 'idle') return
    set({ phase: { kind: 'importing' }, adopting: name })
    try {
      const report = await catalogAdoptBegin(name)
      set({
        phase: { kind: 'mapping' },
        report,
        selected: initialSelection(report.suggestions),
        // **Not `report.workflow_id`.** `catalog_adopt_begin` fetches to a temp
        // file named `latentcreate-adopt-<row>.json`, so `workflow_id` is the
        // slug of *that* -- and `emit_profile` derives the profile id from the
        // display name, so seeding it would produce a model called
        // `latentcreate-adopt-image-flux2-text-to-image-9b`. The gallery title
        // is what the user just clicked.
        name: title.trim() !== '' ? title : name,
      })
    } catch (e) {
      // `adopting` deliberately survives a failure, so the message lands on the
      // row the user clicked rather than in the studio's import panel.
      set({ phase: { kind: 'failed', message: String(e) } })
    }
  },
```

Import `catalogAdoptBegin` from `../bridge/catalog`.

Two edits to existing actions:

- `begin` must `set({ ..., adopting: null })` on entry — a file import is not an adopt, and a stale
  marker from an earlier adopt would hand the screen to a catalog row.
- `reset` must clear `adopting: null` alongside the rest.

Nothing else in the store changes: `initialSelection`, `roleRows`, `canSave`, `saveNotes`,
`mappingsOf` and `save` are reused exactly as they are. `save` already calls
`saveImportedProfile(report.workflow_id, …)`, which is correct — the *workflow* is stored under that
id; only the display name comes from the title.

## Spec — `app/src/components/RoleMapping.tsx` (new)

Move, **without behaviour changes**, everything `ImportWorkflow` renders once a flow has started: the
`importing`/`saving` busy line, the `failed` message + Back, the `saved` confirmation + a reset
button, and the mapping screen (name field, warnings, `roleRows` list, `saveNotes`, Save/Cancel).
Also move the `useEffect` that calls `refreshModels()` on `phase.kind === 'saved'` — both surfaces
need a saved profile to appear in the Models step without a reload.

```tsx
/**
 * The import flow once it has started: busy, failed, saved, or the role-mapping
 * screen. Rendered by `ImportWorkflow` for a file the user picked and by the
 * catalog step for a gallery row being brought in -- one store, two entry
 * points, one screen.
 *
 * Renders `state/import.ts` and derives nothing: `roleRows`, `canSave` and
 * `saveNotes` exist for exactly this reason.
 */
export function RoleMapping({ savedLabel }: { savedLabel?: string })
```

Returns `null` when `phase.kind === 'idle'`. `savedLabel` is the one difference between the two
callers: the sentence shown after a successful save. Default (the studio's wording, unchanged):
`It is in the list above.` The catalog passes its own — see below. Keep the existing `reset` button on
the saved and failed branches; keep its label `Import another` when `savedLabel` is not given and
`Done` when it is.

## Spec — `app/src/components/ImportWorkflow.tsx`

It keeps the picker and delegates the rest:

- Read `adopting` from the store.
- When `adopting !== null`, a gallery adopt owns the store: render the picker **disabled** with one
  line — `Bringing in a model on the Setup screen. Finish there first.` Do **not** render
  `RoleMapping`; that screen belongs to the catalog row.
- Otherwise render the picker when `phase.kind === 'idle'`, and `<RoleMapping />` when it is not.
- The `pick()` helper, the cancel-is-not-an-error comment, and the copy-not-reference note are
  unchanged.

## Spec — `app/src/components/ModelCatalog.tsx`

Only `BareRow` changes, plus the stale component doc.

In `BareRow`, add from the import store: `adopt`, `adopting`, and `phase`. Then, after the
missing-files list:

```tsx
      {/* Only a row this install can already run, and only when no other import
          flow is open -- the store refuses a second one, and a live button that
          silently does nothing is worse than a disabled one. */}
      {view !== null && verdict !== 'checking' && verdict.kind === 'ready' && adopting === null ? (
        <div className="setup-actions">
          <button
            type="button"
            className="setup-button setup-button-primary"
            onClick={() => void adopt(row.name, row.title)}
            disabled={phase.kind !== 'idle'}
          >
            Bring in
          </button>
        </div>
      ) : null}

      {/* The mapping screen, under the row it belongs to. */}
      {adopting === row.name ? (
        <RoleMapping savedLabel="It is in the Models step above." />
      ) : null}
```

Note the `verdict !== 'checking'` guard: `readiness[name]` is a `CatalogVerdict | 'checking'`, so
narrow before reading `.kind`.

Rendering `RoleMapping` as a child is fine for the rules of hooks — it is a component, not a branch in
`BareRow`'s own hook order. `BareRow`'s new store reads are unconditional, as they must be.

Update the component doc comment on `ModelCatalog`, which still says rows carry no action button —
untrue since T-505c and doubly so now. State what is there: curated rows install, ready bare rows are
brought in.

## Tests — `app/src/state/import.test.ts`

The file currently tests pure functions only; add a `describe` for the store, mocking both bridges the
way `catalog.test.ts` does (`vi.mock('../bridge/catalog', …)` and `vi.mock('../bridge/import', …)`),
and resetting `useImportStore.setState` in `beforeEach`.

- **`it('seeds the name from the gallery title, not the temp workflow id')`** — the headline. `adopt`
  with a report whose `workflow_id` is `latentcreate-adopt-image-flux2-text-to-image-9b` and a title
  `Klein 9B: Text to Image` leaves `name === 'Klein 9B: Text to Image'`. This is what stops the saved
  profile being *named and ided* after a temp file.
- **`it('falls back to the row name when the gallery has no title')`** — empty title → `name` is the
  row name.
- **`it('marks the row that owns the flow')`** — `adopting` is the row name after `adopt`, and
  `phase.kind === 'mapping'`.
- **`it('keeps the row marked when the adopt fails')`** — a rejecting `catalogAdoptBegin` leaves
  `phase.kind === 'failed'` **and** `adopting` still set, so the message lands on the row.
- **`it('refuses a second adopt while a flow is open')`** — with `phase` already `mapping`, `adopt`
  does not call the bridge and does not change `adopting`.
- **`it('clears the adopt marker on a file import and on reset')`** — `begin` and `reset` both leave
  `adopting === null`.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] A ready bare row shows **Bring in**; a `not_ready`, `unknown` or still-checking row does not; a
      curated row still shows **Install** and never **Bring in**.
- [ ] Clicking it opens the mapping screen **under that row**, with the name seeded from the gallery
      title.
- [ ] Saving writes a profile and refreshes the Models step.
- [ ] While a flow is open, every other row's button is disabled and the studio's picker says where
      the flow is.
- [ ] Only the five files listed change.

## Click-through (producer)

Klein 9B is installed, so this runs end to end.

1. **Setup → Model catalog → Image.** Search `klein`. The row **Klein 9B: Text to Image** reads
   **Installed** and now carries a **Bring in** button.
2. Click **Bring in**. The mapping screen opens *under that row* (not in Audio Studio).
3. Check the mapping — this is what T-505d-c bought:
   - **Name** reads `Klein 9B: Text to Image`. **If it reads `latentcreate-adopt-…`, that is the
     defect this lane's headline test exists for.**
   - **Style tags** ticked → `75/74.text`, and `75/67.text` is **not** offered here.
   - **Negative prompt** ticked → `75/67.text`, its reason mentioning the negative conditioning.
   - **Seed** → `75/73.noise_seed`; **Steps** → `75/62.steps`; **CFG** → `75/63.cfg`.
   - **Lyrics** and **Duration (s)** read "No input in this workflow looks like this."
     **Expected, not a defect** — the role list is deliberately the same for every model so a person
     can see what was *not* matched. Worth noting if it reads badly on an image model; a kind-aware
     role list is a later question, not this lane's.
4. **Save as a profile.** It confirms with the new profile id and says it is in the Models step.
5. Scroll up to **Models**: the new image profile is listed.
6. Watch-items:
   - The catalog row still reads **Installed** and stays **bare** — an adopted profile is
     workflow-backed with `template: null`, so it does not join the curated index. **Expected**; see
     out-of-scope below.
   - Open **Audio Studio** mid-flow (between steps 2 and 4): the import panel should say the flow is
     on Setup, not offer a picker that would clobber it.
   - Cancel on the mapping screen returns the row to just its **Bring in** button.

## Out of scope

- **Marking an adopted row "adopted" across a reload.** The saved line is live state only. Doing it
  properly means the profile carrying which gallery row it came from — a backend field, not a UI
  trick, and not needed to use the model. Noted so the bare row after adoption is not read as a bug.
- **A kind-aware role list** (hiding Lyrics/Duration for an image model). See step 3.
- **A width/height role.** Klein exposes `75/62.width|height`, `75/66.width|height` and the
  `PrimitiveInt` pair driving them; there is no dimensions role and adding one is a T-506 decision.
- **Generating from the adopted profile.** T-506 owns the image pipeline.
- **Any backend, bridge or CSS change.**

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read tasks/t-505d-d-brief.md --read WORKFLOW.md --read CONVENTIONS.md --read app/src/bridge/catalog.ts --read app/src/bridge/import.ts --read app/src/state/catalog.ts --file app/src/state/import.ts --file app/src/components/RoleMapping.tsx --file app/src/components/ImportWorkflow.tsx --file app/src/components/ModelCatalog.tsx --file app/src/state/import.test.ts
```

`RoleMapping.tsx` does not exist yet — pass it as a `--file` so Aider creates it.
