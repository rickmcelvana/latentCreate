# T-107a: profile loader (shipped + user directories)
**Depends:** T-003 (`ModelProfile`), T-004 (`library::config`) | **Crate/dir:** `crates/library`
**Files to create/modify:**
- `crates/library/src/profiles.rs` (create)
- `crates/library/src/lib.rs` (modify: one `pub mod`, four `pub use`)

## Goal
`library` loads model profiles from two directories -- the shipped `profiles/` and the
user's own -- into one id-keyed set, with a user profile replacing a shipped one of the
same id. Loading **never fails**: an unreadable or malformed file becomes a warning and is
skipped, so one bad JSON file cannot stop the app offering every other model. This is the
loader the setup wizard (T-110/T-111) and AudioStudio read their model list from.

## Spec
Exactly the reference implementation below. The contract, restated so the tests read as
claims rather than mechanics:

- **Never fails.** No `Result` on either entry point, mirroring `config::load` (T-004).
  Every failure is a `ProfileWarning` the UI can show; the app carries on with what it
  could read.
- **A missing directory is silent.** Having no user profile directory is the normal
  first-run state, not a fault. Only a directory that exists and cannot be listed warns.
- **File-name order decides duplicates**, not `read_dir` order -- which is
  platform-dependent, so two machines would otherwise run different models from the same
  directory. Within one directory the first file by name wins and the loser is named in a
  `DuplicateId` warning.
- **User wins across directories, wholesale.** No field-level merge: a half-overridden
  profile would describe a model nobody has ever run. The replacement is reported as
  `Shadowed`, because a user who edits the shipped file and sees no effect otherwise has
  no way to find out why.
- **Only `*.json` files are considered.** A `README.md` or a `.bak` beside the profiles is
  not a broken profile and must not warn -- warnings the user learns to ignore are worse
  than no warnings.
- **An empty id never enters the map.** It cannot be selected, and it would collide
  silently with the next one.
- `ProfileSet` and every type it contains are `Serialize`/`Deserialize`: they cross the
  Tauri boundary in T-110 (CONVENTIONS, Rust bullet 5).

Directory *locations* are the caller's business -- both entry points take `&Path`. The
shipped directory is a Tauri resource path and the user directory sits under the app data
dir; resolving them is T-110's wiring, not this task's. `PROFILES_DIR` is exported for
that caller.

## Reference implementation
Transcribe verbatim. This compiles, `cargo fmt` is a no-op on it, `cargo clippy
--all-targets -- -D warnings` is clean, and its 7 tests pass.

### `crates/library/src/profiles.rs` (new file, complete)
```rust
//! Model profile loading: the shipped `profiles/` directory plus the user's own.
//!
//! Mirrors `config`'s contract -- loading **never fails**. A profile file that
//! cannot be read or parsed becomes a warning and is skipped, because one bad
//! JSON file must not stop the app offering every other model. User profiles
//! win id collisions against shipped ones; that is the documented override
//! mechanism (ARCHITECTURE 5), and it is reported rather than silent.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use create_core::profile::ModelProfile;
use serde::{Deserialize, Serialize};

/// Directory name holding profiles, in both the shipped resource directory and
/// the app data directory.
pub const PROFILES_DIR: &str = "profiles";

/// Where a profile was read from. User profiles win id collisions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProfileSource {
    /// Shipped with the app.
    Shipped,
    /// Written by the user, in the app data directory.
    User,
}

/// A profile plus where it came from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LoadedProfile {
    pub profile: ModelProfile,
    pub source: ProfileSource,
    /// File the profile was read from, for the diagnostics pane.
    pub path: PathBuf,
}

/// Something about profile loading the user should be told.
///
/// Every variant is recoverable by design: the app carries on with the profiles
/// it could read. Silence would leave a user staring at a missing model with no
/// way to learn why.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ProfileWarning {
    /// The directory exists but could not be listed.
    DirUnreadable { dir: String, detail: String },
    /// The file could not be read.
    Unreadable { path: String, detail: String },
    /// The file is not a valid profile.
    Malformed { path: String, detail: String },
    /// A profile declared an empty id, which cannot be selected or overridden.
    EmptyId { path: String },
    /// Two files in one directory declare the same id. The first in file-name
    /// order is kept.
    DuplicateId {
        id: String,
        kept: String,
        skipped: String,
    },
    /// A user profile replaced a shipped one. Not an error -- it is how a user
    /// customises a shipped model -- but it explains why edits to the shipped
    /// file have no effect.
    Shadowed {
        id: String,
        shipped: String,
        user: String,
    },
}

/// Every profile the app can offer, keyed by [`ModelProfile::id`], plus
/// anything worth reporting about the load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileSet {
    pub profiles: BTreeMap<String, LoadedProfile>,
    #[serde(default)]
    pub warnings: Vec<ProfileWarning>,
}

/// Reads every `*.json` profile in one directory. **Never fails.**
///
/// A missing directory yields no profiles and no warning: having no user
/// profile directory is the normal first-run state, not a fault. Files are
/// visited in file-name order so a duplicate id resolves identically on every
/// platform -- `read_dir` order is not.
pub fn load_dir(
    dir: &Path,
    source: ProfileSource,
) -> (BTreeMap<String, LoadedProfile>, Vec<ProfileWarning>) {
    let mut profiles = BTreeMap::new();
    let mut warnings = Vec::new();

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return (profiles, warnings),
        Err(e) => {
            warnings.push(ProfileWarning::DirUnreadable {
                dir: dir.to_string_lossy().into_owned(),
                detail: e.to_string(),
            });
            return (profiles, warnings);
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    paths.sort();

    for path in paths {
        let display = path.to_string_lossy().into_owned();
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(e) => {
                warnings.push(ProfileWarning::Unreadable {
                    path: display,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        let profile: ModelProfile = match serde_json::from_str(&text) {
            Ok(profile) => profile,
            Err(e) => {
                warnings.push(ProfileWarning::Malformed {
                    path: display,
                    detail: e.to_string(),
                });
                continue;
            }
        };
        if profile.id.is_empty() {
            warnings.push(ProfileWarning::EmptyId { path: display });
            continue;
        }
        if let Some(kept) = profiles.get(&profile.id) {
            warnings.push(ProfileWarning::DuplicateId {
                id: profile.id.clone(),
                kept: kept.path.to_string_lossy().into_owned(),
                skipped: display,
            });
            continue;
        }
        profiles.insert(
            profile.id.clone(),
            LoadedProfile {
                profile,
                source,
                path,
            },
        );
    }

    (profiles, warnings)
}

/// Loads shipped and user profiles into one set. **Never fails.**
///
/// A user profile with the same id as a shipped one replaces it entirely --
/// there is no field-level merge, because a half-overridden profile would be a
/// model nobody has ever run. The replacement is reported as
/// [`ProfileWarning::Shadowed`].
pub fn load(shipped_dir: &Path, user_dir: &Path) -> ProfileSet {
    let (mut profiles, mut warnings) = load_dir(shipped_dir, ProfileSource::Shipped);
    let (user_profiles, user_warnings) = load_dir(user_dir, ProfileSource::User);
    warnings.extend(user_warnings);

    for (id, user) in user_profiles {
        if let Some(shipped) = profiles.get(&id) {
            warnings.push(ProfileWarning::Shadowed {
                id: id.clone(),
                shipped: shipped.path.to_string_lossy().into_owned(),
                user: user.path.to_string_lossy().into_owned(),
            });
        }
        profiles.insert(id, user);
    }

    ProfileSet { profiles, warnings }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Smallest profile that satisfies the schema, for id/collision tests.
    fn profile_json(id: &str) -> String {
        format!(
            r#"{{
  "id": "{id}",
  "display_name": "Test {id}",
  "kind": "music",
  "license": "MIT",
  "comfy": {{ "output": {{ "save_node": "SaveAudioAdvanced" }} }},
  "inputs": {{}}
}}"#
        )
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        fs::write(&path, body).expect("write fixture");
        path
    }

    /// Protects: no user profile directory is the normal first run, not a
    /// fault. A warning here would train users to ignore warnings.
    #[test]
    fn test_load_dir_missing_directory_is_silent() {
        let dir = tempfile::tempdir().expect("tempdir");
        let absent = dir.path().join("no-such-dir");
        let (profiles, warnings) = load_dir(&absent, ProfileSource::User);
        assert!(profiles.is_empty());
        assert!(warnings.is_empty());
    }

    /// Protects: the override mechanism -- a user profile replaces the shipped
    /// one of the same id entirely, and the replacement is reported so an
    /// edit to the shipped file that "does nothing" is explainable.
    #[test]
    fn test_user_profile_replaces_shipped_and_reports_it() {
        let shipped_dir = tempfile::tempdir().expect("tempdir");
        let user_dir = tempfile::tempdir().expect("tempdir");
        let shipped_path = write(shipped_dir.path(), "ace.json", &profile_json("ace"));
        let user_path = write(user_dir.path(), "mine.json", &profile_json("ace"));

        let set = load(shipped_dir.path(), user_dir.path());

        assert_eq!(set.profiles.len(), 1);
        let loaded = set.profiles.get("ace").expect("ace profile");
        assert_eq!(loaded.source, ProfileSource::User);
        assert_eq!(loaded.path, user_path);
        assert_eq!(
            set.warnings,
            vec![ProfileWarning::Shadowed {
                id: "ace".to_string(),
                shipped: shipped_path.to_string_lossy().into_owned(),
                user: user_path.to_string_lossy().into_owned(),
            }]
        );
    }

    /// Protects: one unparseable file must not hide every other model. The
    /// bad file is named; the good one still loads.
    #[test]
    fn test_malformed_profile_is_skipped_and_others_still_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "good.json", &profile_json("good"));
        let bad_path = write(dir.path(), "bad.json", "{ not json");

        let (profiles, warnings) = load_dir(dir.path(), ProfileSource::Shipped);

        assert_eq!(profiles.len(), 1);
        assert!(profiles.contains_key("good"));
        assert_eq!(warnings.len(), 1);
        match &warnings[0] {
            ProfileWarning::Malformed { path, detail } => {
                assert_eq!(path, &bad_path.to_string_lossy().into_owned());
                assert!(!detail.is_empty());
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
    }

    /// Protects: warnings stay meaningful. A README or a stray `.bak` beside
    /// the profiles is not a broken profile.
    #[test]
    fn test_non_json_files_are_ignored_silently() {
        let dir = tempfile::tempdir().expect("tempdir");
        write(dir.path(), "README.md", "not a profile");
        write(dir.path(), "ace.json.bak", "{ not json");

        let (profiles, warnings) = load_dir(dir.path(), ProfileSource::User);

        assert!(profiles.is_empty());
        assert!(warnings.is_empty());
    }

    /// Protects: a duplicate id resolves the same way everywhere. `read_dir`
    /// order is platform-dependent, so the first file in *file-name* order
    /// wins -- otherwise two machines would run different models from the
    /// same directory.
    #[test]
    fn test_duplicate_id_keeps_first_by_file_name() {
        let dir = tempfile::tempdir().expect("tempdir");
        let first = write(dir.path(), "a-first.json", &profile_json("dup"));
        let second = write(dir.path(), "z-second.json", &profile_json("dup"));

        let (profiles, warnings) = load_dir(dir.path(), ProfileSource::User);

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles.get("dup").expect("dup profile").path, first);
        assert_eq!(
            warnings,
            vec![ProfileWarning::DuplicateId {
                id: "dup".to_string(),
                kept: first.to_string_lossy().into_owned(),
                skipped: second.to_string_lossy().into_owned(),
            }]
        );
    }

    /// Protects: an empty id never enters the map. It cannot be selected, and
    /// it would collide with the next empty-id profile silently.
    #[test]
    fn test_empty_id_is_rejected() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = write(dir.path(), "empty.json", &profile_json(""));

        let (profiles, warnings) = load_dir(dir.path(), ProfileSource::Shipped);

        assert!(profiles.is_empty());
        assert_eq!(
            warnings,
            vec![ProfileWarning::EmptyId {
                path: path.to_string_lossy().into_owned(),
            }]
        );
    }

    /// Protects: the profiles this repo actually ships parse from disk under
    /// the current schema. A schema change that breaks a shipped file fails
    /// here rather than at first run.
    #[test]
    fn test_shipped_profiles_directory_loads_every_model() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
        let (profiles, warnings) = load_dir(&shipped, ProfileSource::Shipped);

        assert!(warnings.is_empty(), "shipped profiles warned: {warnings:?}");
        assert!(profiles.contains_key("ace-step-1.5-turbo"));
        assert!(profiles.contains_key("minimax-music-3"));
        for loaded in profiles.values() {
            assert_eq!(loaded.source, ProfileSource::Shipped);
        }
    }
}
```

### `crates/library/src/lib.rs` (complete file after the change)
Two changes only: `pub mod profiles;` between `config` and `secrets`, and four `pub use`
lines between the `config::LoadedConfig` and `secrets::SecretKey` re-exports -- alphabetical
by module then by type, matching what is there. Pinned precisely because T-105b lost a
round trip to a hand-waved "alphabetical".

```rust
//! On-disk store: projects, tracks, provenance sidecars, config.
//!
//! JSON files under the app data dir, no database (ARCHITECTURE.md §8).
//! Secrets live in the OS keychain, never in config. Populated by T-004.

pub mod config;
pub mod profiles;
pub mod secrets;

/// Re-export of [`config::Config`].
pub use config::Config;
/// Re-export of [`config::ConfigWarning`].
pub use config::ConfigWarning;
/// Re-export of [`config::LoadedConfig`].
pub use config::LoadedConfig;
/// Re-export of [`profiles::LoadedProfile`].
pub use profiles::LoadedProfile;
/// Re-export of [`profiles::ProfileSet`].
pub use profiles::ProfileSet;
/// Re-export of [`profiles::ProfileSource`].
pub use profiles::ProfileSource;
/// Re-export of [`profiles::ProfileWarning`].
pub use profiles::ProfileWarning;
/// Re-export of [`secrets::SecretKey`].
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

#[cfg(test)]
mod tests {
    /// Gives the crate a test target from the start and pins its published
    /// name, so a rename cannot silently break the Tauri shell's path deps.
    #[test]
    fn test_crate_name_is_stable() {
        assert_eq!(env!("CARGO_PKG_NAME"), "library");
    }
}
```

## Acceptance criteria
- [ ] `cargo test -p library` passes; `library` goes from 11 to **18 tests** (plus the 1
      ignored live-keychain test, unchanged)
- [ ] `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` clean
- [ ] `npm run gate` green
- [ ] no changes outside the two listed files
- [ ] `crates/library/Cargo.toml` unchanged -- `serde`, `serde_json` and the dev-dep
      `tempfile` are already present; **no new dependencies**

## Out of scope
- Resolving the real shipped/user directory paths (T-110 wiring).
- Any Tauri command or frontend surface for the profile list (T-110/T-111).
- Checking a profile's slot addresses against its template -- that is **T-107b**.
- Custom workflow import (ARCHITECTURE 5b, Phase 3).

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/create-core/src/profile.rs --read crates/library/src/config.rs --file crates/library/src/profiles.rs --file crates/library/src/lib.rs
```
`create-core/src/profile.rs` is `--read` because the reference code constructs and
deserialises `ModelProfile`; `config.rs` because the new module deliberately mirrors its
never-fails contract and warning shape. Neither may be edited (WORKFLOW 3).
