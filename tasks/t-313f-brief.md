# T-313f — the import view

**Lane: architect-direct.** The last part of T-313.

**Depends:** T-313a–e (all landed). **Crate/dir:** `app`.

**Files to modify:**

- `app/src/components/ImportWorkflow.tsx` — **new**
- `app/src/views/AudioStudio.tsx` — mount it in the profile-picker section
- `app/src/theme.css` — the styles it needs

## Why

Five landed tasks and no way in. `import_workflow` stores and inspects, `suggest_roles` ranks,
`save_imported_profile` emits, `place_working_copy` runs the result, and `state/import.ts` holds
every decision — reachable only by hand-editing JSON.

## The dependency question, settled before briefing

`@tauri-apps/plugin-dialog` is **already wired end to end**: the npm package, `tauri-plugin-dialog`
in `Cargo.toml`, and `dialog:default` in `capabilities/default.json`. Nothing in `app/src` has ever
called it, so this is its first use — but it needs no plumbing, which is the difference between this
task being a view and being a view plus a plugin install.

## Where it lives

**In the profile-picker section of the Audio view.** An imported workflow *becomes a model profile*,
so the place to offer the import is where a person is already choosing a model. Not the Setup
wizard: importing is not first-run configuration, it is something a user does when they have a graph
they want to use, which is usually later.

## Spec

`<ImportWorkflow>` renders `useImportStore` and **derives nothing** — `roleRows`, `canSave` and
`phase` already exist for exactly this reason (phase file: a value derived in JSX here is a defect
nothing in the gate can see).

Per phase:

- **`idle`** — one button, "Import a workflow…". Under it, a line of copy stating the cost the owner
  decision imposes: **the app keeps a copy, so later edits in ComfyUI do not follow**. This is the
  one place a person can learn that before it surprises them, and it must not be omitted to keep the
  screen tidy.
- **`importing` / `saving`** — a busy line naming what is happening.
- **`mapping`** — the name field, then `roleRows`. Each row shows its label, then its candidates as
  checkboxes with the **`reason`** beside each. A row with no candidates shows its `emptyNote`.
  Warnings, if any, appear as advisory text that never disables Save. Save is enabled by `canSave`.
- **`saved`** — a confirmation naming the profile, and **the models view is refreshed** so the new
  profile appears in the picker above without a reload. `useModelsStore.refresh()` is the existing
  call that does this.
- **`failed`** — the message, and a way back to `idle`.

The file picker is `open()` from `@tauri-apps/plugin-dialog`, filtered to `json`, `multiple: false`.
A cancelled dialog returns `null` and must leave the phase at `idle` — not `failed`. Cancelling is
not an error, and this is the same class of mistake as reporting a cancelled *job* as failed, which
this project has already made once (MCP-SURFACE 21, `TerminalOutcome::Cancelled`).

**The reason text is not optional.** T-313c generates it precisely so a person can check a guess
about their own graph — `109.value` reading "drives 3.seed, 94.seed" is confirmable at a glance, and
without it the mapping screen asks for trust it has not earned.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] The component derives nothing: no `.filter`, `.map` over suggestions, or conditional beyond
      rendering what the store already decided. Frontend test count should barely move — as with
      T-310b, **a flat count is what proves the split held**.
- [ ] Cancelling the file dialog leaves the view in `idle` with no error shown.
- [ ] The copy-not-reference cost is stated on screen.

## Manual verify (producer click-through)

This carries **three deferred click-throughs** — T-313b, T-313d and T-313e have never had a caller.

1. Audio view → **Import a workflow…** → pick a `File > Save (As)` export. The mapping screen
   appears.
2. **The seed row is offered but not ticked**, showing a reason like "drives 3.seed, 94.seed", while
   tags/lyrics/duration are ticked. *This is the whole of T-313c and T-313e in one glance* — if the
   seed row is pre-ticked, the confidence field is decoration.
3. Duration shows **two** ticked slots on an ACE-Step-shaped graph.
4. Name it, Save. It appears in the profile picker **without a reload**.
5. Select it and generate. A track lands in the Library.
6. Now the negative paths: import a `File > Export (API)` export (refused, names the menu item), and
   cancel the file dialog (nothing happens, no error).
7. Check `%APPDATA%\com.latentbeats.create\workflows\` holds the copy and `profiles\` the profile.

**Step 5 is the Phase 3 milestone line** "an imported user workflow generates successfully".

## Out of scope

- **Editing or deleting an imported profile or workflow.** Its own task; the files are in two known
  directories and deleting by hand works.
- **Re-import to pick up ComfyUI edits.** The owner decision makes re-import the mechanism.
- **Mapping roles beyond the seven.** T-313c's list.
- **A slot browser** for mapping something the suggester did not offer. Real, and worth having only
  once the click-through shows the suggestions are not enough.

## Changed during implementation

**The CSS tokens in the first draft were invented.** `--space-2`, `--border-subtle`,
`--surface-raised`, `--radius-sm`, `--font-sm` and six others do not exist; `theme.css` uses
`--gap-sm`, `--border`, `--panel-hover`, `--radius` and a literal font size. **The gate passed
anyway** — an undefined CSS custom property resolves to nothing and fails silently, so `tsc`,
oxlint, 310 tests and `vite build` were all green while the panel would have rendered with no
padding, no borders and no background.

Caught by grepping every `var(--…)` in the new block against the `:root` definitions rather than by
anything in the gate. Worth recording as a standing check: **`theme.css` is the one file where the
gate proves nothing**, and this is the second class of defect this phase that only a person looking
at the screen would otherwise have found.

Frontend stays at **310**, which is what proves the split held — the same evidence T-310b used.
