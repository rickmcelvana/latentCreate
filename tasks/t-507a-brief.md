# T-507a: the empty/degraded-states sweep — every view says what it is waiting on

**Depends:** nothing new. Uses `useConfigStore`, `useNavStore`, `useModelsStore` as they are.
**Dir:** `app/src` | **Lane:** Aider — two views, one new pure selector + its test, and CSS reuse.
**No new backend, no new bridge command.** Everything here is presentation over state that already
exists. If a change here seems to need a new Tauri command or a probe on mount, stop and say so
rather than adding one.

This is the frontend half of T-507. The backend carry-over from T-505d-d's click-through — an
adopted profile reading **"Cannot check / Undeclared"** on the Models step — is a separate lane
(**T-507b**, Rust `emit` + import wiring) and is **out of scope here**. Do not touch `emit.rs`,
`readiness.rs`, or `models.ts`'s `rowFor`.

## The audit that produced this brief

Every view was read against the ARCHITECTURE §10 rule — *"degraded states show as status pills
(e.g. 'ComfyUI offline — reconnect') not modal walls"* — and against the most polished view
(**CoverArt**, T-506), which is the reference:

| View | Cold-start / offline handling today | Verdict |
|---|---|---|
| **Setup** | Status pills + a next step per state; Retry everywhere; "Cannot see which models are installed" when offline. | Complete. No change. |
| **CoverArt** | `imageStudioState`/`imageStudioNote` state machine (`loading`/`no-profiles`/`none-chosen`/`missing`/`ready`), an **Open Setup** button when there is no image profile, an offline disclaimer, and gallery error/warning/empty states. | The reference. No change. |
| **Library** | `EMPTY_LIBRARY`, error+Retry, warnings, project error/warnings; a Create form is always present. | Complete. No change. |
| **AudioStudio** | A fallback note when the configured profile is gone, and the same offline disclaimer as CoverArt. **But at cold start (`view === null`) the profile list is a bare empty `<ul>` with no line saying why.** | **Fix (§2).** |
| **LyricsStudio** | Nothing about the lyrics model at all. The LLM is **optional and skippable** in Setup, so a first-run user very plausibly has none configured — and the view is silent: you fill the brief, press Generate, and only *then* get a red failure banner. | **Fix (§1) — the headline.** |

So this lane is two view changes. The other three views were checked and are left alone; say so in
the commit body.

## Files to create/modify (four)

- `app/src/state/lyrics.ts` — one new exported pure selector (**add only**; touch nothing else)
- `app/src/state/lyrics.test.ts` — its test
- `app/src/views/LyricsStudio.tsx` — the cue + disabled Generate/Optimize
- `app/src/views/AudioStudio.tsx` — the cold-start line
- `app/src/theme.css` — only if a class below is genuinely absent (most are reused)

---

## §1 — LyricsStudio: say when there is no lyrics model

### 1a. The selector (`state/lyrics.ts`)

The signal is **pure config**, not a probe. "Is a lyrics model configured?" is
`config.llm?.model` being a non-blank string. We do **not** probe the endpoint on Lyrics mount:
the Setup step's own comment explains why a view must not re-probe on mount (it spawns work behind
the user's back and can raise a keychain prompt — T-004), and *reachability* of a configured
endpoint is already handled the right way, by the generate error banner. What is missing is only
the **not-configured-at-all** state, and that is a file read.

Mirror `effectiveBaseUrl`'s blank-handling exactly (whitespace-only counts as unset):

```ts
import type { Config } from '../bridge/config'

/**
 * Whether a lyrics model is configured.
 *
 * A pure config read, not a probe: the LLM is optional in Setup, so "none
 * configured" is a real first-run state the Lyrics view must show before a
 * generation fails, not after. Reachability of a *configured* endpoint is a
 * different question, left to the generate error banner. Whitespace-only is
 * unset, matching `effectiveBaseUrl` -- a cleared field must not read as chosen.
 */
export function lyricsModelConfigured(config: Config | null): boolean {
  const model = config?.llm?.model ?? null
  return model !== null && model.trim() !== ''
}
```

### 1b. The test (`state/lyrics.test.ts`)

Follow the file's existing style (a `config` fixture factory if one is there; otherwise a minimal
inline `Config`). Each case names the invariant it protects:

- **null config → false** — *before config loads, the view must not promise a model it cannot
  confirm.*
- **`llm: null` → false** — *the skippable step was skipped.*
- **`llm.model: null` → false** — *an endpoint set but no model chosen is not a usable lyrics model.*
- **`llm.model: '   '` → false** — *whitespace-only is unset (the `effectiveBaseUrl` rule), or a
  cleared field would read as chosen.*
- **`llm.model: 'gemma-4-12b'` → true** — *a real model is configured.*

### 1c. The view (`views/LyricsStudio.tsx`)

Read the flag once, near where `profileId` is derived, subscribing to the derived boolean (not the
whole config object — WORKFLOW §4.10, the pattern already used two lines up for `profileId`):

```tsx
import { useNavStore } from '../state/nav'
import { lyricsModelConfigured } from '../state/lyrics'
// ...
const hasLyricsModel = useConfigStore((state) => lyricsModelConfigured(state.config))
```

**The cue.** Directly under the `<p className="view-subtitle">…`, before `<DocumentPicker />`,
render a status pill + next step + a jump to Setup **only when `!hasLyricsModel`**. Reuse the
existing pill and the `profile-picker-setup` button CoverArt already uses for its "Open Setup"
affordance, so this is not new CSS:

```tsx
{!hasLyricsModel ? (
  <section className="panel profile-picker">
    <header className="setup-step-head">
      <h2 className="profile-picker-title">Lyrics model</h2>
      <span className="status-pill status-pill-warn">No lyrics model</span>
    </header>
    <p className="profile-picker-fallback">
      Lyrics are written by a model you provide, and none is set up yet. You can still
      write and edit lyrics by hand below.
    </p>
    <button
      type="button"
      className="profile-picker-setup"
      onClick={() => useNavStore.getState().setView('setup')}
    >
      Open Setup
    </button>
  </section>
) : null}
```

The wording is deliberate: it is a **status, not a wall** (§10) — the brief form, the document
picker and the hand-editing/versions all stay on screen and usable. The one thing that must not
silently fail is generation.

**Disable the two model actions** while unconfigured — an enabled Generate that only ever produces
the red banner is exactly the silent-until-failure trap this lane removes:

```tsx
// Generate submit button:
disabled={generating || !hasLyricsModel}
// Optimize prompt button (add the clause, keep the rest):
disabled={generating || optimizing || reviewing || accepted || !hasLyricsModel}
```

Leave **Save**, **Check**, Restore/Approve/Delete, and the document picker untouched — none of them
calls the LLM.

> Why a config read and not the llm store's `status`: `useLlmStore.status` is `null` until Setup's
> LlmStep probes, so a user who opens Lyrics first would see nothing. `config.llm.model` is written
> by `choose`/`test` and read straight from `config.json`, so it is correct from the first render
> and needs no network call. This is the same reason the store's own comment (llm.ts `choose`)
> gives for persisting the model rather than holding it in the store.

---

## §2 — AudioStudio: a line at cold start instead of a bare list

AudioStudio always has a shipped default profile (`ace-step-1.5-turbo`), so it has no `no-profiles`
state — a full state machine like CoverArt's would be dead branches here. The one real gap is the
gap between mount and the first `refresh()` returning: `view === null`, `rows` is empty, and the
profile picker renders an empty `<ul>` with no explanation.

Add one line, matching the shape Setup's ModelsStep and CoverArt use. Place it inside
`<section className="panel profile-picker">`, above the existing `selected === null` note:

```tsx
{view === null ? (
  <p className="profile-picker-disclaimer">Checking for installed models…</p>
) : null}
```

`profile-picker-disclaimer` is the class AudioStudio already uses two lines down for the offline
note, so no new CSS. This is the whole of §2 — do not add an `audioStudioState` machine or gate the
ParamPanel/LoraStack/GenerateBar on readiness; audio has a working default and those panels are
correct to show against it.

---

## Out of scope (name them, do not do them)

- **T-507b** — populating `comfy.models` on an adopted/imported profile so the Models step reads
  **Ready** instead of **Cannot check**. That is Rust (`emit.rs`) + import wiring and gets its own
  brief after this lands. Do not touch `emit.rs`, `readiness.rs`, or `rowFor`.
- No probe-on-mount in any view.
- No changes to Setup, Library, or CoverArt.

## Gate & acceptance

- `npm run gate` green (oxlint, tsc strict, vitest, and the Rust side untouched).
- The new selector test covers the five cases above.
- Manual shape check the producer will click through:
  1. With **no lyrics model** configured, Lyrics shows the "No lyrics model" pill + Open Setup;
     Generate and Optimize are disabled; the brief form and editor are still usable by hand.
  2. Open Setup jumps the rail to Setup.
  3. Configure a model in Setup, return to Lyrics: the cue is gone, Generate/Optimize enabled.
  4. Audio at first paint (before models load) shows "Checking for installed models…" rather than
     an empty picker; it clears once the list loads.

## Aider launch

`--read` the three standard docs, this brief, and the four reference files the executor needs to
see but must not edit (CoverArt is the pattern to mirror; `llm.ts` carries the blank-handling rule;
`nav.ts` has `setView`; `config.ts` has the `Config`/`LlmConfig` types). `--file` the five editable
files.

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-507a-brief.md --read app/src/views/CoverArt.tsx --read app/src/state/llm.ts --read app/src/state/nav.ts --read app/src/bridge/config.ts --file app/src/state/lyrics.ts --file app/src/state/lyrics.test.ts --file app/src/views/LyricsStudio.tsx --file app/src/views/AudioStudio.tsx --file app/src/theme.css
```
