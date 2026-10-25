//! Spotting the same job listed on several sites.
//!
//! Two listings are one job when the normalized company and title match and the
//! descriptions are similar (or one is too short to compare).

use std::collections::HashSet;

const COMPANY_SUFFIXES: &[&str] = &[
    "inc",
    "llc",
    "ltd",
    "limited",
    "gmbh",
    "corp",
    "corporation",
    "co",
    "company",
    "plc",
    "sa",
    "srl",
    "bv",
    "ag",
    "oy",
    "ab",
    "as",
    "pty",
    "pte",
    "sl",
    "sas",
    "the",
];

const TITLE_NOISE: &[&str] = &[
    "remote", "fully", "100", "full", "time", "fulltime", "m", "f", "d", "x",
];

fn tokens(s: &str) -> Vec<String> {
    s.to_lowercase()
        .replace('&', " and ")
        .split(|c: char| !c.is_alphanumeric() && c != '+' && c != '#')
        .filter(|w| !w.is_empty())
        .map(str::to_string)
        .collect()
}

pub fn company_key(company: &str) -> String {
    let t = tokens(company);
    let kept: Vec<&str> = t
        .iter()
        .map(String::as_str)
        .filter(|w| !COMPANY_SUFFIXES.contains(w))
        .collect();
    if kept.is_empty() {
        t.join("")
    } else {
        kept.join("")
    }
}

pub fn title_key(title: &str) -> String {
    tokens(title)
        .iter()
        .map(|w| match w.as_str() {
            "sr" => "senior",
            "jr" => "junior",
            "eng" => "engineer",
            "dev" => "developer",
            "frontend" | "front" => "front",
            "backend" | "back" => "back",
            "fullstack" => "full stack",
            w => w,
        })
        .filter(|w| !TITLE_NOISE.contains(w) && *w != "end")
        .collect::<Vec<_>>()
        .join(" ")
}

/// Key shared by listings of the same job: "company|title".
pub fn base_key(company: &str, title: &str) -> String {
    format!("{}|{}", company_key(company), title_key(title))
}

const COMPARE_WORDS: usize = 400;
/// Below this many words a description is too short to tell listings apart.
const MIN_WORDS: usize = 40;
const SIMILAR: f64 = 0.5;

/// Distinct words among the first [`COMPARE_WORDS`], and how many words there were.
fn word_set(text: &str) -> (HashSet<String>, usize) {
    let words: Vec<String> = tokens(text).into_iter().take(COMPARE_WORDS).collect();
    let n = words.len();
    (words.into_iter().collect(), n)
}

/// Whether two descriptions read like the same posting.
pub fn similar_descriptions(a: &str, b: &str) -> bool {
    let ((a, a_len), (b, b_len)) = (word_set(a), word_set(b));
    if a_len < MIN_WORDS || b_len < MIN_WORDS {
        return true;
    }
    let common = a.intersection(&b).count() as f64;
    // Overlap coefficient: a short repost of a longer description still matches.
    common / a.len().min(b.len()) as f64 >= SIMILAR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_formatting() {
        assert_eq!(
            base_key("Lumen Health, Inc.", "Sr. Front-end Engineer (Remote)"),
            base_key("Lumen Health", "Senior Frontend Engineer")
        );
        assert_ne!(
            base_key("Lumen Health", "Senior Frontend Engineer"),
            base_key("Lumen Health", "Senior Backend Engineer")
        );
        assert_eq!(company_key("The Company"), "thecompany");
    }

    #[test]
    fn compares_descriptions() {
        let long = "We are hiring a frontend engineer to build accessible patient tools with React and TypeScript. ".repeat(3)
            + "You will work with design, product and research on our scheduling, messaging and billing apps every day.";
        let other = "Join our data platform team to build pipelines in Python, Spark and Airflow for analytics. ".repeat(3)
            + "You will own ingestion, modeling, quality checks and the warehouse that the whole company relies on.";
        assert!(similar_descriptions(
            &long,
            &format!("{long} Apply on our site.")
        ));
        assert!(!similar_descriptions(&long, &other));
        assert!(similar_descriptions(&long, "Short teaser."));
    }
}
