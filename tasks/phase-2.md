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

### T-201 — the on-disk project and lyric store  — **LANDED** ([design record](t-201a-brief.md))
`LyricDoc`/`LyricVersion`/`LyricSource` have existed in `create-core` since T-003b with
nothing to persist them. Everything below produces documents, and Phase 3's provenance
records a `LyricRef { doc_id, version }` -- a doc id that lives only in a Zustand store is a
dangling reference the first time the app restarts, so the store comes first.

Scope: create/list/load projects under `library/projects/<slug>/`, mint `LyricDocId`, append
a version, set `approved`, atomic writes (`config.rs`'s pattern), malformed files reported as
warnings rather than failing the load (`profiles.rs`'s pattern). No UI for project management
in this phase -- a default project is created on demand; Phase 4's Library view manages them.

Briefed as a three-way split at ~400 lines per run, then **landed whole as architect
work**: the reference code for all three parts was already compiled, gate-green and
mutation-tested by the time the first brief was finished, and transcribing it would have
bought nothing (producer's call, 2026-08-25). The split is still the shape of the code:

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
  Plus `library::lyrics`: one JSON file per document, ids minted from the project's
  counter and never from the surviving files, and `list_docs` driven by `Project::lyrics`
  so a stray file is ignored while a listed id with no file becomes a warning.

### T-202 — `create-core::lyrics`: the brief type and system-prompt assembly  — **LANDED**
Pure, no I/O, no async. `LyricBrief` (theme, style tags, mood, structure, language, POV,
era/references, explicit allowed, target duration) with the strong prefills ARCHITECTURE 6
requires, plus `assemble_system_prompt(&ModelProfile, &LyricBrief)` -- role line, the
profile's `lyrics_contract` and `prompt_guide`, then the hard rules -- and the user message.
A profile with no `lyrics_contract` must still assemble a valid prompt.

- **T-202a — the brief and the two numbers derived from it**  LANDED ([brief](t-202a-brief.md)).
  `LyricBrief` with filled-in defaults, `PointOfView`, `expand_structure` (an unrecognised
  token is passed through, never dropped; whitespace is not a separator, or "Spoken word"
  becomes two sections) and `token_budget`, which returns 1260 for the default brief
  against a measured cost of 383 and 422 completion tokens.
- **T-202b — prompt assembly**  LANDED ([brief](t-202b-brief.md)). Everything model-specific is
  read from the profile, because the two shipped profiles disagree about the capitalisation
  of their own structure tags. **The prompt carries no rule against production or
  vocal-style directions in the lyrics**: adding one was measured over 14 live generations
  and the runs carrying it averaged more of the behaviour it forbids (LLM-SURFACE 12.5). A
  test exists solely to stop it being re-added.

### T-203 — `create-core::lyrics::lint`: structure-tag validation  — **LANDED** (split in two)
Advisory findings over lyric text against a profile: a bracketed token that is not a
structure tag (the failure the model actually makes), a requested section missing, `[inst]`
handling. **T-202's live generations sized this task**, counted over all 13 that were
saved: no prompt variant stopped the stray direction blocks (46 of them, in 10 of the 13
files), the requested order held in **13 of 13**, and the only section ever added beyond
the six requested was an `[Outro]`, in 9 of 13. So "the requested sections appear, in
order" is a check a lyric can pass, and "and nothing else" is one most lyrics fail over an
outro the user probably wants: information, never a failure. Numbering-tolerant matching -- `[Verse 2]` matches `[Verse]` -- because the shipped
template numbers and the profile does not. Returns typed findings with a severity, never a
bool, and never a verdict that can block. Pure and heavily tested; mutation-test the guards.


- **T-203a — the scanner and the directions**  LANDED ([brief](t-203a-brief.md)). `LintSeverity`
  (no `Error` variant -- nothing published by ComfyUI can make any of this authoritative),
  the complete `LintFinding`, the tag scanner, and the two rules that catch what the model
  actually writes: a bracket that is not a structure tag, and text sharing a line with one.
  Three captured generations go into `testdata/lyrics/` as fixtures, unedited.
- **T-203b — the structure rules**  LANDED ([brief](t-203b-brief.md)). Missing, out-of-order and
  extra sections, with the severities set by the corpus: missing and reordered are warnings
  because no real generation produced either, and an extra section is Info because 9 of 13
  added an `[Outro]`.

**The corpus found a defect in the first version of the scanner**, which is the reason the
fixtures are real output rather than hand-written. An earlier rule required a tag line to
be nothing but tags. One generation wrote every direction as
`[Verse] (dreamy female vocals)` -- and that correctly structured song, one of only three
in the corpus with no bracketed strays, came back reported as having no structure at all.

### T-204 — `llm-bridge`: `reasoning_effort` on `ChatRequest`  — **LANDED**
One optional field, omitted from the wire when `None`. Plus an `--ignored` live test that
proves `"none"` suppresses reasoning on a thinking model, since that is the whole reason the
field exists. Small task; kept separate so the policy decision lands with its evidence.
Landed directly as architect work (no Aider run -- the producer's standing call: a task this
small, written and tested by the architect, is not worth an executor round trip).

### T-205 — Tauri lyric streaming command and event pump  — **LANDED**
`lyrics_generate` / `lyrics_cancel`, modelled on `src-tauri/src/jobs.rs`: spawn, keep the
abort handle, emit `lyrics://delta` (content only), `lyrics://thinking` (reasoning), and
terminal `lyrics://done { finish_reason, usage }` / `lyrics://failed`. Applies the
`reasoning_effort` policy from finding 2. **`finish_reason` reaches the frontend intact** --
truncation is an outcome the UI has to state, not an error to swallow.
Landed directly as architect work (no Aider run), consistent with the producer's standing
call on T-204.

### T-206 — frontend bridge and `lyrics` store  — **LANDED**
Typed wrappers plus the Zustand store: brief state with prefills, streaming accumulation
(`Content` into the draft, `Reasoning` into a bounded status trace), and the truncated flag.
Components never call `invoke` (ARCHITECTURE 11). Landed directly as architect work.
**"version list, approve" moved to T-209** -- a version needs its `LyricSource::Llm { model }`
and the on-disk store, neither of which the streaming store holds (the backend reads the model
from config, and `library::lyrics` has no Tauri commands yet).

### T-207 — LyricsStudio: the brief form  — **LANDED**
Prefilled from the selected profile's `prompt_guide.examples`, structure picker, plain-text
language. One primary action. Landed directly as architect work. Also added the
`profile_guide` Tauri command (the profile's `prompt_guide` reached the frontend nowhere
before), and a config `load()` at app startup in `App.tsx` -- `default_profile_id` was never
read at runtime, so the selected-profile prefill had no source. Falls back to
`DEFAULT_PROFILE_ID` (`ace-step-1.5-turbo`) when none is configured.

### T-208 — LyricsStudio: the generation UI  — **LANDED**
Streaming into the draft, the thinking trace rendered as visible status (44 seconds of
silence is what this prevents), cancel, and a truncation banner offering a retry with more
budget when `finish_reason` is `length`. Landed directly as architect work.

### T-209 — Versioned editor, lint surfacing, approve to handoff  — **LANDED** (split in three)
Version list with restore, edits recorded as `LyricSource::Edited`, T-203's findings shown
inline as advisories, approve sets `LyricDoc::approved`, and the handoff to AudioStudio is a
store action rather than navigation state. **Carries the "version list, approve" that T-206
deferred, and therefore also the Tauri commands wiring `library::lyrics` (create/save/list
docs) that nothing has exposed to the frontend yet.**
Landed directly as architect work in three commits: T-209a (backend `lyrics_open`/`lyrics_save`/
`lyrics_lint` + a default project created on demand), T-209b (frontend `bridge/lyricdoc.ts` + the
versioned store: `loadDoc`/`commit`/`approve`/`restore`/`lint`, auto-commit on generation done),
T-209c (the editor UI: editable draft, version list with restore/approve, lint advisories).
Generation auto-commits as `Llm` (model read from config); an explicit Save commits `human`
(first version) or `edited` (later). The handoff is `approvedText(doc)`, read by Phase 3.

### T-210 — Consent-gated prompt optimizer and the shared diff component  — **LANDED**
Optimizer call over the same endpoint, original vs optimized side by side with an inline
word diff, Accept / Edit / Revert. The user-approved text is what is sent and stored, with
`prompt_optimized` recorded on the version. Never auto-applied. `<PromptDiff>` is built to be
reused for audio tags in Phase 3.

Landed directly as architect work in two commits: T-210a (`create-core::lyrics::optimize` --
the optimizer system prompt, the rewritable/fixed label split, `clean_optimized`; the
`lyrics_optimize` command returning both texts plus `truncated`; `prompt_override` on
`lyrics_generate`), T-210b (`components/wordDiff.ts`, the `<PromptDiff>` component, and the
store's `optimization`/`proposed`/`promptOverride`).

**What is optimized is the assembled brief, not the lyrics.** The user message is a list of
labelled lines (T-202, chosen partly for this), and the optimizer is told which lines it may
rewrite (Theme, Genre and style tags, Mood, Era and references) and which it must reproduce
word for word (Structure, Language, Point of view, Explicit content allowed, Target duration).
A test asserts the two lists cover every line `assemble_user_message` can emit, so a new brief
field cannot reach the model with no rule at all.

**The diff is the only enforcement, and that is the design.** A model that rewrites a settings
line produces a highlighted change the user has to accept before it goes anywhere. Adding a
backend check would be a second answer to a question the consent step already answers.

**The optimizer prompt has not been measured.** The lyric prompt was captured working before
it was written down (LLM-SURFACE 12.5); this one is a first draft. T-211 is where it meets a
real model.

### T-211 — Phase 2 milestone verification (live)

Producer-run, per WORKFLOW 5: this is the check nothing offline can make. **Lyrics need no
ComfyUI** -- only the LLM endpoint -- so a failure here is never about the audio service.

**Preconditions**
- Ollama (or any OpenAI-compatible endpoint) running, and a lyric model configured through
  the wizard's LLM step. Prefer a **thinking** model (gemma4): step 4 is meaningless without
  one, since it is the `reasoning_effort` policy being exercised.
- `npm run gate` green on the commit under test.

**Steps 1 and 4 are automated, and were run on 2026-08-26 -- both pass.** Re-run either after
any change to a prompt, the lint, or the `reasoning_effort` policy; the numbers below are the
baseline to compare against.

```
cargo test -p app -- --ignored optimizer --nocapture
cargo test -p app -- --ignored lyric_generation --nocapture
```

**Step 1 -- the optimizer measurement.** Five round trips; reports whether the rewrite came
back as a well-formed brief and which of the five fixed lines it altered, plus run 1's rewrite
in full. **Result 2026-08-26, `gemma4:12b-32k`: brief intact 5/5, fixed lines reproduced 5/5,
no truncation, 3.4-3.6 s per call.** The prompt's rules hold and are kept as written.

*What the first run of this harness found:* the model **adds** an `Era and references` line
when the brief leaves that field empty, in 5 of 5 runs. The check's first version tested the
label list for equality and called all five runs a failure. It was the check that was wrong --
Era is a rewritable label, the added line is well-formed, and it reaches the user as an added
line in the diff. `LabelReport` now reports dropped, invented and shuffled labels separately,
and stays quiet about an added rewritable one. A rule about model output has to be run against
model output; this is the second time in this phase that lesson has been paid for.

**Step 4 -- the lyric measurement (the two checks only a live run can make).** Three
generations from the real assembled prompt, then the lint over what came back.
**Result 2026-08-26, `gemma4:12b-32k`:**
- *A thinking model writes a whole song.* 1333 / 1726 / 1592 characters, `finish_reason: stop`
  every run, **0 characters of reasoning**, first content delta at **0.19-0.43 s**. The
  baseline without `reasoning_effort: "none"` was 85 characters and 44.08 s (LLM-SURFACE 12.1),
  so the policy is reaching the request end to end. Whole generation 6.9-9.2 s.
- *The lint fires on what the model actually writes.* 4 / 6 / 1 findings, all stray production
  cues the model put inside the lyrics -- `[dreamy female vocals]`, `[driving beat]`,
  `[slow build]`, `[fade out]` -- plus an `ExtraSection` for `[Outro]`. Stray directions in
  **3 of 3**, against the 10-of-13 rate predicted by LLM-SURFACE 12.5. The scanner works and
  the model is not well-behaved, which is exactly why the lint exists rather than a prompt rule.

**Steps 2, 3 and 5 were run on 2026-08-26 and pass**, with three UI defects found and fixed in
T-213 (draft textarea colour, the approval notice sitting below the lint findings and so off
screen, and a clean check rendering nothing). **Re-run steps 2 and 3 against those fixes before
tagging `phase2-done`.** Full results in PROJECT.md's session log; the on-disk provenance from
step 5 is quoted there verbatim.

**Step 2 -- brief to approved lyric (the ROADMAP check).**
1. Open Lyrics. The form opens prefilled, and the subtitle names the profile being written for.
2. Generate. Watch the status: `Starting...` then `Thinking...` (reasoning on the status line)
   then `Writing...`. **The thinking text is the point** -- if the panel sits silent for tens
   of seconds, T-208's proof-of-life is not working.
3. Let it finish. The draft is editable, and a version appears in the list labelled with the
   model name.
4. Edit the draft, Save. A second version appears, labelled `edited from v1`.
5. Check. Lint findings render as advisories -- warnings and info, nothing blocking.
6. Approve a version. The approved badge appears and the "ready for audio" line shows.

**Step 3 -- the optimizer, accepted and reverted.**
1. Optimize prompt. The diff appears: original left, rewrite right, changed words highlighted.
2. Confirm the **Optimize button is now disabled** -- one rewrite in play at a time.
3. Revert. The diff disappears; Generate is back to sending the brief.
4. Optimize again, then Edit, change a word by hand, Done editing, Accept. The banner reads
   "Generate will send your accepted prompt".
5. Change any brief field. **The banner must disappear** -- an accepted prompt does not
   survive the brief it was written against.
6. Optimize, Accept, Generate. The lyric that comes back was written from the accepted prompt.

**Step 5 -- what landed on disk.** Open the lyric document under the app config dir
(`library/projects/my-first-song/lyrics/<doc-id>.json`) and confirm the claims T-209 and T-210
make about provenance:
- every version present, with its `source` (`llm` with the model name, `human`, `edited`);
- `approved` naming the version approved in step 2;
- **`prompt_optimized: true` on the version generated in step 3.6 and `false` on the others.**
  This is the one T-210 claim that only a real run proves, and the flag is the record of the
  user's consent.

**Recording the result.** Results go in PROJECT.md's session log, pass or fail, with the
measurement report pasted verbatim. A step that fails becomes a T-212 fix-up brief rather than
a silent retry; the phase closes and tags `phase2-done` only when steps 1-5 all pass.
