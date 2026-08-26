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
