# T-507a-2: Lyrics also flags a configured-but-unreachable model (click-through fix)

**Follows:** T-507a (landed `d33b964`, awaiting click-through). This is that click-through's one
finding, fixed the same session.
**Dir:** `app/src` | **Lane:** Aider — extend one selector into a small state machine, its tests,
and the one view that uses it.
**No new backend, no new probe.** The signal is the llm store's **existing** status, which Setup's
LlmStep already probes on mount — and since the app opens on Setup, that status is populated from
launch. Do **not** add a probe to LyricsStudio (the §10 no-mount-probe rule the owner set in Setup).

## What the click-through found

With a lyrics model configured (`config.llm.model = "qwen3.5-27b"`) but **not offered by the
endpoint** (Ollama doesn't have it), T-507a's cue stayed hidden and Generate was enabled — because
`lyricsModelConfigured` only asks "is *a* model named?", not "will it work?". Generate then failed
into a raw error banner with no next step. The owner chose (2026-09-05) to **reuse Setup's probe**:
Lyrics reflects the llm store's status, showing a cue and disabling Generate/Optimize for the states
that cannot work, and falling back to today's behaviour when there is no probe to read.

The five `lyricsModelConfigured` cases from T-507a stay; this adds the reachability layer on top.

## Files to modify (three)

- `app/src/state/lyrics.ts` — the state machine (keep `lyricsModelConfigured`)
- `app/src/state/lyrics.test.ts` — tests for the new functions (keep the existing five)
- `app/src/views/LyricsStudio.tsx` — drive the cue + disabling off the state machine
- `app/src/theme.css` — **no change expected**; the block reuses `panel profile-picker`,
  `setup-step-head`, `profile-picker-title`, `status-pill status-pill-warn`, `profile-picker-fallback`,
  `profile-picker-setup`. Touch it only if a class is genuinely missing.

---

## §1 — the state machine (`state/lyrics.ts`)

Add these near `lyricsModelConfigured`. The blank check moves into a shared private `modelPresent`
so the two entry points cannot drift. Import `LlmStatus` from the bridge.

```ts
import type { LlmStatus } from '../bridge/llm'
// ... (Config import already added in T-507a)

function modelPresent(model: string | null): boolean {
  return model !== null && model.trim() !== ''
}

/** Kept from T-507a; now delegates so the blank rule lives in one place. */
export function lyricsModelConfigured(config: Config | null): boolean {
  return modelPresent(config?.llm?.model ?? null)
}

/**
 * What Lyrics can say about its model right now.
 *
 * `none` -- no model named (T-507a's first-run state).
 * `unknown` -- a model is named but Setup has not probed the endpoint this
 *   session, so reachability is genuinely unknown. We do NOT probe here (the
 *   §10 no-mount-probe rule) and do NOT block: this is today's behaviour, a
 *   generation may still fail into the banner.
 * `unreachable` -- the probe says the endpoint is down.
 * `not-offered` -- the endpoint answered, but the configured model is not in
 *   its list. This is the click-through's case: a model chosen once and since
 *   removed from the server.
 * `ready` -- named, reachable, and offered.
 *
 * `not_configured` maps to `unknown`: it cannot co-occur with a named model and
 * a defaulted base URL (llm.ts notes the wizard never produces it), so it is
 * not a failure we will assert against.
 */
export type LyricsModelState = 'ready' | 'unknown' | 'none' | 'unreachable' | 'not-offered'

export function lyricsModelState(model: string | null, status: LlmStatus | null): LyricsModelState {
  if (!modelPresent(model)) return 'none'
  if (status === null) return 'unknown'
  if (status.state === 'unreachable') return 'unreachable'
  if (status.state === 'ready') {
    const name = model!.trim()
    return status.models.some((m) => m.id === name) ? 'ready' : 'not-offered'
  }
  return 'unknown'
}

/** The states where Generate/Optimize must be disabled. */
export function lyricsModelBlocks(state: LyricsModelState): boolean {
  return state === 'none' || state === 'unreachable' || state === 'not-offered'
}

export interface LyricsModelNote {
  pill: string
  message: string
}

/**
 * The cue for a state, or null when there is nothing to say. Every message ends
 * pointing at Setup, per CONVENTIONS (a user-facing degraded state names a next
 * step). `model` is named only by `not-offered`, to help the user find what
 * went stale.
 */
export function lyricsModelNote(
  state: LyricsModelState,
  model: string | null,
): LyricsModelNote | null {
  switch (state) {
    case 'ready':
    case 'unknown':
      return null
    case 'none':
      return {
        pill: 'No lyrics model',
        message:
          'Lyrics are written by a model you provide, and none is set up yet. You can still write and edit lyrics by hand below.',
      }
    case 'unreachable':
      return {
        pill: 'Model unreachable',
        message: 'The lyrics model can’t be reached right now. Check it’s running, or pick another in Setup.',
      }
    case 'not-offered':
      return {
        pill: 'Model unavailable',
        message: `“${model}” isn’t offered by the endpoint anymore. Pick another in Setup.`,
      }
  }
}
```

## §2 — the tests (`state/lyrics.test.ts`)

Keep the existing `describe('lyricsModelConfigured', …)` five cases unchanged. Add a describe block
for the state machine. Need two tiny fixtures — an `LlmModelRow` and a `ready` `LlmStatus`:

```ts
import type { LlmStatus, LlmModelRow } from '../bridge/llm'

function modelRow(id: string): LlmModelRow {
  return { id, can_chat: null, thinks: null, is_remote: null, remote_host: null, size_bytes: null }
}
function ready(ids: string[]): LlmStatus {
  return { state: 'ready', models: ids.map(modelRow), enriched: false, preselect: null, has_key: false }
}
```

`describe('lyricsModelState', …)`, each case naming the invariant:

- **`(null, ready([...]))` → 'none'** — *no model named is no model, whatever the endpoint offers.*
- **`('   ', null)` → 'none'** — *whitespace-only is unset (the T-507a rule).*
- **`('m', null)` → 'unknown'** — *a named model with no probe yet is not asserted broken; today's
  behaviour is preserved.*
- **`('m', { state: 'unreachable', detail: 'x', hint: null })` → 'unreachable'** — *a down endpoint
  is a blocking state.*
- **`('qwen3.5-27b', ready(['gemma-4-12b']))` → 'not-offered'** — *the click-through case: named but
  the endpoint does not list it.*
- **`('gemma-4-12b', ready(['gemma-4-12b']))` → 'ready'** — *named, reachable, offered.*
- **`('m', { state: 'not_configured' })` → 'unknown'** — *a contradictory state is not asserted as a
  failure.*

`describe('lyricsModelBlocks', …)`:
- **none / unreachable / not-offered → true**, **ready / unknown → false** — *the disable set is
  exactly the states that cannot generate.*

`describe('lyricsModelNote', …)`:
- **ready → null**, **unknown → null** — *nothing to say when it works or is unknown.*
- **none / unreachable / not-offered → non-null with the expected `pill`.**
- **not-offered's `message` contains the model name** — *the user must be able to see which model
  went stale.*

## §3 — the view (`views/LyricsStudio.tsx`)

Replace the `hasLyricsModel` boolean and its single cue with the state machine. Subscribe narrowly
(the WORKFLOW §4.10 pattern already used for `profileId`): the model **string** from config, the
**status** from the llm store — never the whole `config` object.

```tsx
import { lyricsModelState, lyricsModelBlocks, lyricsModelNote } from '../state/lyrics'
import { useLlmStore } from '../state/llm'
// remove the `lyricsModelConfigured` import (the state machine replaces it here)
```

In the component body, replacing the `hasLyricsModel` line:

```tsx
const lyricsModelName = useConfigStore((state) => state.config?.llm?.model ?? null)
const llmStatus = useLlmStore((state) => state.status)
const lyricsNote = lyricsModelNote(lyricsModelState(lyricsModelName, llmStatus), lyricsModelName)
const lyricsBlocked = lyricsModelBlocks(lyricsModelState(lyricsModelName, llmStatus))
```

The cue block (replacing the whole `{!hasLyricsModel ? (…) : null}` section) renders on
`lyricsNote !== null` and reads its two fields:

```tsx
{lyricsNote !== null ? (
  <section className="panel profile-picker">
    <header className="setup-step-head">
      <h2 className="profile-picker-title">Lyrics model</h2>
      <span className="status-pill status-pill-warn">{lyricsNote.pill}</span>
    </header>
    <p className="profile-picker-fallback">{lyricsNote.message}</p>
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

The two button `disabled` clauses swap `!hasLyricsModel` for `lyricsBlocked`:

```tsx
// Generate:
disabled={generating || lyricsBlocked}
// Optimize:
disabled={generating || optimizing || reviewing || accepted || lyricsBlocked}
```

Everything else in the view (Save, Check, versions, the brief form) stays untouched — a status, not
a wall.

## Out of scope

- No probe added to any view.
- No backend change. `configured_llm`'s "no lyric model configured" error is unrelated and correct.
- T-507b (the adopted-profile `comfy.models` fix) is still its own later lane.

## Gate & acceptance

- `npm run gate` green.
- New tests cover the seven state cases, the block set, and the three notes.
- Producer click-through:
  1. With `qwen3.5-27b` still in config but Ollama not offering it → Lyrics shows **"Model
     unavailable"** naming `qwen3.5-27b`; Generate/Optimize disabled; Open Setup jumps to Setup.
  2. Stop ComfyUI/endpoint entirely and relaunch (so Setup's probe returns unreachable) → Lyrics
     shows **"Model unreachable"**.
  3. Pick a model the endpoint *does* offer in Setup, return to Lyrics → no cue, buttons enabled.
  4. (Regression) Clear the model entirely → the original **"No lyrics model"** cue still shows.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-507a-2-brief.md --read app/src/bridge/llm.ts --read app/src/state/llm.ts --read app/src/state/nav.ts --read app/src/bridge/config.ts --file app/src/state/lyrics.ts --file app/src/state/lyrics.test.ts --file app/src/views/LyricsStudio.tsx --file app/src/theme.css
```
