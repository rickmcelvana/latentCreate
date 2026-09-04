# T-506c-a: the Cover Art backend seam — an image profile choice, and reading artwork back

**Depends:** T-506b (artwork is written) | **Crates:** `library`, `src-tauri`, plus the TS mirror
**Lane:** **architect-direct.** Five files, of which the only non-trivial one is a 5-line config
field; `art.rs` is `tracks.rs`'s two read commands with the nouns changed. Writing the reference
here *is* writing the task, which is exactly the case WORKFLOW §1 says not to send out.

**Files to create/modify (five, nothing else):**
- `crates/library/src/config.rs` — `Config.default_image_profile_id`
- `testdata/wire/loaded-config.json` — the shared cross-language fixture
- `app/src/bridge/config.ts` — the TypeScript mirror of the same field
- `src-tauri/src/art.rs` — **new**, `library_art` and `art_image_path`
- `src-tauri/src/lib.rs` — `mod art;` and the two registrations

## Goal

Cover Art can remember which image model the user picked, and can read back the artwork T-506b
files. Nothing else: no UI, no generation call, no store.

## Why the config field is its own thing rather than reusing `default_profile_id`

The two studios choose independently. One field would mean picking an image model in Cover Art
silently changed the model the Audio Studio generates with, and the profile pickers already filter
by `kind`, so the two would fight on every switch.

**And the image side has no default to fall back to.** `DEFAULT_PROFILE_ID` works for audio because
`ace-step-1.5-turbo` ships; the app ships **no image profile** — the only way to have one is to
adopt it from the catalog (T-505d). So the field is `Option<String>` and `None` stays `None`: the
CoverArt view will say there is no image model yet and point at Setup (T-506d), rather than
generating with something the user never chose. Same rule `selectedProfile` already follows on the
audio side, one level up.

## Spec

### 1. `library::config`

```rust
    /// `ModelProfile::id` last used for cover art.
    ///
    /// Separate from `default_profile_id` because the two studios choose
    /// independently: one field would make picking an image model in Cover Art
    /// change the model the Audio Studio generates with. `None` means no image
    /// profile has been chosen, and -- unlike the audio side -- there is no
    /// shipped default to fall back to, because the app ships no image profile.
    #[serde(default)]
    pub default_image_profile_id: Option<String>,
```

beside `default_profile_id`, plus `default_image_profile_id: None` in the `Default` impl.
`save_config` takes the whole `Config`, so there is no patch struct to keep in step.

### 2. The wire fixture, and why it will fail first

`test_wire_fixture_matches_current_types` parses `testdata/wire/loaded-config.json` into
`LoadedConfig`, re-serialises it, and compares. Adding a field makes the re-serialised value carry
a key the fixture does not, so **that test fails until the fixture gains the key** — and its own
failure message says the TypeScript types must change in the same commit. That tripwire is the
point of the file; this task is the first time it fires for a new field, and it should be allowed
to fire rather than worked around.

Add `"default_image_profile_id": null` to the fixture's `config` object, and
`default_image_profile_id: string | null` to `Config` in `app/src/bridge/config.ts`.
`app/src/state/config.test.ts` reads the same fixture from the other side; it should need no edit,
but if it type-errors, that is the tripwire working.

### 3. `src-tauri/src/art.rs` — two read commands

```rust
//! Tauri commands over the on-disk artwork library.
//!
//! Named for what it wraps (`library::art`), the house pattern `tracks.rs`
//! describes. The shadowing hazard that file's header warns about is specific
//! to a module named `library`; `art` collides with nothing.

use create_core::project::ArtId;
use library::ArtSet;
use tauri::State;

use crate::ConfigDir;

/// Every artwork in the selected project, with warnings for any sidecars that
/// could not be read.
///
/// `Err` only when the project itself cannot be resolved; an unreadable sidecar
/// is a warning inside `ArtSet` rather than an empty gallery. The track twin,
/// `library_tracks`, has the same split for the same reason.
#[tauri::command]
pub fn library_art(config_dir: State<'_, ConfigDir>) -> Result<ArtSet, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    Ok(library::art::list_art(&config_dir.0, &project))
}

/// The absolute path to one artwork's image file, for the webview to display.
///
/// `id` passes `load_art`'s whitelist before anything is joined, and the stored
/// `file` is resolved through `resolve_art_file`, which refuses an absolute path
/// or a `..` escape from a hand-edited sidecar. The asset protocol's own scope
/// is the second gate.
#[tauri::command]
pub fn art_image_path(config_dir: State<'_, ConfigDir>, id: String) -> Result<String, String> {
    let project = crate::projectctx::selected_project(&config_dir.0).map_err(|e| e.to_string())?;
    let art = library::art::load_art(&config_dir.0, &project.slug, &ArtId(id))
        .map_err(|e| e.to_string())?;
    let abs = library::art::resolve_art_file(&config_dir.0, &project.slug, &art.file)
        .map_err(|e| e.to_string())?;
    Ok(abs.to_string_lossy().into_owned())
}
```

**Checked, not assumed:** the asset-protocol scope in `src-tauri/tauri.conf.json` is
`$APPCONFIG/projects/**`, which already covers `projects/<slug>/art/*.png`. No scope change is
needed, and the next lane should not go looking for one.

### 4. `src-tauri/src/lib.rs`

`mod art;` in alphabetical position, and `art::library_art` / `art::art_image_path` registered
beside the `tracks::` commands.

## Tests

**`art.rs` gets none, and that is the existing rule rather than a gap here.** Both commands take
Tauri `State`, which no test in this crate builds — `src-tauri/src/tracks.rs` has no test module
either, for exactly this reason. Every decision they make is one line of delegation to
`library::art`, which is covered by 15 tests from T-506a; what is unproven is the wiring, and that
is seen at T-506d's click-through, the same way `library_tracks` was seen at T-311's.

What **is** tested, in `library::config`:

- **a config written before Cover Art existed still loads** — deserialise a `config.json` with no
  `default_image_profile_id` key and assert it is `None`. There is an existing older-config test to
  mirror.
- **the wire fixture round-trips** — the existing `test_wire_fixture_matches_current_types`, which
  must be made to pass by updating the fixture, not by loosening the assertion.
- **the two profile choices are independent** — set `default_profile_id` and
  `default_image_profile_id` to different ids, save, reload, and assert both survive with their own
  values. Without it, a copy-paste that wrote one field into the other would pass everything else.

## Acceptance criteria
- [ ] `npm run gate` green
- [ ] no changes outside the five listed files
- [ ] `Config` gains **only** `default_image_profile_id`, defaulted
- [ ] the wire fixture and `bridge/config.ts` change **in this same commit** as the Rust field

## Out of scope
- **Everything frontend beyond the one type line** — the store factory, the art stores, the
  `generateImage` bridge: T-506c-b, briefed after this lands.
- **`delete_art` and any write command** — T-506e.
- **A `ProfileStatus`-style readiness surface for image profiles.** An adopted profile declares no
  model files, so its readiness is already `Undeclared` and nothing gates on it (T-507).

## If unclear
Do not guess. Output a numbered list of questions and stop.

## Aider launch
Not applicable — architect-direct (WORKFLOW §1). The brief is written first and the diff is
reviewed against it as if someone else had written it, which is the part that catches things.
