# T-004: config store, OS-keychain secrets, and Tauri commands
**Depends:** T-003b | **Crates:** `crates/library`, `src-tauri` | **Executor:** Aider

**Files to create/modify:**
`crates/library/Cargo.toml` (deps),
`crates/library/src/lib.rs` (modify),
`crates/library/src/config.rs` (new),
`crates/library/src/secrets.rs` (new),
`src-tauri/src/lib.rs` (modify)

> **Scope split.** The original T-004 also covered `app/src/bridge/config.ts` and
> `app/src/state/config.ts`. Those are **T-004b**, so the Rust review is not mixed with a
> TypeScript one. This task ends at the Tauri command boundary.

## Goal
Persist non-secret settings to `config.json` in the app config directory, keep secrets in
the OS keychain, and expose both to the frontend as Tauri commands. Loading must never
fail: a missing or corrupt file yields defaults plus a warning, never a crash and never
silent data loss.

## Verified before writing this brief — do not re-derive
1. **`keyring` 4.1.6.** `Entry::new(service, user)?`, `.set_password(&str)`,
   `.get_password() -> Result<String>`, `.delete_credential()`. Compiled *and executed*
   against the real Windows Credential Manager on 2026-08-23: set/get/delete round-trips.
2. **Feature flags are the trap.** Defaults are `v1` + `windows-native-keyring-store` +
   `zbus-secret-service-keyring-store`; **the macOS backend is NOT a default**. Without
   `apple-native-keyring-store` a macOS build compiles and then has no store at runtime.
   All three are enabled explicitly below; enabling them cross-platform is harmless
   (verified compiling on Windows with all three on).
3. **Atomic replace works on Windows.** `write tmp -> sync_all -> fs::rename` over an
   existing file replaces contents and consumes the temp. Verified empirically, same day.

## Dependencies
`crates/library/Cargo.toml`, exactly:
```toml
[dependencies]
create-core = { path = "../create-core" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
thiserror = "2.0"
keyring = { version = "4.1", features = [
    "apple-native-keyring-store",
    "windows-native-keyring-store",
    "zbus-secret-service-keyring-store",
] }

[dev-dependencies]
tempfile = "3"
```

## Spec — reference implementation

### `crates/library/src/config.rs`
```rust
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::LibraryError;

/// Bumped when `Config`'s shape changes incompatibly, so a future build can migrate
/// rather than guess.
pub const CONFIG_SCHEMA_VERSION: u32 = 1;

/// File name inside the app config directory.
pub const CONFIG_FILE: &str = "config.json";

/// Whether ComfyUI runs on this machine or in Comfy Cloud.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ComfyMode {
    #[default]
    Local,
    Cloud,
}

/// How to reach ComfyUI. Never holds an API key -- that lives in the keychain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct ComfyConfig {
    pub mode: ComfyMode,
    /// Endpoint for local mode. `None` means the documented default,
    /// `http://127.0.0.1:8188`.
    #[serde(default)]
    pub url: Option<String>,
    /// Path to the `comfy` binary when it is not on `PATH` (`COMFY_BIN`).
    #[serde(default)]
    pub comfy_bin: Option<String>,
}

/// Which family of HTTP API the lyric LLM speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    /// Any OpenAI-compatible endpoint: Ollama, LM Studio, llama.cpp, vLLM, OpenRouter.
    #[default]
    OpenAiCompat,
    Ollama,
    OpenAi,
    Anthropic,
}

/// Lyric-writing model settings. Never holds an API key.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LlmConfig {
    pub provider: LlmProvider,
    #[serde(default)]
    pub base_url: Option<String>,
    /// Model id as the endpoint names it, e.g. `"gemma4:12b"`.
    #[serde(default)]
    pub model: Option<String>,
}

/// Everything persisted to `config.json`.
///
/// **Secrets are never stored here.** API keys go to the OS keychain
/// (`crate::secrets`); this file is plain text the user may share when reporting a bug.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub comfy: ComfyConfig,
    #[serde(default)]
    pub llm: Option<LlmConfig>,
    /// `ModelProfile::id` last used for audio.
    #[serde(default)]
    pub default_profile_id: Option<String>,
}

fn default_schema_version() -> u32 {
    CONFIG_SCHEMA_VERSION
}

impl Default for Config {
    fn default() -> Self {
        Self {
            schema_version: CONFIG_SCHEMA_VERSION,
            comfy: ComfyConfig::default(),
            llm: None,
            default_profile_id: None,
        }
    }
}

/// Something went wrong loading config, but not badly enough to stop the app.
///
/// Surfaced to the user rather than logged and forgotten: silently reverting someone's
/// settings to defaults is worse than saying so.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ConfigWarning {
    /// No config file yet -- normal on first run.
    Missing,
    /// The file existed but could not be parsed. It was moved aside, not deleted.
    Corrupt {
        /// Where the unreadable file was preserved.
        backup: String,
        /// Parser message, for the bug report.
        detail: String,
    },
}

/// Config plus anything the user should be told about loading it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedConfig {
    pub config: Config,
    #[serde(default)]
    pub warnings: Vec<ConfigWarning>,
}

/// Loads `config.json` from `dir`. **Never fails.**
///
/// Missing file -> defaults + [`ConfigWarning::Missing`]. Unparseable file -> the bad
/// file is renamed to `config.json.corrupt-<n>` and defaults are returned with
/// [`ConfigWarning::Corrupt`]; the user's data is preserved for recovery, never
/// overwritten in place.
pub fn load(dir: &Path) -> LoadedConfig {
    let path = dir.join(CONFIG_FILE);
    let text = match fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => {
            return LoadedConfig {
                config: Config::default(),
                warnings: vec![ConfigWarning::Missing],
            }
        }
    };

    match serde_json::from_str::<Config>(&text) {
        Ok(config) => LoadedConfig {
            config,
            warnings: Vec::new(),
        },
        Err(e) => {
            let backup = next_corrupt_path(dir);
            // A failed rename must not stop the app starting; the warning still fires.
            let _ = fs::rename(&path, &backup);
            LoadedConfig {
                config: Config::default(),
                warnings: vec![ConfigWarning::Corrupt {
                    backup: backup.to_string_lossy().into_owned(),
                    detail: e.to_string(),
                }],
            }
        }
    }
}

/// First unused `config.json.corrupt-<n>` in `dir`, so repeated bad starts never
/// clobber an earlier salvaged copy.
fn next_corrupt_path(dir: &Path) -> PathBuf {
    for n in 1..1000 {
        let candidate = dir.join(format!("{CONFIG_FILE}.corrupt-{n}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{CONFIG_FILE}.corrupt"))
}

/// Writes `config.json` atomically: temp file in the same directory, flushed to disk,
/// then renamed over the target.
///
/// Same-directory temp keeps the rename on one volume, where it is atomic. A crash
/// mid-write therefore leaves either the old file or the new one, never a half-written
/// config that would look "corrupt" on next start.
pub fn save(dir: &Path, config: &Config) -> Result<(), LibraryError> {
    fs::create_dir_all(dir)?;
    let target = dir.join(CONFIG_FILE);
    let tmp = dir.join(format!("{CONFIG_FILE}.tmp"));

    let json = serde_json::to_string_pretty(config)?;
    {
        let mut file = fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&tmp, &target)?;
    Ok(())
}
```

### `crates/library/src/secrets.rs`
```rust
use keyring::Entry;

use crate::LibraryError;

/// Keychain service name. Shared by every latentCreate secret.
const SERVICE: &str = "latentCreate";

/// The secrets this app is allowed to store.
///
/// A closed set on purpose: the frontend names a secret by string, and without a
/// whitelist a compromised or buggy webview could write arbitrary entries into the
/// user's keychain.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKey {
    /// API key for Comfy Cloud.
    ComfyCloudApiKey,
    /// API key for the lyric LLM provider.
    LlmApiKey,
}

impl SecretKey {
    /// Stable keychain account name. Changing one of these orphans existing entries.
    pub fn as_str(self) -> &'static str {
        match self {
            SecretKey::ComfyCloudApiKey => "comfy_cloud_api_key",
            SecretKey::LlmApiKey => "llm_api_key",
        }
    }

    /// Parses a name from the frontend, rejecting anything not in the whitelist.
    pub fn parse(name: &str) -> Result<Self, LibraryError> {
        match name {
            "comfy_cloud_api_key" => Ok(SecretKey::ComfyCloudApiKey),
            "llm_api_key" => Ok(SecretKey::LlmApiKey),
            other => Err(LibraryError::UnknownSecret(other.to_string())),
        }
    }
}

fn entry(key: SecretKey) -> Result<Entry, LibraryError> {
    Entry::new(SERVICE, key.as_str()).map_err(LibraryError::from)
}

/// Stores `value` in the OS keychain, replacing any existing entry.
pub fn set_secret(key: SecretKey, value: &str) -> Result<(), LibraryError> {
    entry(key)?.set_password(value).map_err(LibraryError::from)
}

/// Reads a secret.
///
/// **Never expose this through a Tauri command.** Secret *values* must not cross into
/// the webview: Rust reads them when it builds an outbound request, and the frontend
/// only ever learns whether one exists ([`has_secret`]).
pub fn get_secret(key: SecretKey) -> Result<String, LibraryError> {
    entry(key)?.get_password().map_err(LibraryError::from)
}

/// Whether a secret is stored. Any keychain error reads as "not stored" -- the caller
/// wants a UI checkmark, not an error path.
pub fn has_secret(key: SecretKey) -> bool {
    entry(key).and_then(|e| e.get_password().map_err(LibraryError::from)).is_ok()
}

/// Removes a secret. Deleting one that does not exist is not an error.
pub fn delete_secret(key: SecretKey) -> Result<(), LibraryError> {
    match entry(key)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(LibraryError::from(e)),
    }
}
```

### `crates/library/src/lib.rs`
Keep the existing crate docs and `test_crate_name_is_stable`; add:
```rust
pub mod config;
pub mod secrets;

pub use config::{Config, ConfigWarning, LoadedConfig};
pub use secrets::SecretKey;

use thiserror::Error;

/// Anything that can go wrong reading or writing the library.
#[derive(Debug, Error)]
pub enum LibraryError {
    /// Filesystem failure.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// Malformed JSON on write, or a value that cannot be serialised.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    /// OS keychain failure.
    #[error("keychain error: {0}")]
    Keyring(#[from] keyring::Error),
    /// The frontend named a secret outside the whitelist.
    #[error("unknown secret name: {0}")]
    UnknownSecret(String),
}
```

### `src-tauri/src/lib.rs`
Keep `app_version` and its test. Add managed state holding the config directory, and five
commands. Errors cross to the frontend as `String` — the webview cannot do anything with a
typed Rust error, and the message is what the UI shows.

```rust
use std::path::PathBuf;
use tauri::Manager;

/// Resolved once at startup so every command shares one location.
struct ConfigDir(PathBuf);

#[tauri::command]
fn load_config(state: tauri::State<'_, ConfigDir>) -> library::LoadedConfig {
    library::config::load(&state.0)
}

#[tauri::command]
fn save_config(
    state: tauri::State<'_, ConfigDir>,
    config: library::Config,
) -> Result<(), String> {
    library::config::save(&state.0, &config).map_err(|e| e.to_string())
}

#[tauri::command]
fn set_secret(name: String, value: String) -> Result<(), String> {
    let key = library::SecretKey::parse(&name).map_err(|e| e.to_string())?;
    library::secrets::set_secret(key, &value).map_err(|e| e.to_string())
}

#[tauri::command]
fn has_secret(name: String) -> Result<bool, String> {
    let key = library::SecretKey::parse(&name).map_err(|e| e.to_string())?;
    Ok(library::secrets::has_secret(key))
}

#[tauri::command]
fn delete_secret(name: String) -> Result<(), String> {
    let key = library::SecretKey::parse(&name).map_err(|e| e.to_string())?;
    library::secrets::delete_secret(key).map_err(|e| e.to_string())
}
```
In `run()`, before `.run(...)`, add a `.setup(...)` that resolves
`app.path().app_config_dir()` and `app.manage(ConfigDir(dir))`, and extend
`invoke_handler` to `tauri::generate_handler![app_version, load_config, save_config, set_secret, has_secret, delete_secret]`.

**There is deliberately no `get_secret` command.** If the exact `app_config_dir()` call
does not compile, consult the Tauri 2 docs and adjust — do not invent an API.

## Tests

### `config.rs` — all use `tempfile::tempdir()`
- `test_load_missing_returns_defaults_with_warning` — empty dir yields `Config::default()`
  and exactly `[ConfigWarning::Missing]`.
- `test_save_then_load_roundtrips` — a non-default config survives.
- `test_load_corrupt_preserves_file_and_returns_defaults` — write `"{ not json"`, load,
  then assert: defaults returned, one `Corrupt` warning, the backup path **exists and
  still holds the original bytes**, and `config.json` no longer parses as present. Losing
  a user's settings silently is the failure this guards.
- `test_repeated_corrupt_loads_do_not_overwrite_backup` — two corrupt loads produce
  `corrupt-1` and `corrupt-2`.
- `test_save_leaves_no_temp_file` — after `save`, no `config.json.tmp` remains.
- `test_save_creates_missing_directory`.
- `test_secrets_never_appear_in_config_json` — build a config, save it, read the raw file
  as a string and assert it contains none of `"api_key"`, `"password"`, `"secret"`,
  `"token"`.

### `secrets.rs`
- `test_parse_rejects_unknown_secret_name` — `SecretKey::parse("../../etc/passwd")` and
  `parse("arbitrary")` both return `LibraryError::UnknownSecret`. **This test must not
  touch the keychain.**
- `test_secret_key_names_are_stable` — assert the two `as_str()` values literally, since
  changing one orphans every existing user's stored key.
- **Any test that actually reads or writes the keychain must be `#[ignore]`d**, with a
  comment explaining why: CI's headless Linux runner has no secret service, so a live
  keychain test would fail there for reasons unrelated to the code. Write **one** such
  test, `test_set_get_delete_roundtrip`, marked `#[ignore]`, for manual runs via
  `cargo test -p library -- --ignored`.

## Acceptance criteria
- [ ] `npm run gate` green from the repo root
- [ ] `cargo test -p library` passes; ignored keychain test is skipped, not failing
- [ ] No `get_secret` Tauri command exists anywhere
- [ ] `config.json` written in tests contains no secret-shaped keys
- [ ] Public types, enums and functions documented per CONVENTIONS.md
- [ ] No dependencies beyond those listed
- [ ] No changes outside the listed files

## Out of scope
The TypeScript bridge and Zustand store (T-004b). Any UI. Reading config anywhere other
than through these commands. Migration between schema versions (`schema_version` is
recorded now so a future migration is possible; nothing migrates yet).

## Notes for the executor
- `library` must **not** depend on `tauri`. It takes a `&Path`; the shell resolves it.
- Do not add a command that returns a secret value, however convenient.
- Ask of every test you write: *would this fail if the thing it guards were broken?*

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --file crates/library/Cargo.toml --file crates/library/src/lib.rs --file crates/library/src/config.rs --file crates/library/src/secrets.rs --file src-tauri/src/lib.rs
```
