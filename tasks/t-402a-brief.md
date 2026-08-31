# T-402a: playback + visualizer -- the asset protocol and the audio path

**Depends:** T-401 (landed) | **Crate/dir:** src-tauri, crates/library
**Files to create/modify:**
- `src-tauri/tauri.conf.json` (modify: enable the asset protocol, set a CSP)
- `src-tauri/Cargo.toml` (modify: add the `protocol-asset` feature the asset protocol requires)
- `crates/library/src/tracks.rs` (modify: add `resolve_track_file` + tests)
- `src-tauri/src/tracks.rs` (modify: add the `track_audio_path` command)
- `src-tauri/src/lib.rs` (modify: register `tracks::track_audio_path`)

## Goal

The Library lists tracks whose `Track.file` is relative (`tracks/tr-0001.flac`), but nothing
resolves that to an absolute path the webview can play, and the asset protocol the webview needs
is not enabled. This brief makes a track's audio reachable end to end: the webview asks for
`track_audio_path(id)`, the backend resolves it to an absolute path inside the project's `tracks/`
directory, and the asset protocol is configured to serve files under the app config dir. No
frontend yet -- that is T-402b/c.

## Verified surfaces (do not re-derive)

- **`convertFileSrc(filePath, protocol?)`** lives in `@tauri-apps/api/core` (2.11.1 installed).
  It turns an absolute path into an `asset://localhost/...` URL (macOS/Linux) or
  `http://asset.localhost/...` (Windows/Android). Its doc requires `asset:` and
  `http://asset.localhost` in the CSP, and `enable: true` plus a `scope` array in the
  `assetProtocol` config.
- **`app.security.assetProtocol`** is `{ "enable": bool, "scope": [...] }` (camelCase). The scope
  entries are glob patterns, each optionally starting with a base-directory variable; `$APPCONFIG`
  resolves to `app_config_dir()` -- exactly the directory `library` writes projects into
  (PROJECT.md "Where the app writes"). The matcher uses native separators and
  `require_literal_separator`, so `$APPCONFIG/projects/**` matches nested track files.
- **`app.security.csp`** is `null` today, which means no CSP is injected. Setting it requires a
  complete policy. The Tauri-recommended base is `default-src 'self'; connect-src ipc:
  http://ipc.localhost` (tauri-utils config.rs). The app loads no inline scripts, no inline
  styles, no images beyond the favicon, and no external resources (grepped), so the policy below
  is complete.
- **The gate cannot check any of this.** `npm run gate` runs `vite build`, never `tauri build`,
  so the asset protocol, the CSP and `convertFileSrc` are a **producer click-through item**, not a
  CI item (PROJECT.md handoff). The producer verifies playback on a **built** app, because the
  CSP is injected into the HTML Tauri serves, not into the Vite dev server's responses.

## Spec

### `tauri.conf.json`

Replace the `app.security` block:

```json
    "security": {
      "csp": "default-src 'self'; connect-src ipc: http://ipc.localhost; media-src asset: http://asset.localhost; style-src 'self' 'unsafe-inline'",
      "assetProtocol": {
        "enable": true,
        "scope": ["$APPCONFIG/projects/**"]
      }
    }
```

Rationale, for the review:
- `media-src asset: http://asset.localhost` is the whole point -- it lets the `<audio>` element
  load a file through the asset protocol.
- `style-src 'self' 'unsafe-inline'` is defensive: the app has no inline styles today, but the
  CSP is applied in a built app, and a future inline style must not ship as a blank screen.
- The scope is `$APPCONFIG/projects/**`, not `$APPCONFIG/**`: only project files (tracks now, art
  later) should be reachable through a URL, never `config.json` or `session.log`.

### `src-tauri/Cargo.toml`

The `tauri` dependency must carry the `protocol-asset` feature, or the build fails at the
`tauri-build` step with "the `tauri` dependency features ... does not match the allowlist ...
add the `protocol-asset` feature". Change:

```toml
tauri = { version = "2.11.3", features = ["protocol-asset"] }
```

*(Added to this brief after the first Aider run: the gate's build step caught that the original
brief omitted this, because `npm run gate` compiles `src-tauri` and the asset-protocol config is
validated at build time -- the one part of the config change the gate **can** see.)*

### `library::tracks::resolve_track_file`

Add one function after `audio_path` (before `mint_track_id`). `project_dir` is already imported
in this module.

```rust
/// Resolves a track's stored `file` -- relative to the project directory, e.g.
/// `"tracks/tr-0001.flac"` -- to an absolute path the webview can play.
///
/// `Track.file` is written by this app, but it lives in a JSON sidecar the user
/// can open and edit: a hand-edited sidecar could name any file. An absolute
/// path, or one whose `..` walks out of the project, is refused rather than
/// handed to the webview as a path to serve. The asset protocol's own scope is
/// the second gate; this is the first.
pub fn resolve_track_file(root: &Path, slug: &str, file: &str) -> Result<PathBuf, LibraryError> {
    let rel = Path::new(file);
    let escapes = rel.is_absolute()
        || rel
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir));
    if escapes {
        return Err(LibraryError::UnusableName(file.to_string()));
    }
    Ok(project_dir(root, slug)?.join(rel))
}
```

### The `track_audio_path` command (`src-tauri/src/tracks.rs`)

Add to the imports:

```rust
use create_core::project::TrackId;
```

Add the command after `library_tracks`:

```rust
/// The absolute path to one track's audio file, for the webview to play.
///
/// `id` is validated by `load_track`'s whitelist before anything is joined, and
/// the stored `file` is resolved and checked to stay inside the project's
/// `tracks/` directory by [`library::tracks::resolve_track_file`]. Returns
/// `Err` for an unknown id, an unreadable sidecar, or a stored path that
/// escapes -- the frontend maps that to a play error rather than a crash.
#[tauri::command]
pub fn track_audio_path(config_dir: State<'_, ConfigDir>, id: String) -> Result<String, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let track = library::tracks::load_track(&config_dir.0, &project.slug, &TrackId(id))
        .map_err(|e| e.to_string())?;
    let abs = library::tracks::resolve_track_file(&config_dir.0, &project.slug, &track.file)
        .map_err(|e| e.to_string())?;
    Ok(abs.to_string_lossy().into_owned())
}
```

The command is deliberately thin glue, the same way `library_tracks` is: the resolution chain
(`selected_project` -> `load_track` -> `resolve_track_file`) is three already-tested functions,
and all three share one `project` value from one `selected_project` call, so there is no
cross-caller drift to test at the command level. Do not add a command-level test.

### Register the command (`src-tauri/src/lib.rs`)

In the `invoke_handler` list, after `tracks::library_tracks`, add:

```rust
            tracks::track_audio_path,
```

## Tests (in `crates/library/src/tracks.rs`, existing `mod tests`)

The test module already has a `project(root)` helper and imports everything these need. Add three
tests, each with the invariant it protects:

```rust
    /// Invariant: a track's stored path resolves to an absolute file under the
    /// project. It must be absolute or `convertFileSrc` cannot turn it into an
    /// asset URL.
    #[test]
    fn test_resolve_track_file_returns_an_absolute_path() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        std::fs::create_dir_all(tracks_dir(root.path(), &proj.slug).unwrap()).unwrap();

        let resolved = resolve_track_file(root.path(), &proj.slug, "tracks/tr-0001.flac").unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with(Path::new("tracks").join("tr-0001.flac")));
    }

    /// Invariant: a sidecar edited to name an absolute path is refused, not
    /// served. `Path::join` replaces its base with an absolute right-hand side,
    /// so this must be caught before the join.
    #[test]
    fn test_resolve_track_file_refuses_an_absolute_path() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        let abs = std::env::temp_dir().join("outside.flac");

        let err = resolve_track_file(root.path(), &proj.slug, abs.to_str().unwrap()).unwrap_err();
        assert!(matches!(err, LibraryError::UnusableName(_)));
    }

    /// Invariant: a `..` in a hand-edited sidecar cannot walk out of the
    /// project, wherever it appears in the path.
    #[test]
    fn test_resolve_track_file_refuses_a_parent_escape() {
        let root = tempfile::tempdir().unwrap();
        let proj = project(root.path());
        std::fs::create_dir_all(tracks_dir(root.path(), &proj.slug).unwrap()).unwrap();

        let leading = Path::new("..").join("config.json");
        for file in ["tracks/../outside.flac", leading.to_str().unwrap()] {
            let err = resolve_track_file(root.path(), &proj.slug, file).unwrap_err();
            assert!(
                matches!(err, LibraryError::UnusableName(_)),
                "file {file:?} should be refused"
            );
        }
    }
```

## Acceptance criteria

- [ ] `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` and `cargo test --workspace` green; `library` goes 55 -> **58** tests.
- [ ] `resolve_track_file` is the only new logic; the command is thin glue over it.
- [ ] The three tests above each fail when their guard is removed (mutation check: deleting the `rel.is_absolute()` check fails `test_resolve_track_file_refuses_an_absolute_path` only; deleting the `ParentDir` check fails `test_resolve_track_file_refuses_a_parent_escape` only).
- [ ] No changes outside the five listed files.
- [ ] No non-ASCII characters anywhere in the diff.

## Out of scope

- The frontend (bridge, player store, Player/Visualizer components, Library play button): T-402b/c.
- `devCsp`: the CSP above applies to dev and prod; the app has no dev-only resource needs.
- Serving `config.json` / `session.log`: the scope is `$APPCONFIG/projects/**`, so they are not reachable.
- Delete/rename/export/reveal (T-405).

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read src-tauri/src/projectctx.rs --read src-tauri/src/lib.rs --read crates/library/src/projects.rs --read crates/library/src/lib.rs --read crates/create-core/src/provenance.rs --file src-tauri/tauri.conf.json --file src-tauri/Cargo.toml --file crates/library/src/tracks.rs --file src-tauri/src/tracks.rs --file src-tauri/src/lib.rs
```
