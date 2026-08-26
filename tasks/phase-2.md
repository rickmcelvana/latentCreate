# Phase 2 — Lyrics Studio

Goal: a user fills in a brief, watches lyrics stream in from their own LLM, edits them into
shape across versions, and approves one for AudioStudio. Nothing here generates audio; what
it produces is the `LyricRef` that Phase 3's `GenerationSpec` and provenance sidecar point at.

**Read first:** [docs/LLM-SURFACE.md section 12](../docs/LLM-SURFACE.md) and
[docs/MCP-SURFACE.md section 15](../docs/MCP-SURFACE.md). Both were captured live on
2026-08-25 against the recommended model and the shipped ACE-Step template. Between them they
overturn three things this phase would otherwise have assumed.

---

## Before T-201: verify the lyric surface  DONE 2026-08-25

Four findings, each of which changes a task below rather than decorating it.

1. **A full song is 99% chain-of-thought by default.** A 2000-token budget bought 85
   characters of lyrics and 7458 of reasoning, `finish_reason: length`, with the **first
   content delta 44.08 s into a 44.65 s stream**. Sizing the budget against the length of a
   song is sizing it against the wrong thing (LLM-SURFACE 12.1).
2. **`reasoning_effort: "none"` fixes it; `think: false` is accepted and silently ignored.**
   The same request goes from 6.30 s / no answer to 0.95 s / a clean chorus. `"low"` is not
   honoured either. Verified against Ollama only, so the field is sent **only when the model
   is known to think** -- the wizard's `thinks` flag, which exists only where the app can
   enrich. Endpoints it cannot enrich never see the field, so the unverified path is never
   taken (LLM-SURFACE 12.2, 12.3).
3. **With reasoning off, a `V-C-V-C-B-C` song is about 400 completion tokens** and starts
   arriving in half a second (LLM-SURFACE 12.3).
4. **The model breaks the profile's own lyric rule every time.** Both captures put production
   cues -- `[Driving synthwave bassline enters...]`, `[Vocal style: ethereal, airy]` -- inside
   the lyrics, with the contract stated plainly in the system prompt. And **nothing in
   ComfyUI publishes the tag vocabulary**: `TextEncodeAceStepAudio1.5.lyrics` is a bare
   STRING with empty `choices` and an empty description, while the shipped example numbers
   its verses (`[Verse 1]`) and the profile's `structure_tags` do not (LLM-SURFACE 12.4,
   MCP-SURFACE 15). So the validator is load-bearing, its matching is numbering-tolerant, and
   every finding it makes is advisory -- there is no authority to block against.

**Two decisions taken at the phase boundary** (recorded in PROJECT.md's decisions log):

- **One JSON file per lyric document**, `lyrics/<doc-id>.json`, holding every version inline.
  ARCHITECTURE 8 sketched `lyrics/<v>.md` per version, which predates `LyricDoc` and would
  put a version's text in one file and its `source`/`created_at`/`approved` in another --
  the two-files-disagreeing hazard the one-source-of-truth-per-track rule exists to prevent,
  for a few KB of gain. ARCHITECTURE 8 is updated to match.
- **The brief's `language` is a writing instruction, not a slot value.** The profile's
  `inputs.language` is `from_node_choices` and is read live from the node schema in Phase 3's
  param panel. The lyric brief carries a plain language name for the LLM; mapping it onto
  `94.language` is Phase 3's job. Conflating them would make the Lyrics Studio need a running
  ComfyUI to render a form.

---

## Tasks

Briefs are written one at a time, each after the previous lands. Each gets its own
`tasks/t-2NN-brief.md` when written.

### T-201 — `library::projects`: the on-disk project and lyric store  — split in three
`LyricDoc`/`LyricVersion`/`LyricSource` have existed in `create-core` since T-003b with
nothing to persist them. Everything below produces documents, and Phase 3's provenance
records a `LyricRef { doc_id, version }` -- a doc id that lives only in a Zustand store is a
dangling reference the first time the app restarts, so the store comes first.

Scope: create/list/load projects under `library/projects/<slug>/`, mint `LyricDocId`, append
a version, set `approved`, atomic writes (`config.rs`'s pattern), malformed files reported as
warnings rather than failing the load (`profiles.rs`'s pattern). No UI for project management
in this phase -- a default project is created on demand; Phase 4's Library view manages them.

Split at ~400 lines per run, along the seams that already exist in the work:

- **T-201a — atomic writes and the types** ([brief](t-201a-brief.md)). One
  `library::atomic::write_json` shared with `config::save`, plus `Project::new`,
  `Project::next_lyric_seq` and `LyricDoc::push_version`/`approve` in `create-core`. The
  sequence number is monotonic and never reused: minting lyric ids from the surviving file
  list would let a deleted document's id reach a later one, and a track's provenance would
  then point at unrelated lyrics.
- **T-201b — naming and path safety.** `slugify`, the Windows reserved-name guard
  (a project called "Con" must still get a creatable directory), `is_safe_slug` as a
  whitelist so a slug arriving from the frontend cannot escape the library root,
  `ProjectWarning`/`ProjectSet`, and `now_rfc3339` -- the one place this crate reads a
  clock, which is what keeps every other function's tests deterministic. Adds `chrono`.
- **T-201c — the store.** `create_project` (suffixing a taken slug rather than opening
  someone else's project), `save_project`, `load_project` (a missing project is
  `NotFound`, never a default), and `list_projects`, which never fails and prefers the
  directory name over a stale recorded slug -- what a copied project directory looks like.

### T-202 — `create-core::lyrics`: the brief type and system-prompt assembly
Pure, no I/O, no async. `LyricBrief` (theme, style tags, mood, structure, language, POV,
era/references, explicit allowed, target duration) with the strong prefills ARCHITECTURE 6
requires, plus `assemble_system_prompt(&ModelProfile, &LyricBrief)` -- role line, the
profile's `lyrics_contract` and `prompt_guide`, then the hard rules -- and the user message.
A profile with no `lyrics_contract` must still assemble a valid prompt.

### T-203 — `create-core::lyrics::lint`: structure-tag validation
Advisory findings over lyric text against a profile: a bracketed token that is not a
structure tag (the failure the model actually makes), a requested section missing, `[inst]`
handling. Numbering-tolerant matching -- `[Verse 2]` matches `[Verse]` -- because the shipped
template numbers and the profile does not. Returns typed findings with a severity, never a
bool, and never a verdict that can block. Pure and heavily tested; mutation-test the guards.

### T-204 — `llm-bridge`: `reasoning_effort` on `ChatRequest`
One optional field, omitted from the wire when `None`. Plus an `--ignored` live test that
proves `"none"` suppresses reasoning on a thinking model, since that is the whole reason the
field exists. Small task; kept separate so the policy decision lands with its evidence.

### T-205 — Tauri lyric streaming command and event pump
`lyrics_generate` / `lyrics_cancel`, modelled on `src-tauri/src/jobs.rs`: spawn, keep the
abort handle, emit `lyrics://delta` (content only), `lyrics://thinking` (reasoning), and
terminal `lyrics://done { finish_reason, usage }` / `lyrics://failed`. Applies the
`reasoning_effort` policy from finding 2. **`finish_reason` reaches the frontend intact** --
truncation is an outcome the UI has to state, not an error to swallow.

### T-206 — frontend bridge and `lyrics` store
Typed wrappers plus the Zustand store: brief state with prefills, streaming accumulation
(`Content` into the draft, `Reasoning` into a bounded status trace), version list, approve,
and the truncated flag. Components never call `invoke` (ARCHITECTURE 11).

### T-207 — LyricsStudio: the brief form
Prefilled from the selected profile's `prompt_guide.examples`, structure picker, plain-text
language. One primary action.

### T-208 — LyricsStudio: the generation UI
Streaming into the draft, the thinking trace rendered as visible status (44 seconds of
silence is what this prevents), cancel, and a truncation banner offering a retry with more
budget when `finish_reason` is `length`.

### T-209 — Versioned editor, lint surfacing, approve to handoff
Version list with restore, edits recorded as `LyricSource::Edited`, T-203's findings shown
inline as advisories, approve sets `LyricDoc::approved`, and the handoff to AudioStudio is a
store action rather than navigation state.

### T-210 — Consent-gated prompt optimizer and the shared diff component
Optimizer call over the same endpoint, original vs optimized side by side with an inline
word diff, Accept / Edit / Revert. The user-approved text is what is sent and stored, with
`prompt_optimized` recorded on the version. Never auto-applied. `<PromptDiff>` is built to be
reused for audio tags in Phase 3.

### T-211 — Phase 2 milestone verification (live)
ROADMAP's check, run for real: brief -> lyrics stream in -> edit -> approve, and the
optimizer diff accepts and reverts cleanly. Plus the two checks only a live run can make --
that a thinking model chosen in the wizard generates a whole song, and that the lint fires on
the production-cue lines the model actually writes.
