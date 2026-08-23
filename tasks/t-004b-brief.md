# T-004b: config bridge, Zustand store, and a cross-language wire fixture
**Depends:** T-004 | **Dirs:** `app/`, plus one test in `crates/library` | **Executor:** Aider

**Files to create:**
`testdata/wire/loaded-config.json`,
`app/src/bridge/config.ts`,
`app/src/state/config.ts`,
`app/src/state/config.test.ts`

**Files to modify:** `crates/library/src/config.rs` (add **one** test — nothing else)

## Goal
Typed frontend access to the five config/secret commands, a Zustand store that owns config
state, and a shared fixture that makes Rust↔TypeScript wire drift a **build failure**
rather than a runtime surprise.

## The wire format — measured, not guessed
Dumped from the real Rust types on 2026-08-23 by serialising `LoadedConfig`. Mirror it
exactly:

```json
{
  "config": {
    "schema_version": 1,
    "comfy": { "mode": "local", "url": null, "comfy_bin": null },
    "llm": null,
    "default_profile_id": null
  },
  "warnings": [{ "kind": "missing" }]
}
```
Populated, with a corrupt warning:
```json
{
  "config": {
    "schema_version": 1,
    "comfy": { "mode": "cloud", "url": "http://127.0.0.1:8188", "comfy_bin": null },
    "llm": {
      "provider": "open_ai_compat",
      "base_url": "http://localhost:11434/v1",
      "model": "gemma4:12b"
    },
    "default_profile_id": "ace-step-1.5-turbo"
  },
  "warnings": [
    { "kind": "corrupt", "backup": "C:/x/config.json.corrupt-1", "detail": "expected value" }
  ]
}
```

⚠ **`LlmProvider` serialises as `"open_ai_compat"`, `"ollama"`, `"open_ai"`, `"anthropic"`.**
Note `open_ai_compat` and `open_ai` — serde splits on each capital, so the obvious guesses
(`openai`, `openaiCompat`) are wrong. These four strings are verified output; do not
"correct" them.

Field names are **snake_case** on the wire. Keep them snake_case in the TypeScript types
too: renaming to camelCase means a translation layer that can drift, and this boundary is
narrow enough not to need one.

## Files

### `testdata/wire/loaded-config.json`
Exactly the **populated** JSON above (the second block). This one file is read by both a
Rust test and a TypeScript test, so a field rename on either side breaks a build.

### `crates/library/src/config.rs` — add one test only
```rust
#[test]
fn test_wire_fixture_matches_current_types() {
    // Shared with app/src/state/config.test.ts. If a rename makes this fail, the
    // TypeScript types in app/src/bridge/config.ts must change in the same commit --
    // that is the entire point of the shared file.
    const FIXTURE: &str = include_str!("../../../testdata/wire/loaded-config.json");
    let loaded: LoadedConfig = serde_json::from_str(FIXTURE).expect("fixture must parse");
    let reserialised = serde_json::to_value(&loaded).unwrap();
    let original: serde_json::Value = serde_json::from_str(FIXTURE).unwrap();
    assert_eq!(
        reserialised, original,
        "wire format changed; update testdata/wire/loaded-config.json AND the TypeScript \
         types in app/src/bridge/config.ts"
    );
}
```

### `app/src/bridge/config.ts`
```ts
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

/** How to reach ComfyUI. Never carries an API key — those live in the OS keychain. */
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
 * screen loads — **never on every render or in a polling loop**.
 */
export async function hasSecret(name: SecretName): Promise<boolean> {
  return await invoke<boolean>('has_secret', { name })
}

/** Removes a secret. Deleting one that is not stored is not an error. */
export async function deleteSecret(name: SecretName): Promise<void> {
  await invoke('delete_secret', { name })
}
```

### `app/src/state/config.ts`
```ts
import { create } from 'zustand'
import {
  deleteSecret,
  hasSecret,
  isTauri,
  loadConfig,
  saveConfig,
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
  /** Which secrets are present. Only refreshed on demand — never polled. */
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
    await setSecretSafely(name, value)
    set({ secrets: { ...get().secrets, [name]: true } })
  },

  removeSecret: async (name) => {
    await deleteSecret(name)
    set({ secrets: { ...get().secrets, [name]: false } })
  },

  dismissWarnings: () => set({ warnings: [] }),
}))
```
Import `setSecret` from the bridge and use it directly — `setSecretSafely` above is a
placeholder name; call the bridge's `setSecret(name, value)`. Keep the rest as written.

### `app/src/state/config.test.ts`
Mock the bridge with `vi.mock('../bridge/config', ...)`, keeping the real types. Reset the
store between tests with `useConfigStore.setState({...})`.

Required tests:
- `test_load_populates_config_and_warnings` — mock `loadConfig` to resolve the **imported
  `testdata/wire/loaded-config.json`**, then assert `config.llm?.provider` is
  `'open_ai_compat'`, `config.comfy.mode` is `'cloud'`, and the single warning is the
  `corrupt` variant with a backup path. Importing the shared fixture is what pins the TS
  types to Rust's output.
- `test_load_sets_unavailable_outside_tauri` — `isTauri` mocked false; status becomes
  `'unavailable'`, no error, and `loadConfig` is never called.
- `test_load_sets_error_when_command_rejects`.
- `test_save_merges_patch_over_current_config` — seed a config, save a patch touching only
  `default_profile_id`, assert `saveConfig` received a whole `Config` with the other
  fields unchanged.
- `test_save_before_load_does_not_call_backend` — asserts `saveConfig` was not called and
  an error is set. Writing a half-built config over a good one is the failure this guards.
- `test_refresh_secrets_records_presence_per_name`.
- `test_refresh_secrets_is_not_called_outside_tauri`.

Import the fixture with `import fixture from '../../../testdata/wire/loaded-config.json'`.
If the tsconfig rejects that, add `"resolveJsonModule": true` to `tsconfig.app.json` — that
is the only tsconfig change permitted here.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root
- [ ] `cargo test -p library` includes `test_wire_fixture_matches_current_types`
- [ ] All seven named vitest tests present and passing
- [ ] Components still import nothing from `@tauri-apps/*` — only `bridge/`
- [ ] No new dependencies
- [ ] No changes outside the listed files (plus the one permitted tsconfig flag)

## Out of scope
Any UI or view changes — the Setup screen is Phase 1. Wiring the store into `App.tsx`.
Validating config values (e.g. that a URL is reachable). Migrations.

## Notes for the executor
- Tests run in vitest's **node** environment; there is no jsdom and none is to be added.
- `verbatimModuleSyntax` is on: use `import type` for type-only imports.
- Keep wire field names snake_case in TypeScript; do not introduce a camelCase mapping.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/library/src/config.rs --file testdata/wire/loaded-config.json --file app/src/bridge/config.ts --file app/src/state/config.ts --file app/src/state/config.test.ts --file crates/library/src/config.rs
```
