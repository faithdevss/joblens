//! Interview topics for a job, built from the gap check and the posting. Nothing is
//! invented: every story points to a real line from the user's profile.

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::gap::{self, Coverage, GapRequirement};
use crate::{
    ai::analysis::Evidence,
    error::AppResult,
    jobs::{EligibilityStatus, feed},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum TopicKind {
    /// The user has a story for it.
    Story,
    /// On the resume under other words.
    Worded,
    /// Not on the resume.
    Gap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PrepTopic {
    /// Stable key for the "prepared" mark.
    pub key: String,
    pub topic: String,
    pub kind: TopicKind,
    /// What they may ask (stories) or how to handle it (gaps).
    pub prompt: String,
    pub evidence: Vec<Evidence>,
    pub prepared: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InterviewPrep {
    pub topics: Vec<PrepTopic>,
    pub questions: Vec<String>,
}

fn lower_first(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        // Keep acronyms ("AWS") and names with inner capitals ("TypeScript").
        Some(f) if !c.clone().any(char::is_uppercase) && s.split_whitespace().count() > 1 => {
            f.to_lowercase().chain(c).collect()
        }
        _ => s.to_string(),
    }
}

fn topic(r: &GapRequirement) -> PrepTopic {
    let key = format!("req:{}", r.text.to_lowercase());
    let starts_with_number = r.text.chars().next().is_some_and(|c| c.is_ascii_digit());
    let (kind, prompt) = match r.coverage {
        Coverage::Covered if starts_with_number => (
            TopicKind::Story,
            "Walk me through your background and how it fits this role.".to_string(),
        ),
        Coverage::Covered => (
            TopicKind::Story,
            format!("Tell me about a time you worked with {}.", lower_first(&r.text)),
        ),
        Coverage::Partly => (
            TopicKind::Worded,
            match r.evidence.first() {
                Some(e) => format!("Use their words. Closest on your resume: “{}”.", e.text),
                None => "Use their words for what you’ve done that comes closest.".to_string(),
            },
        ),
        Coverage::Missing => (
            TopicKind::Gap,
            "Say so plainly, then name the nearest thing you’ve done and how you’d get up to speed."
                .to_string(),
        ),
    };
    PrepTopic {
        key,
        topic: r.text.clone(),
        kind,
        prompt,
        evidence: r.evidence.clone(),
        prepared: false,
    }
}

pub fn questions(
    company: &str,
    has_salary: bool,
    eligibility: EligibilityStatus,
    country: &str,
) -> Vec<String> {
    let mut q = vec![
        format!("What would success look like in this role at {company} after the first 90 days?"),
        "How does the team work across time zones: overlap hours, meetings, async habits?"
            .to_string(),
        if has_salary {
            "Where in the posted range do offers usually land, and what moves it?".to_string()
        } else {
            "The posting doesn’t list a salary. What’s the budgeted range?".to_string()
        },
    ];
    if eligibility != EligibilityStatus::Eligible {
        q.push(if country.is_empty() {
            "Can you confirm which countries you can hire from?".to_string()
        } else {
            format!("Can you confirm you can hire someone based in {country}?")
        });
    }
    q
}

pub fn prep(conn: &Connection, job_id: i64) -> AppResult<InterviewPrep> {
    let g = gap::check(conn, job_id)?;
    let detail = feed::job_detail(conn, job_id)?;
    let marks: Vec<String> = {
        let mut stmt = conn.prepare("SELECT topic FROM prep_marks WHERE job_id = ?1")?;
        stmt.query_map([job_id], |r| r.get(0))?
            .collect::<Result<_, _>>()?
    };
    let mut topics: Vec<PrepTopic> = g.requirements.iter().map(topic).collect();
    // Stories first, then rewording, then gaps.
    topics.sort_by_key(|t| match t.kind {
        TopicKind::Story => 0,
        TopicKind::Worded => 1,
        TopicKind::Gap => 2,
    });
    for t in &mut topics {
        t.prepared = marks.contains(&t.key);
    }
    let country = crate::db::store::get_search_settings(conn)?.country;
    let country = crate::jobs::geo::country_name(&country).to_string();
    Ok(InterviewPrep {
        topics,
        questions: questions(
            &detail.job.company,
            detail.job.salary.is_some(),
            detail.job.eligibility.status,
            &country,
        ),
    })
}

pub fn set_prepared(conn: &Connection, job_id: i64, key: &str, prepared: bool) -> AppResult<()> {
    if prepared {
        conn.execute(
            "INSERT OR IGNORE INTO prep_marks (job_id, topic) VALUES (?1, ?2)",
            params![job_id, key],
        )?;
    } else {
        conn.execute(
            "DELETE FROM prep_marks WHERE job_id = ?1 AND topic = ?2",
            params![job_id, key],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(text: &str, coverage: Coverage, ev: &[&str]) -> GapRequirement {
        GapRequirement {
            text: text.into(),
            required: true,
            coverage,
            evidence: ev
                .iter()
                .map(|t| Evidence {
                    id: "a1".into(),
                    text: (*t).into(),
                    lines: vec![1],
                })
                .collect(),
            note: String::new(),
        }
    }

    #[test]
    fn topics_point_to_real_lines() {
        let s = topic(&req(
            "Design systems",
            Coverage::Covered,
            &["Built the component library"],
        ));
        assert_eq!(s.kind, TopicKind::Story);
        assert_eq!(
            s.prompt,
            "Tell me about a time you worked with design systems."
        );
        assert_eq!(s.evidence[0].text, "Built the component library");
        let t = topic(&req("TypeScript", Coverage::Covered, &["x"]));
        assert_eq!(t.prompt, "Tell me about a time you worked with TypeScript.");
        let w = topic(&req(
            "Unit testing",
            Coverage::Partly,
            &["Wrote Jest tests"],
        ));
        assert!(w.prompt.contains("“Wrote Jest tests”"));
        assert_eq!(
            topic(&req("GraphQL", Coverage::Missing, &[])).kind,
            TopicKind::Gap
        );
    }

    #[test]
    fn questions_fit_the_posting() {
        let q = questions("Acme", false, EligibilityStatus::NotStated, "Portugal");
        assert!(q[0].contains("Acme"));
        assert!(q.iter().any(|x| x.contains("budgeted range")));
        assert!(q.iter().any(|x| x.contains("based in Portugal")));
        assert_eq!(
            questions("Acme", true, EligibilityStatus::Eligible, "").len(),
            3
        );
    }

    #[test]
    fn marks_persist() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO jobs (canonical_key, title, company, description) VALUES ('k', 'Engineer', 'Acme', 'We use React daily.')",
            [],
        )
        .unwrap();
        let p = prep(&conn, 1).unwrap();
        let key = p.topics[0].key.clone();
        set_prepared(&conn, 1, &key, true).unwrap();
        set_prepared(&conn, 1, &key, true).unwrap();
        assert!(prep(&conn, 1).unwrap().topics[0].prepared);
        set_prepared(&conn, 1, &key, false).unwrap();
        assert!(!prep(&conn, 1).unwrap().topics[0].prepared);
    }
}
