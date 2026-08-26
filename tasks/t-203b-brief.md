# T-203b: does the lyric have the sections the brief asked for
**Depends:** T-203a | **Crate/dir:** crates/create-core
**Files to create/modify:**
- `crates/create-core/src/lyrics/lint.rs` (modify -- adds rules to `lint_lyrics`)

## Goal

Three structure rules on top of T-203a's scanner: a requested section that never appears,
requested sections that appear out of order, and sections beyond what was asked for. The
`LintFinding` variants already exist; this task is the rules that produce them.

## Spec

### The severities are set by measurement, and one of them is counter-intuitive

Counted over the 13 saved generations (LLM-SURFACE 12.5):

| what | result |
|---|---|
| requested order correct, as a subsequence | **13 of 13** |
| an extra song section beyond the six requested | **9 of 13** |
| what that extra section always was | **`[Outro]`**, every time |

So **a missing or reordered section is a warning** -- no real generation produced one, and
if the user sees one something genuinely went wrong. And **an extra section is `Info`, not
a warning**: two thirds of real generations add an outro the user very likely wants, and a
lint that warns on the common case is a lint people learn to ignore.

### The three rules

- **`MissingSection`** -- a requested section with no matching tag anywhere. Reported
  **once** per section name however many times the brief asked for it: `V-C-V-C-B-C` with
  no chorus at all is one problem, not three.
- **`OutOfOrder`** -- the requested sequence is not a subsequence of the sections found.
  Checked **only when nothing is missing**, because a missing section already breaks the
  order and two findings for one cause is noise.
- **`ExtraSection`** -- surplus occurrences, counted per section name, so three requested
  choruses do not make the second and third look unexpected.

### The instrumental marker is not a section

`[inst]` is a gap in the singing, not a part of the song. It is never missing and never
extra. It appeared in 7 of the 13 generations without ever being asked for, so counting it
as a section would put an Info finding on more than half of all output for no reason.

## Reference implementation

Compiled, `cargo fmt` clean, clippy clean, guards mutation-tested, and swept over all 13
saved generations: 0 missing, 0 out-of-order, and an extra section in exactly the 9 files
that added an outro -- matching an independent count of the same corpus.

Insert into `lint_lyrics`, immediately before the closing `findings` return that T-203a
left:

```rust
    // Sections are the tags that name a part of the song: not the instrumental
    // marker, and not the directions already reported above.
    let sections: Vec<&FoundTag> = found
        .iter()
        .filter(|tag| {
            !is_instrumental(&tag.name)
                && (declared.contains(&tag.name) || requested.contains(&tag.name))
        })
        .collect();

    let mut missing = Vec::new();
    for want in &requested {
        if !missing.contains(want) && !sections.iter().any(|tag| &tag.name == want) {
            missing.push(want.clone());
        }
    }
    for section in &missing {
        findings.push(LintFinding::MissingSection {
            section: section.clone(),
        });
    }

    // Order is only meaningful once everything asked for is present.
    if missing.is_empty() {
        let mut remaining = sections.iter();
        let in_order = requested
            .iter()
            .all(|want| remaining.any(|tag| &tag.name == want));
        if !in_order {
            findings.push(LintFinding::OutOfOrder {
                requested: requested.clone(),
            });
        }
    }

    // Surplus occurrences, counted per section name so three requested choruses
    // do not make the second and third look unexpected.
    let mut seen: Vec<(String, usize)> = Vec::new();
    for tag in &sections {
        let wanted = requested.iter().filter(|want| *want == &tag.name).count();
        let count = match seen.iter_mut().find(|(name, _)| name == &tag.name) {
            Some((_, count)) => {
                *count += 1;
                *count
            }
            None => {
                seen.push((tag.name.clone(), 1));
                1
            }
        };
        if count > wanted {
            findings.push(LintFinding::ExtraSection {
                tag: tag.text.clone(),
                line: tag.line,
            });
        }
    }
```

### Tests

Add alongside T-203a's twelve.

```rust
    /// Invariant: the extra `[Outro]` a real generation adds is Info, not a
    /// warning. Nine of the thirteen saved generations added one, and failing a
    /// lyric for an outro the user probably wants would make the lint noise.
    #[test]
    fn test_an_added_outro_is_info_not_a_warning() {
        let findings = lint_lyrics(
            &ace(),
            &LyricBrief::default(),
            &fixture("generated-with-directions.txt"),
        );
        let extras: Vec<&LintFinding> = findings
            .iter()
            .filter(|f| matches!(f, LintFinding::ExtraSection { .. }))
            .collect();
        assert_eq!(extras.len(), 1, "{findings:#?}");
        assert!(
            matches!(extras[0], LintFinding::ExtraSection { tag, .. } if tag == "[Outro]"),
            "{:#?}",
            extras[0]
        );
        assert_eq!(extras[0].severity(), LintSeverity::Info);
        assert!(
            !warnings(&findings).iter().any(|f| matches!(
                f,
                LintFinding::MissingSection { .. } | LintFinding::OutOfOrder { .. }
            )),
            "the requested structure was followed: {findings:#?}"
        );
    }

    /// Invariant: a section the brief asked for and never got is reported by
    /// name, and reported once however many times it was requested.
    #[test]
    fn test_missing_section_is_reported_once() {
        let text =
            "[Verse]\nfirst\n[Chorus]\nhook\n[Verse]\nsecond\n[Chorus]\nhook\n[Chorus]\nhook\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert_eq!(
            findings,
            vec![LintFinding::MissingSection {
                section: "bridge".to_string()
            }]
        );
    }

    /// Invariant: everything present but in the wrong order is its own finding,
    /// not silence. Reported only when nothing is missing, because a missing
    /// section already breaks the order and two findings for one cause is noise.
    #[test]
    fn test_out_of_order_is_reported_when_nothing_is_missing() {
        let text = "[Chorus]\nhook\n[Verse]\nfirst\n[Chorus]\nhook\n[Verse]\nsecond\n[Bridge]\nturn\n[Chorus]\nhook\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), text);
        assert!(
            findings
                .iter()
                .any(|f| matches!(f, LintFinding::OutOfOrder { .. })),
            "{findings:#?}"
        );

        let missing_too = "[Chorus]\nhook\n[Verse]\nfirst\n";
        let findings = lint_lyrics(&ace(), &LyricBrief::default(), missing_too);
        assert!(
            !findings
                .iter()
                .any(|f| matches!(f, LintFinding::OutOfOrder { .. })),
            "a missing section must not also report as disorder: {findings:#?}"
        );
    }
```

`test_an_added_outro_is_info_not_a_warning` runs against the real generation fixture and
asserts the *severity*, not just the presence, of the finding. That is the assertion that
would fail if someone later decided an extra section deserved a warning.

## Acceptance criteria

- [ ] `cargo test -p create-core` passes; `npm run gate` green.
- [ ] The three named tests exist and pass, alongside T-203a's twelve.
- [ ] These mutations each make a named test fail (verified before the brief was written):
      - an extra section becomes a `Warning` -> `test_an_added_outro_is_info_not_a_warning`
      - extra sections stop being counted -> `test_an_added_outro_is_info_not_a_warning`
      - the order check never runs ->
        `test_out_of_order_is_reported_when_nothing_is_missing`
- [ ] No changes outside the one listed file. No new dependencies.

## Out of scope

- Any new `LintFinding` variant. The enum was completed in T-203a.
- Judging whether a section is any good, or how long it is. One generation put an empty
  `[Bridge]` under a direction block with no words at all; that is one occurrence in
  thirteen and does not yet justify a rule.
- Surfacing findings in the UI -- T-209.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --read crates/create-core/src/lyrics.rs --file crates/create-core/src/lyrics/lint.rs
```
