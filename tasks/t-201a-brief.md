# T-201a: atomic writes, and the lyric/project types the store needs
**Depends:** none | **Crate/dir:** crates/create-core, crates/library
**Files to create/modify:**
- `crates/create-core/src/project.rs` (modify)
- `crates/library/src/atomic.rs` (create)
- `crates/library/src/config.rs` (modify)
- `crates/library/src/lib.rs` (modify)

## Goal

Give the project store the two things it needs before it can be written: one atomic
JSON write shared by every file this crate owns, and the pure operations on `Project`
and `LyricDoc` that the store will persist. No new store functions land here -- T-201b
and T-201c add the naming rules and the on-disk store on top of this.

## Spec

### 1. `create-core`: `Project` gains a monotonic lyric sequence, and a constructor

`Project` gains one field, `next_lyric_seq: u32`, defaulting to `1` through a serde
default so project files written before it still load.

**The invariant, because it is the reason the field exists:** lyric document ids are
minted from this counter and it is never decremented, so a deleted document's id can
never be handed to a later one. Minting from the surviving file list instead would let
a track's provenance `LyricRef` end up pointing at unrelated lyrics -- a reproducible
track that reproduces the wrong song.

Add after the `albums` field, inside `pub struct Project`:

```rust
    /// Sequence number the next lyric document in this project will be minted from.
    ///
    /// Monotonic and **never reused**, even after a document is deleted. Minting from
    /// the surviving file list instead would hand a deleted document's id to a later
    /// one, and a track's provenance `LyricRef` would then point at unrelated lyrics.
    #[serde(default = "default_lyric_seq")]
    pub next_lyric_seq: u32,
}

fn default_lyric_seq() -> u32 {
    1
}

impl Project {
    /// A new, empty project. `created_at` is RFC 3339; the caller supplies it so
    /// nothing here reads a clock.
    pub fn new(
        slug: impl Into<String>,
        name: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            slug: slug.into(),
            name: name.into(),
            created_at: created_at.into(),
            tracks: Vec::new(),
            lyrics: Vec::new(),
            albums: Vec::new(),
            next_lyric_seq: default_lyric_seq(),
        }
    }
}
```

(The closing `}` above is the existing end of `pub struct Project` -- do not add a
second one.)

### 2. `create-core`: `LyricDoc` gains `push_version` and `approve`

Both are pure and take their timestamp as an argument, so no test here depends on a
clock. Add inside the existing `impl LyricDoc`, after `latest()`:

```rust
    /// Appends a version and returns its number.
    ///
    /// Numbers continue from the highest already present, so a restored older
    /// version cannot collide with one that came after it. Existing versions are
    /// never touched -- an edit produces a new version, it does not rewrite the
    /// text it came from.
    pub fn push_version(
        &mut self,
        text: impl Into<String>,
        source: LyricSource,
        created_at: impl Into<String>,
    ) -> u32 {
        let number = self.latest().map_or(1, |v| v.number + 1);
        self.versions.push(LyricVersion {
            number,
            text: text.into(),
            created_at: created_at.into(),
            source,
        });
        number
    }

    /// Approves an existing version, returning `false` and changing nothing when
    /// no version has that number.
    ///
    /// Approval is what makes a version available to AudioStudio, so approving a
    /// number that does not exist must not clear the approval the user already
    /// made.
    pub fn approve(&mut self, number: u32) -> bool {
        if !self.versions.iter().any(|v| v.number == number) {
            return false;
        }
        self.approved = Some(number);
        true
    }
```

**`push_version` numbers from the highest version present, not from `versions.len()`.**
Those agree on every document that has never lost a version and differ on every document
that has; the test below is written against a document with a gap for exactly that
reason.

### 3. `library`: one atomic write, shared

New module `crates/library/src/atomic.rs`, `pub(crate)` only. `config::save` is
rewritten to call it, which is the whole point -- two hand-rolled rename dances in one
crate is two chances to get the temp-file handling subtly different.

Behaviour is unchanged from what `config::save` does today, plus two things it did not
do: parent directories are created for any depth, and a failed write removes its temp
file so a later `read_dir` never lists one.

### 4. `library`: wire the module

`crates/library/src/lib.rs` gains `mod atomic;` above `pub mod config;`. It is private:
nothing outside this crate writes into the library directory.

## Reference implementation

Compiled, `cargo fmt` clean, `cargo clippy --all-targets -- -D warnings` clean, and the
guards mutation-tested (see acceptance criteria). Transcribe it.

### `crates/library/src/atomic.rs` (new file, complete)

```rust
//! Atomic JSON writes, shared by every file this crate owns.
//!
//! One implementation rather than one per store: the rename dance is easy to
//! write subtly differently twice, and the difference only shows up as a
//! half-written file after a crash.

use std::ffi::OsString;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::LibraryError;

/// Writes `value` to `path` as pretty JSON, atomically.
///
/// The temp file is a sibling, so the rename stays on one volume where it is
/// atomic. A crash mid-write leaves either the old file or the new one, never a
/// half-written one that the next load would report as corrupt. Missing parent
/// directories are created, and a failed write removes its temp file so a later
/// directory listing never sees it.
pub(crate) fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), LibraryError> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value)?;
    let tmp = temp_path(path);
    if let Err(e) = write_all(&tmp, json.as_bytes()) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Create, write, flush to disk. Separate so the caller can clean up its temp
/// file on any of the three failing.
fn write_all(path: &Path, bytes: &[u8]) -> Result<(), LibraryError> {
    let mut file = fs::File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    Ok(())
}

/// `config.json` -> `config.json.tmp`.
///
/// Appended to the whole file name rather than set with `with_extension`, which
/// would turn `config.json` into `config.tmp` -- and two files differing only by
/// extension would then share one temp path.
fn temp_path(path: &Path) -> PathBuf {
    let mut name = OsString::from(path.as_os_str());
    name.push(".tmp");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Invariant: the value written is the value read back, and the parent
    /// directory is created rather than the write failing.
    #[test]
    fn test_write_json_creates_parents_and_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("deeper").join("thing.json");
        write_json(&path, &vec!["a".to_string(), "b".to_string()]).unwrap();

        let text = fs::read_to_string(&path).unwrap();
        let back: Vec<String> = serde_json::from_str(&text).unwrap();
        assert_eq!(back, vec!["a".to_string(), "b".to_string()]);
    }

    /// Invariant: no `.tmp` sibling survives a successful write. A store that
    /// lists a directory to find its contents would otherwise see one.
    #[test]
    fn test_write_json_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("thing.json");
        write_json(&path, &42u32).unwrap();

        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, vec!["thing.json".to_string()]);
    }

    /// Invariant: the temp path keeps the whole file name, so `a.json` and
    /// `a.md` never contend for one temp file.
    #[test]
    fn test_temp_path_appends_rather_than_replacing_the_extension() {
        assert_eq!(
            temp_path(Path::new("/tmp/config.json")),
            PathBuf::from("/tmp/config.json.tmp")
        );
    }
}
```

### `crates/library/src/config.rs` (replace `save` and drop one import)

Delete `use std::io::Write;` from the imports at the top (`use std::fs;` stays -- `load`
still uses it). Replace the whole of `pub fn save` and its doc comment with:

```rust
/// Writes `config.json` atomically (temp sibling, flushed, renamed over the target).
///
/// A crash mid-write leaves either the old file or the new one, never a half-written
/// config that would look "corrupt" on next start. The mechanism is
/// [`crate::atomic::write_json`], shared with the project store so there is one
/// rename dance in the crate rather than two that could drift.
pub fn save(dir: &Path, config: &Config) -> Result<(), LibraryError> {
    crate::atomic::write_json(&dir.join(CONFIG_FILE), config)
}
```

`config.rs`'s existing tests are unchanged and must still pass -- including
`test_save_leaves_no_temp_file`, which is what proves the refactor kept the behaviour.

### `crates/library/src/lib.rs` (one line)

```rust
mod atomic;
pub mod config;
```

### `crates/create-core/src/project.rs` tests (append inside the existing `mod tests`)

```rust
    /// Invariant: a new version continues from the highest number present, so a
    /// document whose latest version is 3 can never mint a second 3.
    #[test]
    fn test_push_version_continues_from_the_highest_number() {
        let mut doc = LyricDoc {
            id: LyricDocId("ld-0001".to_string()),
            title: None,
            versions: Vec::new(),
            approved: None,
        };
        assert_eq!(
            doc.push_version("one", LyricSource::Human, "2026-08-25T10:00:00Z"),
            1
        );
        assert_eq!(
            doc.push_version(
                "two",
                LyricSource::Llm {
                    model: "gemma4:12b-32k".to_string(),
                    prompt_optimized: false,
                },
                "2026-08-25T10:01:00Z",
            ),
            2
        );
        assert_eq!(
            doc.push_version(
                "three",
                LyricSource::Edited { from_version: 1 },
                "2026-08-25T10:02:00Z"
            ),
            3
        );
        assert_eq!(doc.versions.len(), 3);
        assert_eq!(doc.versions[0].text, "one");
    }

    /// Invariant: the next number comes from the highest version present, not
    /// from how many there are. A document that has lost a version -- or that
    /// was written by a build which numbered differently -- must not mint a
    /// number an existing version already holds, because a `LyricRef` in a
    /// track's provenance points at lyrics by number.
    #[test]
    fn test_push_version_after_a_gap_does_not_reuse_a_number() {
        let mut doc = LyricDoc {
            id: LyricDocId("ld-0001".to_string()),
            title: None,
            versions: vec![
                LyricVersion {
                    number: 1,
                    text: "first".to_string(),
                    created_at: "2026-08-25T10:00:00Z".to_string(),
                    source: LyricSource::Human,
                },
                LyricVersion {
                    number: 5,
                    text: "fifth".to_string(),
                    created_at: "2026-08-25T10:05:00Z".to_string(),
                    source: LyricSource::Human,
                },
            ],
            approved: Some(5),
        };
        assert_eq!(
            doc.push_version("next", LyricSource::Human, "2026-08-25T10:06:00Z"),
            6
        );
        let numbers: Vec<u32> = doc.versions.iter().map(|v| v.number).collect();
        assert_eq!(numbers, vec![1, 5, 6]);
    }

    /// Invariant: approving a number that does not exist leaves the approval the
    /// user already made untouched. Clearing it would silently withdraw a lyric
    /// from AudioStudio.
    #[test]
    fn test_approve_missing_version_keeps_the_previous_approval() {
        let mut doc = LyricDoc {
            id: LyricDocId("ld-0001".to_string()),
            title: None,
            versions: Vec::new(),
            approved: None,
        };
        doc.push_version("one", LyricSource::Human, "2026-08-25T10:00:00Z");
        assert!(doc.approve(1));
        assert_eq!(doc.approved, Some(1));

        assert!(!doc.approve(9));
        assert_eq!(doc.approved, Some(1));
    }

    /// Invariant: a project file written before `next_lyric_seq` existed still
    /// loads, and starts minting at 1 rather than 0.
    #[test]
    fn test_project_without_seq_field_defaults_to_one() {
        let json = r#"{"slug":"demo","name":"Demo","created_at":"2026-08-25T10:00:00Z"}"#;
        let project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.next_lyric_seq, 1);
        assert!(project.lyrics.is_empty());
    }
```

## Acceptance criteria

- [ ] `cargo test -p create-core` and `cargo test -p library` pass; `npm run gate` green.
- [ ] The four new `create-core` tests and the three new `atomic` tests exist and pass.
- [ ] Every pre-existing `config` test still passes, `test_save_leaves_no_temp_file`
      included -- the refactor changes the implementation, not the behaviour.
- [ ] `mod atomic;` is private; nothing outside the crate can call `write_json`.
- [ ] These four mutations each make a named test fail (verified before the brief was
      written; re-check any you touch):
      - `approve()` stops checking the version exists ->
        `test_approve_missing_version_keeps_the_previous_approval`
      - `push_version()` numbers from `versions.len() + 1` ->
        `test_push_version_after_a_gap_does_not_reuse_a_number`
      - `write_json` leaves its temp file behind -> `test_write_json_leaves_no_temp_file`
      - `temp_path` uses `with_extension("tmp")` ->
        `test_temp_path_appends_rather_than_replacing_the_extension`
- [ ] No changes outside the four listed files. **No new dependency** in this task --
      `chrono` arrives with T-201b, which is what needs a clock.

## Out of scope

- The project store itself (`create_project` / `load` / `list` / slug rules) -- T-201b
  and T-201c.
- Lyric document files on disk -- T-201c.
- Any Tauri command, and anything in `app/`.
- Deleting a project or a document. Deletes go to OS trash and no task has needed one
  yet; do not add a hard delete here.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read crates/create-core/src/generation.rs --file crates/create-core/src/project.rs --file crates/library/src/atomic.rs --file crates/library/src/config.rs --file crates/library/src/lib.rs
```

`generation.rs` is `--read` because `LyricDocId` is defined there and the tests construct
it; nothing in this task changes that file.
