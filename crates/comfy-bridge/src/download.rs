//! Model downloads into the directory ComfyUI itself searches (ARCHITECTURE 3).
//!
//! This replaced comfy-mcp's `download_model`, which wrote into comfy-cli's own
//! "workspace" -- on a Comfy Desktop install, a different drive from the one
//! ComfyUI reads, so a finished download never read as installed (T-519).
//! Here the destination comes from ComfyUI's `/internal/folder_paths`, and
//! because ComfyUI's model lists refresh live, a file renamed into place is
//! visible to the next readiness check with no restart.
//!
//! A transfer writes `<file>.partial` and renames on completion, so ComfyUI
//! never lists a half-written model. An interrupted transfer **resumes** from
//! the partial file with a `Range` request -- these files run to 17 GB.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use bytes::Bytes;
use futures_util::{Stream, StreamExt};
use tokio::io::AsyncWriteExt;

/// Why a model could not be downloaded, each with what to do about it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum DownloadError {
    /// ComfyUI does not report a directory for this folder.
    #[error("ComfyUI has no `{folder}` model folder. Update ComfyUI, or download the file into its models folder by hand.")]
    NoFolder {
        /// The profile's folder name.
        folder: String,
    },
    /// The directory ComfyUI reports does not exist here: ComfyUI is on another
    /// machine, or the path is on a drive that is not mounted.
    #[error("ComfyUI keeps `{folder}` models in {dir}, which is not on this computer. Install it from the computer ComfyUI runs on.")]
    NotOnThisMachine {
        /// The folder name.
        folder: String,
        /// The directory ComfyUI reported.
        dir: PathBuf,
    },
    /// The server answered with an error status.
    #[error(
        "The download server answered {status}. Try again later, or download the file by hand."
    )]
    Http {
        /// The HTTP status.
        status: u16,
    },
    /// The connection failed or dropped. The partial file is kept, so trying
    /// again resumes.
    #[error("The download stopped: {0}. Install again to resume where it left off.")]
    Network(String),
    /// The finished file is not the size the profile records -- a truncated or
    /// substituted download. The file is kept as `.partial`, never installed.
    #[error(
        "The download finished at {got} bytes but should be {expected}. Install again to retry."
    )]
    SizeMismatch {
        /// `size_bytes` from the profile.
        expected: u64,
        /// What arrived.
        got: u64,
    },
    /// The disk refused.
    #[error("Could not write {path}: {detail}")]
    Io {
        /// The file being written.
        path: PathBuf,
        /// The OS error.
        detail: String,
    },
    /// Stopped on request. The partial file is kept for a later resume.
    #[error("Download cancelled.")]
    Cancelled,
}

/// Where a model for `folder` belongs, given ComfyUI's search paths.
///
/// The first directory whose last component **is** the folder name -- so
/// `diffusion_models` lands in `...\diffusion_models`, not the legacy `...\unet`
/// ComfyUI also searches (COMFY-HTTP-SURFACE 3) -- else the first directory.
/// `exists` is injected so the rule is testable without a filesystem; a
/// directory that does not exist here is [`DownloadError::NotOnThisMachine`].
pub fn destination_dir(
    folder: &str,
    paths: &BTreeMap<String, Vec<PathBuf>>,
    exists: impl Fn(&Path) -> bool,
) -> Result<PathBuf, DownloadError> {
    let dirs =
        paths
            .get(folder)
            .filter(|d| !d.is_empty())
            .ok_or_else(|| DownloadError::NoFolder {
                folder: folder.to_string(),
            })?;
    let named = |d: &&PathBuf| {
        // ComfyUI reports Windows paths with backslashes; `Path` on another OS
        // would not split them, so compare the last component textually.
        let text = d.to_string_lossy();
        let last = text
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("");
        last.eq_ignore_ascii_case(folder)
    };
    let dir = dirs.iter().find(named).unwrap_or(&dirs[0]).clone();
    if exists(&dir) {
        Ok(dir)
    } else {
        Err(DownloadError::NotOnThisMachine {
            folder: folder.to_string(),
            dir,
        })
    }
}

/// The in-progress sibling of a destination file.
pub fn partial_path(dest: &Path) -> PathBuf {
    let mut os = dest.as_os_str().to_os_string();
    os.push(".partial");
    PathBuf::from(os)
}

/// One file to fetch.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    /// The curated, ungated source.
    pub url: String,
    /// Where the finished file goes.
    pub dest: PathBuf,
    /// `size_bytes` from the profile, checked on completion when known.
    pub expected_size: Option<u64>,
}

/// A reqwest client for model files: no overall timeout (a 17 GB file takes as
/// long as it takes), but a read timeout so a stalled connection fails.
pub fn download_client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(15))
        .read_timeout(Duration::from_secs(60))
        .build()
        .unwrap_or_default()
}

/// Fetch one file, resuming a partial download if one is there.
///
/// `on_progress(done, total)` is called as bytes arrive (callers throttle
/// what they emit). `cancel` is checked between chunks.
pub async fn run_transfer(
    client: &reqwest::Client,
    transfer: &Transfer,
    mut on_progress: impl FnMut(u64, Option<u64>),
    cancel: &AtomicBool,
) -> Result<(), DownloadError> {
    let partial = partial_path(&transfer.dest);
    let resume_from = tokio::fs::metadata(&partial)
        .await
        .map(|m| m.len())
        .unwrap_or(0);

    let mut request = client.get(&transfer.url);
    if resume_from > 0 {
        request = request.header("Range", format!("bytes={resume_from}-"));
    }
    let response = request
        .send()
        .await
        .map_err(|e| DownloadError::Network(e.to_string()))?;
    let status = response.status().as_u16();
    // 416: the partial is already complete (or longer than the file).
    if status == 416 && resume_from > 0 {
        return finish(&partial, &transfer.dest, transfer.expected_size).await;
    }
    if !(200..300).contains(&status) {
        return Err(DownloadError::Http { status });
    }
    // 206 honours the range; a plain 200 means the server ignored it and is
    // sending the whole file, so start over.
    let offset = if status == 206 { resume_from } else { 0 };
    let total = response
        .content_length()
        .map(|len| len + offset)
        .or(transfer.expected_size);
    let stream = response
        .bytes_stream()
        .map(|chunk| chunk.map_err(|e| e.to_string()));
    write_stream(stream, &partial, offset, total, &mut on_progress, cancel).await?;
    finish(&partial, &transfer.dest, transfer.expected_size).await
}

/// Append (or, at `offset == 0`, write) a byte stream to `partial`.
pub async fn write_stream<S>(
    mut stream: S,
    partial: &Path,
    offset: u64,
    total: Option<u64>,
    on_progress: &mut impl FnMut(u64, Option<u64>),
    cancel: &AtomicBool,
) -> Result<u64, DownloadError>
where
    S: Stream<Item = Result<Bytes, String>> + Unpin,
{
    let io = |e: std::io::Error| DownloadError::Io {
        path: partial.to_path_buf(),
        detail: e.to_string(),
    };
    let mut file = tokio::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(offset > 0)
        .truncate(offset == 0)
        .open(partial)
        .await
        .map_err(io)?;
    let mut done = offset;
    on_progress(done, total);
    while let Some(chunk) = stream.next().await {
        if cancel.load(Ordering::Relaxed) {
            file.flush().await.map_err(io)?;
            return Err(DownloadError::Cancelled);
        }
        let chunk = chunk.map_err(DownloadError::Network)?;
        file.write_all(&chunk).await.map_err(io)?;
        done += chunk.len() as u64;
        on_progress(done, total);
    }
    file.flush().await.map_err(io)?;
    Ok(done)
}

/// Check the size and rename the partial file into place.
pub async fn finish(
    partial: &Path,
    dest: &Path,
    expected: Option<u64>,
) -> Result<(), DownloadError> {
    let io = |e: std::io::Error| DownloadError::Io {
        path: dest.to_path_buf(),
        detail: e.to_string(),
    };
    let got = tokio::fs::metadata(partial).await.map_err(io)?.len();
    if let Some(expected) = expected {
        if got != expected {
            return Err(DownloadError::SizeMismatch { expected, got });
        }
    }
    tokio::fs::rename(partial, dest).await.map_err(io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;

    fn desktop_paths() -> BTreeMap<String, Vec<PathBuf>> {
        let path = format!(
            "{}/../../testdata/comfy-http/folder_paths_desktop.json",
            env!("CARGO_MANIFEST_DIR")
        );
        serde_json::from_str(&std::fs::read_to_string(path).expect("fixture")).expect("json")
    }

    /// Protects: on Comfy Desktop a diffusion model lands in the shared
    /// `diffusion_models` directory, not the legacy `unet` one listed first, and
    /// not the `output` sibling -- the live paths, not invented ones.
    #[test]
    fn test_destination_prefers_the_directory_named_for_the_folder() {
        let paths = desktop_paths();
        let dir = destination_dir("diffusion_models", &paths, |_| true).expect("dir");
        assert_eq!(
            dir,
            PathBuf::from(r"G:\Comfy-Desktop\ComfyUI-Shared\models\diffusion_models")
        );
        let dir = destination_dir("checkpoints", &paths, |_| true).expect("dir");
        assert_eq!(
            dir,
            PathBuf::from(r"G:\Comfy-Desktop\ComfyUI-Shared\models\checkpoints")
        );
    }

    #[test]
    fn test_destination_falls_back_to_the_first_directory() {
        let mut paths = BTreeMap::new();
        paths.insert(
            "vae".to_string(),
            vec![PathBuf::from("/srv/comfy/vae-models")],
        );
        let dir = destination_dir("vae", &paths, |_| true).expect("dir");
        assert_eq!(dir, PathBuf::from("/srv/comfy/vae-models"));
    }

    /// Protects: a ComfyUI whose directories are not on this machine refuses
    /// with a sentence saying so -- never a download to a guessed path.
    #[test]
    fn test_destination_refuses_a_directory_not_on_this_machine() {
        let err = destination_dir("loras", &desktop_paths(), |_| false).expect_err("absent");
        assert!(matches!(err, DownloadError::NotOnThisMachine { .. }));
        assert!(err.to_string().contains("not on this computer"));
    }

    #[test]
    fn test_destination_refuses_an_unknown_folder() {
        let err = destination_dir("unet", &desktop_paths(), |_| true).expect_err("none");
        assert_eq!(
            err,
            DownloadError::NoFolder {
                folder: "unet".into()
            }
        );
    }

    #[tokio::test]
    async fn test_write_stream_then_finish_installs_the_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("model.safetensors");
        let partial = partial_path(&dest);
        let chunks = stream::iter(vec![
            Ok(Bytes::from_static(b"abc")),
            Ok(Bytes::from_static(b"de")),
        ]);
        let mut seen = Vec::new();
        let done = write_stream(
            chunks,
            &partial,
            0,
            Some(5),
            &mut |d, _| seen.push(d),
            &AtomicBool::new(false),
        )
        .await
        .expect("written");
        assert_eq!(done, 5);
        assert_eq!(seen, [0, 3, 5]);
        assert!(!dest.exists(), "never visible before the rename");
        finish(&partial, &dest, Some(5)).await.expect("installed");
        assert_eq!(std::fs::read(&dest).expect("read"), b"abcde");
        assert!(!partial.exists());
    }

    /// Protects: a resumed transfer appends to what is already there.
    #[tokio::test]
    async fn test_write_stream_appends_when_resuming() {
        let dir = tempfile::tempdir().expect("tempdir");
        let partial = dir.path().join("m.partial");
        std::fs::write(&partial, b"abc").expect("seed");
        let chunks = stream::iter(vec![Ok(Bytes::from_static(b"de"))]);
        write_stream(
            chunks,
            &partial,
            3,
            Some(5),
            &mut |_, _| {},
            &AtomicBool::new(false),
        )
        .await
        .expect("written");
        assert_eq!(std::fs::read(&partial).expect("read"), b"abcde");
    }

    /// Protects: a truncated download is never renamed into ComfyUI's folder.
    #[tokio::test]
    async fn test_finish_refuses_the_wrong_size() {
        let dir = tempfile::tempdir().expect("tempdir");
        let dest = dir.path().join("m.safetensors");
        let partial = partial_path(&dest);
        std::fs::write(&partial, b"abc").expect("seed");
        let err = finish(&partial, &dest, Some(10)).await.expect_err("short");
        assert_eq!(
            err,
            DownloadError::SizeMismatch {
                expected: 10,
                got: 3
            }
        );
        assert!(!dest.exists());
        assert!(partial.exists(), "kept for a retry");
    }

    #[tokio::test]
    async fn test_cancel_stops_between_chunks_and_keeps_the_partial() {
        let dir = tempfile::tempdir().expect("tempdir");
        let partial = dir.path().join("m.partial");
        let chunks = stream::iter(vec![Ok(Bytes::from_static(b"abc"))]);
        let err = write_stream(
            chunks,
            &partial,
            0,
            None,
            &mut |_, _| {},
            &AtomicBool::new(true),
        )
        .await
        .expect_err("cancelled");
        assert_eq!(err, DownloadError::Cancelled);
        assert!(partial.exists());
    }
}
