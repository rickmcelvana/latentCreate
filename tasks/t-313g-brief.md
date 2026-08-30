# T-313g — two defects the first emitted profile showed

**Lane: architect-direct.** Two small fixes from T-313f's click-through.

**Depends:** T-313f (landed). **Crate/dir:** `create-core`, `app`.

## Why

T-313f's click-through passed all seven steps, and then **reading the emitted profile** — the first
one this app has ever produced — showed two things no step asked about. Both are wrong *by default*,
which is what matters for a flow whose entire job is a good default.

## 1. `cfg_scale` is not `cfg`

The emitted profile mapped one `cfg` control to **two unrelated parameters**:

```json
"cfg": { "slots": ["3.cfg", "94.cfg_scale"] }
```

`3.cfg` is the KSampler's diffusion CFG. `94.cfg_scale` is the **LM planner's** sampling scale. The
shipped ACE-Step profile settles this beyond argument: it puts `cfg_scale` inside an **advanced
`planner` group** alongside `temperature`, `top_p`, `top_k` and `min_p`, and does not map top-level
`cfg` at all. They are different knobs on different nodes.

T-313c's name table lists `cfg_scale` as a synonym for `cfg`, so both matched `Strong` and both were
pre-ticked. **Fix:** drop `cfg_scale` from `Role::Cfg`'s names. A user who wants the planner's scale
can still reach it — just not by having one slider silently drive both.

`guidance` stays: it is the same concept under a different name on other models, and nothing has
shown otherwise.

## 2. A profile with no seed makes "variations" meaningless

The producer left the seed row unticked, which is exactly right — on an ACE-Step-shaped graph the
seed is *always* the `Possible` hop (`109.value`), and T-313e is deliberate that a `possible`
candidate is never ticked for someone.

The consequence was not thought through: **an imported profile with no seed input has no seed
control, and T-312's "queue N variations by seed" then queues N runs varying nothing.** It does not
error. The tracks differ anyway, because ACE-Step is not reproducible run-to-run (MCP-SURFACE 17.3),
so nothing on screen would ever reveal it.

Since the default path always produces this, the flow has to say so.

**Fix:** a `saveNotes(selected)` in `state/import.ts` returning advisory lines shown above Save.
Seed unmapped is the one that matters and the only one this task adds.

**Not fixed by pre-ticking the seed.** That would re-introduce exactly the silent-guess failure
T-313e exists to prevent. The answer is to say what the choice costs, not to make it for them.

## Spec

```rust
// roles.rs
Role::Cfg => &["cfg", "guidance"],
```

```ts
// state/import.ts
/** Advisory lines shown above Save. Never disable it. */
export function saveNotes(selected: Selection): string[]
```

Seed unmapped yields:
`"No seed mapped, so Variations will queue identical settings. Tick a seed row to change that."`

Render them in `<ImportWorkflow>` above the Save button, styled like the existing warnings, and
**never** consult them in `canSave` — for the same reason warnings do not block saving.

## Acceptance criteria

- [ ] `npm run gate` green.
- [ ] **`test_cfg_does_not_claim_the_planner_scale`** (create-core) — ACE-Step's `Cfg` candidates
      contain `3.cfg` and **not** `94.cfg_scale`. The invariant: one control never silently drives
      two unrelated parameters.
- [ ] **`test_an_unmapped_seed_is_called_out`** — `saveNotes` names the consequence when seed is
      absent, and is empty when it is present.
- [ ] **`test_notes_never_block_saving`** — `canSave` is true with notes outstanding.
- [ ] Mutation: restoring `cfg_scale` to the synonym list must fail a test.

## Out of scope

- **Grouping the planner inputs** the way the shipped profile does. `InputSpec::Group` exists, but
  deciding which of a stranger's inputs belong together is a different problem from naming one.
- **Pre-ticking the seed.** See above.
- **Warning about any other unmapped role.** Seed is the only one with a feature silently attached
  to it.
