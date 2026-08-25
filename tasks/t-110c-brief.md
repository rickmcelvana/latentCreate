# T-110c: the wizard's ComfyUI step (frontend)
**Depends:** T-110b | **Crate/dir:** `app`
**Files to create/modify:**
- `app/src/bridge/comfy.ts` (create)
- `app/src/state/comfy.ts` (create)
- `app/src/state/comfy.test.ts` (create)
- `app/src/views/Setup.tsx` (replace -- it is a 13-line placeholder today)
- `app/src/theme.css` (modify: append one block, changed lines only)

**~440 lines, a little over the ~400 guide.** Roughly a quarter is declarative CSS, and
splitting a view from the store it reads would leave neither half testable on its own.

## Goal
The Setup view's ComfyUI step: a status pill, a next step for every degraded state, the
facts worth showing when it is up, and the two buttons.

## Spec
Exactly the reference implementation below.

**Every degraded state says what to do next.** CONVENTIONS: user-facing errors say what to do
next, not just what failed. `pillFor` is pure so that rule is testable, and
`test_gives_every_degraded_state_a_next_step` sweeps `ALL_STATES` -- adding a backend variant
without wording fails the suite rather than shipping a blank panel.

**Unknown VRAM stays unknown.** `formatVram(null)` and `formatVram(0)` both return `null`, so
nothing renders. A `0.0 GiB` reading on a working machine looks like a broken app. GiB is the
unit because that is what GPU tools show: the captured card reports 17102733312 bytes, sold
as "16 GB", displayed as 15.9 GiB.

**Nothing polls.** The view checks once on mount and otherwise only when the user clicks.
A wizard that re-probes on a timer spawns `comfy-mcp` processes behind the user's back
(ARCHITECTURE 3: Rust pushes, the frontend never polls).

**Store rules.** Zustand is subscribed with one selector per field, never the bare store
(review checklist item 10). `invoke` appears only in `app/src/bridge/` (item 5). The `busy`
flag is set in a `finally` so a failed call cannot leave the buttons disabled forever.

**Start ComfyUI appears only for `server_down`.** In every other state it would either do
nothing or fail, and a button that cannot work is worse than no button.

## Verification already done
The five rendered states were driven through the real store in a browser and checked by DOM
and computed-style reads: correct pill text, correct tone colour (`--warning` #d29922 for
the three degraded states, `--success` #3fb950 for ready), the install command shown only for
`not_installed`, the facts and update badge only for `ready`, and **Start ComfyUI** only for
`server_down`.

⚠ **One thing is deliberately unverified**: the 180 ms colour transition on `.status-pill`.
The review browser does not composite frames, so transitions never advance there and a
computed colour read mid-transition returns the start value -- WORKFLOW section 5 records
this. The colours above were read from a clone with `transition: none`. **Producer
click-through: confirm the pill animates rather than jumping.**

## Reference implementation
Transcribe verbatim. `tsc -b`, `oxlint`, `vitest` and `vite build` all pass; the 7 new tests
bring the app suite to 28.

### 1. `app/src/bridge/comfy.ts` (new file, complete)
```typescript
import { invoke } from '@tauri-apps/api/core'

/**
 * Mirrors Rust `src-tauri/src/comfy.rs` `ComfyStatus`, a serde-tagged union
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 *
 * Every failure ComfyUI can present is a variant here rather than a thrown
 * error, so the view renders a pill with a next step instead of parsing
 * message strings (CONVENTIONS: degraded services degrade, never block).
 */
export type ComfyStatus =
  | { state: 'not_installed'; install_command: string }
  | { state: 'unreachable'; detail: string }
  | { state: 'server_down'; workspace: string | null }
  | {
      state: 'ready'
      url: string | null
      vram_bytes: number | null
      workspace: string | null
      comfy_cli_version: string | null
      update_available: boolean
    }

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/**
 * Connect if needed and report what the wizard should show.
 *
 * Rejects only when the app itself fails (it could not open its own session
 * log). A missing `comfy-mcp`, a dead ComfyUI and a broken connection all
 * resolve to a status.
 */
export async function comfyStatus(bin?: string): Promise<ComfyStatus> {
  return await invoke<ComfyStatus>('comfy_status', { bin })
}

/**
 * Start ComfyUI, then report the resulting status.
 *
 * Only offered when the status is `server_down`. A port already in use is not
 * treated as a failure -- the following health check reports what is really
 * there.
 */
export async function comfyLaunch(bin?: string): Promise<ComfyStatus> {
  return await invoke<ComfyStatus>('comfy_launch', { bin })
}
```

### 2. `app/src/state/comfy.ts` (new file, complete)
```typescript
import { create } from 'zustand'
import { comfyLaunch, comfyStatus, isTauri, type ComfyStatus } from '../bridge/comfy'

/** How the pill should read for a given status. */
export interface PillView {
  /** `ok` | `warn` | `neutral` -- drives the pill's colour class. */
  tone: 'ok' | 'warn' | 'neutral'
  label: string
  /** One sentence saying what to do next, or null when nothing is needed. */
  nextStep: string | null
}

/**
 * Map a status to what the user sees.
 *
 * Pure, so the wording of every degraded state is testable without a Tauri
 * bridge or a running ComfyUI. CONVENTIONS requires user-facing errors to say
 * what to do next, not just what failed -- that rule is enforced here by
 * `nextStep` being non-null for every state that is not `ready`.
 */
export function pillFor(status: ComfyStatus | null): PillView {
  if (status === null) {
    return { tone: 'neutral', label: 'Checking ComfyUI...', nextStep: null }
  }
  switch (status.state) {
    case 'not_installed':
      return {
        tone: 'warn',
        label: 'comfy-mcp not found',
        nextStep: `Run ${status.install_command}, then Retry.`,
      }
    case 'unreachable':
      return {
        tone: 'warn',
        label: 'ComfyUI unreachable',
        nextStep: `${status.detail} Check the install, then Retry.`,
      }
    case 'server_down':
      return {
        tone: 'warn',
        label: 'ComfyUI is not running',
        nextStep: 'Start ComfyUI, then Retry.',
      }
    case 'ready':
      return {
        tone: 'ok',
        label: status.url === null ? 'ComfyUI ready' : `ComfyUI ready at ${status.url}`,
        nextStep: null,
      }
  }
}

/**
 * Total VRAM as a short human string, or null when it is unknown.
 *
 * Unknown stays unknown: comfy-cli does not always report a GPU, and showing
 * `0 GB` on a working machine reads as a broken app. GiB, because that is what
 * every GPU tool shows -- 17102733312 bytes is a "16 GB" card at 15.9 GiB.
 */
export function formatVram(bytes: number | null): string | null {
  if (bytes === null || bytes <= 0) return null
  return `${(bytes / 1024 ** 3).toFixed(1)} GiB VRAM`
}

interface ComfyState {
  status: ComfyStatus | null
  /** True while a status or launch call is in flight. */
  busy: boolean
  refresh: () => Promise<void>
  launch: () => Promise<void>
}

export const useComfyStore = create<ComfyState>((set) => ({
  status: null,
  busy: false,

  refresh: async () => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      set({ status: await comfyStatus() })
    } finally {
      set({ busy: false })
    }
  },

  launch: async () => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      set({ status: await comfyLaunch() })
    } finally {
      set({ busy: false })
    }
  },
}))
```

### 3. `app/src/state/comfy.test.ts` (new file, complete)
```typescript
import { describe, expect, it } from 'vitest'
import type { ComfyStatus } from '../bridge/comfy'
import { formatVram, pillFor } from './comfy'

/** Every state the backend can report, so the sweep below cannot go stale. */
const ALL_STATES: ComfyStatus[] = [
  { state: 'not_installed', install_command: 'pip install comfy-mcp' },
  { state: 'unreachable', detail: 'connection closed.' },
  { state: 'server_down', workspace: 'C:/Comfy/ComfyUI' },
  {
    state: 'ready',
    url: 'http://127.0.0.1:8188',
    vram_bytes: 17102733312,
    workspace: 'C:/Comfy/ComfyUI',
    comfy_cli_version: '1.16.0',
    update_available: true,
  },
]

describe('pillFor', () => {
  /**
   * Protects a product rule, not a rendering detail: CONVENTIONS requires
   * user-facing errors to say what to do next, not just what failed. Adding a
   * degraded state without a next step fails here.
   */
  it('gives every degraded state a next step', () => {
    for (const status of ALL_STATES) {
      const pill = pillFor(status)
      if (status.state === 'ready') {
        expect(pill.nextStep).toBeNull()
        expect(pill.tone).toBe('ok')
      } else {
        expect(pill.nextStep, `${status.state} must say what to do next`).not.toBeNull()
        expect(pill.nextStep).not.toBe('')
        expect(pill.tone).toBe('warn')
      }
    }
  })

  /** Protects: the install command reaches the user verbatim, so it is copyable. */
  it('quotes the install command when comfy-mcp is missing', () => {
    const pill = pillFor({ state: 'not_installed', install_command: 'pip install comfy-mcp' })
    expect(pill.nextStep).toContain('pip install comfy-mcp')
  })

  /** Protects: the reason a connection failed is not swallowed. */
  it('carries the failure detail when unreachable', () => {
    const pill = pillFor({ state: 'unreachable', detail: 'spawn failed.' })
    expect(pill.nextStep).toContain('spawn failed.')
  })

  /** Protects: the ready pill names where ComfyUI is, so a user running two
   * installs can tell which one answered. */
  it('shows the url when ready', () => {
    const pill = pillFor(ALL_STATES[3])
    expect(pill.label).toContain('http://127.0.0.1:8188')
    expect(pill.tone).toBe('ok')
  })

  /** Protects: the pre-check state is neutral, not a failure. A red pill
   * before the first check has even returned reads as broken. */
  it('is neutral before the first check returns', () => {
    const pill = pillFor(null)
    expect(pill.tone).toBe('neutral')
    expect(pill.nextStep).toBeNull()
  })
})

describe('formatVram', () => {
  /**
   * Protects: unknown stays unknown. comfy-cli does not always report a GPU,
   * and rendering that absence as `0.0 GiB` puts a hardware warning on a
   * perfectly working machine.
   */
  it('returns null for unknown or zero, never a zero reading', () => {
    expect(formatVram(null)).toBeNull()
    expect(formatVram(0)).toBeNull()
  })

  /** Protects: the unit. The captured card reports 17102733312 bytes, which
   * every GPU tool calls 16 GB and is 15.9 GiB -- so the number shown must
   * match what the user sees elsewhere. */
  it('formats the captured card as GiB', () => {
    expect(formatVram(17102733312)).toBe('15.9 GiB VRAM')
  })
})
```

### 4. `app/src/views/Setup.tsx` (replace the file entirely)
```tsx
import { useEffect } from 'react'
import type { ComfyStatus } from '../bridge/comfy'
import { useComfyStore, formatVram, pillFor } from '../state/comfy'

/**
 * Setup wizard, ComfyUI step.
 *
 * Checks once on mount and otherwise only when the user asks. Nothing here
 * polls: a wizard that re-probes on a timer spawns `comfy-mcp` processes
 * behind the user's back.
 */
export function Setup() {
  const status = useComfyStore((state) => state.status)
  const busy = useComfyStore((state) => state.busy)
  const refresh = useComfyStore((state) => state.refresh)
  const launch = useComfyStore((state) => state.launch)

  useEffect(() => {
    void refresh()
  }, [refresh])

  const pill = pillFor(status)

  return (
    <>
      <h1 className="view-title">Setup</h1>
      <p className="view-subtitle">
        Connect ComfyUI and, optionally, a model for writing lyrics.
      </p>

      <section className="panel setup-step">
        <header className="setup-step-head">
          <h2 className="setup-step-title">ComfyUI</h2>
          <span className={`status-pill status-pill-${pill.tone}`}>{pill.label}</span>
        </header>

        {pill.nextStep !== null ? <p className="setup-next-step">{pill.nextStep}</p> : null}

        {status !== null && status.state === 'not_installed' ? (
          <code className="setup-command">{status.install_command}</code>
        ) : null}

        {status !== null && status.state === 'ready' ? (
          <ComfyFacts status={status} />
        ) : null}

        <div className="setup-actions">
          <button type="button" className="setup-button" onClick={() => void refresh()} disabled={busy}>
            {busy ? 'Checking...' : 'Retry'}
          </button>
          {status !== null && status.state === 'server_down' ? (
            <button
              type="button"
              className="setup-button setup-button-primary"
              onClick={() => void launch()}
              disabled={busy}
            >
              Start ComfyUI
            </button>
          ) : null}
        </div>
      </section>
    </>
  )
}

/** The details worth showing once ComfyUI is up. */
function ComfyFacts({ status }: { status: Extract<ComfyStatus, { state: 'ready' }> }) {
  const vram = formatVram(status.vram_bytes)
  return (
    <dl className="setup-facts">
      {vram !== null ? (
        <div className="setup-fact">
          <dt>Hardware</dt>
          <dd>{vram}</dd>
        </div>
      ) : null}
      {status.workspace !== null ? (
        <div className="setup-fact">
          <dt>Workspace</dt>
          <dd>{status.workspace}</dd>
        </div>
      ) : null}
      {status.comfy_cli_version !== null ? (
        <div className="setup-fact">
          <dt>comfy-cli</dt>
          <dd>
            {status.comfy_cli_version}
            {status.update_available ? (
              <span className="setup-update">update available</span>
            ) : null}
          </dd>
        </div>
      ) : null}
    </dl>
  )
}
```

### 5. `app/src/theme.css` -- append this block at the end
**Do not touch any existing rule.** Append only:

```css
/* --- Setup wizard steps --- */

.setup-step {
  display: flex;
  flex-direction: column;
  gap: var(--gap-md);
}

.setup-step-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: var(--gap-md);
}

.setup-step-title {
  margin: 0;
  font-size: 16px;
  font-weight: 600;
  color: var(--text);
}

.status-pill-neutral {
  color: var(--text-muted);
  border-color: var(--border);
}

.setup-next-step {
  margin: 0;
  color: var(--text-muted);
  font-size: 14px;
  line-height: 1.5;
}

.setup-command {
  align-self: flex-start;
  padding: var(--gap-sm) var(--gap-md);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: var(--radius);
  color: var(--accent);
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 13px;
  user-select: all;
}

.setup-facts {
  display: flex;
  flex-direction: column;
  gap: var(--gap-sm);
  margin: 0;
}

.setup-fact {
  display: flex;
  gap: var(--gap-md);
  font-size: 13px;
}

.setup-fact dt {
  min-width: 96px;
  color: var(--text-muted);
}

.setup-fact dd {
  margin: 0;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: var(--gap-sm);
}

.setup-update {
  padding: 0 var(--gap-sm);
  border: 1px solid var(--warning);
  border-radius: 999px;
  color: var(--warning);
  font-size: 11px;
}

.setup-actions {
  display: flex;
  gap: var(--gap-sm);
}

.setup-button {
  padding: var(--gap-sm) var(--gap-lg);
  background: var(--panel-hover);
  border: 1px solid var(--border-bright);
  border-radius: var(--radius);
  color: var(--text);
  font-family: var(--font);
  font-size: 13px;
  cursor: pointer;
  transition: background var(--transition), border-color var(--transition);
}

.setup-button:hover:not(:disabled) {
  background: var(--panel);
  border-color: var(--accent);
}

.setup-button:disabled {
  opacity: 0.55;
  cursor: default;
}

.setup-button-primary {
  border-color: var(--accent);
  color: var(--accent);
}
```

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] `vitest` reports **28 tests** across 6 files
- [ ] every `className` in `Setup.tsx` has a rule in `theme.css`, including all three
      `status-pill-*` tones (review checklist item 5)
- [ ] no `invoke` or `listen` outside `app/src/bridge/` (item 5)
- [ ] Zustand subscribed with selectors, never the bare store (item 10)
- [ ] no changes outside the five listed files

## Out of scope
- The models step (T-111) and the LLM step (T-112).
- Persisting `comfy.comfy_bin` to config -- the wizard reads status with the default binary;
  writing config comes with the step that offers the setting.
- Any animation work beyond the existing `.status-pill` transition.

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read src-tauri/src/comfy.rs --read app/src/state/jobs.ts --read app/src/bridge/jobs.ts --file app/src/bridge/comfy.ts --file app/src/state/comfy.ts --file app/src/state/comfy.test.ts --file app/src/views/Setup.tsx --file app/src/theme.css
```
`src-tauri/src/comfy.rs` is `--read` because the TypeScript union mirrors its serde tags
exactly; `state/jobs.ts` and `bridge/jobs.ts` are the house patterns the new store and bridge
follow. None may be edited (WORKFLOW 3).
