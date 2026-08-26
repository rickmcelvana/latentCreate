//! The lyric brief, and the prompt assembled from it.
//!
//! Pure: no I/O, no async, no clock. The shell turns the two strings this module
//! returns into an `llm_bridge::ChatRequest`; nothing here knows what a provider
//! is.
//!
//! **The assembled prompt is the one that was captured live**, not a new one
//! (docs/LLM-SURFACE.md section 12). Its shape produced a complete, correctly
//! structured song from the model this app recommends for lyrics, and the
//! parts that vary by model are read from the profile rather than written here
//! -- the two shipped profiles disagree about the capitalisation of their own
//! structure tags, so a hardcoded list would tell a MiniMax user to write
//! `[Verse]` at a model that expects `[verse]`.

use crate::profile::{InputSpec, ModelProfile};
use serde::{Deserialize, Serialize};

pub mod lint;

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
}
