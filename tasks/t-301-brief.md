# T-301: remove the lyric-model suggestions

**Depends:** — (first task of Phase 3) | **Crate/dir:** `create-core`, `library`, `src-tauri`, `app`
**Files to create/modify:**

Delete outright:
- `data/lyric-llms.json`
- `crates/create-core/src/suggestions.rs`
- `crates/library/src/suggestions.rs`

Modify:
- `crates/create-core/src/lib.rs`
- `crates/library/src/lib.rs`
- `src-tauri/src/lib.rs`
- `src-tauri/src/llm.rs`
- `src-tauri/tauri.conf.json`
- `app/src/bridge/llm.ts`
- `app/src/state/llm.ts`
- `app/src/state/llm.test.ts`
- `app/src/views/Setup.tsx`
- `app/src/theme.css`
- `docs/MODELS.md`

## Goal

The app suggests no lyric-writing model. Every mechanism for recommending,
preselecting-by-recommendation, chipping and pull-commanding a model comes out, along
with the data file that drives it. What stays is the machinery for **choosing** a model
and for telling the truth about one: capability tri-states, the remote-model privacy
disclosure, and a configured model always winning.

Owner decision, 2026-08-27 (PROJECT.md decisions log): users making music already have a
go-to model for lyrics, many of them are not on Ollama, and what the app owes them is
connecting to whatever they already use — not an opinion about which model is good.

## Spec

### 1. Delete the suggestion layer

`crates/create-core/src/suggestions.rs` (`LyricLlmSuggestion`, `LyricLlmSuggestions`,
`matches`, `for_model`, `preselect`, `missing`) and `crates/library/src/suggestions.rs`
(`SuggestionWarning`, `SuggestionSet`, `load`, `SUGGESTIONS_FILE`) are deleted whole,
tests included. Remove from `crates/create-core/src/lib.rs`:

```rust
pub mod suggestions;
pub use suggestions::*;
```

and from `crates/library/src/lib.rs`:

```rust
pub mod suggestions;
pub use suggestions::SuggestionSet;
pub use suggestions::SuggestionWarning;
```

### 2. `preselect` degrades, it does not disappear

`LyricLlmSuggestions::preselect` did two jobs. The suggestion half goes; the **settings**
half — a configured model wins, an uninstalled configured model does not pin the picker —
is behaviour the app still needs and still owes tests. It becomes a private function in
`src-tauri/src/llm.rs`, replacing the `suggestions.preselect(...)` call in `llm_probe`.

Integrate verbatim (already `rustfmt`-clean; rename the test module to sit alongside the
existing `mod tests` in that file, or fold these three tests into it):

```rust
/// Which model the picker should select, given what is already configured.
///
/// **A configured model always wins** when it is still on the endpoint -- the
/// user's own choice is a setting, not a hint to be re-decided on every visit.
/// A configured model that is no longer offered returns `None` rather than
/// pinning the picker to something unusable.
///
/// This is the whole of what survives `LyricLlmSuggestions::preselect`: the
/// suggestion half is gone, the settings half is not.
fn preselect(selectable: &[&str], configured: Option<&str>) -> Option<String> {
    configured
        .filter(|current| selectable.contains(current))
        .map(str::to_string)
}

#[cfg(test)]
mod pretests {
    use super::*;

    /// Protects: the user's own choice is never overridden. This is the
    /// difference between a suggestion and a setting -- a wizard that re-picks
    /// on every visit silently discards a deliberate decision.
    #[test]
    fn test_a_configured_model_is_kept() {
        let available = ["qwen3.5:9b", "some-other-model"];
        assert_eq!(
            preselect(&available, Some("qwen3.5:9b")),
            Some("qwen3.5:9b".to_string())
        );
    }

    /// Protects: a configured model that is no longer installed does not pin
    /// the picker to something unusable, and nothing is chosen in its place.
    /// Picking a model for the user is what this task exists to stop.
    #[test]
    fn test_an_uninstalled_configured_model_selects_nothing() {
        let available = ["qwen3.5:9b"];
        assert_eq!(preselect(&available, Some("gemma4:26b")), None);
    }

    /// Protects: nothing configured means nothing selected. The picker opens
    /// unset and the user chooses.
    #[test]
    fn test_nothing_configured_selects_nothing() {
        assert_eq!(preselect(&["qwen3.5:9b"], None), None);
        assert_eq!(preselect(&[], None), None);
    }
}
```

### 3. `src-tauri/src/llm.rs`

- Delete `struct Suggested`, `impl Suggested`, and `struct MissingSuggestion`.
- Delete the `suggested` field from `LlmModelRow`.
- Delete `missing_suggestions` from `LlmStatus::Ready`. **`preselect` and `has_key` stay.**
- `fn row(...)` loses its `suggestions: &LyricLlmSuggestions` parameter and its
  `suggested:` line. Update the six call sites in that file's tests.
- Delete the `use create_core::suggestions::{...}` import, the
  `library::suggestions::load(...)` call, and the `missing_suggestions` construction in
  `llm_probe`.
- Delete the test `test_a_suggested_variant_gets_its_chip` and the `fn suggestions()`
  test helper. Every other test in the file stays and must still pass.

### 4. `DataDir` goes with it

⚠ **`llm_probe` is the only consumer of `DataDir` in the whole app** — everything else
(`profile.rs`, `models.rs`, `lyrics.rs`, `lyricdoc.rs`, `optimize.rs`, `install.rs`) uses
`ProfilesDir`. Leaving `DataDir` behind is a dead-code warning, and the gate runs
`clippy -D warnings`.

- Remove the `data_dir: State<'_, DataDir>` parameter from `llm_probe` and the
  `use crate::DataDir;` import.
- Remove `struct DataDir(PathBuf)` and its doc comment from `src-tauri/src/lib.rs`.
- Remove `app.manage(DataDir(shipped_dir(app.handle(), "data")));`.

**No frontend change follows from this.** Tauri injects `State` arguments, so the
`invoke('llm_probe', { baseUrl, configuredModel })` call is unaffected. Do not touch it.

### 5. ⚠ `src-tauri/tauri.conf.json` — the bundle glob

`bundle.resources` lists `"../data/*.json"`. `data/` holds **only** `lyric-llms.json`, so
deleting it empties the directory, git does not track empty directories, and a fresh clone
has no `data/` at all for the glob to match.

Remove the `"../data/*.json"` line, leaving `"../profiles/*.json"`.

⚠ **`npm run gate` will not catch this if you get it wrong** — the gate runs `vite build`,
not `tauri build`, so nothing in it assembles the bundle. This is a producer click-through
item, not a gate item.

### 6. Frontend types — `app/src/bridge/llm.ts`

- Delete `interface Suggested` and `interface MissingSuggestion`.
- Delete `suggested` from `LlmModelRow` and `missing_suggestions` from the `ready` variant
  of `LlmStatus`. Keep `preselect` and `has_key`.

### 7. `app/src/state/llm.ts`

One line out of `modelView`:

```ts
if (row.suggested !== null) chips.push('recommended for lyrics')
```

Everything else in `modelView` stays exactly as it is — `cannot chat`, `remote`,
`thinks first`, `capabilities unknown`, `selectable`, and the disclosure sentence. Those
are correctness, not promotion (see Out of scope).

### 8. `app/src/views/Setup.tsx`

Delete the whole `status.missing_suggestions.map(...)` block (the `llm-suggestion` div,
its `setup-next-step` paragraph and its `setup-command` code element).

Then replace the `not_configured` line. It currently reads:

```tsx
<p className="setup-next-step">Set an endpoint to write lyrics with a model.</p>
```

with copy that says what the app actually talks to and what it is currently trying —
naming no model:

```tsx
<p className="setup-next-step">
  Lyrics are written by a model you provide. latentCreate works with any OpenAI-compatible
  endpoint -- a local server, or a hosted API with a key.
</p>
<p className="setup-next-step">
  Nothing answered at <code className="setup-command">{DEFAULT_BASE_URL}</code>, the local
  address tried by default.
</p>
```

The second paragraph is the point: the default endpoint is currently a hardcoded constant
the user can neither see nor change, and a user on a hosted API has no way to know that is
what failed. Saying it is the honest minimum until T-301b adds the field.

### 9. `app/src/state/llm.test.ts`

- Drop `suggested: null` from the `row()` test factory and `missing_suggestions: []` from
  the status factory.
- Delete `it('chips a suggested model', ...)`.
- The test around line 234 whose comment explains "preselect exists so a configured model
  wins over any suggestion" — keep the test, rewrite the comment. The rule it guards is
  now "a configured model wins, full stop", which is still worth guarding.

### 10. `app/src/theme.css`

Remove the `.llm-suggestion` rule (line ~507). **Keep `.setup-command`** — the ComfyUI
step uses it for `install_command` (Setup.tsx:44).

### 11. `docs/MODELS.md`

Replace the section "Lyric-writing LLMs (suggestions, not requirements)" — its table and
the "Wizard behavior" paragraph — with a short section carrying no model names, along
these lines:

> ## Lyric-writing LLMs
>
> The app works with **any** OpenAI-compatible endpoint (ARCHITECTURE §4) and recommends
> no particular model. People making music already have a model they write with; the
> app's job is connecting to it, not having an opinion about it.
>
> Nothing here is a gate: the wizard lists what the endpoint offers, says what it can and
> cannot verify about each one, and the user chooses. Where capabilities cannot be checked
> the list says so rather than guessing (LLM-SURFACE §11.1, §11.2).
>
> *(A suggestion list shipped as `data/lyric-llms.json` until 2026-08-27; see PROJECT.md's
> decisions log for why it was removed.)*

Leave every other section of MODELS.md alone — the audio-model tables are unrelated.

## Acceptance criteria

- [ ] `rg -i "suggest"` over `crates/`, `src-tauri/src/`, `app/src/`, `data/` returns
      nothing but unrelated prose. No `lyric-llms.json` anywhere in the tree.
- [ ] The three `preselect` tests above pass in `src-tauri`.
- [ ] Every pre-existing test in `src-tauri/src/llm.rs` still passes except the two named
      for deletion in §3.
- [ ] `app/src/state/llm.test.ts` passes with the suggestion test removed; **the
      capability, remote-disclosure and unknown-capability tests are untouched and green**.
- [ ] `npm run gate` clean: `cargo fmt --check`, `clippy -D warnings` (watch for dead
      `DataDir`), `cargo test --workspace`, `tsc -b`, `oxlint`, `vitest`, `vite build`.
- [ ] No changes outside the listed files.

**Producer click-through** (the gate cannot see these):
- [ ] `npm run dev` starts and the wizard's Lyrics-model step renders with no console error
      — this is the check that the `tauri.conf.json` resource change is right.
- [ ] With Ollama running: models list, chips read as before minus "recommended for
      lyrics", a remote model still shows its disclosure sentence, choosing one still
      persists to `config.json`.
- [ ] With nothing listening on 11434: the new copy appears and names the address.

## Out of scope

- **Do not touch the remote-model privacy disclosure, the `Option<bool>` capability
  tri-states, or the "capabilities unknown" chip.** Those exist because reporting an
  unchecked model as local would tell a user their unreleased lyrics stay on their machine
  when nobody verified it. They are not recommendations and they are not in this task.
- **Do not add an endpoint or API-key input.** The base URL is a hardcoded constant in
  five places and there is no field for it, and no field for the key that `has_key`
  already reports on. That is **T-301b**, and it is the task that actually delivers "users
  can connect to what they already use". Inventing a form here would collide with it.
- Do not change `reasoning_effort` or the `thinks` gating. That is **T-302**, and it needs
  a measurement against a non-Ollama endpoint before anything about it changes.
- Do not rename `LlmStatus`, its variants, or any Tauri command.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read tasks/t-301-brief.md --read crates/create-core/src/suggestions.rs --read crates/library/src/suggestions.rs --file crates/create-core/src/lib.rs --file crates/library/src/lib.rs --file src-tauri/src/lib.rs --file src-tauri/src/llm.rs --file src-tauri/tauri.conf.json --file app/src/bridge/llm.ts --file app/src/state/llm.ts --file app/src/state/llm.test.ts --file app/src/views/Setup.tsx --file app/src/theme.css --file docs/MODELS.md
```

The two `suggestions.rs` files are `--read`, not `--file`: they are being **deleted**, and
the executor needs to see what referenced them, not edit them. Delete them with
`git rm` after the run rather than asking the executor to empty them.
