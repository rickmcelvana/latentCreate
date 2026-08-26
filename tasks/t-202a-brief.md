# T-202a: the lyric brief, its prefills, and the two numbers derived from it
**Depends:** none | **Crate/dir:** crates/create-core
**Files to create/modify:**
- `crates/create-core/src/lyrics.rs` (create)
- `crates/create-core/src/lib.rs` (modify)

## Goal

The typed brief the Lyrics Studio form is built on, prefilled with a brief someone could
send unchanged, plus the two things derived from it: the expanded section list and the
completion-token budget. Pure -- no I/O, no async, no clock. Prompt assembly is T-202b,
which fills in the rest of the same file.

## Spec

### `LyricBrief` and `PointOfView`

The fields ARCHITECTURE 6 names: theme, style tags, mood, structure, language, point of
view, era/references, explicit allowed, target duration. Serde types, because this crosses
the Tauri boundary and lands in provenance (CONVENTIONS).

Two of them carry a decision worth stating:

- **`language` is a writing instruction, not a slot value.** The profile's `language`
  input is `from_node_choices` and is read live from the node schema by Phase 3's param
  panel. Keeping the brief's language a plain name is what lets the Lyrics Studio render
  with no running ComfyUI (PROJECT.md decisions log, 2026-08-25).
- **`Default` is a filled-in brief, not an empty one.** ARCHITECTURE 6 requires the form
  to open with strong examples; empty boxes produce a generic song and teach the user
  nothing about what the fields do.

### `expand_structure`

`"V-C-V-C-B-C"` becomes Verse, Chorus, Verse, Chorus, Bridge, Chorus. The rules that
matter, both of which are tested:

- **An unrecognised token is passed through verbatim**, never dropped. Dropping it would
  silently change the structure the user asked for -- the same class of mistake as
  editing their lyrics.
- **Whitespace is not a separator.** Splitting on spaces would turn a section the user
  named "Spoken word" into two sections. Separators are `-`, `,` and `/`.

There is deliberately no letter for an instrumental section: each profile names its own
instrumental token, and guessing one would put a tag in the prompt the model does not
take.

### `token_budget`

Grounded in measurement rather than taste. Two live runs of the default brief against
`gemma4:12b-32k` used **383 and 422 completion tokens** (LLM-SURFACE 12.3); this returns
**1260** for that brief, and is clamped to 800..4096.

**The headroom is for lyrics, not for thinking.** A reasoning model spends any budget on
chain-of-thought first -- 2000 tokens bought 85 characters of song (LLM-SURFACE 12.1) --
and the answer to that is suppressing the reasoning in T-204/T-205, never a bigger number
here. Say so in the doc comment, because the next person to see a truncated song will
reach for this constant.

## Reference implementation

Compiled, `cargo fmt` clean, clippy clean, and the guards mutation-tested. Transcribe it.

### `crates/create-core/src/lyrics.rs` (new file -- this task writes the first half)

The module doc mentions prompt assembly, which arrives in T-202b. Keep it as written; the
file is finished by the next task.

```rust
//! The lyric brief, and the prompt assembled from it.
//!
//! Pure: no I/O, no async, no clock. The shell turns the two strings this module
//! returns into an `llm_bridge::ChatRequest`; nothing here knows what a provider
//! is.
//!
//! **The assembled prompt is the one that was captured live**, not a new one
//! (docs/LLM-SURFACE.md section 12). Its shape produced a complete, correctly
//! structured song from the model this app recommends for lyrics, and the parts
//! that vary by model are read from the profile rather than written here -- the
//! two shipped profiles disagree about the capitalisation of their own structure
//! tags, so a hardcoded list would tell a MiniMax user to write `[Verse]` at a
//! model that expects `[verse]`.

use serde::{Deserialize, Serialize};

use crate::profile::{InputSpec, ModelProfile};

/// Whose voice the lyric is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PointOfView {
    #[default]
    FirstPerson,
    SecondPerson,
    ThirdPerson,
}

impl PointOfView {
    /// How this reads inside the prompt.
    pub fn as_prompt_text(self) -> &'static str {
        match self {
            Self::FirstPerson => "first person",
            Self::SecondPerson => "second person",
            Self::ThirdPerson => "third person",
        }
    }
}

/// What the user asked for, before it becomes a prompt.
///
/// Every field is prefilled by [`LyricBrief::default`]: ARCHITECTURE 6 requires
/// the form to open with strong examples rather than empty boxes, because an
/// empty brief produces a generic song and teaches the user nothing about what
/// the fields do.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LyricBrief {
    /// What the song is about.
    pub theme: String,
    /// Genre and style tags, comma-separated the way the audio side wants them.
    pub style_tags: String,
    pub mood: String,
    /// Section letters, e.g. `"V-C-V-C-B-C"`. See [`expand_structure`].
    pub structure: String,
    /// The language to write in, as a person would name it (`"English"`).
    ///
    /// **A writing instruction, not a slot value.** The profile's `language`
    /// input is a live node enum consumed by the audio pipeline in Phase 3;
    /// conflating the two would make writing lyrics require a running ComfyUI.
    pub language: String,
    pub point_of_view: PointOfView,
    /// Era or artist references, when the user gives any.
    #[serde(default)]
    pub era_refs: Option<String>,
    /// Whether explicit language is allowed. Stated either way, so the model is
    /// never left to guess from the genre.
    pub explicit_allowed: bool,
    /// Target song length. Constrains how much lyric to write, and feeds
    /// [`token_budget`].
    pub target_duration_s: u32,
}

impl Default for LyricBrief {
    fn default() -> Self {
        Self {
            theme: "A night drive out of a city you are leaving for good".to_string(),
            style_tags: "synthwave, retro, 80s, dreamy, female vocal, driving beat".to_string(),
            mood: "bittersweet, hopeful".to_string(),
            structure: "V-C-V-C-B-C".to_string(),
            language: "English".to_string(),
            point_of_view: PointOfView::FirstPerson,
            era_refs: None,
            explicit_allowed: false,
            target_duration_s: 120,
        }
    }
}

/// Base token allowance for one generation.
const TOKEN_BUDGET_BASE: u32 = 300;

/// Added per section in the requested structure.
const TOKENS_PER_SECTION: u32 = 140;

/// Floor, so a one-section brief still has room to answer.
const TOKEN_BUDGET_MIN: u32 = 800;

/// Ceiling, so a pasted structure of a hundred sections cannot ask for a budget
/// no endpoint will honour.
const TOKEN_BUDGET_MAX: u32 = 4096;

/// Completion-token allowance for one lyric generation.
///
/// Grounded in a measurement, not a guess: a 120-second `V-C-V-C-B-C` song from
/// `gemma4:12b-32k` used **383 and 422 completion tokens** across two live runs
/// (LLM-SURFACE 12.3), and this returns 1260 for that brief.
///
/// **The headroom is for lyrics, not for thinking.** A model that reasons will
/// spend any budget on chain-of-thought first -- 2000 tokens bought 85
/// characters of song (LLM-SURFACE 12.1) -- and the answer to that is
/// suppressing the reasoning, never a larger number here.
pub fn token_budget(brief: &LyricBrief) -> u32 {
    let sections = expand_structure(&brief.structure).len() as u32;
    let raw = TOKEN_BUDGET_BASE
        .saturating_add(sections.saturating_mul(TOKENS_PER_SECTION))
        .saturating_add(brief.target_duration_s);
    raw.clamp(TOKEN_BUDGET_MIN, TOKEN_BUDGET_MAX)
}

/// Expands a structure string into named sections.
///
/// `"V-C-V-C-B-C"` becomes Verse, Chorus, Verse, Chorus, Bridge, Chorus. Single
/// letters are expanded from the table below; **anything else is passed through
/// verbatim**, so a user who types `"Verse-Chorus-Spoken word"` gets what they
/// asked for. Dropping an unrecognised token would silently change the structure
/// the user requested, which is the same class of mistake as editing their
/// lyrics.
///
/// Separators are `-`, `,` and `/`, and **not whitespace**: splitting on spaces
/// too would turn a section the user named "Spoken word" into two sections, and
/// mangling their structure is the thing this function must not do. A brief
/// typed as `"V C V C"` therefore arrives as one unrecognised token and is
/// passed through as written.
///
/// There is deliberately no letter for an instrumental section: the profile
/// names its own instrumental token, and guessing one would put a tag in the
/// prompt that the model does not accept.
pub fn expand_structure(structure: &str) -> Vec<String> {
    structure
        .split(['-', ',', '/'])
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .map(|token| match token.to_ascii_uppercase().as_str() {
            "V" => "Verse".to_string(),
            "C" => "Chorus".to_string(),
            "B" => "Bridge".to_string(),
            "I" => "Intro".to_string(),
            "O" => "Outro".to_string(),
            "P" => "Pre-Chorus".to_string(),
            "H" => "Hook".to_string(),
            "R" => "Refrain".to_string(),
            _ => token.to_string(),
        })
        .collect()
}
```

**Drop the `use crate::profile::{InputSpec, ModelProfile};` line from the block above in
this task.** Nothing here touches a profile, and clippy at `-D warnings` fails the build on
an unused import. T-202b adds it back with the code that needs it; the serde import stays.

### `crates/create-core/src/lyrics.rs` tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// Invariant: letters expand and anything else survives untouched. Dropping
    /// a token the table does not know would silently rewrite the structure the
    /// user asked for.
    #[test]
    fn test_expand_structure_keeps_unknown_tokens_verbatim() {
        assert_eq!(
            expand_structure("V-C-V-C-B-C"),
            vec!["Verse", "Chorus", "Verse", "Chorus", "Bridge", "Chorus"]
        );
        assert_eq!(
            expand_structure("I-V-P-C-Spoken word-O"),
            vec![
                "Intro",
                "Verse",
                "Pre-Chorus",
                "Chorus",
                "Spoken word",
                "Outro"
            ],
            "a multi-word section name must survive as one section"
        );
        assert_eq!(
            expand_structure("v, c , b"),
            vec!["Verse", "Chorus", "Bridge"]
        );
        assert_eq!(expand_structure("V/C/V"), vec!["Verse", "Chorus", "Verse"]);
        assert!(expand_structure("   ").is_empty());
    }

    /// Invariant: the budget leaves real headroom over what a song of this shape
    /// actually costs. Two live runs of the default brief used 383 and 422
    /// completion tokens (LLM-SURFACE 12.3); a budget that merely matched them
    /// would truncate the next song that runs a little long.
    #[test]
    fn test_token_budget_clears_the_measured_cost_of_a_real_song() {
        const MEASURED_WORST_CASE: u32 = 422;
        let budget = token_budget(&LyricBrief::default());
        assert!(
            budget >= MEASURED_WORST_CASE * 2,
            "budget {budget} leaves too little over the measured {MEASURED_WORST_CASE}"
        );
        assert!(budget <= TOKEN_BUDGET_MAX);
    }

    /// Invariant: the budget grows with the work asked for, and never falls
    /// below the floor or above the ceiling.
    #[test]
    fn test_token_budget_scales_and_stays_in_range() {
        let short = LyricBrief {
            structure: "C".to_string(),
            target_duration_s: 10,
            ..LyricBrief::default()
        };
        let long = LyricBrief {
            structure: "I-V-P-C-V-P-C-B-C-O".to_string(),
            target_duration_s: 300,
            ..LyricBrief::default()
        };
        let absurd = LyricBrief {
            structure: "V-".repeat(200),
            target_duration_s: 3000,
            ..LyricBrief::default()
        };

        assert_eq!(token_budget(&short), TOKEN_BUDGET_MIN);
        assert!(token_budget(&long) > token_budget(&LyricBrief::default()));
        assert_eq!(token_budget(&absurd), TOKEN_BUDGET_MAX);
    }

    /// Invariant: the prefilled brief is a brief someone could send as-is. An
    /// empty default would open the form with empty boxes, which ARCHITECTURE 6
    /// rules out.
    #[test]
    fn test_default_brief_is_filled_in() {
        let brief = LyricBrief::default();
        assert!(!brief.theme.is_empty());
        assert!(!brief.style_tags.is_empty());
        assert!(!brief.mood.is_empty());
        assert!(!brief.language.is_empty());
        assert!(brief.target_duration_s > 0);
        assert!(!expand_structure(&brief.structure).is_empty());
    }

    /// Invariant: the brief survives the Tauri boundary unchanged. It is the
    /// record of what the user asked for, and it reaches provenance.
    #[test]
    fn test_brief_round_trips_through_json() {
        let brief = LyricBrief {
            era_refs: Some("early Chromatics".to_string()),
            point_of_view: PointOfView::ThirdPerson,
            explicit_allowed: true,
            ..LyricBrief::default()
        };
        let json = serde_json::to_string(&brief).unwrap();
        assert!(json.contains("\"third_person\""), "{json}");
        let back: LyricBrief = serde_json::from_str(&json).unwrap();
        assert_eq!(back, brief);
    }
}
```

### `crates/create-core/src/lib.rs`

`pub mod lyrics;` beside `pub mod generation;`, `pub use lyrics::*;` beside the other
re-exports, and one line in the crate doc comment:

```rust
//! [`lyrics`] holds the lyric brief and the prompt assembled from it.
```

## Acceptance criteria

- [ ] `cargo test -p create-core` passes; `npm run gate` green.
- [ ] The five named tests exist and pass.
- [ ] These mutations each make a named test fail (verified before the brief was written):
      - `expand_structure` rewrites an unknown token instead of passing it through ->
        `test_expand_structure_keeps_unknown_tokens_verbatim`
      - `token_budget` ignores the brief and returns the floor ->
        `test_token_budget_clears_the_measured_cost_of_a_real_song`
- [ ] No changes outside the two listed files. No new dependencies.

## Out of scope

- `assemble_system_prompt` / `assemble_user_message` and the two private profile readers
  they use -- T-202b.
- Anything that validates lyric text -- T-203.
- Any Tauri command, and anything in `app/`.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --file crates/create-core/src/lyrics.rs --file crates/create-core/src/lib.rs
```
