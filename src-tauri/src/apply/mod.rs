//! Preparing an application: the resume gap check and the application writer.
//!
//! Both keep to one rule: nothing is said about the user that their profile doesn't
//! back. Sentences carry the profile items that support them; anything else is flagged
//! for the user to confirm or remove.

pub mod export;
pub mod gap;
pub mod prep;
pub mod writer;

use std::{collections::HashSet, sync::LazyLock};

use regex::Regex;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    ai::analysis::{Evidence, ProfileItem},
    jobs::text::words,
};

/// How well a sentence is backed by the profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Backing {
    /// Cites profile items that support it.
    Backed,
    /// Makes no claim about the user (greeting, why they're writing, closing).
    Plain,
    /// Says something about the user that nothing in the profile supports.
    Unbacked,
    /// Was unbacked; the user said it's true.
    Confirmed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Sentence {
    pub text: String,
    pub backing: Backing,
    pub evidence: Vec<Evidence>,
    /// Why it's unbacked, in plain words.
    pub note: String,
}

static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d(?:[\d,.]*\d)?").unwrap());

/// Number words as digits. "One" is left out: "one of", "one task" are rarely claims.
const NUMBER_WORDS: &[(&str, &str)] = &[
    ("two", "2"),
    ("three", "3"),
    ("four", "4"),
    ("five", "5"),
    ("six", "6"),
    ("seven", "7"),
    ("eight", "8"),
    ("nine", "9"),
    ("ten", "10"),
    ("eleven", "11"),
    ("twelve", "12"),
    ("fifteen", "15"),
    ("twenty", "20"),
    ("thirty", "30"),
    ("fifty", "50"),
    ("hundred", "100"),
    ("dozen", "12"),
    ("dozens", "12"),
    ("hundreds", "100"),
];

/// Numbers in a text, without thousands separators ("1,200" → "1200"); number words
/// ("seven") count too.
fn numbers(text: &str) -> Vec<String> {
    let mut out: Vec<String> = NUMBER
        .find_iter(text)
        .map(|m| m.as_str().replace(',', ""))
        .collect();
    for w in words(text) {
        if let Some((_, d)) = NUMBER_WORDS.iter().find(|(word, _)| *word == w) {
            out.push((*d).to_string());
        }
    }
    out
}

/// Numbers in `text` that appear in none of `support` and aren't `allowed`.
pub fn new_numbers(text: &str, support: &[&str], allowed: &[u32]) -> Vec<String> {
    let known: HashSet<String> = support.iter().flat_map(|s| numbers(s)).collect();
    let mut out: Vec<String> = numbers(text)
        .into_iter()
        .filter(|n| !known.contains(n) && !allowed.iter().any(|a| a.to_string() == *n))
        .collect();
    out.dedup();
    out
}

static FIRST_PERSON: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:^|[^a-z])(?:i|i['’](?:ve|m|d|ll)|me|my)(?:[^a-z]|$)").unwrap()
});
static CLAIM_WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?-u:\b)(?:built|build|building|led|lead|leading|managed|manage|managing|worked|work|working|developed|develop|developing|created|designed|shipped|launched|delivered|improved|increased|reduced|grew|ran|wrote|owned|own|experience|experienced|expert|expertise|skilled|proficient|background|years?|speciali[sz]e[sd]?|knowledge|familiar|certified|degree|graduated|studied|mentored|helped|used|using|use|implemented|architected|maintained|migrated|scaled|optimi[sz]ed|automated|trained|taught|won|award|fluent|native)(?-u:\b)",
    )
    .unwrap()
});

/// True when an uncited sentence still says something about the user: a number, a
/// skill from their profile, or "I/my" with a word like "built" or "experience".
/// Errs towards flagging; the user can confirm a flagged sentence in one click.
pub fn looks_like_claim(text: &str, skills: &[String], ignore: &[String]) -> bool {
    // The job's own title and company ("Lead Engineer at Acme") aren't claims.
    let mut t = text.to_string();
    for phrase in ignore.iter().filter(|p| !p.trim().is_empty()) {
        t = t.replace(phrase.as_str(), " ");
    }
    if !numbers(&t).is_empty() {
        return true;
    }
    let lower = format!(" {} ", words(&t).join(" "));
    if skills.iter().any(|s| {
        let s = words(s).join(" ");
        !s.is_empty() && lower.contains(&format!(" {s} "))
    }) {
        return true;
    }
    FIRST_PERSON.is_match(&t) && CLAIM_WORD.is_match(&t)
}

/// What a sentence can be checked against.
pub struct Support {
    pub items: Vec<ProfileItem>,
    /// Profile skill names; an uncited sentence naming one is a claim.
    pub skills: Vec<String>,
    /// Job title and company: words a sentence may use without making a claim.
    pub job_words: Vec<String>,
    /// Numbers the app worked out itself, like years of experience.
    pub allowed_numbers: Vec<u32>,
}

impl Support {
    pub fn item(&self, id: &str) -> Option<&ProfileItem> {
        self.items.iter().find(|i| i.id == id)
    }

    pub fn evidence(&self, id: &str) -> Option<Evidence> {
        self.item(id).map(|i| Evidence {
            id: i.id.clone(),
            text: i.text.clone(),
            lines: i.lines.clone(),
        })
    }

    /// Builds a sentence and decides how well it's backed.
    pub fn sentence(&self, text: &str, evidence: Vec<Evidence>) -> Sentence {
        let text = text.trim().to_string();
        if evidence.is_empty() {
            let claim = looks_like_claim(&text, &self.skills, &self.job_words);
            return Sentence {
                backing: if claim {
                    Backing::Unbacked
                } else {
                    Backing::Plain
                },
                note: if claim {
                    "Nothing in your resume backs this.".into()
                } else {
                    String::new()
                },
                text,
                evidence,
            };
        }
        let mut support: Vec<&str> = evidence.iter().map(|e| e.text.as_str()).collect();
        support.extend(self.job_words.iter().map(String::as_str));
        let extra = new_numbers(&text, &support, &self.allowed_numbers);
        if extra.is_empty() {
            Sentence {
                text,
                backing: Backing::Backed,
                evidence,
                note: String::new(),
            }
        } else {
            Sentence {
                note: format!(
                    "“{}” isn’t in the resume lines it’s based on.",
                    extra.join("”, “")
                ),
                text,
                backing: Backing::Unbacked,
                evidence,
            }
        }
    }

    /// For sentences the user typed: finds profile items that cover most of its words.
    pub fn find_evidence(&self, text: &str) -> Vec<Evidence> {
        let ignore: HashSet<String> = self.job_words.iter().flat_map(|w| words(w)).collect();
        let wanted: HashSet<String> = content_words(text)
            .into_iter()
            .filter(|w| !ignore.contains(w))
            .collect();
        if wanted.is_empty() {
            return Vec::new();
        }
        let mut covered: HashSet<String> = HashSet::new();
        let mut chosen: Vec<&ProfileItem> = Vec::new();
        // Greedy: add the item covering the most still-uncovered words, up to three.
        while chosen.len() < 3 {
            let best = self
                .items
                .iter()
                .filter(|i| !chosen.iter().any(|c| c.id == i.id))
                .map(|i| {
                    let have: HashSet<String> = content_words(&i.text).into_iter().collect();
                    let gain = wanted
                        .iter()
                        .filter(|w| !covered.contains(*w) && have.contains(*w))
                        .count();
                    (gain, i, have)
                })
                .max_by_key(|(gain, _, _)| *gain);
            match best {
                Some((gain, item, have)) if gain > 0 => {
                    covered.extend(wanted.iter().filter(|w| have.contains(*w)).cloned());
                    chosen.push(item);
                }
                _ => break,
            }
        }
        if (covered.len() as f64) < 0.6 * wanted.len() as f64 {
            return Vec::new();
        }
        chosen
            .into_iter()
            .map(|i| Evidence {
                id: i.id.clone(),
                text: i.text.clone(),
                lines: i.lines.clone(),
            })
            .collect()
    }
}

const STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "that",
    "this",
    "from",
    "into",
    "onto",
    "about",
    "your",
    "you",
    "our",
    "their",
    "have",
    "has",
    "had",
    "was",
    "were",
    "are",
    "been",
    "being",
    "will",
    "would",
    "could",
    "should",
    "can",
    "also",
    "which",
    "while",
    "where",
    "when",
    "what",
    "who",
    "how",
    "than",
    "then",
    "there",
    "here",
    "they",
    "them",
    "its",
    "it's",
    "i've",
    "i'm",
    "i'd",
    "i’ve",
    "i’m",
    "i’d",
    "role",
    "work",
    "worked",
    "working",
    "team",
    "teams",
    "time",
    "most",
    "more",
    "very",
    "just",
    "like",
    "such",
    "each",
    "every",
    "over",
    "some",
    "all",
    "any",
    "one",
    "two",
    "years",
    "year",
    "recently",
    "experience",
    "company",
    "job",
    "position",
    "opening",
];

/// Lowercase words that carry meaning, for overlap checks.
fn content_words(text: &str) -> Vec<String> {
    words(text)
        .into_iter()
        .filter(|w| w.chars().count() >= 3 && !STOP_WORDS.contains(&w.as_str()))
        .map(|w| w.trim_end_matches(['\'', '’']).to_string())
        .collect()
}

/// Splits text into sentences on ". ", "! ", "? " and line breaks.
pub fn split_sentences(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut current = String::new();
        let chars: Vec<char> = line.chars().collect();
        for (i, c) in chars.iter().enumerate() {
            current.push(*c);
            let ends = matches!(c, '.' | '!' | '?')
                && chars.get(i + 1).is_some_and(|n| n.is_whitespace())
                // "e.g. " and "approx. " don't end sentences.
                && !current.trim_end().to_lowercase().ends_with("e.g.")
                && !current.trim_end().to_lowercase().ends_with("i.e.")
                && chars
                    .get(i + 2)
                    .is_none_or(|n| n.is_uppercase() || !n.is_alphabetic());
            if ends {
                let s = current.trim().to_string();
                if !s.is_empty() {
                    out.push(s);
                }
                current.clear();
            }
        }
        let s = current.trim().to_string();
        if !s.is_empty() {
            out.push(s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn support() -> Support {
        Support {
            items: vec![
                ProfileItem {
                    id: "a1".into(),
                    text: "Cut checkout load time by 40% for 300k monthly customers".into(),
                    lines: vec![5],
                },
                ProfileItem {
                    id: "s1".into(),
                    text: "Skill: React".into(),
                    lines: vec![9],
                },
                ProfileItem {
                    id: "r1".into(),
                    text: "Senior Frontend Engineer at Brook (2021 – Present)".into(),
                    lines: vec![3],
                },
            ],
            skills: vec!["React".into(), "GraphQL".into()],
            job_words: vec!["Lead Frontend Engineer".into(), "Acme 2".into()],
            allowed_numbers: vec![6],
        }
    }

    #[test]
    fn plain_sentences_are_not_claims() {
        let s = support();
        for t in [
            "I’m applying for the Lead Frontend Engineer role at Acme 2.",
            "I’d welcome the chance to talk about how I could help Acme 2.",
            "Thank you for your time.",
            "If this sounds like a fit, I’d be glad to talk.",
            "I’d be happy to share more in a call.",
            "My resume is attached.",
        ] {
            assert_eq!(s.sentence(t, vec![]).backing, Backing::Plain, "{t}");
        }
    }

    #[test]
    fn uncited_claims_are_flagged() {
        let s = support();
        for t in [
            "I have 10 years of Kubernetes experience.",
            "I led a team of engineers.",
            "I mentored seven engineers.",
            "I know GraphQL well.",
            "Improved revenue by 20%.",
        ] {
            assert_eq!(s.sentence(t, vec![]).backing, Backing::Unbacked, "{t}");
        }
    }

    #[test]
    fn cited_sentences_cannot_add_numbers() {
        let s = support();
        let ev = vec![s.evidence("a1").unwrap()];
        let ok = s.sentence("At Brook, I cut checkout load time by 40%.", ev.clone());
        assert_eq!(ok.backing, Backing::Backed);
        let bad = s.sentence("I cut load time by 60% for 300k customers.", ev.clone());
        assert_eq!(bad.backing, Backing::Unbacked);
        assert!(bad.note.contains("60"), "{}", bad.note);
        let words = s.sentence("I cut checkout load time for ten teams.", ev);
        assert_eq!(words.backing, Backing::Unbacked);
        // Years worked out by the app are allowed.
        for t in [
            "I have about 6 years of experience.",
            "I have about six years of experience.",
        ] {
            let years = s.sentence(t, vec![s.evidence("r1").unwrap()]);
            assert_eq!(years.backing, Backing::Backed, "{t}");
        }
    }

    #[test]
    fn finds_evidence_for_typed_sentences() {
        let s = support();
        let ev = s.find_evidence("I cut checkout load time for monthly customers.");
        assert_eq!(ev.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["a1"]);
        assert!(s.find_evidence("I ran a bakery in Paris.").is_empty());
        assert!(s.find_evidence("Thanks!").is_empty());
    }

    #[test]
    fn splits_sentences() {
        assert_eq!(
            split_sentences("I built X, e.g. a cache. Then Y! Done?\nNew line"),
            ["I built X, e.g. a cache.", "Then Y!", "Done?", "New line"]
        );
        assert_eq!(
            split_sentences("Node.js and v2.5 stay."),
            ["Node.js and v2.5 stay."]
        );
    }
}
