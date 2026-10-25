//! AI requirement analysis of one job against the profile.
//!
//! The model lists what the job asks for and, for each requirement, which profile items
//! support it, citing them by ID. Answers that cite IDs we didn't send are rejected, so
//! every piece of evidence shown is a real line from the user's own profile. Scores are
//! worked out here from that list, not taken from the model.

use std::{
    collections::{HashMap, HashSet},
    hash::{Hash, Hasher},
    path::Path,
    sync::atomic::AtomicBool,
};

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{Provider, claude};
use crate::{
    error::AppResult,
    jobs::{MatchBreakdown, Weights, score},
    profile::Profile,
};

/// Most requirements we keep per job.
const MAX_REQUIREMENTS: usize = 25;
/// Characters of job description sent; long postings are mostly boilerplate at the end.
const MAX_DESCRIPTION_CHARS: usize = 12_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RequirementKind {
    Skill,
    Experience,
    Role,
    Seniority,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum RequirementStatus {
    /// The profile clearly shows it.
    Matched,
    /// The profile may show it under another name; worth rewording.
    WordedDifferently,
    Missing,
}

/// A profile item cited as evidence, resolved to its text and resume lines.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Evidence {
    pub id: String,
    pub text: String,
    /// Indexes into the stored resume lines; empty for items added by hand.
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Requirement {
    pub text: String,
    pub kind: RequirementKind,
    /// Must-have, as opposed to nice-to-have.
    pub required: bool,
    pub status: RequirementStatus,
    pub evidence: Vec<Evidence>,
    /// Short reason, e.g. what the user could reword.
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobAnalysis {
    pub provider: Provider,
    pub model: String,
    pub created_at: String,
    pub summary: String,
    pub years_required: Option<u32>,
    pub seniority: Option<String>,
    pub requirements: Vec<Requirement>,
}

/// One profile item the model may cite.
#[derive(Debug, Clone, PartialEq)]
pub struct ProfileItem {
    pub id: String,
    pub text: String,
    pub lines: Vec<u32>,
}

/// The profile as a list of citable items. Contact details are left out: they never
/// leave the computer.
pub fn profile_items(p: &Profile) -> Vec<ProfileItem> {
    let mut items = Vec::new();
    let mut push = |id: &str, text: String, lines: &[u32]| {
        let text = text.trim().to_string();
        if !text.is_empty() {
            items.push(ProfileItem {
                id: id.to_string(),
                text,
                lines: lines.to_vec(),
            });
        }
    };
    if !p.contact.headline.is_empty() {
        push("headline", p.contact.headline.clone(), &p.contact.lines);
    }
    push("summary", p.summary.clone(), &p.summary_lines);
    for r in &p.roles {
        let mut head = r.title.clone();
        if !r.company.is_empty() {
            head = format!("{head} at {}", r.company);
        }
        if !r.dates.is_empty() {
            head = format!("{head} ({})", r.dates);
        }
        push(&r.id, head, &r.lines);
        for a in &r.achievements {
            push(&a.id, a.text.clone(), &a.lines);
        }
    }
    for s in &p.skills {
        push(&s.id, format!("Skill: {}", s.name), &s.lines);
    }
    for e in &p.education {
        push(
            &e.id,
            format!("{} — {} {}", e.degree, e.school, e.dates),
            &e.lines,
        );
    }
    for pr in &p.projects {
        push(
            &pr.id,
            format!("Project: {} — {}", pr.name, pr.description),
            &pr.lines,
        );
    }
    for c in &p.certifications {
        push(
            &c.id,
            format!("Certification: {} {} {}", c.name, c.issuer, c.year),
            &c.lines,
        );
    }
    for l in &p.languages {
        push(&l.id, format!("Language: {} {}", l.name, l.level), &l.lines);
    }
    // IDs must be unique to be citable; hand-added items may lack one.
    let mut seen = HashSet::new();
    for (n, item) in items.iter_mut().enumerate() {
        if item.id.is_empty() || !seen.insert(item.id.clone()) {
            item.id = format!("item{n}");
            seen.insert(item.id.clone());
        }
    }
    items
}

/// Changes whenever anything the analysis depends on changes.
pub fn profile_hash(items: &[ProfileItem]) -> String {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for i in items {
        (&i.id, &i.text).hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

pub const SYSTEM_PROMPT: &str = "You compare a job posting with a candidate's profile. \
List the job's concrete requirements (skills, tools, years of experience, domain, role, seniority, languages, other must-haves). \
For each, decide: \"matched\" if a profile item clearly shows it; \"worded_differently\" if a profile item likely shows it \
under another name or more indirectly; \"missing\" if nothing in the profile supports it. \
Cite supporting profile items only by the IDs given in square brackets; never invent IDs. \
\"matched\" needs at least one cited ID. Be strict and honest; do not flatter. \
Skip benefits, company boilerplate, legal text, and logistics (location, time zone, hours, contract type, start date); the app checks those separately. Keep each requirement under 12 words and each note under 20 words.";

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["summary", "years_required", "seniority", "requirements"],
        "properties": {
            "summary": { "type": "string", "description": "One sentence: how well the profile fits." },
            "years_required": { "type": ["integer", "null"] },
            "seniority": { "type": ["string", "null"], "enum": ["junior", "mid", "senior", "staff", "lead", null] },
            "requirements": {
                "type": "array",
                "maxItems": MAX_REQUIREMENTS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["text", "kind", "required", "status", "evidence", "note"],
                    "properties": {
                        "text": { "type": "string" },
                        "kind": { "type": "string", "enum": ["skill", "experience", "role", "seniority", "other"] },
                        "required": { "type": "boolean" },
                        "status": { "type": "string", "enum": ["matched", "worded_differently", "missing"] },
                        "evidence": { "type": "array", "items": { "type": "string" } },
                        "note": { "type": "string" }
                    }
                }
            }
        }
    })
}

pub fn prompt(title: &str, company: &str, description: &str, items: &[ProfileItem]) -> String {
    let description: String = description.chars().take(MAX_DESCRIPTION_CHARS).collect();
    let profile: Vec<String> = items
        .iter()
        .map(|i| format!("[{}] {}", i.id, i.text))
        .collect();
    format!(
        "JOB\nTitle: {title}\nCompany: {company}\n\n{description}\n\nCANDIDATE PROFILE\n{}\n",
        profile.join("\n")
    )
}

#[derive(Deserialize)]
struct RawAnswer {
    summary: String,
    years_required: Option<i64>,
    seniority: Option<String>,
    requirements: Vec<RawRequirement>,
}

#[derive(Deserialize)]
struct RawRequirement {
    text: String,
    kind: RequirementKind,
    required: bool,
    status: RequirementStatus,
    evidence: Vec<String>,
    note: String,
}

/// Checks the model's answer against the profile we sent. Any made-up citation rejects
/// the whole answer.
pub fn validate(answer: &Value, items: &[ProfileItem]) -> Result<ValidAnswer, String> {
    let raw: RawAnswer = serde_json::from_value(answer.clone())
        .map_err(|e| format!("The answer didn’t match the expected shape: {e}"))?;
    let by_id: HashMap<&str, &ProfileItem> = items.iter().map(|i| (i.id.as_str(), i)).collect();
    let mut unknown = Vec::new();
    let mut requirements = Vec::new();
    for r in raw.requirements.into_iter().take(MAX_REQUIREMENTS) {
        let text = r.text.trim().to_string();
        if text.is_empty() {
            continue;
        }
        let mut evidence = Vec::new();
        for id in &r.evidence {
            let id = id.trim().trim_matches(['[', ']']);
            match by_id.get(id) {
                Some(item) if !evidence.iter().any(|e: &Evidence| e.id == id) => {
                    evidence.push(Evidence {
                        id: item.id.clone(),
                        text: item.text.clone(),
                        lines: item.lines.clone(),
                    })
                }
                Some(_) => {}
                None => unknown.push(id.to_string()),
            }
        }
        if r.status == RequirementStatus::Matched && evidence.is_empty() && r.evidence.is_empty() {
            return Err(format!(
                "“{text}” was marked as matched without citing the profile."
            ));
        }
        requirements.push(Requirement {
            text,
            kind: r.kind,
            required: r.required,
            status: r.status,
            evidence,
            note: r.note.trim().to_string(),
        });
    }
    if !unknown.is_empty() {
        unknown.sort();
        unknown.dedup();
        return Err(format!(
            "It cited profile items that don’t exist: {}.",
            unknown.join(", ")
        ));
    }
    Ok(ValidAnswer {
        summary: raw.summary.trim().to_string(),
        years_required: raw
            .years_required
            .and_then(|y| u32::try_from(y).ok())
            .filter(|y| *y <= 40),
        seniority: raw.seniority.filter(|s| !s.is_empty()),
        requirements,
    })
}

#[derive(Debug)]
pub struct ValidAnswer {
    pub summary: String,
    pub years_required: Option<u32>,
    pub seniority: Option<String>,
    pub requirements: Vec<Requirement>,
}

/// Asks the provider, retrying once with the reason when the answer is rejected.
pub fn analyze(
    provider: Provider,
    title: &str,
    company: &str,
    description: &str,
    items: &[ProfileItem],
    cwd: &Path,
    cancel: &AtomicBool,
) -> Result<JobAnalysis, String> {
    if provider != Provider::Claude {
        return Err(format!("{} isn’t supported yet.", provider.label()));
    }
    let base = prompt(title, company, description, items);
    let schema = schema();
    let mut input = base.clone();
    let mut last_error = String::new();
    for _ in 0..2 {
        let answer = claude::ask(claude::MODEL, SYSTEM_PROMPT, &input, &schema, cwd, cancel)?;
        match validate(&answer, items) {
            Ok(v) => {
                return Ok(JobAnalysis {
                    provider,
                    model: claude::MODEL.into(),
                    created_at: crate::jobs::dates::now(),
                    summary: v.summary,
                    years_required: v.years_required,
                    seniority: v.seniority,
                    requirements: v.requirements,
                });
            }
            Err(e) => {
                last_error = e;
                input = format!(
                    "{base}\nYour previous answer was rejected: {last_error} Only cite IDs from the CANDIDATE PROFILE list."
                );
            }
        }
    }
    Err(format!(
        "Claude’s answer couldn’t be trusted, so it wasn’t used. {last_error}"
    ))
}

/// The rules breakdown, with the skills factor (and years, when the rules couldn't read
/// them) taken from the AI's requirement list.
pub fn combine(rules: &MatchBreakdown, a: &JobAnalysis, weights: &Weights) -> MatchBreakdown {
    let mut out = rules.clone();
    let (mut got, mut total) = (0.0_f64, 0.0_f64);
    for r in a
        .requirements
        .iter()
        .filter(|r| matches!(r.kind, RequirementKind::Skill | RequirementKind::Other))
    {
        let w = if r.required { 2.0 } else { 1.0 };
        total += w;
        got += w * match r.status {
            RequirementStatus::Matched => 1.0,
            RequirementStatus::WordedDifferently => 0.5,
            RequirementStatus::Missing => 0.0,
        };
    }
    if total > 0.0 {
        out.skills = (got / total * 100.0).round() as u32;
    }
    if out.years_required.is_none() && a.years_required.is_some() {
        out.years_required = a.years_required;
        out.experience = score::experience_factor(a.years_required, out.years_on_profile);
    }
    if out.job_seniority.is_none() {
        out.job_seniority = a.seniority.clone();
    }
    out.score = weights.score(out.skills, out.experience, out.role, out.seniority);
    out
}

// ---- Cache ----

pub fn save(conn: &Connection, job_id: i64, hash: &str, a: &JobAnalysis) -> AppResult<()> {
    conn.execute(
        "INSERT INTO job_analysis (job_id, provider, model, profile_hash, data, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)
         ON CONFLICT (job_id) DO UPDATE SET provider = excluded.provider, model = excluded.model,
            profile_hash = excluded.profile_hash, data = excluded.data, created_at = excluded.created_at",
        params![
            job_id,
            a.provider.as_str(),
            a.model,
            hash,
            serde_json::to_string(a).unwrap_or_default(),
            a.created_at
        ],
    )?;
    Ok(())
}

/// The cached analysis, if it was made against this profile.
pub fn load(conn: &Connection, job_id: i64, hash: &str) -> AppResult<Option<JobAnalysis>> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM job_analysis WHERE job_id = ?1 AND profile_hash = ?2",
            params![job_id, hash],
            |r| r.get(0),
        )
        .optional()?;
    Ok(data.and_then(|d| serde_json::from_str(&d).ok()))
}

/// The cached analysis whatever profile it was made against, and whether it's current.
pub fn load_latest(
    conn: &Connection,
    job_id: i64,
    hash: &str,
) -> AppResult<Option<(JobAnalysis, bool)>> {
    let row: Option<(String, String)> = conn
        .query_row(
            "SELECT data, profile_hash FROM job_analysis WHERE job_id = ?1",
            [job_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(d, h)| serde_json::from_str(&d).ok().map(|a| (a, h == hash))))
}

/// Every cached analysis made against this profile, for scoring the feed.
pub fn load_all(conn: &Connection, hash: &str) -> AppResult<HashMap<i64, JobAnalysis>> {
    let mut stmt = conn.prepare("SELECT job_id, data FROM job_analysis WHERE profile_hash = ?1")?;
    let rows = stmt.query_map([hash], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
    })?;
    let mut out = HashMap::new();
    for row in rows {
        let (id, data) = row?;
        if let Ok(a) = serde_json::from_str(&data) {
            out.insert(id, a);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Achievement, Contact, Role, Skill};

    fn profile() -> Profile {
        Profile {
            contact: Contact {
                name: "Sam Rivera".into(),
                email: "sam@example.com".into(),
                headline: "Frontend Engineer".into(),
                ..Default::default()
            },
            roles: vec![Role {
                id: "r1".into(),
                title: "Senior Frontend Engineer".into(),
                company: "Brook".into(),
                dates: "2021 – Present".into(),
                lines: vec![3],
                achievements: vec![Achievement {
                    id: "a1".into(),
                    text: "Led the TypeScript migration.".into(),
                    lines: vec![4],
                }],
                ..Default::default()
            }],
            skills: vec![
                Skill {
                    id: "s1".into(),
                    name: "React".into(),
                    lines: vec![8],
                },
                Skill {
                    id: "".into(),
                    name: "Figma".into(),
                    lines: vec![],
                },
            ],
            ..Default::default()
        }
    }

    fn answer(evidence: &[&str], status: &str) -> Value {
        json!({
            "summary": "Strong frontend fit.",
            "years_required": 5,
            "seniority": "senior",
            "requirements": [
                { "text": "React", "kind": "skill", "required": true, "status": status, "evidence": evidence, "note": "" },
                { "text": "GraphQL", "kind": "skill", "required": false, "status": "missing", "evidence": [], "note": "Not on your resume." }
            ]
        })
    }

    #[test]
    fn profile_items_leave_out_contact_details() {
        let items = profile_items(&profile());
        let text: String = items
            .iter()
            .map(|i| format!("{} {}\n", i.id, i.text))
            .collect();
        assert!(!text.contains("sam@example.com") && !text.contains("Sam Rivera"));
        assert!(text.contains("r1 Senior Frontend Engineer at Brook (2021 – Present)"));
        assert!(text.contains("a1 Led the TypeScript migration."));
        // The skill without an ID gets one so it can be cited.
        assert!(
            items
                .iter()
                .any(|i| i.id.starts_with("item") && i.text == "Skill: Figma")
        );
    }

    #[test]
    fn hash_changes_with_the_profile() {
        let mut p = profile();
        let a = profile_hash(&profile_items(&p));
        p.skills[0].name = "Vue".into();
        assert_ne!(a, profile_hash(&profile_items(&p)));
    }

    #[test]
    fn accepts_real_citations_and_resolves_them() {
        let items = profile_items(&profile());
        let v = validate(&answer(&["s1", "[a1]"], "matched"), &items).unwrap();
        let r = &v.requirements[0];
        assert_eq!(
            r.evidence.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(),
            ["s1", "a1"]
        );
        assert_eq!(r.evidence[1].lines, vec![4]);
        assert_eq!(v.years_required, Some(5));
    }

    #[test]
    fn rejects_made_up_citations() {
        let items = profile_items(&profile());
        let e = validate(&answer(&["s1", "x9"], "matched"), &items).unwrap_err();
        assert!(e.contains("x9"), "{e}");
        assert!(validate(&answer(&[], "matched"), &items).is_err());
        assert!(validate(&json!({ "summary": 1 }), &items).is_err());
    }

    #[test]
    fn scores_skills_from_requirements() {
        let items = profile_items(&profile());
        let v = validate(&answer(&["s1"], "matched"), &items).unwrap();
        let a = JobAnalysis {
            provider: Provider::Claude,
            model: "haiku".into(),
            created_at: String::new(),
            summary: v.summary,
            years_required: v.years_required,
            seniority: v.seniority,
            requirements: v.requirements,
        };
        let rules = MatchBreakdown {
            skills: 10,
            experience: 70,
            role: 80,
            seniority: 60,
            years_on_profile: Some(6),
            ..Default::default()
        };
        let out = combine(&rules, &a, &Weights::default());
        // Required React matched (2 of 3 weight), optional GraphQL missing.
        assert_eq!(out.skills, 67);
        assert_eq!(out.years_required, Some(5));
        assert_eq!(out.experience, 100);
        assert_eq!(out.job_seniority.as_deref(), Some("senior"));
        assert_eq!(out.score, Weights::default().score(67, 100, 80, 60));
    }

    #[test]
    fn caches_per_profile() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO jobs (canonical_key, title, company) VALUES ('k', 'T', 'C')",
            [],
        )
        .unwrap();
        let a = JobAnalysis {
            provider: Provider::Claude,
            model: "haiku".into(),
            created_at: "2026-10-24T00:00:00Z".into(),
            summary: "ok".into(),
            years_required: None,
            seniority: None,
            requirements: vec![],
        };
        save(&conn, 1, "h1", &a).unwrap();
        assert_eq!(load(&conn, 1, "h1").unwrap(), Some(a));
        assert_eq!(load(&conn, 1, "h2").unwrap(), None);
        assert_eq!(load_all(&conn, "h1").unwrap().len(), 1);
    }

    /// Real Claude call. `cargo test live_analysis -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_analysis() {
        let body = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/sources/remotive.json"
        ))
        .unwrap();
        let job = &crate::jobs::sources::parse_remotive(&body).unwrap()[0];
        let items = profile_items(&profile());
        let dir = tempfile::tempdir().unwrap();
        let start = std::time::Instant::now();
        let a = analyze(
            Provider::Claude,
            &job.title,
            &job.company,
            &job.description,
            &items,
            dir.path(),
            &AtomicBool::new(false),
        )
        .unwrap();
        println!("{:?} — {}", start.elapsed(), a.summary);
        for r in &a.requirements {
            println!(
                "  {:?} {:?} {} ← {:?}",
                r.status,
                r.kind,
                r.text,
                r.evidence.iter().map(|e| &e.id).collect::<Vec<_>>()
            );
        }
    }
}
