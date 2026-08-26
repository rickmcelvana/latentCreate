# T-203a: the lyric scanner, and the directions the model sings
**Depends:** T-202b | **Crate/dir:** crates/create-core
**Files to create/modify:**
- `crates/create-core/src/lyrics/lint.rs` (create)
- `crates/create-core/src/lyrics.rs` (modify -- one line)
- `testdata/lyrics/generated-with-directions.txt` (create)
- `testdata/lyrics/generated-clean.txt` (create)
- `testdata/lyrics/generated-parenthesised-directions.txt` (create)

## Goal

Read the structure tags out of a lyric, and report the bracketed and parenthesised
production directions the model writes into it -- the failure T-202 proved the prompt
cannot prevent. Structure checking (missing, out-of-order, extra sections) is T-203b,
which adds rules to the same function.

## Spec

### Nothing here can block, and nothing here edits

Two facts force it. **Nothing in ComfyUI publishes which structure tags a model accepts**
-- `TextEncodeAceStepAudio1.5.lyrics` is a bare STRING with an empty `choices` list
(MCP-SURFACE 15.1) -- so every rule is a profile author's claim, not something the model
confirmed. And the text being checked is the user's, which this app never modifies without
an explicit accept step. Hence `LintSeverity` has **no `Error` variant**: there is nothing
to be authoritative with.

### What counts as a tag line

A line counts as structure when it **opens** with a bracket token. It may carry several,
because the model writes `[inst] [driving beat]`, and it may carry trailing text.

**The trailing-text case is not hypothetical, and getting it wrong is worse than missing
it.** One of the 13 saved generations wrote every one of its directions as
`[Verse] (dreamy female vocals, ethereal synth pads)`. An earlier version of this rule
required a tag line to be *nothing but* tags, and that file -- a correctly structured song,
one of only three in the corpus with no bracketed strays -- came back reported as having
**no structure at all**. It is `testdata/lyrics/generated-parenthesised-directions.txt`
for that reason.

A bracket in the middle of a sung line ("I saw the [neon] sign") is words, not structure.

### Numbering tolerance is for the user, not the model

`normalize_name` drops a trailing number, so `[Verse 2]` matches a declared `[Verse]`.
Across 99 declared tags in the corpus **the model never numbered one** -- it copies the
prompt exactly. The shipped ACE-Step template, however, writes `[Verse 1]`
(MCP-SURFACE 15.2), so lyrics pasted from ComfyUI, or typed by a songwriter out of habit,
are the case this exists for. Say so in the doc comment or someone will delete it as dead
tolerance.

### The two rules in this task

- **`UnknownTag`** -- a bracket token that is neither declared by the profile nor a section
  the brief asked for. A section the *user* requested is never unknown: the brief is their
  own words about their own song.
- **`TextAfterTag`** -- text sharing a line with a real structure tag. Reported only when at
  least one tag on the line is real structure; after a tag already reported as unknown, a
  second finding for the same line says nothing new.

`LintFinding` is defined complete here, including the three variants T-203b's rules
produce. It is the shared contract, and splitting an enum across two runs would mean
editing it twice.

### Fixtures are real model output

Three captured generations go into `testdata/lyrics/`, unedited. A hand-written fixture
would be written to agree with the code; these were written by the model before the code
existed. Their exact contents are in the scratchpad the producer was given; they are the
files named above.

## Reference implementation

Compiled, `cargo fmt` clean, clippy clean, guards mutation-tested, and swept over all 13
saved generations -- where its counts matched an independent analysis exactly.

### `crates/create-core/src/lyrics.rs` (one line)

Add below the imports:

```rust
pub mod lint;
```

### `crates/create-core/src/lyrics/lint.rs` (new file)

`src/lyrics.rs` and `src/lyrics/lint.rs` coexist -- the 2018 module system allows a file
module to own a directory, so **`lyrics.rs` is not moved to `lyrics/mod.rs`**. Moving it
would turn a one-line change into a whole-file rename in the diff.

```rust
//! Advisory checks over lyric text.
//!
//! **Nothing here can block a generation, and nothing here edits the lyrics.**
//! Two facts force that. Nothing in ComfyUI publishes which structure tags a
//! model accepts -- `TextEncodeAceStepAudio1.5.lyrics` is a bare STRING with an
//! empty `choices` list (MCP-SURFACE 15.1) -- so every rule below is a claim
//! made by a profile author, not something the model confirmed. And the text
//! being checked is the user's, which this app never modifies without an
//! explicit accept step (ARCHITECTURE 6).
//!
//! The rules are sized by measurement, not taste. Over the 13 generations saved
//! from the T-202 prompt runs (two of them in `testdata/lyrics/`): the requested
//! section order held in 13 of 13, an extra `[Outro]` appeared in 9 of 13, and
//! 46 stray production directions appeared across 10 of the 13 files
//! (LLM-SURFACE 12.5). So a missing or reordered section is worth a warning, an
//! extra section is worth a note, and the stray directions are the finding this
//! module mainly exists to make.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{expand_structure, structure_tags, LyricBrief};
use crate::profile::ModelProfile;

/// How loudly a finding should be shown.
///
/// There is deliberately no `Error`. A rule nothing can verify (see the module
/// docs) must not be able to stop a user generating from their own words.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LintSeverity {
    /// Worth the user's attention before they approve.
    Warning,
    /// Worth showing, but a lyric with only these is fine.
    Info,
}

/// One thing worth saying about a lyric.
///
/// Line numbers are 1-based, so they can be handed straight to an editor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum LintFinding {
    /// A bracketed line that is neither a tag the profile declares nor a
    /// section the brief asked for -- in practice a production or vocal-style
    /// direction, which the model will sing.
    UnknownTag {
        /// The bracketed text exactly as written.
        tag: String,
        line: u32,
    },
    /// A section the brief asked for that never appears.
    MissingSection { section: String },
    /// The requested sections all appear, but not in the requested order.
    OutOfOrder {
        /// The order the brief asked for, for the message.
        requested: Vec<String>,
    },
    /// A section beyond what the brief asked for. **Info**: 9 of 13 real
    /// generations added an `[Outro]`, which is usually welcome.
    ExtraSection {
        /// The bracketed text exactly as written.
        tag: String,
        line: u32,
    },
    /// Text sharing a line with a structure tag, e.g.
    /// `[Verse] (dreamy female vocals)`.
    ///
    /// It is not part of the tag, so it is lyric -- the model sings it. One of
    /// the 13 saved generations wrote every one of its directions this way,
    /// in parentheses, which is why this is a separate finding from
    /// [`LintFinding::UnknownTag`] rather than the same rule.
    TextAfterTag {
        /// The text following the tags on that line, as written.
        text: String,
        line: u32,
    },
    /// The lyric carries no structure tags at all. The model will treat the
    /// whole text as one undifferentiated block.
    NoStructureTags,
}

impl LintFinding {
    /// How loudly to show this finding.
    pub fn severity(&self) -> LintSeverity {
        match self {
            Self::ExtraSection { .. } => LintSeverity::Info,
            _ => LintSeverity::Warning,
        }
    }
}

/// A structure tag as it was found in the text.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FoundTag {
    /// Exactly as written, brackets included.
    text: String,
    /// Comparable form: brackets off, trailing number off, lowercased.
    name: String,
    /// 1-based.
    line: u32,
}

/// Comparable form of a tag or section name.
///
/// **A trailing number is dropped**, so `[Verse 2]` matches a declared
/// `[Verse]`. That tolerance is for the user's own text rather than the
/// model's: across 99 declared tags in the saved generations the model never
/// once numbered one, while the shipped ACE-Step template writes `[Verse 1]`
/// (MCP-SURFACE 15.2), so lyrics pasted from there -- or typed by a songwriter
/// out of habit -- are the case that needs it.
fn normalize_name(raw: &str) -> String {
    let inner = raw
        .trim()
        .trim_start_matches('[')
        .trim_end_matches(']')
        .trim();
    let without_number = inner
        .trim_end_matches(|c: char| c.is_ascii_digit())
        .trim_end();
    let base = if without_number.is_empty() {
        inner
    } else {
        without_number
    };
    base.to_ascii_lowercase()
}

/// One line that opens with structure tags.
#[derive(Debug, Clone, PartialEq, Eq)]
struct TagLine {
    tags: Vec<FoundTag>,
    /// Anything after the tags on that line.
    trailing: Option<String>,
    /// 1-based.
    line: u32,
}

/// Splits a line into its leading bracket tokens and whatever follows.
///
/// A line counts as structure only when it *opens* with a bracket token, so a
/// lyric that mentions something in brackets mid-sentence is left alone. It may
/// carry several tags, because the model writes `[inst] [driving beat]`, and it
/// may carry trailing text, because one generation wrote every direction as
/// `[Verse] (dreamy female vocals)` -- reading that as "not a tag line" would
/// report a correctly structured song as having no structure at all.
fn split_tag_line(line: &str) -> Option<(Vec<String>, Option<String>)> {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return None;
    }
    let mut tags = Vec::new();
    let mut rest = trimmed;
    while rest.starts_with('[') {
        let close = rest.find(']')?;
        tags.push(rest[..=close].to_string());
        rest = rest[close + 1..].trim_start();
    }
    let trailing = if rest.is_empty() {
        None
    } else {
        Some(rest.to_string())
    };
    Some((tags, trailing))
}

/// Every tag-bearing line in the text, in reading order.
fn scan_tag_lines(text: &str) -> Vec<TagLine> {
    let mut lines = Vec::new();
    for (index, raw) in text.lines().enumerate() {
        let Some((tags, trailing)) = split_tag_line(raw) else {
            continue;
        };
        let line = index as u32 + 1;
        lines.push(TagLine {
            tags: tags
                .into_iter()
                .map(|tag| FoundTag {
                    name: normalize_name(&tag),
                    text: tag,
                    line,
                })
                .collect(),
            trailing,
            line,
        });
    }
    lines
}

/// The profile's instrumental marker, in comparable form.
fn instrumental_name(profile: &ModelProfile) -> Option<String> {
    profile
        .lyrics_contract
        .as_ref()?
        .instrumental_token
        .as_ref()
        .map(|token| normalize_name(token))
}

/// Checks one lyric against the profile that will sing it and the brief that
/// asked for it.
///
/// Findings come back in a stable order -- unknown tags by line, then missing
/// sections in the order requested, then ordering, then extra sections by line
/// -- so a UI can render them without sorting and a test can assert the whole
/// list.
///
/// An empty result means nothing was noticed, never that the lyric was
/// approved: see the module docs for why none of this is authoritative.
pub fn lint_lyrics(profile: &ModelProfile, brief: &LyricBrief, text: &str) -> Vec<LintFinding> {
    let scanned = scan_tag_lines(text);
    let found: Vec<FoundTag> = scanned
        .iter()
        .flat_map(|l| l.tags.iter().cloned())
        .collect();
    if found.is_empty() {
        return vec![LintFinding::NoStructureTags];
    }

    let instrumental = instrumental_name(profile);
    let declared: BTreeSet<String> = structure_tags(profile)
        .iter()
        .map(|tag| normalize_name(tag))
        .collect();
    let requested: Vec<String> = expand_structure(&brief.structure)
        .iter()
        .map(|section| normalize_name(section))
        .collect();

    let is_instrumental = |name: &str| instrumental.as_deref() == Some(name);
    let mut findings = Vec::new();

    for tag in &found {
        if is_instrumental(&tag.name) {
            continue;
        }
        if !declared.contains(&tag.name) && !requested.contains(&tag.name) {
            findings.push(LintFinding::UnknownTag {
                tag: tag.text.clone(),
                line: tag.line,
            });
        }
    }

    // Trailing text is only worth reporting when the line's tags are real
    // structure. After a tag that is already reported as unknown, a second
    // finding for the same line says nothing new.
    for scanned in &scanned {
        let Some(trailing) = &scanned.trailing else {
            continue;
        };
        let known = scanned.tags.iter().any(|tag| {
            is_instrumental(&tag.name)
                || declared.contains(&tag.name)
                || requested.contains(&tag.name)
        });
        if known {
            findings.push(LintFinding::TextAfterTag {
                text: trailing.clone(),
                line: scanned.line,
            });
        }
    }

    findings
}
```

The `findings` return above is temporary in the sense that T-203b inserts its block just
before it; nothing about it changes.

### Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn profile(name: &str) -> ModelProfile {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("profiles")
            .join(name);
        let text = std::fs::read_to_string(&path).expect("shipped profile is readable");
        serde_json::from_str(&text).expect("shipped profile parses")
    }

    fn ace() -> ModelProfile {
        profile("ace-step-1.5-turbo.json")
    }

    fn fixture(name: &str) -> String {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("testdata")
            .join("lyrics")
            .join(name);
        std::fs::read_to_string(&path).expect("lyric fixture is readable")
    }

    fn warnings(findings: &[LintFinding]) -> Vec<&LintFinding> {
        findings
            .iter()
            .filter(|f| f.severity() == LintSeverity::Warning)
            .collect()
    }

    /// Invariant: every production direction in a real generation is reported.
    ///
    /// The fixture is model output, not a hand-written example: a hand-written
    /// one would be written to agree with the code. This one has seven
    /// direction blocks, and the count is asserted so a rule that silently
    /// stopped matching most of them would fail.
    #[test]
    fn test_real_generation_flags_every_production_direction() {
        let findings = lint_lyrics(
            &ace(),
            &LyricBrief::default(),
            &fixture("generated-with-directions.txt"),
        );
        let unknown: Vec<&LintFinding> = findings
            .iter()
            .filter(|f| matches!(f, LintFinding::UnknownTag { .. }))
            .collect();
        assert_eq!(unknown.len(), 7, "{findings:#?}");
        assert!(
            matches!(unknown[0], LintFinding::UnknownTag { tag, line }
                if tag == "[female vocal, dreamy]" && *line == 2),
            "{:#?}",
            unknown[0]
        );
    }
    /// Invariant: a generation that writes its directions in parentheses on the
    /// tag line is still read as structured, and the directions are reported.
    ///
    /// This fixture is why the scanner allows trailing text. An earlier rule
    /// required a tag line to be nothing but tags, and this real, correctly
    /// structured song came back reported as having **no structure at all** --
    /// the worst possible answer for the best-behaved file in the corpus.
    #[test]
    fn test_parenthesised_directions_are_reported_and_the_song_still_parses() {
        let findings = lint_lyrics(
            &ace(),
            &LyricBrief::default(),
            &fixture("generated-parenthesised-directions.txt"),
        );
        assert!(
            !findings.contains(&LintFinding::NoStructureTags),
            "the song is tagged: {findings:#?}"
        );
        let trailing: Vec<&LintFinding> = findings
            .iter()
            .filter(|f| matches!(f, LintFinding::TextAfterTag { .. }))
            .collect();
        assert_eq!(trailing.len(), 7, "{findings:#?}");
        assert!(
            matches!(trailing[0], LintFinding::TextAfterTag { text, line }
                if text == "(dreamy female vocals, ethereal synth pads)" && *line == 1),
            "{:#?}",
            trailing[0]
        );
        assert!(
            !findings.iter().any(|f| matches!(
                f,
                LintFinding::MissingSection { .. } | LintFinding::OutOfOrder { .. }
            )),
            "the requested structure was followed: {findings:#?}"
        );
    }
    /// Invariant: a line whose tag is already reported as unknown does not also
    /// report its trailing text. One line, one thing to fix.
    #[test]
    fn test_text_after_an_unknown_tag_is_not_reported_twice() {
        let text = "[Verse]
a
[whispering] I cannot stay
[Chorus]
b
[Verse]
c
[Chorus]
d
[Bridge]
e
[Chorus]
f
";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert_eq!(
            findings,
            vec![LintFinding::UnknownTag {
                tag: "[whispering]".to_string(),
                line: 3
            }]
        );
    }
    /// Invariant: a real section tag on the line is enough to report the
    /// trailing text, even when another tag on the same line was unknown.
    ///
    /// The two findings are about different things: one says a bracket is not a
    /// structure tag, the other says there are words on a structure line that
    /// the model will sing. Reporting only the first would let the words
    /// through, which is the failure this whole module exists to catch.
    #[test]
    fn test_a_real_tag_on_the_line_still_reports_the_trailing_text() {
        let text = "[Verse] [whispered] I cannot stay
a
[Chorus]
b
[Verse]
c
[Chorus]
d
[Bridge]
e
[Chorus]
f
";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert_eq!(
            findings,
            vec![
                LintFinding::UnknownTag {
                    tag: "[whispered]".to_string(),
                    line: 1
                },
                LintFinding::TextAfterTag {
                    text: "I cannot stay".to_string(),
                    line: 1
                },
            ]
        );
    }
    /// Invariant: a generation that kept to the tags produces no warnings at
    /// all. Without this, a rule that fired on everything would still pass the
    /// test above.
    #[test]
    fn test_a_clean_generation_produces_no_warnings() {
        let findings = lint_lyrics(
            &ace(),
            &LyricBrief::default(),
            &fixture("generated-clean.txt"),
        );
        assert!(warnings(&findings).is_empty(), "{findings:#?}");
    }
    /// Invariant: `[Verse 2]` counts as a Verse. The shipped ACE-Step template
    /// numbers its verses, so a lyric pasted from ComfyUI must not come back
    /// covered in warnings about tags the model itself ships.
    #[test]
    fn test_numbered_tags_match_the_unnumbered_declaration() {
        let text = "[Verse 1]\nfirst\n[Chorus]\nhook\n[Verse 2]\nsecond\n[Chorus]\nhook\n[Bridge]\nturn\n[Chorus]\nhook\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert!(findings.is_empty(), "{findings:#?}");
    }
    /// Invariant: an untagged lyric gets exactly one finding. Reporting every
    /// requested section as missing as well would bury the one thing the user
    /// needs to do.
    #[test]
    fn test_untagged_lyrics_report_once() {
        let findings = lint_lyrics(
            &ace(),
            &LyricBrief::default(),
            "Neon on the dashboard\nmidnight in the rain\n",
        );
        assert_eq!(findings, vec![LintFinding::NoStructureTags]);
    }
    /// Invariant: a section the user asked for is never an unknown tag, even
    /// when the profile does not declare it. The brief is the user's own words
    /// about their own song.
    #[test]
    fn test_a_section_the_brief_requested_is_not_unknown() {
        let brief = LyricBrief {
            structure: "V-Spoken word-C".to_string(),
            ..LyricBrief::default()
        };
        let text = "[Verse]\nfirst\n[Spoken word]\nsaid aloud\n[Chorus]\nhook\n";
        let findings = lint_lyrics(&ace(), &brief, text);
        assert!(findings.is_empty(), "{findings:#?}");
    }
    /// Invariant: the instrumental marker is neither missing nor extra. It
    /// marks a gap in the singing, not a section of the song, and it appeared
    /// in 7 of the 13 saved generations without ever being asked for.
    #[test]
    fn test_instrumental_marker_is_not_a_section() {
        let text = "[inst]\n[Verse]\nfirst\n[Chorus]\nhook\n[inst]\n[Verse]\nsecond\n[Chorus]\nhook\n[Bridge]\nturn\n[Chorus]\nhook\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert!(findings.is_empty(), "{findings:#?}");
    }
    /// Invariant: brackets inside a sung line are words, not structure. A lyric
    /// mentioning something in brackets must not be read as a tag.
    #[test]
    fn test_brackets_inside_a_lyric_line_are_not_tags() {
        let text = "[Verse]\nI saw the [neon] sign and kept driving\n[Chorus]\nhook\n[Verse]\nb\n[Chorus]\nhook\n[Bridge]\nc\n[Chorus]\nhook\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert!(findings.is_empty(), "{findings:#?}");
    }
    /// Invariant: two tags on one line are both read. The model writes
    /// `[inst] [driving beat]`, and reading that as a single token would let
    /// the direction through unreported.
    #[test]
    fn test_two_tags_on_one_line_are_both_read() {
        let text = "[inst] [driving 80s synth beat]\n[Verse]\na\n[Chorus]\nb\n[Verse]\nc\n[Chorus]\nd\n[Bridge]\ne\n[Chorus]\nf\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert_eq!(
            findings,
            vec![LintFinding::UnknownTag {
                tag: "[driving 80s synth beat]".to_string(),
                line: 1
            }]
        );
    }
    /// Invariant: findings survive the Tauri boundary, severity included.
    #[test]
    fn test_findings_round_trip_through_json() {
        let findings = vec![
            LintFinding::UnknownTag {
                tag: "[female vocal, dreamy]".to_string(),
                line: 2,
            },
            LintFinding::NoStructureTags,
        ];
        let json = serde_json::to_string(&findings).unwrap();
        assert!(json.contains("\"unknown_tag\""), "{json}");
        assert!(json.contains("\"no_structure_tags\""), "{json}");
        let back: Vec<LintFinding> = serde_json::from_str(&json).unwrap();
        assert_eq!(back, findings);
    }
}
```

## Acceptance criteria

- [ ] `cargo test -p create-core` passes; `npm run gate` green.
- [ ] The twelve named tests exist and pass, and the three fixtures are committed
      **unedited** -- reflowing them changes the line numbers the tests assert.
- [ ] These mutations each make a named test fail (verified before the brief was written):
      - the scanner stops reading tags -> twelve tests, including
        `test_a_clean_generation_produces_no_warnings`
      - unknown tags stop being reported ->
        `test_real_generation_flags_every_production_direction`
      - trailing text is never captured ->
        `test_parenthesised_directions_are_reported_and_the_song_still_parses`
      - numbering tolerance is dropped ->
        `test_numbered_tags_match_the_unnumbered_declaration`
      - the trailing-text check requires *every* tag on the line to be known ->
        `test_a_real_tag_on_the_line_still_reports_the_trailing_text`
- [ ] No changes outside the listed files. No new dependencies.

## Out of scope

- Missing, out-of-order and extra sections -- T-203b.
- Any judgement about whether a lyric is good, and any rewriting of it.
- Surfacing findings in the UI -- T-209.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --read crates/create-core/src/profile.rs --file crates/create-core/src/lyrics/lint.rs --file crates/create-core/src/lyrics.rs
```

`profile.rs` is `--read` because the new code reaches into `ModelProfile` and
`LyricsContract`. The fixtures are added by the producer, not the executor -- they are
captured data, not code to write.
