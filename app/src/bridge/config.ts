import { invoke } from '@tauri-apps/api/core'

/** Mirrors Rust `library::config::ComfyMode`. */
export type ComfyMode = 'local' | 'cloud'

/**
 * Mirrors Rust `library::config::LlmProvider`.
 *
 * `open_ai_compat` and `open_ai` look odd but are exactly what serde emits for
 * `OpenAiCompat` / `OpenAi`; they are verified, not guessed.
 */
export type LlmProvider = 'open_ai_compat' | 'ollama' | 'open_ai' | 'anthropic'

/** How to reach ComfyUI. Never carries an API key -- those live in the OS keychain. */
export interface ComfyConfig {
  mode: ComfyMode
  url: string | null
  comfy_bin: string | null
}

/** Lyric-model settings. Never carries an API key. */
export interface LlmConfig {
  provider: LlmProvider
  base_url: string | null
  model: string | null
  /** Whether the endpoint accepted `reasoning_effort: "none"` in the wizard's test call. */
  accepts_reasoning_effort: boolean | null
}

/** Mirrors Rust `library::config::Config`. Field names are snake_case on the wire. */
export interface Config {
  schema_version: number
  comfy: ComfyConfig
  llm: LlmConfig | null
  default_profile_id: string | null
}

/** Something the user should be told about loading config; never fatal. */
export type ConfigWarning =
  | { kind: 'missing' }
  | { kind: 'corrupt'; backup: string; detail: string }

/** What `load_config` returns. */
export interface LoadedConfig {
  config: Config
  warnings: ConfigWarning[]
}

/**
 * Secrets this app may store. Matches the Rust whitelist exactly; anything else is
 * rejected by the backend before it reaches the keychain.
 */
export type SecretName = 'comfy_cloud_api_key' | 'llm_api_key'

/** True when running inside the Tauri webview rather than a plain browser. */
export function isTauri(): boolean {
  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

/** Loads persisted config plus any warnings. Never rejects for a missing file. */
export async function loadConfig(): Promise<LoadedConfig> {
  return await invoke<LoadedConfig>('load_config')
}

/** Writes config atomically. */
export async function saveConfig(config: Config): Promise<void> {
  await invoke('save_config', { config })
}

/** Stores a secret in the OS keychain. */
export async function setSecret(name: SecretName, value: string): Promise<void> {
  await invoke('set_secret', { name, value })
}

/**
 * Whether a secret is stored.
 *
 * The backend answers by reading the secret, because no cheaper existence check exists.
 * On macOS the first call can raise the keychain-access prompt, so call this when a
 * screen loads -- **never on every render or in a polling loop**.
 */
export async function hasSecret(name: SecretName): Promise<boolean> {
  return await invoke<boolean>('has_secret', { name })
}

/** Removes a secret. Deleting one that is not stored is not an error. */
export async function deleteSecret(name: SecretName): Promise<void> {
  await invoke('delete_secret', { name })
}
