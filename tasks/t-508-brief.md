# T-508: Installer builds

**Depends:** none (release arc) | **Crate/dir:** `.github/workflows/`, `src-tauri/tauri.conf.json`, `README.md`
**Files to create/modify:**
- `.github/workflows/release.yml` (new)
- `src-tauri/tauri.conf.json` (add `bundle.macOS.signingIdentity`)
- `README.md` (document the release workflow)

## Goal

A GitHub Actions workflow that produces installable builds — Windows NSIS `.exe`,
macOS `.dmg` (both Apple Silicon and Intel), and Linux AppImage — and attaches them to a
GitHub release. This is the phase milestone: a person on a machine that never had the dev
toolchain can download and install the app.

## Decisions (owner, 2026-09-06)

- **Unsigned first.** No code signing, no MSIX. Windows SmartScreen and macOS Gatekeeper
  will warn; macOS users right-click → Open. Signing is a later task gated on the owner
  providing a Windows cert and an Apple Developer ID.
- **All three platforms at once.** The CI matrix already runs all three OSes, and
  `tauri-action` makes the extra platforms cheap. The milestone requires "at least
  Windows"; the rest come for free.

## Spec

### The workflow (`.github/workflows/release.yml`)

- Trigger: `push` of a `v*` tag, plus `workflow_dispatch` (manual run).
- `permissions: contents: write` (the action creates the release and uploads assets).
- A matrix over four build legs:
  - `macos-latest` + `--target aarch64-apple-darwin` (Apple Silicon)
  - `macos-latest` + `--target x86_64-apple-darwin` (Intel)
  - `ubuntu-22.04` (AppImage)
  - `windows-latest` (NSIS)
- Steps, in order:
  1. `actions/checkout@v4`
  2. Linux-only: install the README's documented prerequisites **plus `patchelf`**
     (the AppImage bundler needs it; the existing `ci.yml` does not install it because
     it never bundles).
  3. `actions/setup-node@v4` (node 22, npm cache on `package-lock.json`).
  4. `dtolnay/rust-toolchain@stable` with the two macOS targets added only on macOS
     runners (the `matrix.platform == 'macos-latest' && ... || ''` idiom).
  5. `Swatinem/rust-cache@v2` keyed on the platform (same as `ci.yml`).
  6. `npm ci` at the root (the workspace install — WORKFLOW §4b: CI installs the way
     the README tells a contributor to).
  7. `tauri-apps/tauri-action@v1` with `GITHUB_TOKEN`, `tagName: v__VERSION__`,
     `releaseName: 'latentCreate v__VERSION__'`, `releaseDraft: true`, `prerelease: false`,
     and `args: ${{ matrix.args }}`.

The action runs `tauri build` (which runs `beforeBuildCommand` → `tsc -b && vite build`
automatically), then creates the release and uploads the bundles. `releaseDraft: true`
means the release is created as a draft the owner publishes by hand — no accidental
public release from a tag push.

### The config change (`src-tauri/tauri.conf.json`)

Add `"signingIdentity": "-"` under `bundle.macOS`. This is **ad-hoc signing**, the
documented way to avoid macOS treating an unsigned Apple Silicon build downloaded from
GitHub as "damaged" (it still requires the right-click → Open whitelist, but the app is
not reported as broken). It is a no-op on Intel and on the other platforms.

### The README change

Replace the "CI does not run `tauri build`" note with a short "Releases" paragraph:
installers are built by the `Release` workflow on a `v*` tag (or manually), attached to
a draft GitHub release, and are **unsigned** — SmartScreen/Gatekeeper will warn.

## Acceptance criteria

- [ ] `release.yml` is valid YAML and mirrors the `ci.yml` conventions (same node
      version, same Linux package set, same rust-cache keying).
- [ ] `tauri.conf.json` still parses (the `macOS` key is new but the schema allows it).
- [ ] README's release note is accurate and does not contradict the existing
      "Building for production" section.
- [ ] No changes outside the three listed files.

## Out of scope

- Code signing (Windows cert, Apple Developer ID), notarization, MSIX.
- The updater (`createUpdaterArtifacts` / `latest.json`) — no updater plugin is
  configured, so `tauri-action`'s updater JSON is a no-op.
- T-509 (THIRD-PARTY-LICENSES) and T-510 (public-repo readiness) — separate tasks.

## Verification note

This is a CI-only change: it cannot be exercised by `npm run gate` (which deliberately
does not run `tauri build`). The real check is the milestone — a `v*` tag push producing
four installers on a draft release. The owner runs that once; the workflow is written to
fail loudly (matrix `fail-fast: false`, per-leg artifacts) so a single platform's
bundling problem does not hide the others.

## If unclear

Do not guess. Output a numbered list of questions and stop.
