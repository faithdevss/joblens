//! Spotting the same job listed on several sites.
//!
//! Two listings are one job when the normalized company and title match and the
//! descriptions are similar (or one is too short to compare).

use std::collections::HashSet;

use super::RawJob;

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
    "group",
    "holding",
    "holdings",
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

fn comparable(text: &str) -> bool {
    word_set(text).1 >= MIN_WORDS
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

/// One job a source lists several times, once per location.
pub struct Repost {
    /// The first listing, with every listing's location.
    pub job: RawJob,
    /// The other listings' ids and links.
    pub also: Vec<(String, String)>,
}

/// Adds the places in `more` ("A; B") that `location` doesn't name yet.
pub fn add_places(location: &mut String, more: &str) {
    for place in more.split("; ").map(str::trim).filter(|p| !p.is_empty()) {
        if location.split("; ").any(|l| l.trim() == place) {
            continue;
        }
        if !location.trim().is_empty() {
            location.push_str("; ");
        }
        location.push_str(place);
    }
}

/// Groups a source's listings that are the same job posted again for another place:
/// same company and title, and descriptions long enough to compare that match. A
/// source listing two roles under one title without saying more keeps them apart.
pub fn merge_reposts(jobs: Vec<RawJob>) -> Vec<Repost> {
    let mut out: Vec<(String, Repost)> = Vec::new();
    for job in jobs {
        let key = base_key(&job.company, &job.title);
        let same = out.iter_mut().find(|(k, r)| {
            *k == key
                && comparable(&r.job.description)
                && comparable(&job.description)
                && similar_descriptions(&r.job.description, &job.description)
        });
        match same {
            Some((_, r)) => {
                add_places(&mut r.job.location, &job.location);
                r.also.push((job.external_id, job.url));
            }
            None => out.push((
                key,
                Repost {
                    job,
                    also: Vec::new(),
                },
            )),
        }
    }
    out.into_iter().map(|(_, r)| r).collect()
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
        assert_eq!(company_key("Bosch Group"), company_key("Bosch"));
    }

    #[test]
    fn merges_reposts_for_other_places() {
        let desc = "Audit business and IT operations across finance, sales and support, report findings to leadership. ".repeat(5);
        let listing = |id: &str, title: &str, location: &str, description: &str| RawJob {
            external_id: id.into(),
            title: title.into(),
            company: "ServiceNow".into(),
            location: location.into(),
            description: description.into(),
            ..Default::default()
        };
        let merged = merge_reposts(vec![
            listing("1", "Senior Auditor", "Remote, Texas, United States", &desc),
            listing(
                "2",
                "Senior Auditor",
                "Remote, Florida, United States",
                &desc,
            ),
            listing("3", "Senior Auditor", "Remote, Texas, United States", &desc),
            listing("4", "Staff Engineer", "Remote, Texas, United States", &desc),
        ]);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].job.external_id, "1");
        assert_eq!(
            merged[0].job.location,
            "Remote, Texas, United States; Remote, Florida, United States"
        );
        let ids: Vec<&str> = merged[0].also.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["2", "3"]);
        assert!(merged[1].also.is_empty());

        let other = "Build data pipelines in Python and Spark for the analytics warehouse team every single day. ".repeat(5);
        let distinct = merge_reposts(vec![
            listing("1", "Senior Auditor", "Texas", &desc),
            listing("2", "Senior Auditor", "Florida", &other),
        ]);
        assert_eq!(
            distinct.len(),
            2,
            "different roles with the same title stay apart"
        );
        let bare = merge_reposts(vec![
            listing("1", "Senior Auditor", "Texas", ""),
            listing("2", "Senior Auditor", "Florida", ""),
        ]);
        assert_eq!(bare.len(), 2, "nothing to compare, so not merged");
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

    fn post(id: &str, company: &str, title: &str, location: &str, description: &str) -> RawJob {
        RawJob {
            external_id: id.into(),
            title: title.into(),
            company: company.into(),
            url: format!("https://example.com/{id}"),
            location: location.into(),
            description: description.into(),
            ..Default::default()
        }
    }

    fn long_text(topic: &str) -> String {
        format!("We are hiring for {topic} and you will own the work end to end with the team. ")
            .repeat(5)
    }

    #[test]
    fn add_places_skips_blanks_and_repeats() {
        let mut loc = String::new();
        add_places(&mut loc, "Texas");
        assert_eq!(loc, "Texas");
        add_places(&mut loc, "Texas; Florida");
        assert_eq!(
            loc, "Texas; Florida",
            "an already merged list adds only what's new"
        );
        add_places(&mut loc, " Florida ; ; Ohio");
        assert_eq!(loc, "Texas; Florida; Ohio");
        add_places(&mut loc, "");
        assert_eq!(loc, "Texas; Florida; Ohio");
        let mut blank = "  ".to_string();
        add_places(&mut blank, "Spain");
        assert_eq!(blank.trim(), "Spain");
    }

    #[test]
    fn merge_reposts_edge_cases() {
        assert!(merge_reposts(Vec::new()).is_empty());

        let desc = long_text("audit");
        // The first listing has no place; the repost's fills it in.
        let m = merge_reposts(vec![
            post("1", "Acme", "Auditor", "", &desc),
            post("2", "Acme", "Auditor", "Remote, Spain", &desc),
        ]);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].job.location, "Remote, Spain");
        // Each extra listing keeps its own link.
        assert_eq!(
            m[0].also,
            [("2".to_string(), "https://example.com/2".to_string())]
        );

        // Title and company spelled differently are still the same job.
        let m = merge_reposts(vec![
            post("1", "Acme, Inc.", "Sr. Auditor (Remote)", "Texas", &desc),
            post("2", "Acme", "Senior Auditor", "Ohio", &desc),
        ]);
        assert_eq!(m.len(), 1);

        // Another company with the same title and text is another job.
        let m = merge_reposts(vec![
            post("1", "Acme", "Auditor", "Texas", &desc),
            post("2", "Beta", "Auditor", "Texas", &desc),
        ]);
        assert_eq!(m.len(), 2);

        // A different role between two reposts doesn't break the group.
        let other =
            "Lead onboarding calls, write help center articles, train new customers on billing. "
                .repeat(5);
        let m = merge_reposts(vec![
            post("a", "Acme", "Auditor", "Texas", &desc),
            post("b", "Acme", "Auditor", "Utah", &other),
            post("c", "Acme", "Auditor", "Ohio", &desc),
        ]);
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].job.location, "Texas; Ohio");
        assert_eq!(m[1].job.location, "Utah");

        // A short teaser can't be told apart from another role, so it stays apart.
        let m = merge_reposts(vec![
            post("1", "Acme", "Auditor", "Texas", &desc),
            post("2", "Acme", "Auditor", "Ohio", "Apply now."),
        ]);
        assert_eq!(m.len(), 2);

        // The order listings come in is kept.
        let m = merge_reposts(vec![
            post("z", "Acme", "Zeta", "", &desc),
            post("a", "Acme", "Alpha", "", &desc),
        ]);
        let ids: Vec<&str> = m.iter().map(|r| r.job.external_id.as_str()).collect();
        assert_eq!(ids, ["z", "a"]);
    }

    #[test]
    fn company_suffixes_edge_cases() {
        assert_eq!(company_key("Acme Holdings Ltd"), company_key("Acme"));
        // A name made only of suffixes is kept whole rather than emptied.
        assert_eq!(company_key("The Group"), "thegroup");
        assert_ne!(company_key("Group"), "");
    }
}
