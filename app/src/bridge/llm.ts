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
      preselect: string | null
      /** Whether a key is stored. The key itself never crosses the boundary. */
      has_key: boolean
    }

/** The outcome of a test call. Mirrors Rust `LlmTestResult`. */
export interface LlmTestResult {
  ok: boolean
  content: string
  saw_reasoning: boolean
  /** Whether the endpoint accepted `reasoning_effort: "none"` when probed. Null = could not tell. */
  accepts_reasoning_effort: boolean | null
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
