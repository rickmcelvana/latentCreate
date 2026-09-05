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
    /// the current schema. A schema change that breaks a shipped file -- or a
    /// hand-authored curated profile (T-511) with a typo -- fails here rather
    /// than vanishing silently at first run (`load_dir` only warns).
    #[test]
    fn test_shipped_profiles_directory_loads_every_model() {
        let shipped = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles");
        let (profiles, warnings) = load_dir(&shipped, ProfileSource::Shipped);

        assert!(warnings.is_empty(), "shipped profiles warned: {warnings:?}");
        assert!(profiles.contains_key("ace-step-1.5-turbo"));
        assert!(profiles.contains_key("minimax-music-3"));
        // Curated image models (T-511) ship the same way; pin them as they land.
        assert!(profiles.contains_key("flux-1-schnell-fp8"));
        assert!(profiles.contains_key("chroma-1-hd"));
        assert!(profiles.contains_key("sdxl-base-1.0"));
        for loaded in profiles.values() {
            assert_eq!(loaded.source, ProfileSource::Shipped);
            assert!(
                !loaded.profile.display_name.is_empty(),
                "{}: empty display_name",
                loaded.profile.id
            );
            assert!(
                !loaded.profile.license.is_empty(),
                "{}: empty license",
                loaded.profile.id
            );
        }
    }
}
