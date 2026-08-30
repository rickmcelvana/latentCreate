# T-313e — the import data path and its store

**Lane: architect-direct.** **The view is T-313f**, and the split is the one every UI task this
phase has used (T-308b/c, T-309a/b, T-310a/b, T-311c/e) for the reason the phase file states: every
Phase 2 milestone defect was **correct logic derived inline in a view**, invisible to `tsc`, oxlint
and the whole suite.

**Depends:** T-313a–d (all landed). **Crate/dir:** `create-core`, `src-tauri`, `app`.

**Files to modify:**

- `crates/create-core/src/roles.rs` — `suggest_roles` returns a named, serializable type
- `src-tauri/src/import.rs` — `ImportReport` gains `suggestions`
- `app/src/bridge/import.ts` — **new**: the two commands and their wire types
- `app/src/state/import.ts` — **new**: the store and every decision it makes
- `app/src/state/import.test.ts` — **new**

## Why

Four seams work and nothing joins them. `import_workflow` stores and inspects, `suggest_roles`
ranks, `save_imported_profile` emits, `place_working_copy` runs the result — and the only way to
reach any of it is a hand-written JSON profile.

This task is the join, minus the pixels.

## The one invariant that must survive the trip

T-313c decided that a candidate reached by **following a link** is `Possible`, never `Strong`,
because nothing about `109.value` says "seed" — it is right because of the graph's shape. The whole
point of that distinction is the **pre-tick rule**: `Strong` is checked for the user, `Possible` is
offered for them to confirm.

That rule lives here. `create-core` can only label; the store is where the label either becomes
behaviour or is quietly lost. **If this task pre-ticks everything, T-313c's confidence field becomes
decoration** and the app silently accepts a graph-shape guess as the user's seed mapping.

It is worth being blunt about the failure: the user clicks Save, the profile is written, generation
works, and the seed they think they set is the one the app guessed. Nothing errors.

## Spec

### 1. Suggestions reach the report

`suggest_roles` currently returns `Vec<(Role, Vec<Candidate>)>`. A tuple serializes as a positional
array, which is a poor wire type and a worse one to read in TS. Change it to:

```rust
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RoleSuggestion {
    pub role: Role,
    pub candidates: Vec<Candidate>,
}
```

`ImportReport` gains `suggestions: Vec<RoleSuggestion>`, filled inside `import_into` — which already
holds both the graph and the slots at that moment, so it costs no extra round trip and no second
read of the stored file.

### 2. `app/src/bridge/import.ts`

Wire types mirroring the Rust, and two calls: `importWorkflow(source)`, `saveImportedProfile(...)`.
Nothing else. Mirror comments in the house style (`/** Mirrors Rust ... */`).

### 3. `app/src/state/import.ts` — every decision

```ts
export type ImportPhase =
  | { kind: 'idle' }
  | { kind: 'importing' }
  | { kind: 'mapping' }
  | { kind: 'saving' }
  | { kind: 'saved'; profileId: string }
  | { kind: 'failed'; message: string }
```

Pure functions, each tested:

- **`initialSelection(suggestions)`** — every `Strong` candidate's address checked; **no `Possible`
  one, ever**. A role whose only candidates are `Possible` starts **empty**, which is the honest
  state: we found something, and we are not claiming it.
- **`roleRows(suggestions, selected)`** — one row per role, carrying label, the candidates with
  their `reason` and checked state, and whether the role is mapped at all. The view renders this and
  derives nothing.
- **`toggleAddress(selected, role, address)`** — a role holds a **set** of addresses, because
  ACE-Step's duration legitimately needs two.
- **`canSave(phase, name, selected)`** — a non-empty trimmed name **and** at least one mapped role.
  A profile with no inputs is a picker entry that can do nothing.
- **`warningsFor(report)`** — passed through for display, and **never** a reason `canSave` is false
  (MCP-SURFACE 29.3).

Role labels reuse nothing from Rust; they are UI copy. Keep them the same words `emit::label_for`
uses so the mapping screen and the generated panel agree.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_a_possible_candidate_is_never_pre_selected`** — the headline, built from a suggestion
      list shaped like ACE-Step's real one (`Seed` offering only `109.value` as `Possible`). Assert
      `Seed` starts **unselected** while `Tags` starts selected. The invariant: T-313c's confidence
      field is behaviour, not decoration.
- [ ] **`test_every_strong_candidate_starts_selected`** — including a role with **two** `Strong`
      candidates (ACE-Step's duration), where both are ticked. The one-role-many-slots case.
- [ ] **`test_a_role_with_no_candidates_is_still_a_row`** — shown as unmapped rather than hidden, so
      a person can see what the app could not find.
- [ ] **`test_save_needs_a_name_and_at_least_one_mapping`** — all four combinations.
- [ ] **`test_warnings_never_block_saving`** — a report carrying `edge_type_mismatch` warnings still
      saves. Pairs with T-313b's Rust-side test; this is the same rule on the other side of the wire,
      and the place it would most plausibly be re-imposed by accident.
- [ ] **`test_toggling_an_address_adds_and_removes_it`** — and does not disturb other roles.
- [ ] Mutation: pre-selecting `Possible` candidates must fail a test; letting `canSave` ignore the
      name must fail a test.
- [ ] The store derives nothing in a component — there is no component yet, which is the point.

## Out of scope

- **The view.** T-313f: `<ImportWorkflow>`, the file picker, and where the entry point lives.
- **Click-throughs.** T-313b's and T-313d's are still owed and still have no caller. They join
  T-313f's list.
- **Editing or deleting an imported profile or workflow.** Its own task.
- **Re-import to pick up ComfyUI edits.** The owner decision makes re-import the mechanism; a
  first-class "update this profile" flow is a later task if it is wanted.

## Changed during implementation

**1. `suggest_roles` returns `Vec<RoleSuggestion>`** rather than tuples, as specified — which meant
editing T-313c's tests. Worth it: the tuple was an internal convenience and a poor wire type.

**2. A `Slot` wire type had to be defined**, not imported. The brief assumed one existed frontend
side; nothing in `app/src/` had ever needed slots before. It rides on the report unread by the
store, so a later "show me every slot" escape hatch needs no second command.

**3. `canSave` takes no warnings argument at all.** The brief said warnings must never block saving;
making the parameter absent is what turns that from a rule someone must remember into a change
someone would have to make deliberately.

Two mutations, two killed. Pre-selecting `possible` candidates fails **two** tests — the pre-tick
one and the row-state one — which is the right shape for the invariant this task exists to carry.

Frontend 299 -> 310.
