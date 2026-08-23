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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_missing_returns_defaults_with_warning() {
        let dir = tempfile::tempdir().unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.warnings, vec![ConfigWarning::Missing]);
    }

    #[test]
    fn test_save_then_load_roundtrips() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            comfy: ComfyConfig {
                mode: ComfyMode::Cloud,
                url: Some("http://cloud.example".to_string()),
                comfy_bin: Some("/opt/comfy".to_string()),
            },
            llm: Some(LlmConfig {
                provider: LlmProvider::Anthropic,
                base_url: Some("http://llm.example".to_string()),
                model: Some("claude".to_string()),
            }),
            default_profile_id: Some("ace-step-1.5-turbo".to_string()),
        };
        save(dir.path(), &config).unwrap();
        let loaded = load(dir.path());
        assert_eq!(loaded.config, config);
        assert!(loaded.warnings.is_empty());
    }

    #[test]
    fn test_load_corrupt_preserves_file_and_returns_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        let bad = b"{ not json";
        std::fs::write(&path, bad).unwrap();

        let loaded = load(dir.path());
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.warnings.len(), 1);
        match &loaded.warnings[0] {
            ConfigWarning::Corrupt { backup, detail } => {
                assert!(!detail.is_empty());
                let backup_path = PathBuf::from(backup);
                assert!(backup_path.exists());
                let saved = std::fs::read_to_string(&backup_path).unwrap();
                assert_eq!(saved.as_bytes(), bad);
            }
            other => panic!("expected Corrupt warning, got {:?}", other),
        }
        assert!(!path.exists());
    }

    #[test]
    fn test_repeated_corrupt_loads_do_not_overwrite_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(CONFIG_FILE);
        std::fs::write(&path, b"{ bad 1").unwrap();
        load(dir.path());
        std::fs::write(&path, b"{ bad 2").unwrap();
        load(dir.path());

        let backup1 = dir.path().join(format!("{CONFIG_FILE}.corrupt-1"));
        let backup2 = dir.path().join(format!("{CONFIG_FILE}.corrupt-2"));
        assert!(backup1.exists());
        assert!(backup2.exists());
    }

    #[test]
    fn test_save_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config::default();
        save(dir.path(), &config).unwrap();
        let tmp = dir.path().join(format!("{CONFIG_FILE}.tmp"));
        assert!(!tmp.exists());
    }

    #[test]
    fn test_save_creates_missing_directory() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("a").join("b");
        let config = Config::default();
        save(&nested, &config).unwrap();
        assert!(nested.exists());
        assert!(nested.join(CONFIG_FILE).exists());
    }

    #[test]
    fn test_secrets_never_appear_in_config_json() {
        let dir = tempfile::tempdir().unwrap();
        let config = Config {
            schema_version: CONFIG_SCHEMA_VERSION,
            comfy: ComfyConfig {
                mode: ComfyMode::Local,
                url: Some("http://127.0.0.1:8188".to_string()),
                comfy_bin: Some("/usr/local/bin/comfy".to_string()),
            },
            llm: Some(LlmConfig {
                provider: LlmProvider::OpenAiCompat,
                base_url: Some("http://localhost:11434/v1".to_string()),
                model: Some("gemma4:12b".to_string()),
            }),
            default_profile_id: Some("ace-step-1.5-turbo".to_string()),
        };
        save(dir.path(), &config).unwrap();
        let raw = std::fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        let lower = raw.to_lowercase();
        assert!(
            !lower.contains("api_key"),
            "config.json must not contain api_key"
        );
        assert!(
            !lower.contains("password"),
            "config.json must not contain password"
        );
        assert!(
            !lower.contains("secret"),
            "config.json must not contain secret"
        );
        assert!(
            !lower.contains("token"),
            "config.json must not contain token"
        );
    }

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
}
