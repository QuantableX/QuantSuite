//! Auto in the model menu (round 4, card 5). At each message plain rules pick
//! a tier, and the settings say which model a tier means for the agent. Read
//! the lists below and change them: nothing here is learned, and nothing here
//! looks at accounts or limits, so Auto can never be a way around either.

use serde::{Deserialize, Serialize};

use crate::driver::ModelInfo;

/// Which model each tier means for one agent. `""` = the built-in choice.
/// A word is enough: the newest model whose id contains it is used.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoTiers {
    pub light: String,
    pub standard: String,
    pub deep: String,
}

impl AutoTiers {
    fn get(&self, tier: Tier) -> &str {
        match tier {
            Tier::Light => &self.light,
            Tier::Standard => &self.standard,
            Tier::Deep => &self.deep,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.light.trim().is_empty() && self.standard.trim().is_empty() && self.deep.trim().is_empty()
    }

    pub fn trimmed(self) -> Self {
        Self { light: self.light.trim().to_string(), standard: self.standard.trim().to_string(), deep: self.deep.trim().to_string() }
    }
}

/// The agents Auto knows without being told. Any other chat agent gets Auto
/// once its tiers are filled in under Settings → Providers.
pub fn built_in(provider: &str) -> Option<AutoTiers> {
    let (light, standard, deep) = match provider {
        "claude" => ("haiku", "sonnet", "opus"),
        // By Codex's own words: Luna "for easier tasks", Sol "everyday work",
        // Astra "the most demanding work".
        "codex" => ("luna", "sol", "astra"),
        _ => return None,
    };
    Some(AutoTiers { light: light.into(), standard: standard.into(), deep: deep.into() })
}

/// The agent's tiers: the user's, the built-in ones for what they left empty.
/// `None` = Auto is not offered for this agent.
pub fn tiers_for(provider: &str, mine: Option<&AutoTiers>) -> Option<AutoTiers> {
    let base = built_in(provider).unwrap_or_default();
    let mine = mine.cloned().unwrap_or_default().trimmed();
    let pick = |own: String, fallback: String| if own.is_empty() { fallback } else { own };
    let tiers = AutoTiers { light: pick(mine.light, base.light), standard: pick(mine.standard, base.standard), deep: pick(mine.deep, base.deep) };
    (!tiers.is_empty()).then_some(tiers)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    Light,
    Standard,
    Deep,
}

impl Tier {
    fn label(self) -> &'static str {
        match self {
            Tier::Light => "Light",
            Tier::Standard => "Standard",
            Tier::Deep => "Deep",
        }
    }
}

// ── The rules ──

/// Words that say the work is big, whatever else the message says.
const BIG_WORK: &[&str] = &[
    "redesign",
    "re-architect",
    "rearchitect",
    "architecture",
    "refactor",
    "rewrite",
    "overhaul",
    "migrate",
    "migration",
    "from scratch",
    "root cause",
    "race condition",
    "deadlock",
    "memory leak",
    "investigate",
    "across the codebase",
    "whole codebase",
    "entire codebase",
    "whole project",
    "entire project",
    "system design",
    "implementation plan",
];

/// Topics where a mistake costs a lot. A plain typo fix in them stays small.
const SENSITIVE: &[&str] = &[
    "auth",
    "authentication",
    "authorization",
    "oauth",
    "login",
    "password",
    "security",
    "vulnerability",
    "encryption",
    "payment",
    "payments",
    "concurrency",
];

/// Edits a small model does well.
const SMALL_EDITS: &[&str] = &[
    "rename",
    "typo",
    "typos",
    "spelling",
    "reword",
    "rephrase",
    "reformat",
    "format",
    "lint",
    "bump",
    "add a comment",
    "remove the comment",
    "one line",
    "one liner",
    "change the text",
    "change the label",
    "change the color",
    "sort the imports",
    "fix the import",
];

/// Questions a small model answers well.
const QUICK_QUESTIONS: &[&str] = &[
    "what does",
    "what is",
    "what s",
    "where is",
    "where s",
    "which file",
    "how do i",
    "show me",
    "list the",
    "summarize",
    "explain this",
];

/// A whole message that only says "carry on": the model stays.
const FOLLOW_UPS: &[&str] = &[
    "yes",
    "y",
    "yep",
    "yeah",
    "yes please",
    "no",
    "nope",
    "ok",
    "okay",
    "sure",
    "go",
    "go ahead",
    "go on",
    "do it",
    "please do",
    "continue",
    "proceed",
    "keep going",
    "next",
    "retry",
    "try again",
    "again",
    "sounds good",
    "looks good",
    "lgtm",
    "thanks",
    "thank you",
];

/// A message this long is a brief, not a small ask.
const LONG_BRIEF_WORDS: usize = 200;
const LONG_BRIEF_CHARS: usize = 1500;
/// A small edit or question is at most this long.
const SMALL_WORDS: usize = 30;
/// This many attachments take the large model.
const MANY_ATTACHMENTS: usize = 4;
/// A thread this big has too much to hold for the small model.
const LONG_THREAD_TOKENS: i64 = 100_000;
const LONG_THREAD_MESSAGES: usize = 40;

/// What the rules look at.
#[derive(Debug, Clone, Copy, Default)]
pub struct Facts<'a> {
    /// What the user typed, without the context that goes along.
    pub text: &'a str,
    pub attachments: usize,
    pub mode: &'a str,
    /// The thread's messages before this one.
    pub user_messages: usize,
    /// The last known size of the conversation.
    pub context_tokens: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    /// Not a message to choose for (`/compact`): no change and no notice.
    Skip,
    /// Stay on the model the last turn used.
    Keep(String),
    Pick(Tier, String),
}

/// The first rule that fits decides.
pub fn decide(f: &Facts<'_>) -> Decision {
    let typed = f.text.trim();
    if typed.starts_with('/') {
        return Decision::Skip;
    }
    if f.mode == "plan" {
        return Decision::Pick(Tier::Deep, "planning".into());
    }
    let text = normalized(typed);
    if f.user_messages > 0 && f.attachments == 0 && FOLLOW_UPS.iter().any(|w| text == normalized(w)) {
        return Decision::Keep("a follow-up, same model".into());
    }
    if let Some(word) = found(&text, BIG_WORK) {
        return Decision::Pick(Tier::Deep, format!("a big change (“{word}”)"));
    }
    if typed.split_whitespace().count() >= LONG_BRIEF_WORDS || typed.chars().count() >= LONG_BRIEF_CHARS {
        return Decision::Pick(Tier::Deep, "a long brief".into());
    }
    if f.attachments >= MANY_ATTACHMENTS {
        return Decision::Pick(Tier::Deep, "many attachments".into());
    }
    let long_thread = f.context_tokens >= LONG_THREAD_TOKENS || f.user_messages >= LONG_THREAD_MESSAGES;
    let small = typed.split_whitespace().count() <= SMALL_WORDS && f.attachments == 0 && !long_thread;
    if small {
        if let Some(word) = found(&text, SMALL_EDITS) {
            return Decision::Pick(Tier::Light, format!("a small change (“{word}”)"));
        }
    }
    if let Some(word) = found(&text, SENSITIVE) {
        return Decision::Pick(Tier::Deep, format!("a sensitive area (“{word}”)"));
    }
    if small && found(&text, QUICK_QUESTIONS).is_some() {
        return Decision::Pick(Tier::Light, "a quick question".into());
    }
    if long_thread {
        return Decision::Pick(Tier::Standard, "a long conversation".into());
    }
    if f.attachments > 0 {
        return Decision::Pick(Tier::Standard, "has attachments".into());
    }
    Decision::Pick(Tier::Standard, "a medium change".into())
}

/// Lowercase words between single spaces, with one space on either side, so
/// that `contains` only ever finds whole words: "auth" is not in "author".
fn normalized(text: &str) -> String {
    let mut out = String::with_capacity(text.len() + 2);
    out.push(' ');
    let mut gap = true;
    for c in text.chars() {
        if c.is_alphanumeric() {
            out.extend(c.to_lowercase());
            gap = false;
        } else if !gap {
            out.push(' ');
            gap = true;
        }
    }
    if !gap {
        out.push(' ');
    }
    out
}

fn found<'a>(text: &str, list: &[&'a str]) -> Option<&'a str> {
    list.iter().copied().find(|phrase| text.contains(&normalized(phrase)))
}

// ── From a tier to a model ──

/// What the message runs on, and what the thread says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    /// What the thread stores: `""` = the agent's default.
    pub model: String,
    pub notice: String,
}

/// The catalog's id for what a tier asks for: the id itself, the model it
/// resolves to, else the first (newest) id containing the word. `None` when
/// the agent does not offer it now, or its list could not be read.
fn resolve(wanted: &str, models: &[ModelInfo]) -> Option<String> {
    let wanted = wanted.trim().to_lowercase();
    if wanted.is_empty() {
        return None;
    }
    if wanted == "default" {
        return Some(String::new());
    }
    // `default` is the CLI's own pick: never a model a tier means.
    let listed = || models.iter().filter(|m| m.id != "default");
    listed()
        .find(|m| m.id.to_lowercase() == wanted)
        .or_else(|| listed().find(|m| m.resolved_model.as_deref().is_some_and(|r| r.to_lowercase() == wanted)))
        .or_else(|| listed().find(|m| m.id.to_lowercase().contains(&wanted)))
        .map(|m| m.id.clone())
}

fn label_of(model: &str, models: &[ModelInfo]) -> String {
    let id = model.trim();
    if id.is_empty() || id == "default" {
        return "Default".into();
    }
    models.iter().find(|m| m.id == id).map(|m| m.display_name.trim()).filter(|n| !n.is_empty()).unwrap_or(id).to_string()
}

/// The model and the notice for a decision. `current` is what the thread is on.
/// A tier whose model is not available falls back to the agent's default, and
/// the notice says so; it never tries another account or another tier.
pub fn choose(decision: &Decision, tiers: &AutoTiers, models: &[ModelInfo], current: &str) -> Option<Choice> {
    match decision {
        Decision::Skip => None,
        Decision::Keep(why) => Some(Choice { model: current.trim().to_string(), notice: format!("Auto: {} — {why}", label_of(current, models)) }),
        Decision::Pick(tier, why) => {
            let wanted = tiers.get(*tier).trim();
            Some(match resolve(wanted, models) {
                Some(model) => Choice { notice: format!("Auto: {} — {why}", label_of(&model, models)), model },
                None if wanted.is_empty() => Choice { model: String::new(), notice: format!("Auto: Default — {why}. No model is set for {}", tier.label()) },
                None => Choice { model: String::new(), notice: format!("Auto: Default — {why}. {wanted} is not available right now") },
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pick(text: &str) -> Decision {
        decide(&Facts { text, mode: "full", ..Default::default() })
    }
    fn tier(d: &Decision) -> Option<Tier> {
        match d {
            Decision::Pick(t, _) => Some(*t),
            _ => None,
        }
    }
    fn model(id: &str, name: &str) -> ModelInfo {
        ModelInfo { id: id.into(), display_name: name.into(), ..Default::default() }
    }
    /// What Claude Code really reported on 2026-11-05 (the top of its list).
    fn claude() -> Vec<ModelInfo> {
        vec![
            ModelInfo { resolved_model: Some("claude-opus-5-5".into()), ..model("default", "Default (recommended)") },
            ModelInfo { resolved_model: Some("claude-opus-5-5".into()), ..model("opus", "Opus 5.5") },
            ModelInfo { resolved_model: Some("claude-sonnet-5-5".into()), ..model("sonnet", "Sonnet 5.5") },
            ModelInfo { resolved_model: Some("claude-haiku-5-5".into()), ..model("haiku", "Haiku 5.5") },
            model("claude-haiku-4-5-20251001", "Haiku 4.5"),
        ]
    }
    /// What Codex really reported on 2026-11-05, newest first.
    fn codex() -> Vec<ModelInfo> {
        ["gpt-6.1-sol", "gpt-6-astra", "gpt-6.1-terra", "gpt-6-sol", "gpt-6-luna", "gpt-5.6-sol", "gpt-5.6-luna"].iter().map(|id| model(id, &id.to_uppercase())).collect()
    }

    #[test]
    fn real_prompts_land_on_the_right_tier() {
        let table = [
            ("rename this variable", Tier::Light),
            ("Fix the typo in the README", Tier::Light),
            ("fix the typo on the login page", Tier::Light),
            ("what does this function do?", Tier::Light),
            ("What's the difference between these two files?", Tier::Light),
            ("where is the retry limit set", Tier::Light),
            ("bump the version to 1.2.0", Tier::Light),
            ("add a dark mode toggle to the settings page", Tier::Standard),
            ("the export button does nothing when I click it, please fix it", Tier::Standard),
            ("write tests for the parser", Tier::Standard),
            ("redesign the auth flow", Tier::Deep),
            ("refactor the storage layer to use sqlite", Tier::Deep),
            ("why does the app freeze on startup? find the root cause", Tier::Deep),
            ("rename the session type across the codebase", Tier::Deep),
            ("the login token expires too early", Tier::Deep),
            ("is this password check safe", Tier::Deep),
        ];
        for (text, want) in table {
            assert_eq!(tier(&pick(text)), Some(want), "{text}");
        }
    }

    #[test]
    fn only_whole_words_count() {
        // "author" is not "auth", "formation" is not "format", "whatever" is not "what".
        assert_eq!(tier(&pick("add the author to the formation list")), Some(Tier::Standard));
        assert_eq!(tier(&pick("rewrite")), Some(Tier::Deep));
        assert_eq!(tier(&pick("rewriter settings need a new default")), Some(Tier::Standard));
    }

    #[test]
    fn plan_mode_is_always_deep() {
        let d = decide(&Facts { text: "rename this variable", mode: "plan", ..Default::default() });
        assert_eq!(d, Decision::Pick(Tier::Deep, "planning".into()));
    }

    #[test]
    fn a_long_message_is_a_brief() {
        let long = "please change the thing ".repeat(50);
        assert_eq!(pick(&long), Decision::Pick(Tier::Deep, "a long brief".into()));
        let paste = "x".repeat(LONG_BRIEF_CHARS);
        assert_eq!(pick(&paste), Decision::Pick(Tier::Deep, "a long brief".into()));
        // A small word in a long message does not make it small.
        let rename = format!("rename {}", "and also change this other thing too ".repeat(6));
        assert_eq!(tier(&pick(&rename)), Some(Tier::Standard));
    }

    #[test]
    fn attachments_move_it_up() {
        let with = |attachments| decide(&Facts { text: "what is this", attachments, mode: "full", ..Default::default() });
        assert_eq!(tier(&with(0)), Some(Tier::Light));
        assert_eq!(with(1), Decision::Pick(Tier::Standard, "has attachments".into()));
        assert_eq!(with(MANY_ATTACHMENTS), Decision::Pick(Tier::Deep, "many attachments".into()));
    }

    #[test]
    fn a_big_thread_never_goes_to_the_small_model() {
        let by_tokens = Facts { text: "rename this variable", mode: "full", context_tokens: LONG_THREAD_TOKENS, user_messages: 3, ..Default::default() };
        assert_eq!(decide(&by_tokens), Decision::Pick(Tier::Standard, "a long conversation".into()));
        let by_messages = Facts { text: "what is this", mode: "full", user_messages: LONG_THREAD_MESSAGES, ..Default::default() };
        assert_eq!(tier(&decide(&by_messages)), Some(Tier::Standard));
        let small = Facts { text: "rename this variable", mode: "full", context_tokens: LONG_THREAD_TOKENS - 1, user_messages: 3, ..Default::default() };
        assert_eq!(tier(&decide(&small)), Some(Tier::Light));
    }

    #[test]
    fn a_follow_up_keeps_the_model_of_the_last_turn() {
        for text in ["yes", "Go ahead.", "ok!", "continue", "LGTM"] {
            let f = Facts { text, mode: "full", user_messages: 2, ..Default::default() };
            assert!(matches!(decide(&f), Decision::Keep(_)), "{text}");
        }
        // Nothing to follow yet, or more than a follow-up.
        assert_eq!(tier(&pick("yes")), Some(Tier::Standard));
        let more = Facts { text: "yes and also add a toggle", mode: "full", user_messages: 2, ..Default::default() };
        assert_eq!(tier(&decide(&more)), Some(Tier::Standard));
        let image = Facts { text: "yes", attachments: 1, mode: "full", user_messages: 2, ..Default::default() };
        assert_eq!(tier(&decide(&image)), Some(Tier::Standard));
        let kept = choose(&decide(&Facts { text: "yes", mode: "full", user_messages: 1, ..Default::default() }), &built_in("claude").unwrap(), &claude(), "opus").unwrap();
        assert_eq!(kept, Choice { model: "opus".into(), notice: "Auto: Opus 5.5 — a follow-up, same model".into() });
    }

    #[test]
    fn a_command_is_left_alone() {
        assert_eq!(pick("/compact"), Decision::Skip);
        assert_eq!(choose(&Decision::Skip, &built_in("claude").unwrap(), &claude(), "opus"), None);
    }

    #[test]
    fn each_agent_has_its_own_models() {
        let cases = [
            ("claude", claude(), "rename this variable", "haiku", "Auto: Haiku 5.5 — a small change (“rename”)"),
            ("claude", claude(), "add a toggle to the page", "sonnet", "Auto: Sonnet 5.5 — a medium change"),
            ("claude", claude(), "redesign the auth flow", "opus", "Auto: Opus 5.5 — a big change (“redesign”)"),
            ("codex", codex(), "rename this variable", "gpt-6-luna", "Auto: GPT-6-LUNA — a small change (“rename”)"),
            ("codex", codex(), "add a toggle to the page", "gpt-6.1-sol", "Auto: GPT-6.1-SOL — a medium change"),
            ("codex", codex(), "redesign the auth flow", "gpt-6-astra", "Auto: GPT-6-ASTRA — a big change (“redesign”)"),
        ];
        for (agent, models, text, want, notice) in cases {
            let c = choose(&pick(text), &tiers_for(agent, None).unwrap(), &models, "").unwrap();
            assert_eq!((c.model.as_str(), c.notice.as_str()), (want, notice), "{agent}: {text}");
        }
    }

    #[test]
    fn your_settings_win_over_the_built_in_tiers() {
        let mine = AutoTiers { deep: " fable ".into(), ..Default::default() };
        let tiers = tiers_for("claude", Some(&mine)).unwrap();
        assert_eq!(tiers, AutoTiers { light: "haiku".into(), standard: "sonnet".into(), deep: "fable".into() });
        assert_eq!(resolve("claude-opus-5-5", &claude()).as_deref(), Some("opus"), "by the model it resolves to, not `default`");
        assert_eq!(resolve("HAIKU", &claude()).as_deref(), Some("haiku"));
        assert_eq!(resolve("default", &claude()).as_deref(), Some(""));
    }

    #[test]
    fn an_agent_without_tiers_is_not_offered_auto() {
        assert_eq!(tiers_for("gemini", None), None);
        assert_eq!(tiers_for("gemini", Some(&AutoTiers::default())), None);
        let mine = AutoTiers { standard: "pro".into(), ..Default::default() };
        assert_eq!(tiers_for("gemini", Some(&mine)).unwrap().standard, "pro");
    }

    #[test]
    fn an_unavailable_model_falls_back_and_says_so() {
        let gone: Vec<ModelInfo> = claude().into_iter().filter(|m| !m.id.contains("haiku")).collect();
        let c = choose(&pick("rename this variable"), &built_in("claude").unwrap(), &gone, "sonnet").unwrap();
        assert_eq!(c, Choice { model: String::new(), notice: "Auto: Default — a small change (“rename”). haiku is not available right now".into() });
        // The model list could not be read: nothing can be promised.
        let c = choose(&pick("redesign it"), &built_in("codex").unwrap(), &[], "gpt-6-sol").unwrap();
        assert_eq!(c.model, "");
        assert!(c.notice.contains("astra is not available"), "{}", c.notice);
        // A tier left empty on an agent with others set.
        let partial = AutoTiers { standard: "pro".into(), ..Default::default() };
        let c = choose(&pick("rename this variable"), &partial, &[model("pro", "Pro")], "pro").unwrap();
        assert_eq!(c, Choice { model: String::new(), notice: "Auto: Default — a small change (“rename”). No model is set for Light".into() });
    }
}
