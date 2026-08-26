# T-202b: assembling the system prompt and the user message
**Depends:** T-202a | **Crate/dir:** crates/create-core
**Files to create/modify:**
- `crates/create-core/src/lyrics.rs` (modify -- finishes the file T-202a started)

## Goal

Turn a profile and a brief into the two strings the shell sends: a system prompt built
from what the profile says about its own lyric format, and a user message that is the
brief, one labelled line at a time. Pure; nothing here knows what a provider is, and the
shell builds the `ChatRequest`.

## Spec

### The prompt is the captured one, not a new one

The shape below was run live against `gemma4:12b-32k` before this brief was written and
produced complete, correctly structured songs (docs/LLM-SURFACE.md section 12). Do not
improve it. Two specific findings constrain it:

**1. Everything model-specific is read from the profile.** The two shipped profiles
disagree about the capitalisation of their own structure tags -- `ace-step-1.5-turbo`
declares `[Verse]`, `minimax-music-3` declares `[verse]` -- and nothing in ComfyUI
publishes which a model really accepts (MCP-SURFACE 15). A hardcoded tag list would tell
a MiniMax user to write a tag their model does not use.

**2. There is no rule forbidding production or vocal-style directions in the lyrics.**
The model writes them, the profile's contract asks it not to, and adding a hard rule
against them was measured over **14 live generations**: the runs carrying the rule
averaged *more* stray direction blocks than the runs without it (LLM-SURFACE 12.5).
Naming the forbidden thing does not suppress it. The profile's own `lyrics_contract` note
stays, because those are the profile author's words about their model rather than an
instruction this app invented. `test_no_rule_against_production_directions` exists to stop
the rule being re-added on intuition; it is not a formality.

### Optional blocks are genuinely optional

`lyrics_contract` and `prompt_guide` are `Option` in the schema. A profile carrying
neither must still assemble a usable prompt -- role line plus hard rules. A model without
a contract block is not a model without lyrics.

### The user message

Labelled lines, not prose: it is the shape that was captured working, and it is what the
optimizer's diff view will show back to the user (ARCHITECTURE 6) -- a paragraph would
diff badly. The structure line carries both the raw string and its expansion, so the model
is not left to interpret the letters; its own reasoning trace showed it doing that
expansion by hand when it was not given one.

An optional field the user left blank contributes no line at all, rather than a label with
nothing after it.

## Reference implementation

Compiled, `cargo fmt` clean, clippy clean, guards mutation-tested. Transcribe it.

### Imports

T-202a left `crates/create-core/src/lyrics.rs` importing only serde. Add:

```rust
use crate::profile::{InputSpec, ModelProfile};
```

### Two private readers, then the two public functions

Append after `expand_structure`:

```rust
/// The structure tags a profile's lyrics input declares.
///
/// Read from the profile rather than assumed: `ace-step-1.5-turbo` declares
/// `[Verse]` and `minimax-music-3` declares `[verse]`, and nothing in ComfyUI
/// publishes which a model really accepts (MCP-SURFACE 15).
fn structure_tags(profile: &ModelProfile) -> &[String] {
    profile
        .inputs
        .values()
        .find_map(|spec| match spec {
            InputSpec::Lyrics { structure_tags, .. } => Some(structure_tags.as_slice()),
            _ => None,
        })
        .unwrap_or(&[])
}

/// The first worked lyric example the profile's prompt guide carries, if any.
fn lyric_example(profile: &ModelProfile) -> Option<&str> {
    profile
        .prompt_guide
        .as_ref()?
        .examples
        .iter()
        .find_map(|example| example.lyrics.as_deref())
}

/// Builds the system prompt for one profile and brief.
///
/// Role line, then what the profile says about its own lyric format, then a
/// worked example when it has one, then the hard rules.
///
/// **There is deliberately no rule forbidding production or vocal-style
/// directions in the lyrics**, even though the model writes them and the profile
/// asks it not to. Adding one was measured over 14 live generations and did not
/// help -- the runs carrying that rule averaged more stray direction blocks than
/// the runs without it (LLM-SURFACE 12.5). Naming the behaviour in the prompt
/// does not suppress it, so it is caught after generation by the lint instead.
/// Do not re-add the rule without new measurements. A profile with no
/// `lyrics_contract` and no `prompt_guide` still yields a usable prompt -- those
/// blocks are optional in the schema, and a model without them is not a model
/// without lyrics.
pub fn assemble_system_prompt(profile: &ModelProfile, brief: &LyricBrief) -> String {
    let mut out = format!(
        "You are a professional songwriter writing for {}.\n",
        profile.display_name
    );

    if let Some(contract) = &profile.lyrics_contract {
        out.push_str(&format!("Lyrics format: {}.\n", contract.format));
        let tags = structure_tags(profile);
        if !tags.is_empty() {
            out.push_str(&format!(
                "Use these structure tags, exactly as written: {}.\n",
                tags.join(", ")
            ));
        }
        if let Some(token) = &contract.instrumental_token {
            out.push_str(&format!(
                "Use {token} for instrumental sections, with no words under it.\n"
            ));
        }
        if let Some(notes) = &contract.notes {
            out.push_str(notes);
            out.push('\n');
        }
    }

    if let Some(example) = lyric_example(profile) {
        out.push_str("\nAn example of the expected shape:\n");
        out.push_str(example);
        out.push('\n');
    }

    out.push_str("\nHard rules:\n");
    out.push_str("- Output ONLY the lyrics. No title, no commentary, no explanation, no markdown code fences.\n");
    out.push_str("- Follow the requested structure exactly, in the order given.\n");
    out.push_str(&format!("- Write the lyrics in {}.\n", brief.language));
    if !brief.explicit_allowed {
        out.push_str("- No explicit language.\n");
    }
    out
}

/// Builds the user message: the brief, one labelled line at a time.
///
/// Labelled lines rather than prose because that is the shape that was captured
/// working, and because it is what the optimizer's diff view will show back to
/// the user (ARCHITECTURE 6) -- a paragraph would diff badly.
pub fn assemble_user_message(brief: &LyricBrief) -> String {
    let sections = expand_structure(&brief.structure);
    let mut out = String::new();
    out.push_str(&format!("Theme: {}\n", brief.theme));
    out.push_str(&format!("Genre and style tags: {}\n", brief.style_tags));
    out.push_str(&format!("Mood: {}\n", brief.mood));
    out.push_str(&format!(
        "Structure: {} ({})\n",
        brief.structure,
        sections.join(", ")
    ));
    out.push_str(&format!("Language: {}\n", brief.language));
    out.push_str(&format!(
        "Point of view: {}\n",
        brief.point_of_view.as_prompt_text()
    ));
    if let Some(refs) = &brief.era_refs {
        if !refs.trim().is_empty() {
            out.push_str(&format!("Era and references: {refs}\n"));
        }
    }
    out.push_str(&format!(
        "Explicit content allowed: {}\n",
        if brief.explicit_allowed { "yes" } else { "no" }
    ));
    out.push_str(&format!(
        "Target duration: {} seconds\n",
        brief.target_duration_s
    ));
    out
}
```

### Tests

Replace T-202a's `mod tests { use super::*; ... }` header with the one below -- it gains
the profile fixtures -- and add these tests alongside the five already there.

**The fixtures are the two shipped profiles, read from `profiles/`.** Hand-written
fixtures would be written to agree with the code; these are the files the app ships, and
they are what proves the tag capitalisation really differs.

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

    fn minimax() -> ModelProfile {
        profile("minimax-music-3.json")
    }

    /// The line that tells the model which tags to write.
    ///
    /// Pulled out on purpose: both shipped profiles carry a worked example that
    /// already contains their tags, so asserting over the whole prompt passes
    /// even when the tag line is gone entirely. Verified by mutation -- the
    /// first version of the test below did exactly that.
    fn tags_line(prompt: &str) -> &str {
        prompt
            .lines()
            .find(|line| line.starts_with("Use these structure tags"))
            .expect("the prompt must declare the profile's structure tags")
    }

    /// Invariant: the tags in the prompt are the ones the profile declares, in
    /// the case it declares them. The two shipped profiles disagree, and telling
    /// a MiniMax user to write `[Verse]` would be telling them to write a tag
    /// their model does not use.
    #[test]
    fn test_structure_tags_come_from_the_profile_not_a_constant() {
        let brief = LyricBrief::default();
        let ace_prompt = assemble_system_prompt(&ace(), &brief);
        let minimax_prompt = assemble_system_prompt(&minimax(), &brief);

        let ace_tags = tags_line(&ace_prompt);
        assert!(ace_tags.contains("[Verse]"), "{ace_tags}");
        assert!(ace_tags.contains("[inst]"), "{ace_tags}");
        assert!(!ace_tags.contains("[verse]"), "{ace_tags}");

        let minimax_tags = tags_line(&minimax_prompt);
        assert!(minimax_tags.contains("[verse]"), "{minimax_tags}");
        assert!(minimax_tags.contains("[intro]"), "{minimax_tags}");
        assert!(!minimax_tags.contains("[Verse]"), "{minimax_tags}");
    }

    /// Invariant: the role line names the model being written for, so a user who
    /// switches profiles gets a prompt aimed at the new one.
    #[test]
    fn test_role_line_names_the_profile() {
        let brief = LyricBrief::default();
        assert!(assemble_system_prompt(&ace(), &brief)
            .starts_with("You are a professional songwriter writing for ACE-Step 1.5 XL Turbo.\n"));
        assert!(assemble_system_prompt(&minimax(), &brief)
            .starts_with("You are a professional songwriter writing for MiniMax Music 3.\n"));
    }

    /// Invariant: the profile's own worked example reaches the prompt
    /// (ARCHITECTURE 6), and a profile without one contributes no empty example
    /// section. The example is what pins tag capitalisation for a model whose
    /// tags are lowercase.
    #[test]
    fn test_profile_lyric_example_reaches_the_prompt() {
        let minimax = minimax();
        let example = minimax.prompt_guide.as_ref().unwrap().examples[0]
            .lyrics
            .clone()
            .unwrap();
        let prompt = assemble_system_prompt(&minimax, &LyricBrief::default());
        assert!(
            prompt.contains("An example of the expected shape:"),
            "{prompt}"
        );
        assert!(prompt.contains(&example), "{prompt}");

        let mut no_guide = minimax.clone();
        no_guide.prompt_guide = None;
        let bare = assemble_system_prompt(&no_guide, &LyricBrief::default());
        assert!(!bare.contains("An example of the expected shape"), "{bare}");
    }

    /// Invariant: `lyrics_contract` and `prompt_guide` are optional in the
    /// schema, and a profile carrying neither still gets a prompt with a role
    /// line and the hard rules. Returning an empty or malformed prompt would
    /// make lyric writing depend on a field the schema does not require.
    #[test]
    fn test_a_profile_without_a_contract_still_assembles() {
        let mut bare = ace();
        bare.lyrics_contract = None;
        bare.prompt_guide = None;
        let prompt = assemble_system_prompt(&bare, &LyricBrief::default());

        assert!(prompt.starts_with("You are a professional songwriter"));
        assert!(prompt.contains("Hard rules:"));
        assert!(prompt.contains("Output ONLY the lyrics"));
        assert!(!prompt.contains("Lyrics format:"));
    }

    /// Invariant: the prompt carries no rule against production or vocal-style
    /// directions in the lyrics. It reads like an obvious thing to add, and it
    /// was added, measured over 14 live generations, and removed: the runs
    /// carrying it averaged more stray direction blocks than the runs without
    /// it (LLM-SURFACE 12.5). This test exists so the rule cannot come back on
    /// intuition alone -- the lint catches what the prompt cannot.
    #[test]
    fn test_no_rule_against_production_directions() {
        let prompt = assemble_system_prompt(&ace(), &LyricBrief::default());
        let rules = prompt
            .split_once("Hard rules:")
            .map(|(_, rules)| rules.to_string())
            .unwrap_or_default();
        for forbidden in ["production", "arrangement", "vocal-style"] {
            assert!(
                !rules.contains(forbidden),
                "hard rules mention {forbidden}, which was measured not to help: {rules}"
            );
        }
    }

    /// Invariant: the requested language reaches the prompt. The model writes in
    /// whatever it feels like otherwise -- one live capture dropped a Hangul
    /// character into an English lyric (LLM-SURFACE 12.4).
    #[test]
    fn test_requested_language_appears_in_both_messages() {
        let brief = LyricBrief {
            language: "Portuguese".to_string(),
            ..LyricBrief::default()
        };
        assert!(assemble_system_prompt(&ace(), &brief).contains("Write the lyrics in Portuguese."));
        assert!(assemble_user_message(&brief).contains("Language: Portuguese\n"));
    }

    /// Invariant: the explicit-content answer is stated either way. Omitting the
    /// line when explicit content is allowed would leave the model guessing from
    /// the genre.
    #[test]
    fn test_explicit_flag_is_stated_in_both_directions() {
        let clean = LyricBrief::default();
        let explicit = LyricBrief {
            explicit_allowed: true,
            ..LyricBrief::default()
        };

        assert!(assemble_user_message(&clean).contains("Explicit content allowed: no\n"));
        assert!(assemble_user_message(&explicit).contains("Explicit content allowed: yes\n"));
        assert!(assemble_system_prompt(&ace(), &clean).contains("- No explicit language.\n"));
        assert!(!assemble_system_prompt(&ace(), &explicit).contains("No explicit language"));
    }

    /// Invariant: an optional field the user left empty contributes no line at
    /// all, rather than a label with nothing after it.
    #[test]
    fn test_blank_era_refs_add_no_line() {
        let none = LyricBrief::default();
        let blank = LyricBrief {
            era_refs: Some("   ".to_string()),
            ..LyricBrief::default()
        };
        let filled = LyricBrief {
            era_refs: Some("early Chromatics".to_string()),
            ..LyricBrief::default()
        };

        assert!(!assemble_user_message(&none).contains("Era and references"));
        assert!(!assemble_user_message(&blank).contains("Era and references"));
        assert!(assemble_user_message(&filled).contains("Era and references: early Chromatics\n"));
    }

    /// Invariant: the expansion reaches the prompt beside the raw string, so the
    /// model is not left to interpret the letters. Its own reasoning trace shows
    /// it doing exactly that expansion by hand when it is not given one.
    #[test]
    fn test_user_message_spells_out_the_structure() {
        let message = assemble_user_message(&LyricBrief::default());
        assert!(
            message.contains(
                "Structure: V-C-V-C-B-C (Verse, Chorus, Verse, Chorus, Bridge, Chorus)\n"
            ),
            "{message}"
        );
    }
```

**Note the `tags_line` helper, and why it exists.** The first version of
`test_structure_tags_come_from_the_profile_not_a_constant` asserted over the whole prompt
and **survived a mutation that removed the tag line entirely** -- both profiles carry a
worked example that already contains their tags, so the assertions passed on the example
text. That is the third time in this project a fixture has hidden the rule it was written
to guard (T-110, T-112, T-201). Assert on the line, not the document.

## Acceptance criteria

- [ ] `cargo test -p create-core` passes; `npm run gate` green.
- [ ] The nine named tests exist and pass, alongside T-202a's five.
- [ ] These mutations each make a named test fail (verified before the brief was written):
      - `structure_tags` stops reading the profile ->
        `test_structure_tags_come_from_the_profile_not_a_constant`
      - the worked example is dropped from the prompt ->
        `test_profile_lyric_example_reaches_the_prompt`
      - the requested language is hardcoded to English ->
        `test_requested_language_appears_in_both_messages`
      - the explicit rule is always emitted ->
        `test_explicit_flag_is_stated_in_both_directions`
      - a blank `era_refs` still adds a line -> `test_blank_era_refs_add_no_line`
- [ ] No changes outside the one listed file. No new dependencies.

## Out of scope

- Any change to the prompt's wording. If it looks improvable, measure it against the
  endpoint first and put the numbers in LLM-SURFACE -- that is how the anti-direction rule
  came out again.
- Validating the lyrics that come back -- T-203.
- The Tauri command that sends these strings -- T-205.

## If unclear

Do not guess. Output a numbered list of questions and stop.

## Aider launch

```bash
aider --no-auto-commits --model ollama_chat/kimi-k2.7-code:cloud --read WORKFLOW.md --read CONVENTIONS.md --read ARCHITECTURE.md --read docs/LLM-SURFACE.md --read crates/create-core/src/profile.rs --file crates/create-core/src/lyrics.rs
```

`profile.rs` is `--read` because the new code matches on `InputSpec::Lyrics` and reaches
into `ModelProfile`, `LyricsContract` and `PromptGuide`; nothing in this task changes it.
