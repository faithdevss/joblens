//! Resume gap check for one job: which requirements the resume covers, how it reads as
//! plain text, basic checks, and suggested edits drawn only from the user's own lines.
//!
//! There's deliberately no overall "ATS score"; only what's covered and what isn't.

use std::{path::Path, sync::LazyLock, sync::atomic::AtomicBool};

use regex::Regex;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::new_numbers;
use crate::{
    ai::{
        Provider,
        analysis::{Evidence, JobAnalysis, RequirementStatus},
        claude,
    },
    db::store,
    error::{AppError, AppResult},
    jobs::{
        MatchBreakdown, feed,
        text::{Phrases, normalize_phrase},
    },
    profile::{Profile, Skill},
    resume::parse::section_heading,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Coverage {
    Covered,
    /// Probably there, but worded differently from the job.
    Partly,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GapRequirement {
    pub text: String,
    pub required: bool,
    pub coverage: Coverage,
    pub evidence: Vec<Evidence>,
    pub note: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BasicCheck {
    pub id: String,
    pub ok: bool,
    pub label: String,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum EditDecision {
    Accepted,
    Rejected,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum EditSource {
    Rules,
    Ai,
}

/// Where an accepted edit goes in the profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EditTarget {
    AddSkill {
        lines: Vec<u32>,
    },
    /// Replace the text of an achievement, the summary or a project description.
    Rewrite {
        item_id: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestedEdit {
    pub id: String,
    pub source: EditSource,
    /// Why this helps, e.g. "Uses the job’s wording for “unit testing”".
    pub why: String,
    /// Where it goes, e.g. "Skills" or "Brook · achievement".
    pub place: String,
    pub before: String,
    pub after: String,
    /// A caution the user should read before accepting.
    pub warning: String,
    #[ts(skip)]
    pub target: EditTarget,
    pub decision: Option<EditDecision>,
    /// What went into the profile, when accepted.
    pub final_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GapCheck {
    pub requirements: Vec<GapRequirement>,
    /// Requirements come from the AI check rather than the skills dictionary.
    pub from_ai: bool,
    /// The AI check was made before the profile last changed.
    pub ai_stale: bool,
    /// The resume as hiring software reads it: the text extracted from the file.
    pub resume_lines: Vec<String>,
    pub checks: Vec<BasicCheck>,
    pub edits: Vec<SuggestedEdit>,
    /// AI rewording suggestions have been asked for this job.
    pub ai_suggested: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export)]
pub enum EditAction {
    Accept { text: String },
    Reject,
    Undo,
}

// ---- Requirements ----

fn from_analysis(a: &JobAnalysis) -> Vec<GapRequirement> {
    a.requirements
        .iter()
        .map(|r| GapRequirement {
            text: r.text.clone(),
            required: r.required,
            coverage: match r.status {
                RequirementStatus::Matched => Coverage::Covered,
                RequirementStatus::WordedDifferently => Coverage::Partly,
                RequirementStatus::Missing => Coverage::Missing,
            },
            evidence: r.evidence.clone(),
            note: r.note.clone(),
        })
        .collect()
}

fn from_rules(b: &MatchBreakdown, lines: &[String]) -> Vec<GapRequirement> {
    let mut out = Vec::new();
    if let Some(req) = b.years_required {
        let (coverage, note) = match b.years_on_profile {
            Some(have) if have >= req => (
                Coverage::Covered,
                format!("Your roles add up to about {have} years."),
            ),
            Some(have) if have + 1 >= req => (
                Coverage::Partly,
                format!("Your roles add up to about {have} years; close."),
            ),
            Some(have) => (
                Coverage::Missing,
                format!("Your roles add up to about {have} years."),
            ),
            None => (
                Coverage::Partly,
                "We couldn’t read years from your role dates.".into(),
            ),
        };
        out.push(GapRequirement {
            text: format!("{req}+ years of experience"),
            required: true,
            coverage,
            evidence: Vec::new(),
            note,
        });
    }
    for s in &b.matched_skills {
        let text = s
            .lines
            .first()
            .and_then(|l| lines.get(*l as usize))
            .cloned()
            .unwrap_or_else(|| format!("Skill: {}", s.name));
        out.push(GapRequirement {
            text: s.name.clone(),
            required: false,
            coverage: Coverage::Covered,
            evidence: vec![Evidence {
                id: format!("skill:{}", s.name),
                text,
                lines: s.lines.clone(),
            }],
            note: String::new(),
        });
    }
    for s in &b.missing_skills {
        out.push(GapRequirement {
            text: s.clone(),
            required: false,
            coverage: Coverage::Missing,
            evidence: Vec::new(),
            note: String::new(),
        });
    }
    out
}

// ---- Basic checks ----

static NUMERIC_DATE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?-u:\b)(?:0?[1-9]|1[0-2])[/.\-](?:19|20)\d{2}(?-u:\b)").unwrap()
});
static MONTH_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)(?-u:\b)(?:jan|feb|mar|apr|may|jun|jul|aug|sep|sept|oct|nov|dec)[a-z]*\.?(?-u:\b)",
    )
    .unwrap()
});
static YEAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?-u:\b)(?:19[5-9]\d|20\d{2})(?-u:\b)").unwrap());

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DateStyle {
    MonthName,
    Numeric,
    YearOnly,
}

fn date_style(dates: &str) -> Option<DateStyle> {
    if NUMERIC_DATE.is_match(dates) {
        Some(DateStyle::Numeric)
    } else if MONTH_NAME.is_match(dates) && YEAR.is_match(dates) {
        Some(DateStyle::MonthName)
    } else if YEAR.is_match(dates) {
        Some(DateStyle::YearOnly)
    } else {
        None
    }
}

pub fn basic_checks(p: &Profile, lines: &[String]) -> Vec<BasicCheck> {
    let mut out = Vec::new();
    let c = &p.contact;
    let (email, phone) = (!c.email.trim().is_empty(), !c.phone.trim().is_empty());
    out.push(BasicCheck {
        id: "contact".into(),
        ok: email && phone,
        label: match (email, phone) {
            (true, true) => "Email and phone found",
            (true, false) => "No phone number found",
            (false, true) => "No email found",
            (false, false) => "No email or phone found",
        }
        .into(),
        note: if c.name.trim().is_empty() {
            "Your name wasn’t found either. Put it at the top of the resume.".into()
        } else if email && phone {
            String::new()
        } else {
            "Most application forms read these from the top of your resume.".into()
        },
    });

    if lines.is_empty() {
        out.push(BasicCheck {
            id: "headings".into(),
            ok: false,
            label: "No resume file to check".into(),
            note: "Upload your resume to check its headings.".into(),
        });
    } else {
        let mut found: Vec<&str> = Vec::new();
        for l in lines {
            if let Some(h) = section_heading(l)
                && h != "Other"
                && !found.contains(&h)
            {
                found.push(h);
            }
        }
        let missing: Vec<&str> = ["Experience", "Skills", "Education"]
            .into_iter()
            .filter(|h| !found.contains(h))
            .collect();
        out.push(BasicCheck {
            id: "headings".into(),
            ok: missing.is_empty(),
            label: if missing.is_empty() {
                "Clear section headings".into()
            } else {
                format!("No “{}” heading found", missing.join("”, “"))
            },
            note: if missing.is_empty() {
                found.join(" · ")
            } else {
                "Hiring software sorts your resume by its headings. Use plain ones like “Experience”."
                    .into()
            },
        });
    }

    let dated: Vec<_> = p
        .roles
        .iter()
        .filter(|r| !r.dates.trim().is_empty())
        .collect();
    let undated: Vec<String> = p
        .roles
        .iter()
        .filter(|r| r.dates.trim().is_empty() || date_style(&r.dates).is_none())
        .map(|r| {
            if r.company.is_empty() {
                r.title.clone()
            } else {
                r.company.clone()
            }
        })
        .collect();
    let mut styles: Vec<(DateStyle, &str)> = Vec::new();
    for r in &dated {
        if let Some(s) = date_style(&r.dates)
            && !styles.iter().any(|(x, _)| *x == s)
        {
            styles.push((s, r.dates.trim()));
        }
    }
    out.push(if p.roles.is_empty() {
        BasicCheck {
            id: "dates".into(),
            ok: false,
            label: "No roles found".into(),
            note: "Add your work history on the Resume page.".into(),
        }
    } else if !undated.is_empty() {
        BasicCheck {
            id: "dates".into(),
            ok: false,
            label: "Some roles have no readable dates".into(),
            note: format!("Add dates for: {}.", undated.join(", ")),
        }
    } else if styles.len() > 1 {
        BasicCheck {
            id: "dates".into(),
            ok: false,
            label: "Dates use different formats".into(),
            note: format!("“{}” vs “{}”. Pick one format.", styles[0].1, styles[1].1),
        }
    } else {
        BasicCheck {
            id: "dates".into(),
            ok: true,
            label: "Dates are consistent".into(),
            note: String::new(),
        }
    });
    out
}

// ---- Suggested edits ----

/// Skills the job asks for that appear in the resume text but not in the skills list.
pub fn rule_suggestions(
    p: &Profile,
    lines: &[String],
    job_skills: &[String],
) -> Vec<SuggestedEdit> {
    let have: Vec<String> = p.skills.iter().map(|s| normalize_phrase(&s.name)).collect();
    let line_phrases: Vec<Phrases> = lines.iter().map(|l| Phrases::new(l)).collect();
    let mut out = Vec::new();
    for name in job_skills {
        let key = normalize_phrase(name);
        if key.is_empty() || have.contains(&key) {
            continue;
        }
        let found: Vec<u32> = line_phrases
            .iter()
            .enumerate()
            .filter(|(_, ph)| ph.contains(&key))
            .map(|(i, _)| i as u32)
            .collect();
        let Some(first) = found.first() else {
            continue;
        };
        out.push(SuggestedEdit {
            id: format!("skill:{key}"),
            source: EditSource::Rules,
            why: format!(
                "The job asks for {name}, and your resume mentions it, but it isn’t in your skills list."
            ),
            place: "Skills".into(),
            before: format!("Mentioned in: “{}”", lines[*first as usize]),
            after: name.clone(),
            warning: String::new(),
            target: EditTarget::AddSkill { lines: found },
            decision: None,
            final_text: String::new(),
        });
    }
    out
}

/// Profile text the AI may suggest rewording: (id, where, text).
pub fn rewritable(p: &Profile) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    if !p.summary.trim().is_empty() {
        out.push(("summary".into(), "Summary".into(), p.summary.clone()));
    }
    for r in &p.roles {
        let place = if r.company.is_empty() {
            r.title.clone()
        } else {
            r.company.clone()
        };
        for a in &r.achievements {
            if !a.id.is_empty() && !a.text.trim().is_empty() {
                out.push((
                    a.id.clone(),
                    format!("{place} · achievement"),
                    a.text.clone(),
                ));
            }
        }
    }
    for pr in &p.projects {
        if !pr.id.is_empty() && !pr.description.trim().is_empty() {
            out.push((
                pr.id.clone(),
                format!("{} · project", pr.name),
                pr.description.clone(),
            ));
        }
    }
    out
}

fn item_text(p: &Profile, id: &str) -> Option<String> {
    rewritable(p)
        .into_iter()
        .find(|(i, _, _)| i == id)
        .map(|(_, _, t)| t)
}

fn set_item_text(p: &mut Profile, id: &str, text: &str) -> bool {
    if id == "summary" {
        p.summary = text.into();
        return true;
    }
    for r in &mut p.roles {
        if let Some(a) = r.achievements.iter_mut().find(|a| a.id == id) {
            a.text = text.into();
            return true;
        }
    }
    if let Some(pr) = p.projects.iter_mut().find(|pr| pr.id == id) {
        pr.description = text.into();
        return true;
    }
    false
}

/// Applies an accepted edit; returns the ID of a profile item it created.
fn apply(p: &mut Profile, e: &SuggestedEdit, text: &str) -> Result<Option<String>, String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("The edit is empty.".into());
    }
    match &e.target {
        EditTarget::AddSkill { lines } => {
            let key = normalize_phrase(text);
            if p.skills.iter().any(|s| normalize_phrase(&s.name) == key) {
                return Err(format!("{text} is already in your skills."));
            }
            let mut n = p.skills.len();
            let id = loop {
                let id = format!("s-edit-{n}");
                if !p.skills.iter().any(|s| s.id == id) {
                    break id;
                }
                n += 1;
            };
            p.skills.push(Skill {
                id: id.clone(),
                name: text.into(),
                lines: lines.clone(),
            });
            Ok(Some(id))
        }
        EditTarget::Rewrite { item_id } => {
            if item_text(p, item_id).as_deref() != Some(e.before.as_str()) {
                return Err("That line has changed since this was suggested.".into());
            }
            set_item_text(p, item_id, text);
            Ok(None)
        }
    }
}

fn revert(
    p: &mut Profile,
    e: &SuggestedEdit,
    final_text: &str,
    applied_id: Option<&str>,
) -> Result<(), String> {
    match &e.target {
        EditTarget::AddSkill { .. } => {
            if let Some(id) = applied_id {
                p.skills.retain(|s| s.id != id);
            }
            Ok(())
        }
        EditTarget::Rewrite { item_id } => match item_text(p, item_id) {
            Some(now) if now == final_text => {
                set_item_text(p, item_id, &e.before);
                Ok(())
            }
            // Already back, or the item was deleted: nothing to undo in the profile.
            Some(now) if now == e.before => Ok(()),
            None => Ok(()),
            Some(_) => Err(
                "That line was changed after you accepted this. Edit it on the Resume page.".into(),
            ),
        },
    }
}

// ---- AI rewording ----

const MAX_AI_EDITS: usize = 5;

pub const SUGGEST_PROMPT: &str = "You help a candidate tailor their resume to a job without lying. \
Suggest rewrites of the candidate's existing lines (given with IDs in square brackets) so they use the job's wording \
where the line already shows that experience. Never add tools, technologies, numbers, results, employers, scope or \
responsibilities the original line doesn't state. Leading with the result or naming the skill the job uses for \
something the line already describes is fine. Skip lines that don't need changes. Keep each rewrite under 30 words \
and each reason under 15 words.";

pub fn suggest_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["edits"],
        "properties": {
            "edits": {
                "type": "array",
                "maxItems": MAX_AI_EDITS,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["id", "rewrite", "reason"],
                    "properties": {
                        "id": { "type": "string" },
                        "rewrite": { "type": "string" },
                        "reason": { "type": "string" }
                    }
                }
            }
        }
    })
}

/// Words that look like names of tools or products: capitalized mid-sentence, or with
/// digits or symbols (AWS, Node.js, C#).
fn proper_terms(text: &str) -> Vec<String> {
    text.split_whitespace()
        .enumerate()
        .filter_map(|(i, w)| {
            let w = w.trim_matches(|c: char| !c.is_alphanumeric() && c != '#' && c != '+');
            let first = w.chars().next()?;
            let techy = w
                .chars()
                .any(|c| c.is_ascii_digit() || matches!(c, '.' | '#' | '+'))
                && w.chars().any(char::is_alphabetic);
            ((i > 0 && first.is_uppercase()) || techy).then(|| w.to_string())
        })
        .collect()
}

/// Checks the AI's suggestions against the profile. Unknown IDs reject the answer; a
/// rewrite that adds numbers is dropped; one that adds names of tools gets a warning.
pub fn validate_suggestions(answer: &Value, p: &Profile) -> Result<Vec<SuggestedEdit>, String> {
    #[derive(Deserialize)]
    struct Raw {
        edits: Vec<RawEdit>,
    }
    #[derive(Deserialize)]
    struct RawEdit {
        id: String,
        rewrite: String,
        reason: String,
    }
    let raw: Raw = serde_json::from_value(answer.clone())
        .map_err(|e| format!("The answer didn’t match the expected shape: {e}"))?;
    let items = rewritable(p);
    let mut out = Vec::new();
    for (n, e) in raw.edits.into_iter().take(MAX_AI_EDITS).enumerate() {
        let id = e.id.trim().trim_matches(['[', ']']);
        let Some((id, place, before)) = items.iter().find(|(i, _, _)| i == id) else {
            return Err(format!(
                "It suggested an edit to a line that doesn’t exist: {id}."
            ));
        };
        let after = e.rewrite.trim().to_string();
        if after.is_empty() || after == before.trim() || after.chars().count() > 400 {
            continue;
        }
        if !new_numbers(&after, &[before], &[]).is_empty() {
            continue;
        }
        let before_terms = proper_terms(before).join(" ").to_lowercase();
        let added: Vec<String> = proper_terms(&after)
            .into_iter()
            .filter(|t| !before_terms.contains(&t.to_lowercase()))
            .collect();
        out.push(SuggestedEdit {
            id: format!("ai:{n}:{id}"),
            source: EditSource::Ai,
            why: e.reason.trim().to_string(),
            place: place.clone(),
            before: before.clone(),
            after,
            warning: if added.is_empty() {
                String::new()
            } else {
                format!(
                    "Adds “{}”. Only accept if you really did this.",
                    added.join("”, “")
                )
            },
            target: EditTarget::Rewrite {
                item_id: id.clone(),
            },
            decision: None,
            final_text: String::new(),
        });
    }
    Ok(out)
}

pub fn suggest_prompt(
    title: &str,
    company: &str,
    description: &str,
    gaps: &[GapRequirement],
    p: &Profile,
) -> String {
    let description: String = description.chars().take(8_000).collect();
    let lines: Vec<String> = rewritable(p)
        .into_iter()
        .map(|(id, _, text)| format!("[{id}] {text}"))
        .collect();
    let gaps: Vec<String> = gaps
        .iter()
        .filter(|g| g.coverage != Coverage::Covered)
        .map(|g| {
            format!(
                "- {} ({})",
                g.text,
                if g.coverage == Coverage::Partly {
                    "maybe worded differently"
                } else {
                    "missing"
                }
            )
        })
        .collect();
    format!(
        "JOB\nTitle: {title}\nCompany: {company}\n\n{description}\n\nGAPS FOUND\n{}\n\nCANDIDATE LINES\n{}\n",
        if gaps.is_empty() {
            "(none listed)".into()
        } else {
            gaps.join("\n")
        },
        lines.join("\n")
    )
}

/// Asks the provider for rewordings, retrying once when the answer is rejected.
pub fn suggest(
    provider: Provider,
    prompt: &str,
    p: &Profile,
    cwd: &Path,
    cancel: &AtomicBool,
) -> Result<Vec<SuggestedEdit>, String> {
    if provider != Provider::Claude {
        return Err(format!("{} isn’t supported yet.", provider.label()));
    }
    let schema = suggest_schema();
    let mut input = prompt.to_string();
    let mut last = String::new();
    for _ in 0..2 {
        let answer = claude::ask(claude::MODEL, SUGGEST_PROMPT, &input, &schema, cwd, cancel)?;
        match validate_suggestions(&answer, p) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = e;
                input = format!(
                    "{prompt}\nYour previous answer was rejected: {last} Only use IDs from CANDIDATE LINES."
                );
            }
        }
    }
    Err(format!(
        "Claude’s suggestions couldn’t be trusted, so they weren’t used. {last}"
    ))
}

pub fn save_suggestions(conn: &Connection, job_id: i64, edits: &[SuggestedEdit]) -> AppResult<()> {
    conn.execute(
        "INSERT INTO edit_suggestions (job_id, provider, model, data) VALUES (?1, 'claude', ?2, ?3)
         ON CONFLICT (job_id) DO UPDATE SET provider = excluded.provider, model = excluded.model,
            data = excluded.data, created_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![
            job_id,
            claude::MODEL,
            serde_json::to_string(edits).unwrap_or_default()
        ],
    )?;
    Ok(())
}

fn load_suggestions(conn: &Connection, job_id: i64) -> AppResult<Option<Vec<SuggestedEdit>>> {
    let data: Option<String> = conn
        .query_row(
            "SELECT data FROM edit_suggestions WHERE job_id = ?1",
            [job_id],
            |r| r.get(0),
        )
        .optional()?;
    Ok(data.and_then(|d| serde_json::from_str(&d).ok()))
}

// ---- Putting it together ----

struct Decided {
    edit: SuggestedEdit,
    final_text: String,
    applied_id: Option<String>,
}

fn decisions(conn: &Connection, job_id: i64) -> AppResult<Vec<Decided>> {
    let mut stmt = conn.prepare(
        "SELECT data, decision, final_text, applied_id FROM resume_edits
         WHERE job_id = ?1 ORDER BY created_at, rowid",
    )?;
    let rows = stmt.query_map([job_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (data, decision, final_text, applied_id) = row?;
        let Ok(mut edit) = serde_json::from_str::<SuggestedEdit>(&data) else {
            continue;
        };
        edit.decision = Some(if decision == "accepted" {
            EditDecision::Accepted
        } else {
            EditDecision::Rejected
        });
        edit.final_text = final_text.clone();
        out.push(Decided {
            edit,
            final_text,
            applied_id,
        });
    }
    Ok(out)
}

/// The gap check for a job, against the current profile.
pub fn check(conn: &Connection, job_id: i64) -> AppResult<GapCheck> {
    let detail = feed::job_detail(conn, job_id)?;
    let stored = store::get_profile(conn)?;
    let (profile, lines) = stored
        .map(|s| (s.profile, s.resume_lines))
        .unwrap_or_default();

    let (requirements, from_ai) = match &detail.analysis {
        Some(a) => (from_analysis(a), true),
        None => (from_rules(&detail.breakdown, &lines), false),
    };

    let decided = decisions(conn, job_id)?;
    let cached = load_suggestions(conn, job_id)?;
    let mut pending = rule_suggestions(&profile, &lines, &detail.breakdown.missing_skills);
    if let Some(ai) = &cached {
        // Hide rewordings of lines that have changed since.
        pending.extend(
            ai.iter()
                .filter(|e| match &e.target {
                    EditTarget::Rewrite { item_id } => {
                        item_text(&profile, item_id).as_deref() == Some(e.before.as_str())
                    }
                    EditTarget::AddSkill { .. } => true,
                })
                .cloned(),
        );
    }
    let mut edits: Vec<SuggestedEdit> = pending
        .into_iter()
        .filter(|e| !decided.iter().any(|d| d.edit.id == e.id))
        .collect();
    edits.extend(decided.into_iter().map(|d| d.edit));

    Ok(GapCheck {
        requirements,
        from_ai,
        ai_stale: detail.analysis_stale,
        checks: basic_checks(&profile, &lines),
        resume_lines: lines,
        edits,
        ai_suggested: cached.is_some(),
    })
}

/// Accepts, rejects or undoes a suggested edit. Accepting changes the profile.
pub fn decide(conn: &Connection, job_id: i64, edit_id: &str, action: EditAction) -> AppResult<()> {
    let mut profile = store::get_profile(conn)?
        .map(|s| s.profile)
        .unwrap_or_default();
    let fail = |m: String| AppError::Other(m);
    match action {
        EditAction::Undo => {
            let Some(d) = decisions(conn, job_id)?
                .into_iter()
                .find(|d| d.edit.id == edit_id)
            else {
                return Ok(());
            };
            if d.edit.decision == Some(EditDecision::Accepted) {
                revert(
                    &mut profile,
                    &d.edit,
                    &d.final_text,
                    d.applied_id.as_deref(),
                )
                .map_err(fail)?;
                store::save_profile(conn, &profile)?;
            }
            conn.execute(
                "DELETE FROM resume_edits WHERE job_id = ?1 AND edit_id = ?2",
                params![job_id, edit_id],
            )?;
        }
        EditAction::Accept { .. } | EditAction::Reject => {
            let edit = check(conn, job_id)?
                .edits
                .into_iter()
                .find(|e| e.id == edit_id && e.decision.is_none())
                .ok_or_else(|| fail("That suggestion is no longer available.".into()))?;
            let (decision, final_text, applied_id) = match &action {
                EditAction::Accept { text } => {
                    let applied = apply(&mut profile, &edit, text).map_err(fail)?;
                    store::save_profile(conn, &profile)?;
                    ("accepted", text.trim().to_string(), applied)
                }
                _ => ("rejected", String::new(), None),
            };
            conn.execute(
                "INSERT INTO resume_edits (job_id, edit_id, data, decision, final_text, applied_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![
                    job_id,
                    edit_id,
                    serde_json::to_string(&edit).unwrap_or_default(),
                    decision,
                    final_text,
                    applied_id
                ],
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Achievement, Contact, Role};

    fn profile() -> Profile {
        Profile {
            contact: Contact {
                name: "Sam Rivera".into(),
                email: "sam@example.com".into(),
                ..Default::default()
            },
            roles: vec![
                Role {
                    id: "r1".into(),
                    title: "Senior Frontend Engineer".into(),
                    company: "Brook".into(),
                    dates: "Mar 2021 – Present".into(),
                    achievements: vec![Achievement {
                        id: "a1".into(),
                        text: "Wrote Jest tests for checkout".into(),
                        lines: vec![5],
                    }],
                    ..Default::default()
                },
                Role {
                    id: "r2".into(),
                    title: "Frontend Engineer".into(),
                    company: "Kite".into(),
                    dates: "2018–2021".into(),
                    ..Default::default()
                },
            ],
            skills: vec![Skill {
                id: "s1".into(),
                name: "React".into(),
                lines: vec![9],
            }],
            ..Default::default()
        }
    }

    fn lines() -> Vec<String> {
        [
            "Sam Rivera",
            "sam@example.com",
            "EXPERIENCE",
            "Senior Frontend Engineer, Brook",
            "Mar 2021 – Present",
            "Wrote Jest tests for checkout in TypeScript",
            "SKILLS",
            "",
            "",
            "React",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn basic_checks_report_what_is_missing() {
        let checks = basic_checks(&profile(), &lines());
        let get = |id: &str| checks.iter().find(|c| c.id == id).unwrap();
        assert!(!get("contact").ok);
        assert_eq!(get("contact").label, "No phone number found");
        assert!(!get("headings").ok);
        assert!(
            get("headings").label.contains("Education"),
            "{}",
            get("headings").label
        );
        assert!(!get("dates").ok);
        assert!(
            get("dates").note.contains("Mar 2021 – Present"),
            "{}",
            get("dates").note
        );
    }

    #[test]
    fn suggests_skills_the_resume_already_mentions() {
        let s = rule_suggestions(
            &profile(),
            &lines(),
            &["TypeScript".into(), "React".into(), "GraphQL".into()],
        );
        assert_eq!(s.len(), 1);
        assert_eq!(s[0].after, "TypeScript");
        assert_eq!(s[0].target, EditTarget::AddSkill { lines: vec![5] });
    }

    #[test]
    fn rules_requirements_cover_years_and_skills() {
        let b = MatchBreakdown {
            years_required: Some(5),
            years_on_profile: Some(8),
            matched_skills: vec![crate::jobs::MatchedSkill {
                name: "React".into(),
                lines: vec![9],
            }],
            missing_skills: vec!["GraphQL".into()],
            ..Default::default()
        };
        let r = from_rules(&b, &lines());
        assert_eq!(r[0].coverage, Coverage::Covered);
        assert_eq!(r[1].evidence[0].text, "React");
        assert_eq!(r[2].coverage, Coverage::Missing);
    }

    #[test]
    fn ai_suggestions_are_checked() {
        let p = profile();
        let ok = json!({ "edits": [
            { "id": "a1", "rewrite": "Wrote Jest unit tests for checkout", "reason": "Uses “unit tests”" },
            { "id": "a1", "rewrite": "Wrote 300 Jest tests for checkout", "reason": "adds a number" },
            { "id": "a1", "rewrite": "Wrote Playwright and Jest tests for checkout", "reason": "x" }
        ]});
        let v = validate_suggestions(&ok, &p).unwrap();
        assert_eq!(v.len(), 2, "the one adding a number is dropped");
        assert!(v[0].warning.is_empty());
        assert!(v[1].warning.contains("Playwright"), "{}", v[1].warning);
        let bad = json!({ "edits": [{ "id": "zz", "rewrite": "x", "reason": "y" }] });
        assert!(validate_suggestions(&bad, &p).is_err());
    }

    fn db_with_job() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO jobs (canonical_key, title, company, description) VALUES ('k', 'Frontend Engineer', 'Acme', 'We use React and TypeScript daily.')",
            [],
        )
        .unwrap();
        store::save_import(&conn, &profile(), &lines(), "cv.pdf", 10, Some(1)).unwrap();
        (dir, conn)
    }

    #[test]
    fn accept_and_undo_change_the_profile() {
        let (_dir, conn) = db_with_job();
        let g = check(&conn, 1).unwrap();
        let e = g
            .edits
            .iter()
            .find(|e| e.after == "TypeScript")
            .expect("suggested")
            .clone();
        decide(
            &conn,
            1,
            &e.id,
            EditAction::Accept {
                text: "TypeScript".into(),
            },
        )
        .unwrap();
        let p = store::get_profile(&conn).unwrap().unwrap().profile;
        let added = p.skills.iter().find(|s| s.name == "TypeScript").unwrap();
        assert_eq!(added.lines, vec![5]);
        // Still listed, as accepted, though the rule no longer fires.
        let g = check(&conn, 1).unwrap();
        let shown = g.edits.iter().find(|x| x.id == e.id).unwrap();
        assert_eq!(shown.decision, Some(EditDecision::Accepted));

        decide(&conn, 1, &e.id, EditAction::Undo).unwrap();
        let p = store::get_profile(&conn).unwrap().unwrap().profile;
        assert!(!p.skills.iter().any(|s| s.name == "TypeScript"));
        assert_eq!(
            check(&conn, 1)
                .unwrap()
                .edits
                .iter()
                .find(|x| x.id == e.id)
                .unwrap()
                .decision,
            None
        );
    }

    #[test]
    fn accepted_rewrite_can_be_undone_unless_changed_again() {
        let (_dir, conn) = db_with_job();
        let ai = validate_suggestions(
            &json!({ "edits": [{ "id": "a1", "rewrite": "Wrote Jest unit tests for checkout", "reason": "r" }] }),
            &profile(),
        )
        .unwrap();
        save_suggestions(&conn, 1, &ai).unwrap();
        let id = ai[0].id.clone();
        decide(
            &conn,
            1,
            &id,
            EditAction::Accept {
                text: "Wrote Jest unit tests for the checkout".into(),
            },
        )
        .unwrap();
        let p = store::get_profile(&conn).unwrap().unwrap().profile;
        assert_eq!(
            p.roles[0].achievements[0].text,
            "Wrote Jest unit tests for the checkout"
        );
        decide(&conn, 1, &id, EditAction::Undo).unwrap();
        let mut p = store::get_profile(&conn).unwrap().unwrap().profile;
        assert_eq!(
            p.roles[0].achievements[0].text,
            "Wrote Jest tests for checkout"
        );

        decide(
            &conn,
            1,
            &id,
            EditAction::Accept {
                text: "Wrote Jest unit tests".into(),
            },
        )
        .unwrap();
        p.roles[0].achievements[0].text = "Something else".into();
        store::save_profile(&conn, &p).unwrap();
        assert!(decide(&conn, 1, &id, EditAction::Undo).is_err());
    }
}

/// Real Claude call. `cargo test live_suggest -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_suggest() {
        let body = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/sources/remotive.json"
        ))
        .unwrap();
        let job = &crate::jobs::sources::parse_remotive(&body).unwrap()[0];
        let p = crate::apply::writer::tests_profile();
        let prompt = suggest_prompt(&job.title, &job.company, &job.description, &[], &p);
        let dir = tempfile::tempdir().unwrap();
        let start = std::time::Instant::now();
        let edits = suggest(
            Provider::Claude,
            &prompt,
            &p,
            dir.path(),
            &AtomicBool::new(false),
        )
        .unwrap();
        println!("{:?}", start.elapsed());
        for e in edits {
            println!(
                "  {} → {}\n    why: {} {}",
                e.before, e.after, e.why, e.warning
            );
        }
    }
}
