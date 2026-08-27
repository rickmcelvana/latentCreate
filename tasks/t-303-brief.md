# T-303: persist `default_profile_id`, and give it a picker

**Depends:** T-302b | **Crate/dir:** `app` (frontend only)
**Files to modify:**
- `app/src/state/profiles.ts` *(new)*
- `app/src/state/profiles.test.ts` *(new)*
- `app/src/bridge/profiles.ts`
- `app/src/views/AudioStudio.tsx`
- `app/src/views/LyricsStudio.tsx`
- `app/src/theme.css`

## Goal

The user picks which model profile they are writing and generating for, and the choice
persists. Today `default_profile_id` is **read and never written**: `LyricsStudio.tsx:50`
does `configured ?? DEFAULT_PROFILE_ID` against a config field nothing in the app sets, so
every user silently gets `ace-step-1.5-turbo` whether it suits them or not.

Fourth instance in this repo of a value with no writer -- after T-212's LLM config, and the
two before it. The pattern is the same and so is the fix: **assert the file on disk, not the
store.**

## Spec

### 0. No backend change

`models_status` already returns every profile with the fields a picker needs (`id`,
`display_name`, `kind`, `license`, `license_notes`, `source`, `vram_gb_min`, `readiness`),
`save_config` already persists `default_profile_id`, and `useModelsStore` already fetches and
caches the list. **Do not touch any `.rs` file, and do not add a second fetch** -- the wizard's
models step and this picker must read one source, or they will disagree about what is
installed.

### 1. The effective profile is a selector, not a fallback buried in a view

New `app/src/state/profiles.ts`. This is the T-302b/T-301b move again, and the reason is on
the first page of the phase file: a decision derived in JSX is one no test can reach.

```ts
/** The profile used when none has been chosen -- the app's default model. */
export const DEFAULT_PROFILE_ID = 'ace-step-1.5-turbo'

/**
 * Which profile the studios are working against.
 *
 * The configured id wins. The default is what a user who has never opened the
 * picker gets, not a value they are stuck with.
 */
export function effectiveProfileId(config: Config | null): string {
  const stored = config?.default_profile_id ?? null
  return stored !== null && stored.trim() !== '' ? stored : DEFAULT_PROFILE_ID
}

/**
 * The profiles this picker offers, in the order it offers them.
 *
 * `kind` is a parameter rather than a hardcoded `'music'` because CoverArt
 * (Phase 5) wants the same list filtered the other way, and two nearly
 * identical filters is how they drift.
 */
export function pickable(view: ModelsView | null, kind: ProfileStatus['kind']): ProfileStatus[] {
  return curatedFirst((view?.profiles ?? []).filter((p) => p.kind === kind))
}

/**
 * The configured profile, when it is still one of the loaded ones.
 *
 * `null` while the list has not loaded, and **also** when the configured id
 * names a profile that is no longer there -- a user profile deleted from disk,
 * or a shipped one renamed. The caller must say so rather than quietly
 * substituting the default: silently swapping the model a user chose is the
 * same fault as carrying a stale verified fact (T-302b), one level up.
 */
export function selectedProfile(
  view: ModelsView | null,
  config: Config | null,
): ProfileStatus | null {
  const id = effectiveProfileId(config)
  return (view?.profiles ?? []).find((p) => p.id === id) ?? null
}

/** One picker row, as the view renders it. */
export interface ProfileRow {
  id: string
  displayName: string
  /** Never null. Users ship these tracks commercially (T-111, CONVENTIONS). */
  license: string
  licenseNotes: string | null
  /** "Shipped" / "Yours" -- a user profile is otherwise indistinguishable. */
  origin: string
  /** The profile's own claim, worded as a claim. Null when undeclared. */
  vramClaim: string | null
  readiness: RowView
}

/**
 * Describe one row.
 *
 * A selector rather than JSX because it is the only way the licence rule is
 * testable here: this repo runs vitest in `node` with no DOM, so "assert the
 * licence reached the screen" is not a test that can be written (T-301b
 * learned this the expensive way). Putting the fields in a value moves the
 * rule somewhere a test reaches, and leaves the view a dumb renderer.
 */
export function profileRow(profile: ProfileStatus): ProfileRow {
  return {
    id: profile.id,
    displayName: profile.display_name,
    license: profile.license,
    licenseNotes: profile.license_notes,
    origin: profile.source === 'shipped' ? 'Shipped' : 'Yours',
    vramClaim:
      profile.vram_gb_min === null ? null : `Profile states ${profile.vram_gb_min} GB VRAM`,
    readiness: rowFor(profile.readiness),
  }
}
```

Move `DEFAULT_PROFILE_ID` here from `app/src/bridge/profiles.ts` and update that import in
`LyricsStudio.tsx`. A constant about *app state* does not belong in the bridge layer beside
the `invoke` wrappers.

### 2. Replace the derivation in `LyricsStudio.tsx`

Line 50 is currently:

```tsx
const configured = useConfigStore((state) => state.config?.default_profile_id ?? null)
const profileId = configured ?? DEFAULT_PROFILE_ID
```

Both lines become one selector call. **The Lyrics Studio's behaviour must not otherwise
change** -- it still loads the guide for whatever profile is effective, and it still needs no
running ComfyUI to do it (2026-08-25 decision: the brief's `language` is a writing
instruction, not a slot value, precisely so lyrics do not depend on the audio service).

### 3. The picker in `AudioStudio.tsx`

It replaces the `No generations yet. Finish Setup to enable audio.` panel, above `<JobQueue />`.
Each row carries:

- **`display_name` and the licence.** Non-negotiable, and not a footnote: users ship these
  tracks commercially, the two shipped profiles have materially different terms, and licence
  text comes from the profile and never from the download host (T-111, and the 2026-08-25
  decision). `license_notes` shows where present.
- **`source`** -- a user profile and a shipped one are not obviously different otherwise.
- **`vram_gb_min`** where declared. ⚠ Show it as the profile's own claim, not as a verdict:
  `ace-step-1.5-turbo` says 8 GiB against a 9.3 GiB DiT and the figure is the repo's oldest
  open question. Do not compute a "will this run" judgement from it.
- **Readiness**, via the existing `rowFor` helper.

⚠ **Readiness is information, never a gate.** `models_status` needs a running ComfyUI to
answer it (MCP-SURFACE §14.1) and degrades to `inventory_available: false` with every row
`unknown`. **Every profile stays selectable in that state.** A user must be able to choose
their model and go write lyrics with the audio service down -- disabling the list when
ComfyUI is stopped would make choosing a model depend on the thing the choice is *for*.
`inventory_available: false` earns one line saying readiness could not be checked, which the
wizard already words.

Selecting a row saves immediately:

```ts
await useConfigStore.getState().save({ default_profile_id: id })
```

⚠ **The T-301b shallow-merge rule does not apply here.** `default_profile_id` is a top-level
field of `Config`, so a one-key patch is exactly right. The rule was about `llm`, a nested
object that gets replaced wholesale. Do not carry the whole config through "for safety" --
that would reintroduce the clobbering it was written to prevent.

### 4. Say when the configured profile has gone

When `selectedProfile` returns `null` but the list has loaded, say that the configured profile
is not among the loaded ones, and have the user pick.

⚠ **Corrected at review: do not word this as a fallback.** An earlier draft of this section
said to name both ids and report falling back to the default. **No fallback happens** --
`effectiveProfileId` returns the configured id whether or not a profile answers to it, so
generation fails on that id rather than quietly using another model. Promising a fallback the
app does not perform is worse than saying nothing, and when the missing profile *is* the
default the sentence names the same id twice.

No auto-repair and no silent rewrite of config either: the user chose that profile and
deserves to be told their choice is unavailable rather than to discover a different model
generated their track.

### 5. `theme.css`

Rules for every new class (WORKFLOW §4.5), existing tokens only. The picker is a list of
selectable rows -- `.llm-models` / `.llm-model` in the wizard's LLM step is the closest
existing pattern and worth matching rather than inventing a second look.

## Acceptance criteria

- [ ] `effectiveProfileId` tested: configured id wins; `null` and `''` fall back to the
      default.
- [ ] `pickable` tested: filters by `kind`, and orders by the existing `curatedFirst` rule.
- [ ] `selectedProfile` tested: returns the row when present, **`null` when the configured id
      is not in the list** (the case §4 exists for), and `null` before the list has loaded.
- [ ] `profileRow` tested: **`license` is non-empty for every shipped profile**, `origin`
      distinguishes shipped from user, and `vramClaim` is `null` when undeclared and worded as
      a claim when not. This is the testable form of "every row shows a licence" --
      **do not write a rendering test**: vitest runs in `node` here, there is no jsdom and no
      `@testing-library/react`, and reaching for one means inventing three dependencies
      (T-301b). The view renders `profileRow`'s fields and holds no logic of its own.
- [ ] `npm run gate` clean; no `.rs` file changed.
- [ ] No changes outside the listed files.

**Producer click-through** (the persistence half is invisible to the gate -- this is the T-212
lesson, and this task is the fourth instance of that bug):
- [ ] Pick a profile, then **open `config.json` and confirm `default_profile_id` is written.**
      Not the UI showing it selected -- the file.
- [ ] Restart the app: the choice is still there, and the Lyrics Studio's prefills follow it.
- [ ] **Stop ComfyUI and open the picker:** every profile is still listed and still
      selectable, with readiness unknown rather than the list being disabled.

## Out of scope

- No param panel, no slot values, no LoRA stack -- that is **T-308** and **T-309**. This is
  the picker and the persistence, nothing downstream of the choice.
- Do not add install buttons. Installing lives in the wizard's models step, which already has
  it; a second install path is a second thing to keep correct.
- Do not filter, hide or disable profiles by `vram_gb_min`. The number is unverified (the
  repo's oldest open question) and Phase 3's milestone is the first thing that can settle it.
- Do not touch `models_status`, `useModelsStore.install`, or the wizard.
- Do not change what the Lyrics Studio does with the profile, only where it gets the id.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-303-brief.md --read app/src/bridge/models.ts --read app/src/bridge/config.ts --read app/src/state/models.ts --read app/src/state/config.ts --file app/src/state/profiles.ts --file app/src/state/profiles.test.ts --file app/src/bridge/profiles.ts --file app/src/views/AudioStudio.tsx --file app/src/views/LyricsStudio.tsx --file app/src/theme.css
```

`state/models.ts` is `--read` because the new selectors call `curatedFirst` and the view calls
`rowFor`; `bridge/models.ts` for `ProfileStatus`/`ModelsView`; `state/config.ts` and
`bridge/config.ts` for `save` and `Config`. None of them changes.

`bridge/profiles.ts` **is** `--file`: `DEFAULT_PROFILE_ID` moves out of it (§1), which is one
deletion. It is listed as editable so the executor never has to stop and ask for it -- the
last two runs each lost a round trip to a file the brief had left out.
