import { create } from 'zustand'
import {
  deleteSecret,
  hasSecret,
  isTauri,
  loadConfig,
  saveConfig,
  setSecret,
  type Config,
  type ConfigWarning,
  type SecretName,
} from '../bridge/config'

/**
 * `unavailable` is not an error: it means the app is running in a plain browser
 * (`npm run dev` without Tauri), where no backend exists. UI work happens in that mode,
 * so it must degrade to a status pill rather than an error state (CONVENTIONS.md).
 */
export type ConfigStatus = 'idle' | 'loading' | 'ready' | 'error' | 'unavailable'

interface ConfigState {
  config: Config | null
  warnings: ConfigWarning[]
  status: ConfigStatus
  error: string | null
  /** Which secrets are present. Only refreshed on demand -- never polled. */
  secrets: Partial<Record<SecretName, boolean>>
  load: () => Promise<void>
  save: (patch: Partial<Config>) => Promise<void>
  refreshSecrets: (names: SecretName[]) => Promise<void>
  storeSecret: (name: SecretName, value: string) => Promise<void>
  removeSecret: (name: SecretName) => Promise<void>
  dismissWarnings: () => void
}

export const useConfigStore = create<ConfigState>((set, get) => ({
  config: null,
  warnings: [],
  status: 'idle',
  error: null,
  secrets: {},

  load: async () => {
    if (!isTauri()) {
      set({ status: 'unavailable', error: null })
      return
    }
    set({ status: 'loading', error: null })
    try {
      const loaded = await loadConfig()
      set({ config: loaded.config, warnings: loaded.warnings, status: 'ready' })
    } catch (err: unknown) {
      set({ status: 'error', error: String(err) })
    }
  },

  save: async (patch) => {
    const current = get().config
    if (current === null) {
      set({ status: 'error', error: 'cannot save before config is loaded' })
      return
    }
    const next: Config = { ...current, ...patch }
    // Optimistic: the UI shows the new value immediately, and a failed write surfaces
    // as an error rather than a silent revert.
    set({ config: next, error: null })
    try {
      await saveConfig(next)
    } catch (err: unknown) {
      set({ status: 'error', error: String(err) })
    }
  },

  refreshSecrets: async (names) => {
    if (!isTauri()) return
    const entries = await Promise.all(
      names.map(async (name) => [name, await hasSecret(name)] as const),
    )
    set({ secrets: { ...get().secrets, ...Object.fromEntries(entries) } })
  },

  storeSecret: async (name, value) => {
    await setSecret(name, value)
    set({ secrets: { ...get().secrets, [name]: true } })
  },

  removeSecret: async (name) => {
    await deleteSecret(name)
    set({ secrets: { ...get().secrets, [name]: false } })
  },

  dismissWarnings: () => set({ warnings: [] }),
}))
