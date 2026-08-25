# T-112c: the LLM bridge and store
**Depends:** T-112b | **Crate/dir:** `app/src`
**Files to create:**
- `app/src/bridge/llm.ts`
- `app/src/state/llm.ts`
- `app/src/state/llm.test.ts`

## Goal
Mirror the backend's union, and turn each model into words a user can judge.

## Spec
Exactly the reference implementation below.

**Every capability is `boolean | null`, and null means unknown.** Against a non-Ollama endpoint
nothing can be checked. The rules that follow from that:

- **Never imply privacy for a model that could not be checked.** A silent absence of disclosure
  reads as "this is private". The row carries a `capabilities unknown` chip instead.
- **An unchecked model stays selectable.** Only a model *known* not to chat is refused.
- **A remote model names its host in a sentence, not a chip.** Eight of thirteen models on the
  verification machine send the prompt to `https://ollama.com`; the user's unreleased lyrics
  leaving the machine is a disclosure, not a tag.

**`testSummary` treats a reasoning-only answer as success**, for the reason in T-112b: a
thinking model can spend the whole budget on chain-of-thought and return empty content on a
working endpoint (LLM-SURFACE 11.4).

**The store never re-picks over the user.** The backend decides the preselect, honouring what is
configured; `probe` just takes it.

## Reference implementation

### `app/src/bridge/llm.ts` (create)
```ts
import { invoke } from '@tauri-apps/api/core'

/**
 * One model the endpoint offers. Mirrors Rust `LlmModelRow`.
 *
 * Every capability is nullable, and null means **unknown**, never false. The
 * OpenAI-compatible list returns ids and nothing else, so against a non-Ollama
 * endpoint nothing can be checked -- and telling a user their lyrics stay on
 * their machine when nobody verified that is the one mistake this step must
 * not make (LLM-SURFACE 11.1, 11.2).
 */
export interface LlmModelRow {
  id: string
  can_chat: boolean | null
  thinks: boolean | null
  is_remote: boolean | null
  /** Who the prompt is sent to, when it is sent anywhere. */
  remote_host: string | null
  size_bytes: number | null
  suggested: Suggested | null
}

/** The "recommended for lyrics" chip. Mirrors Rust `Suggested`. */
export interface Suggested {
  label: string
  why: string | null
  vram_hint: string | null
}

/** A suggested model with nothing installed. Mirrors Rust `MissingSuggestion`. */
export interface MissingSuggestion {
  label: string
  why: string | null
  vram_hint: string | null
  /** Shown for the user to run. The app never pulls an LLM. */
  pull_command: string | null
}

/**
 * Mirrors Rust `src-tauri/src/llm.rs` `LlmStatus`, a serde-tagged union
 * (`#[serde(tag = "state", rename_all = "snake_case")]`).
 */
export type LlmStatus =
  | { state: 'not_configured' }
  | { state: 'unreachable'; detail: string; hint: string | null }
  | {
      state: 'ready'
      models: LlmModelRow[]
      /** False means every capability is null and the UI must say so. */
      enriched: boolean
      missing_suggestions: MissingSuggestion[]
      preselect: string | null
      /** Whether a key is stored. The key itself never crosses the boundary. */
      has_key: boolean
    }

/** The outcome of a test call. Mirrors Rust `LlmTestResult`. */
export interface LlmTestResult {
  ok: boolean
  content: string
  saw_reasoning: boolean
  detail: string | null
}

/**
 * Report what the configured endpoint offers.
 *
 * This is also the step's **only** keychain read: `has_key` comes back on the
 * status, so the frontend never calls `has_secret` itself. Answering that
 * question means reading the secret, and on macOS a read can raise a prompt
 * (T-004).
 */
export async function llmProbe(
  baseUrl: string | null,
  configuredModel: string | null,
): Promise<LlmStatus> {
  return await invoke<LlmStatus>('llm_probe', { baseUrl, configuredModel })
}

/**
 * Ask the endpoint one trivial question.
 *
 * Success means a well-formed response, **not** non-empty content: a reasoning
 * model can spend its whole budget thinking (LLM-SURFACE 11.4).
 */
export async function llmTest(baseUrl: string, model: string): Promise<LlmTestResult> {
  return await invoke<LlmTestResult>('llm_test', { baseUrl, model })
}
```

### `app/src/state/llm.ts` (create)
```ts
import { create } from 'zustand'
import { isTauri } from '../bridge/comfy'
import {
  llmProbe,
  llmTest,
  type LlmModelRow,
  type LlmStatus,
  type LlmTestResult,
} from '../bridge/llm'

/** How one model should read in the picker. */
export interface ModelView {
  id: string
  /** Whether the model can be chosen at all. */
  selectable: boolean
  /** Short tags shown beside the id. */
  chips: string[]
  /**
   * The privacy sentence, when generating would send the prompt elsewhere.
   * Null when the model runs locally **or** when nobody could check -- an
   * unverified claim of privacy is worse than none, so the unknown case says
   * so through `chips` instead.
   */
  disclosure: string | null
}

/**
 * Describe one model for the picker.
 *
 * The three capabilities are tri-state. `null` is unknown, which happens
 * against any endpoint that is not Ollama, and unknown is never rendered as
 * "local" or as "works" (LLM-SURFACE 11.1, 11.2).
 */
export function modelView(row: LlmModelRow): ModelView {
  const chips: string[] = []
  if (row.suggested !== null) chips.push('recommended for lyrics')
  if (row.can_chat === false) chips.push('cannot chat')
  if (row.is_remote === true) chips.push('remote')
  if (row.thinks === true) chips.push('thinks first')
  if (row.can_chat === null) chips.push('capabilities unknown')

  return {
    id: row.id,
    selectable: row.can_chat !== false,
    chips,
    disclosure:
      row.is_remote === true
        ? `Runs on ${row.remote_host ?? 'a third-party server'}. Your lyrics leave this machine.`
        : null,
  }
}

/**
 * How the test call's result should read.
 *
 * Reasoning-only is a **success**: a thinking model can spend the whole token
 * budget on chain-of-thought and return empty content on a perfectly healthy
 * endpoint (LLM-SURFACE 11.4). Reporting that as a failure sends a user to fix
 * a setup that already works.
 */
export function testSummary(result: LlmTestResult): string {
  if (!result.ok) return result.detail ?? 'The endpoint did not answer.'
  if (result.content !== '') {
    return result.saw_reasoning
      ? `Answered "${result.content}" after thinking.`
      : `Answered "${result.content}".`
  }
  return result.saw_reasoning
    ? 'Answered, but spent the whole budget thinking. The endpoint works.'
    : 'Answered.'
}

/** Whether the step can offer a test call yet. */
export function canTest(status: LlmStatus | null, model: string | null): boolean {
  return status !== null && status.state === 'ready' && model !== null && model !== ''
}

interface LlmState {
  status: LlmStatus | null
  busy: boolean
  testing: boolean
  result: LlmTestResult | null
  /** The model the user has chosen, or the preselected one. */
  model: string | null
  probe: (baseUrl: string | null, configuredModel: string | null) => Promise<void>
  choose: (model: string) => void
  test: (baseUrl: string) => Promise<void>
}

export const useLlmStore = create<LlmState>((set, get) => ({
  status: null,
  busy: false,
  testing: false,
  result: null,
  model: null,

  probe: async (baseUrl, configuredModel) => {
    if (!isTauri()) return
    set({ busy: true })
    try {
      const status = await llmProbe(baseUrl, configuredModel)
      // The backend decides the preselect, honouring what is configured; the
      // store never re-picks over a choice the user already made.
      set({ status, model: status.state === 'ready' ? status.preselect : null, result: null })
    } finally {
      set({ busy: false })
    }
  },

  choose: (model: string) => set({ model, result: null }),

  test: async (baseUrl: string) => {
    const model = get().model
    if (!isTauri() || model === null || get().testing) return
    set({ testing: true, result: null })
    try {
      set({ result: await llmTest(baseUrl, model) })
    } finally {
      set({ testing: false })
    }
  },
}))
```

### `app/src/state/llm.test.ts` (create)
```ts
import { describe, expect, it } from 'vitest'
import type { LlmModelRow, LlmStatus, LlmTestResult } from '../bridge/llm'
import { canTest, modelView, testSummary } from './llm'

function row(over: Partial<LlmModelRow>): LlmModelRow {
  return {
    id: 'model',
    can_chat: true,
    thinks: false,
    is_remote: false,
    remote_host: null,
    size_bytes: null,
    suggested: null,
    ...over,
  }
}

/** Capabilities all null, as any non-Ollama endpoint reports them. */
const UNCHECKED = row({ can_chat: null, thinks: null, is_remote: null })

function result(over: Partial<LlmTestResult>): LlmTestResult {
  return { ok: true, content: '', saw_reasoning: false, detail: null, ...over }
}

describe('modelView', () => {
  /**
   * Protects the sharpest rule in this step. Eight of the thirteen models on
   * the verification machine run on someone else's servers, and `/v1/models`
   * gives no way to tell. The user's unreleased lyrics leaving the machine is
   * a disclosure they must see wherever the model is chosen.
   */
  it('discloses that a remote model sends lyrics off the machine', () => {
    const view = modelView(
      row({ id: 'kimi-k3:cloud', is_remote: true, remote_host: 'https://ollama.com' }),
    )
    expect(view.disclosure).toContain('https://ollama.com')
    expect(view.disclosure).toContain('leave this machine')
    expect(view.chips).toContain('remote')
  })

  /**
   * Protects: **unknown is not local.** Against a non-Ollama endpoint nothing
   * can be checked, and a silent absence of disclosure reads as "this is
   * private". The row says its capabilities are unknown instead.
   */
  it('never implies privacy for a model it could not check', () => {
    const view = modelView(UNCHECKED)
    expect(view.disclosure).toBeNull()
    expect(view.chips).toContain('capabilities unknown')
  })

  /**
   * Protects: **unknown is not unusable either.** Hiding every unchecked model
   * would strand a user on a non-Ollama endpoint with an empty picker.
   */
  it('keeps an unchecked model selectable', () => {
    expect(modelView(UNCHECKED).selectable).toBe(true)
  })

  /**
   * Protects: an embedding model cannot be chosen. `/v1/models` lists
   * `all-minilm` indistinguishably from a chat model, and picking it fails
   * later at lyric time, far from this screen.
   */
  it('refuses a model that cannot chat', () => {
    const view = modelView(row({ id: 'all-minilm:latest', can_chat: false }))
    expect(view.selectable).toBe(false)
    expect(view.chips).toContain('cannot chat')
  })

  /** Protects: the recommendation chip reaches the row. */
  it('chips a suggested model', () => {
    const view = modelView(
      row({ id: 'gemma4:12b-32k', suggested: { label: 'Gemma 4 12B', why: 'x', vram_hint: null } }),
    )
    expect(view.chips).toContain('recommended for lyrics')
  })

  /**
   * Protects: the thinking flag is shown, because it explains a pause the user
   * would otherwise read as a hang.
   */
  it('marks a model that thinks before answering', () => {
    expect(modelView(row({ thinks: true })).chips).toContain('thinks first')
    expect(modelView(row({ thinks: false })).chips).not.toContain('thinks first')
  })
})

describe('testSummary', () => {
  /**
   * Protects the trap that only a live call reveals. A thinking model spends
   * the token budget on chain-of-thought and returns empty content on a
   * perfectly healthy endpoint. Calling that a failure sends the user to fix a
   * setup that already works (LLM-SURFACE 11.4).
   */
  it('treats a reasoning-only answer as success', () => {
    const summary = testSummary(result({ ok: true, content: '', saw_reasoning: true }))
    expect(summary).toContain('works')
    expect(summary).not.toContain('did not answer')
  })

  /** Protects: a real answer is quoted back, so the user sees the endpoint work. */
  it('quotes the answer when there is one', () => {
    expect(testSummary(result({ content: 'ok' }))).toContain('"ok"')
    expect(testSummary(result({ content: 'ok', saw_reasoning: true }))).toContain('after thinking')
  })

  /** Protects: a genuine failure keeps its reason rather than becoming generic. */
  it('keeps the failure detail', () => {
    const summary = testSummary(result({ ok: false, detail: 'connection refused' }))
    expect(summary).toBe('connection refused')
  })
})

describe('canTest', () => {
  /** Protects: no test call without an endpoint and a chosen model. */
  it('requires a ready endpoint and a model', () => {
    const ready: LlmStatus = {
      state: 'ready',
      models: [],
      enriched: true,
      missing_suggestions: [],
      preselect: null,
      has_key: false,
    }
    expect(canTest(ready, 'gemma4:12b-32k')).toBe(true)
    expect(canTest(ready, null)).toBe(false)
    expect(canTest(ready, '')).toBe(false)
    expect(canTest({ state: 'not_configured' }, 'gemma4:12b-32k')).toBe(false)
    expect(canTest(null, 'gemma4:12b-32k')).toBe(false)
  })
})
```

## Acceptance criteria
- `npm run gate` green, zero oxlint warnings.
- vitest goes 41 -> **51** across **9** files.
- **No non-ASCII characters anywhere in the diff.** Note the disclosure sentence says
  `a third-party server`, deliberately avoiding an apostrophe inside the nested quotes.

## Out of scope
The view (T-112d).

## If unclear
Follow the reference implementation exactly.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read docs/LLM-SURFACE.md --read src-tauri/src/llm.rs --read app/src/state/models.ts --file app/src/bridge/llm.ts --file app/src/state/llm.ts --file app/src/state/llm.test.ts
```
