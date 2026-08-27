import { create } from 'zustand'
import type { Config } from '../bridge/config'
import { isTauri } from '../bridge/comfy'
import {
  llmProbe,
  llmTest,
  type LlmModelRow,
  type LlmStatus,
  type LlmTestResult,
} from '../bridge/llm'
import { useConfigStore } from './config'

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

/** The endpoint the step should use, before the user has typed anything. */
export const DEFAULT_BASE_URL = 'http://127.0.0.1:11434/v1'

/**
 * Which endpoint the step actually talks to.
 *
 * The configured value wins; the default is a prefill, not a fallback the
 * user is stuck with (owner, 2026-08-27). A blank or whitespace-only stored
 * value is treated as unset, because that is what clearing the field leaves
 * behind and an empty string would probe nothing forever.
 */
export function effectiveBaseUrl(config: Config | null): string {
  const stored = config?.llm?.base_url ?? null
  return stored !== null && stored.trim() !== '' ? stored.trim() : DEFAULT_BASE_URL
}

/** What the API-key control should offer. */
export type KeyField = 'stored' | 'entry'

/**
 * Whether to show the stored-key affordance or an input.
 *
 * A store selector rather than a condition in JSX, for the reason Phase 2
 * ended on: a decision derived in a view is one no test can reach. This is
 * the reachable half -- that a stored key hides the input.
 *
 * **The write-only half is guaranteed by construction, not by this**: no
 * Tauri command returns a secret value (T-004), and the input's value comes
 * only from local draft state initialised empty. There is nothing for a test
 * to catch, because there is no code path that could populate it.
 */
export function keyField(status: LlmStatus | null): KeyField {
  return status !== null && status.state === 'ready' && status.has_key ? 'stored' : 'entry'
}

interface LlmState {
  status: LlmStatus | null
  busy: boolean
  testing: boolean
  result: LlmTestResult | null
  /** The model the user has chosen, or the preselected one. */
  model: string | null
  probe: (baseUrl: string | null, configuredModel: string | null) => Promise<void>
  choose: (baseUrl: string, model: string) => Promise<void>
  test: (baseUrl: string) => Promise<void>
  saveEndpoint: (url: string | null) => Promise<void>
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

  // Choosing a model **persists it**. Everything downstream -- the Lyrics
  // Studio's generate and optimize -- reads the endpoint from `config.json`,
  // not from this store, so a selection that lived only here left the app
  // reporting "no lyric LLM configured" against a picker showing a model
  // selected and a test call that passed. The test call proves nothing about
  // persistence: `llm_test` takes the endpoint and model as arguments.
  choose: async (baseUrl: string, model: string) => {
    set({ model, result: null })
    if (!isTauri()) return
    await useConfigStore
      .getState()
      .save({ llm: { provider: 'open_ai_compat', base_url: baseUrl, model } })
  },

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

  // Saving the endpoint must preserve the model, because `useConfigStore.save`
  // does a shallow merge and a partial `llm` patch would drop it (T-212).
  saveEndpoint: async (url) => {
    if (!isTauri()) return
    await useConfigStore.getState().save({
      llm: {
        provider: 'open_ai_compat',
        base_url: url,
        model: useConfigStore.getState().config?.llm?.model ?? null,
      },
    })
  },
}))
