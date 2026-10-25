//! Application writer: a cover letter or short email for one saved job.
//!
//! Drafts are built from the profile, by templates (always available) or by the AI
//! provider (when on). Either way each sentence goes through the honesty check in
//! [`super::Support::sentence`]: sentences about the user must cite profile items that
//! back them, or they're flagged until the user confirms or removes them. Export is
//! blocked while any are flagged.

use std::{collections::HashSet, path::Path, sync::atomic::AtomicBool};

use chrono::Datelike;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use ts_rs::TS;

use super::{Backing, Sentence, Support, split_sentences};
use crate::{
    ai::{
        Provider,
        analysis::{self, Evidence},
        claude,
    },
    db::store,
    error::{AppError, AppResult},
    jobs::{JobState, feed, score, text::words},
    profile::{Profile, Role},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DraftKind {
    CoverLetter,
    Email,
}

impl DraftKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::CoverLetter => "cover_letter",
            Self::Email => "email",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::CoverLetter => "Cover letter",
            Self::Email => "Email",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DraftLength {
    Short,
    Standard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum DraftSource {
    Rules,
    Ai,
    /// Typed by the user.
    Edited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ParagraphRole {
    Intro,
    Highlights,
    Closing,
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ParagraphVersion {
    pub sentences: Vec<Sentence>,
    pub source: DraftSource,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Paragraph {
    pub role: ParagraphRole,
    /// Every wording so far; earlier drafts are kept.
    pub versions: Vec<ParagraphVersion>,
    pub current: u32,
}

impl Paragraph {
    fn new(role: ParagraphRole, v: ParagraphVersion) -> Self {
        Self {
            role,
            versions: vec![v],
            current: 0,
        }
    }

    pub fn shown(&self) -> &ParagraphVersion {
        &self.versions[(self.current as usize).min(self.versions.len() - 1)]
    }

    fn shown_mut(&mut self) -> &mut ParagraphVersion {
        let i = (self.current as usize).min(self.versions.len() - 1);
        &mut self.versions[i]
    }

    fn push(&mut self, v: ParagraphVersion) {
        self.versions.push(v);
        self.current = (self.versions.len() - 1) as u32;
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Draft {
    pub kind: DraftKind,
    pub length: DraftLength,
    /// Profile item IDs the draft features.
    pub highlights: Vec<String>,
    /// Email subject line; empty for letters.
    pub subject: String,
    pub greeting: String,
    pub paragraphs: Vec<Paragraph>,
    pub sign_off: String,
    pub source: DraftSource,
    pub updated_at: String,
}

impl Draft {
    /// Sentences still waiting for the user to confirm or remove them.
    pub fn open_flags(&self) -> usize {
        self.paragraphs
            .iter()
            .flat_map(|p| &p.shown().sentences)
            .filter(|s| s.backing == Backing::Unbacked)
            .count()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct HighlightOption {
    pub id: String,
    pub label: String,
    /// Role and company, or "Project".
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WriterState {
    pub draft: Option<Draft>,
    pub options: Vec<HighlightOption>,
    /// The options that fit this job best, for a first draft.
    pub suggested: Vec<String>,
}

/// Everything about the job and the user a draft is built from.
pub struct Context {
    pub title: String,
    pub company: String,
    pub description: String,
    pub profile: Profile,
    pub years: Option<u32>,
    /// Profile skills the job mentions, with the profile item behind each.
    pub matched_skills: Vec<(String, Option<Evidence>)>,
    pub support: Support,
}

impl Context {
    pub fn load(conn: &Connection, job_id: i64) -> AppResult<Self> {
        let detail = feed::job_detail(conn, job_id)?;
        let profile = store::get_profile(conn)?
            .map(|p| p.profile)
            .unwrap_or_default();
        Ok(Self::new(
            detail.job.title,
            detail.job.company,
            detail.description,
            profile,
            &detail
                .breakdown
                .matched_skills
                .iter()
                .map(|s| s.name.clone())
                .collect::<Vec<_>>(),
        ))
    }

    pub fn new(
        title: String,
        company: String,
        description: String,
        profile: Profile,
        matched: &[String],
    ) -> Self {
        let items = analysis::profile_items(&profile);
        let years = years_on_profile(&profile);
        let support = Support {
            skills: profile.skills.iter().map(|s| s.name.clone()).collect(),
            job_words: vec![title.clone(), company.clone()],
            allowed_numbers: years.into_iter().collect(),
            items,
        };
        let matched_skills = matched
            .iter()
            .map(|name| {
                let ev = profile
                    .skills
                    .iter()
                    .find(|s| s.name.eq_ignore_ascii_case(name))
                    .and_then(|s| support.evidence(&s.id));
                (name.clone(), ev)
            })
            .collect();
        Self {
            title,
            company,
            description,
            profile,
            years,
            matched_skills,
            support,
        }
    }

    fn name(&self) -> String {
        let n = self.profile.contact.name.trim();
        if n.is_empty() {
            "[your name]".into()
        } else {
            n.into()
        }
    }
}

fn years_on_profile(p: &Profile) -> Option<u32> {
    score::years_on_profile(p, chrono::Utc::now().year()).filter(|y| *y >= 1)
}

/// "A CLI that…" → "a CLI that…" mid-sentence. Only common lead words are lowered, so
/// names like "React" stay as written.
fn soften(text: &str) -> String {
    const LEAD: &[&str] = &[
        "a",
        "an",
        "the",
        "responsible",
        "worked",
        "working",
        "helped",
        "helping",
        "open-source",
        "personal",
        "internal",
        "small",
        "simple",
    ];
    let first = text.split_whitespace().next().unwrap_or("");
    if !LEAD.contains(&first.to_lowercase().as_str()) {
        return text.to_string();
    }
    let mut c = text.chars();
    c.next()
        .map(|f| f.to_lowercase().chain(c).collect())
        .unwrap_or_default()
}

// ---- Highlights ----

pub fn options(p: &Profile) -> Vec<HighlightOption> {
    let mut out = Vec::new();
    for r in &p.roles {
        let detail = match (r.title.is_empty(), r.company.is_empty()) {
            (false, false) => format!("{} · {}", r.title, r.company),
            (false, true) => r.title.clone(),
            _ => r.company.clone(),
        };
        for a in &r.achievements {
            if !a.id.is_empty() && !a.text.trim().is_empty() {
                out.push(HighlightOption {
                    id: a.id.clone(),
                    label: a.text.trim().to_string(),
                    detail: detail.clone(),
                });
            }
        }
    }
    for pr in &p.projects {
        if !pr.id.is_empty() && !pr.name.trim().is_empty() {
            out.push(HighlightOption {
                id: pr.id.clone(),
                label: if pr.description.trim().is_empty() {
                    pr.name.clone()
                } else {
                    format!("{} — {}", pr.name, pr.description.trim())
                },
                detail: "Project".into(),
            });
        }
    }
    out
}

/// The three options sharing the most words with the job.
pub fn suggested(opts: &[HighlightOption], job_text: &str) -> Vec<String> {
    let job: HashSet<String> = words(job_text)
        .into_iter()
        .filter(|w| w.chars().count() >= 3)
        .collect();
    let mut scored: Vec<(usize, usize, &HighlightOption)> = opts
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let own: HashSet<String> = words(&o.label).into_iter().collect();
            (own.intersection(&job).count(), i, o)
        })
        .collect();
    // Most overlap first; ties keep resume order (newest roles first).
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored
        .into_iter()
        .take(3)
        .map(|(_, _, o)| o.id.clone())
        .collect()
}

// ---- Templates ----

const PAST_VERBS: &[&str] = &[
    "built", "led", "ran", "wrote", "made", "grew", "cut", "won", "set", "drove", "took", "gave",
    "rebuilt", "rewrote", "began", "brought", "taught", "sold", "kept", "found", "held", "spoke",
    "shipped", "own", "owned",
];

/// "Led the TypeScript migration." → Some("led the TypeScript migration").
fn as_clause(text: &str) -> Option<String> {
    let body = text.trim().trim_end_matches(['.', ';', ',']).trim();
    let first = body.split_whitespace().next()?;
    let lower = first.to_lowercase();
    let verb = (lower.ends_with("ed") && lower.len() > 3) || PAST_VERBS.contains(&lower.as_str());
    if !verb {
        return None;
    }
    // Keep acronyms and names as written.
    let rest: String = first.chars().skip(1).collect();
    if rest.chars().any(char::is_uppercase) {
        return None;
    }
    Some(format!("{lower}{}", &body[first.len()..]))
}

fn place(r: &Role) -> Option<&str> {
    if !r.company.trim().is_empty() {
        Some(r.company.trim())
    } else {
        None
    }
}

fn achievement_sentence(r: &Role, text: &str, variant: usize) -> String {
    let body = soften(text.trim().trim_end_matches(['.', ';']));
    match (place(r), as_clause(text)) {
        (Some(c), Some(clause)) => match variant % 3 {
            0 => format!("At {c}, I {clause}."),
            1 if !r.title.is_empty() => format!("As {} at {c}, I {clause}.", r.title),
            1 => format!("While at {c}, I {clause}."),
            _ => format!("While at {c}, I {clause}."),
        },
        (None, Some(clause)) => match variant % 3 {
            0 => format!("In a previous role, I {clause}."),
            1 => format!("In my work so far, I {clause}."),
            _ => format!("One thing I’ve done: I {clause}."),
        },
        (Some(c), None) => match variant % 3 {
            0 => format!("One example from {c}: {body}."),
            1 => format!("From my time at {c}: {body}."),
            _ => format!("A highlight from {c}: {body}."),
        },
        (None, None) => match variant % 3 {
            0 => format!("One example from my work: {body}."),
            1 => format!("A highlight from my work: {body}."),
            _ => format!("Another example: {body}."),
        },
    }
}

fn project_sentence(name: &str, desc: &str, variant: usize) -> String {
    let desc = soften(desc.trim().trim_end_matches('.'));
    match (desc.is_empty(), variant % 3) {
        (true, 0) => format!("One relevant project is {name}."),
        (true, 1) => format!("I’d also point to my project {name}."),
        (true, _) => format!("Another example is my project {name}."),
        (false, 0) => format!("One relevant project is {name}: {desc}."),
        (false, 1) => format!("I’d also point to {name}: {desc}."),
        (false, _) => format!("Another example is {name}: {desc}."),
    }
}

fn list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [a] => a.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Number of wordings each templated paragraph has.
const VARIANTS: usize = 3;

fn intro(ctx: &Context, variant: usize) -> ParagraphVersion {
    let (t, c) = (&ctx.title, &ctx.company);
    let mut out = vec![ctx.support.sentence(
        &match variant % VARIANTS {
            0 => format!("I’m applying for the {t} role at {c}."),
            1 => format!("I’d like to be considered for the {t} role at {c}."),
            _ => format!("I’m writing about the {t} opening at {c}."),
        },
        vec![],
    )];
    if let Some(r) = ctx
        .profile
        .roles
        .first()
        .filter(|r| !r.title.trim().is_empty())
    {
        let at = place(r).map(|c| format!(" at {c}")).unwrap_or_default();
        let rt = r.title.trim();
        let text = match ctx.years {
            Some(n) => {
                let yrs = if n == 1 {
                    "1 year".into()
                } else {
                    format!("{n} years")
                };
                match variant % VARIANTS {
                    0 => format!("I bring about {yrs} of experience, most recently as {rt}{at}."),
                    1 => format!(
                        "Most recently I’ve worked as {rt}{at}, with about {yrs} of experience in all."
                    ),
                    _ => format!("I have about {yrs} of experience; my latest role is {rt}{at}."),
                }
            }
            None => match variant % VARIANTS {
                0 => format!("Most recently, I’ve worked as {rt}{at}."),
                1 => format!("My latest role is {rt}{at}."),
                _ => format!("I currently or most recently worked as {rt}{at}."),
            },
        };
        let mut ev: Vec<Evidence> = Vec::new();
        if let Some(e) = ctx.support.evidence(&r.id) {
            ev.push(e);
        }
        if ctx.years.is_some() {
            // The years come from every dated role.
            for other in ctx.profile.roles.iter().skip(1) {
                if !other.dates.trim().is_empty()
                    && let Some(e) = ctx.support.evidence(&other.id)
                {
                    ev.push(e);
                }
            }
        }
        if !ev.is_empty() {
            out.push(ctx.support.sentence(&text, ev));
        }
    }
    ParagraphVersion {
        sentences: out,
        source: DraftSource::Rules,
    }
}

fn highlights(
    ctx: &Context,
    kind: DraftKind,
    length: DraftLength,
    ids: &[String],
    variant: usize,
) -> ParagraphVersion {
    let max = if kind == DraftKind::Email || length == DraftLength::Short {
        2
    } else {
        3
    };
    let mut out = Vec::new();
    for id in ids.iter().take(max) {
        let Some(ev) = ctx.support.evidence(id) else {
            continue;
        };
        let p = &ctx.profile;
        let text = if let Some((r, a)) = p
            .roles
            .iter()
            .find_map(|r| r.achievements.iter().find(|a| &a.id == id).map(|a| (r, a)))
        {
            let mut ev = vec![ev];
            if let Some(role) = ctx.support.evidence(&r.id) {
                ev.push(role);
            }
            out.push(
                ctx.support
                    .sentence(&achievement_sentence(r, &a.text, variant), ev),
            );
            continue;
        } else if let Some(pr) = p.projects.iter().find(|pr| &pr.id == id) {
            project_sentence(&pr.name, &pr.description, variant)
        } else {
            continue;
        };
        out.push(ctx.support.sentence(&text, vec![ev]));
    }
    if kind == DraftKind::CoverLetter && length == DraftLength::Standard {
        let backed: Vec<&(String, Option<Evidence>)> = ctx
            .matched_skills
            .iter()
            .filter(|(_, e)| e.is_some())
            .take(4)
            .collect();
        if backed.len() >= 2 {
            let names: Vec<String> = backed.iter().map(|(n, _)| n.clone()).collect();
            let ev = backed.iter().filter_map(|(_, e)| e.clone()).collect();
            let l = list(&names);
            let text = match variant % VARIANTS {
                0 => format!("I work with {l}, which the role asks for."),
                1 => format!("The role calls for {l}, and those are part of my everyday work."),
                _ => format!("Day to day, I work with {l}."),
            };
            out.push(ctx.support.sentence(&text, ev));
        }
    }
    ParagraphVersion {
        sentences: out,
        source: DraftSource::Rules,
    }
}

fn closing(ctx: &Context, kind: DraftKind, variant: usize) -> ParagraphVersion {
    let c = &ctx.company;
    let text = match (kind, variant % VARIANTS) {
        (DraftKind::CoverLetter, 0) => {
            format!(
                "I’d welcome the chance to talk about how I could help {c}. Thank you for your time."
            )
        }
        (DraftKind::CoverLetter, 1) => {
            "If this sounds like a fit, I’d be glad to talk. Thanks for considering my application."
                .into()
        }
        (DraftKind::CoverLetter, _) => {
            "I’d be happy to share more in a call. Thank you for reading.".into()
        }
        (DraftKind::Email, 0) => {
            "My resume is attached. I’d be glad to talk whenever suits you.".into()
        }
        (DraftKind::Email, 1) => "I’ve attached my resume and would be happy to talk.".into(),
        (DraftKind::Email, _) => "Resume attached. Happy to talk if it sounds like a fit.".into(),
    };
    ParagraphVersion {
        sentences: split_sentences(&text)
            .iter()
            .map(|s| ctx.support.sentence(s, vec![]))
            .collect(),
        source: DraftSource::Rules,
    }
}

fn templated(
    ctx: &Context,
    role: ParagraphRole,
    kind: DraftKind,
    length: DraftLength,
    ids: &[String],
    variant: usize,
) -> Option<ParagraphVersion> {
    Some(match role {
        ParagraphRole::Intro => intro(ctx, variant),
        ParagraphRole::Highlights => highlights(ctx, kind, length, ids, variant),
        ParagraphRole::Closing => closing(ctx, kind, variant),
        ParagraphRole::Other => return None,
    })
}

fn subject(ctx: &Context) -> String {
    let n = ctx.profile.contact.name.trim();
    if n.is_empty() {
        format!("Application: {}", ctx.title)
    } else {
        format!("Application: {} — {n}", ctx.title)
    }
}

/// A draft from templates. Works with AI off.
pub fn from_rules(ctx: &Context, kind: DraftKind, length: DraftLength, ids: &[String]) -> Draft {
    let paragraphs = [
        ParagraphRole::Intro,
        ParagraphRole::Highlights,
        ParagraphRole::Closing,
    ]
    .into_iter()
    .filter_map(|role| templated(ctx, role, kind, length, ids, 0).map(|v| Paragraph::new(role, v)))
    .filter(|p| !p.shown().sentences.is_empty())
    .collect();
    Draft {
        kind,
        length,
        highlights: ids.to_vec(),
        subject: if kind == DraftKind::Email {
            subject(ctx)
        } else {
            String::new()
        },
        greeting: match kind {
            DraftKind::CoverLetter => "Dear [hiring manager name],".into(),
            DraftKind::Email => "Hi [hiring manager name],".into(),
        },
        paragraphs,
        sign_off: format!("Best,\n{}", ctx.name()),
        source: DraftSource::Rules,
        updated_at: crate::jobs::dates::now(),
    }
}

/// Next template wording for one paragraph not already among its drafts.
pub fn rewrite_from_rules(ctx: &Context, d: &mut Draft, index: usize) -> Result<(), String> {
    let (kind, length, ids) = (d.kind, d.length, d.highlights.clone());
    let p = d
        .paragraphs
        .get_mut(index)
        .ok_or("That paragraph no longer exists.")?;
    let texts: Vec<String> = p.versions.iter().map(version_text).collect();
    for step in 1..=VARIANTS {
        let variant = (p.versions.len() - 1 + step) % VARIANTS;
        let Some(v) = templated(ctx, p.role, kind, length, &ids, variant) else {
            break;
        };
        if !texts.contains(&version_text(&v)) {
            p.push(v);
            return Ok(());
        }
    }
    Err(
        "No other wording left for this paragraph. Edit it yourself, or turn on AI for more."
            .into(),
    )
}

fn version_text(v: &ParagraphVersion) -> String {
    v.sentences
        .iter()
        .map(|s| s.text.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

// ---- User edits ----

/// Replaces the shown paragraph with text the user typed, as a new draft. Sentences
/// kept from before keep their evidence (and the user's confirmations); new ones are
/// matched against the profile.
pub fn edit_paragraph(
    ctx: &Context,
    d: &mut Draft,
    index: usize,
    text: &str,
) -> Result<(), String> {
    let p = d
        .paragraphs
        .get_mut(index)
        .ok_or("That paragraph no longer exists.")?;
    let known: Vec<Sentence> = p
        .versions
        .iter()
        .flat_map(|v| v.sentences.clone())
        .collect();
    let sentences: Vec<Sentence> = split_sentences(text)
        .into_iter()
        .map(|s| {
            known
                .iter()
                .rev()
                .find(|k| k.text == s)
                .cloned()
                .unwrap_or_else(|| ctx.support.sentence(&s, ctx.support.find_evidence(&s)))
        })
        .collect();
    if sentences.is_empty() {
        d.paragraphs.remove(index);
    } else {
        p.push(ParagraphVersion {
            sentences,
            source: DraftSource::Edited,
        });
    }
    Ok(())
}

/// "It's true — keep" or "Remove" for a flagged sentence.
pub fn resolve(d: &mut Draft, para: usize, sentence: usize, keep: bool) -> Result<(), String> {
    let p = d
        .paragraphs
        .get_mut(para)
        .ok_or("That paragraph no longer exists.")?;
    let v = p.shown_mut();
    let s = v
        .sentences
        .get_mut(sentence)
        .ok_or("That sentence no longer exists.")?;
    if keep {
        s.backing = Backing::Confirmed;
        s.note = "You confirmed this is true.".into();
    } else {
        v.sentences.remove(sentence);
        if v.sentences.is_empty() && p.versions.len() == 1 {
            d.paragraphs.remove(para);
        }
    }
    Ok(())
}

// ---- AI ----

pub const WRITE_PROMPT: &str = "You write job application text (cover letters and short emails) for a candidate, using only facts from their profile. \
Every sentence that says anything about the candidate (experience, skills, tools, results, numbers, years, titles, education) must cite the IDs, \
from the square brackets, of the profile items that support it, and must not go beyond what those items say. Never invent numbers, tools, \
employers, results or responsibilities. Sentences that make no claim about the candidate (why they're writing, interest in the role, closing) \
cite nothing. Don't state facts about the company beyond the job text. Never invent names: greet [hiring manager name] unless the posting names \
the person. Write plainly and specifically in the first person. No clichés (passionate, synergy, rockstar, thrilled), no flattery, \
no exclamation marks. Don't write a sign-off or the candidate's name. One sentence per array item.";

fn sentences_schema() -> Value {
    json!({
        "type": "array",
        "maxItems": 8,
        "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["text", "evidence"],
            "properties": {
                "text": { "type": "string" },
                "evidence": { "type": "array", "items": { "type": "string" } }
            }
        }
    })
}

pub fn draft_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["subject", "greeting", "paragraphs"],
        "properties": {
            "subject": { "type": "string", "description": "Email subject line; empty for a cover letter." },
            "greeting": { "type": "string" },
            "paragraphs": {
                "type": "array",
                "maxItems": 6,
                "items": {
                    "type": "object",
                    "additionalProperties": false,
                    "required": ["role", "sentences"],
                    "properties": {
                        "role": { "type": "string", "enum": ["intro", "highlights", "closing", "other"] },
                        "sentences": sentences_schema()
                    }
                }
            }
        }
    })
}

pub fn paragraph_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["sentences"],
        "properties": { "sentences": sentences_schema() }
    })
}

#[derive(Deserialize)]
struct RawSentence {
    text: String,
    evidence: Vec<String>,
}

#[derive(Deserialize)]
struct RawParagraph {
    role: ParagraphRole,
    sentences: Vec<RawSentence>,
}

#[derive(Deserialize)]
struct RawDraft {
    subject: String,
    greeting: String,
    paragraphs: Vec<RawParagraph>,
}

#[derive(Deserialize)]
struct RawRewrite {
    sentences: Vec<RawSentence>,
}

/// Resolves citations and runs the honesty check. Any made-up ID rejects the answer.
fn check_sentences(ctx: &Context, raw: Vec<RawSentence>) -> Result<Vec<Sentence>, String> {
    let mut out = Vec::new();
    let mut unknown = Vec::new();
    for s in raw {
        if s.text.trim().is_empty() {
            continue;
        }
        let mut ev: Vec<Evidence> = Vec::new();
        for id in &s.evidence {
            let id = id.trim().trim_matches(['[', ']']);
            match ctx.support.evidence(id) {
                Some(e) if !ev.iter().any(|x| x.id == e.id) => ev.push(e),
                Some(_) => {}
                None => unknown.push(id.to_string()),
            }
        }
        out.push(ctx.support.sentence(&s.text, ev));
    }
    if !unknown.is_empty() {
        unknown.sort();
        unknown.dedup();
        return Err(format!(
            "It cited profile items that don’t exist: {}.",
            unknown.join(", ")
        ));
    }
    Ok(out)
}

pub fn validate_draft(
    ctx: &Context,
    answer: &Value,
    kind: DraftKind,
    length: DraftLength,
    ids: &[String],
) -> Result<Draft, String> {
    let raw: RawDraft = serde_json::from_value(answer.clone())
        .map_err(|e| format!("The answer didn’t match the expected shape: {e}"))?;
    let mut paragraphs = Vec::new();
    for p in raw.paragraphs {
        let sentences = check_sentences(ctx, p.sentences)?;
        if !sentences.is_empty() {
            paragraphs.push(Paragraph::new(
                p.role,
                ParagraphVersion {
                    sentences,
                    source: DraftSource::Ai,
                },
            ));
        }
    }
    if paragraphs.is_empty() {
        return Err("The draft was empty.".into());
    }
    let mut d = from_rules(ctx, kind, length, ids);
    d.paragraphs = paragraphs;
    d.source = DraftSource::Ai;
    if !raw.greeting.trim().is_empty() {
        d.greeting = raw.greeting.trim().to_string();
    }
    if kind == DraftKind::Email && !raw.subject.trim().is_empty() {
        d.subject = raw.subject.trim().to_string();
    }
    Ok(d)
}

fn job_and_profile(ctx: &Context) -> String {
    let description: String = ctx.description.chars().take(8_000).collect();
    let profile: Vec<String> = ctx
        .support
        .items
        .iter()
        .map(|i| format!("[{}] {}", i.id, i.text))
        .collect();
    let years = ctx
        .years
        .map(|y| format!("\nThe candidate's roles add up to about {y} years (you may say this, citing the roles).\n"))
        .unwrap_or_default();
    format!(
        "JOB\nTitle: {}\nCompany: {}\n\n{description}\n\nCANDIDATE PROFILE\n{}\n{years}",
        ctx.title,
        ctx.company,
        profile.join("\n")
    )
}

pub fn draft_prompt(ctx: &Context, kind: DraftKind, length: DraftLength, ids: &[String]) -> String {
    let what = match (kind, length) {
        (DraftKind::Email, _) => {
            "a short application email (about 100 words, 2–3 short paragraphs) with a subject line"
        }
        (DraftKind::CoverLetter, DraftLength::Short) => {
            "a short cover letter (about 150 words, 3 paragraphs)"
        }
        (DraftKind::CoverLetter, DraftLength::Standard) => {
            "a cover letter (about 250–300 words, 3–4 paragraphs)"
        }
    };
    let featured: Vec<String> = ids
        .iter()
        .filter_map(|id| ctx.support.item(id))
        .map(|i| format!("[{}] {}", i.id, i.text))
        .collect();
    format!(
        "{}\nFEATURE THESE (build the middle of the text around them)\n{}\n\nWRITE {what}.\n",
        job_and_profile(ctx),
        if featured.is_empty() {
            "(your choice)".into()
        } else {
            featured.join("\n")
        },
    )
}

pub fn rewrite_prompt(ctx: &Context, d: &Draft, index: usize) -> String {
    let current: Vec<String> = d
        .paragraphs
        .iter()
        .enumerate()
        .map(|(i, p)| format!("({}) {}", i + 1, version_text(p.shown())))
        .collect();
    let earlier: Vec<String> = d.paragraphs[index]
        .versions
        .iter()
        .map(|v| format!("- {}", version_text(v)))
        .collect();
    format!(
        "{}\nCURRENT DRAFT\n{}\n\nEARLIER WORDINGS OF PARAGRAPH {n}\n{}\n\nRewrite paragraph {n} with different wording from all of the above. \
         It must still fit between the paragraphs around it. Same rules: cite the profile items behind every claim.\n",
        job_and_profile(ctx),
        current.join("\n"),
        earlier.join("\n"),
        n = index + 1,
    )
}

/// Asks the provider, retrying once with the reason when the answer is rejected.
fn ask_checked<T>(
    provider: Provider,
    prompt: &str,
    schema: &Value,
    cwd: &Path,
    cancel: &AtomicBool,
    check: impl Fn(&Value) -> Result<T, String>,
) -> Result<T, String> {
    if provider != Provider::Claude {
        return Err(format!("{} isn’t supported yet.", provider.label()));
    }
    let mut input = prompt.to_string();
    let mut last = String::new();
    for _ in 0..2 {
        let answer = claude::ask(
            claude::WRITING_MODEL,
            WRITE_PROMPT,
            &input,
            schema,
            cwd,
            cancel,
        )?;
        match check(&answer) {
            Ok(v) => return Ok(v),
            Err(e) => {
                last = e;
                input = format!(
                    "{prompt}\nYour previous answer was rejected: {last} Only cite IDs from the CANDIDATE PROFILE list."
                );
            }
        }
    }
    Err(format!(
        "Claude’s draft couldn’t be trusted, so it wasn’t used. {last}"
    ))
}

pub fn from_ai(
    provider: Provider,
    ctx: &Context,
    kind: DraftKind,
    length: DraftLength,
    ids: &[String],
    cwd: &Path,
    cancel: &AtomicBool,
) -> Result<Draft, String> {
    let prompt = draft_prompt(ctx, kind, length, ids);
    ask_checked(provider, &prompt, &draft_schema(), cwd, cancel, |a| {
        validate_draft(ctx, a, kind, length, ids)
    })
}

pub fn rewrite_with_ai(
    provider: Provider,
    ctx: &Context,
    d: &mut Draft,
    index: usize,
    cwd: &Path,
    cancel: &AtomicBool,
) -> Result<(), String> {
    if index >= d.paragraphs.len() {
        return Err("That paragraph no longer exists.".into());
    }
    let prompt = rewrite_prompt(ctx, d, index);
    let sentences = ask_checked(provider, &prompt, &paragraph_schema(), cwd, cancel, |a| {
        let raw: RawRewrite = serde_json::from_value(a.clone())
            .map_err(|e| format!("The answer didn’t match the expected shape: {e}"))?;
        let s = check_sentences(ctx, raw.sentences)?;
        if s.is_empty() {
            Err("The paragraph was empty.".into())
        } else {
            Ok(s)
        }
    })?;
    d.paragraphs[index].push(ParagraphVersion {
        sentences,
        source: DraftSource::Ai,
    });
    Ok(())
}

// ---- Storage ----

/// The working draft is the newest unsent row for the job and kind.
pub fn load(conn: &Connection, job_id: i64, kind: DraftKind) -> AppResult<Option<Draft>> {
    let content: Option<String> = conn
        .query_row(
            "SELECT content FROM drafts WHERE job_id = ?1 AND kind = ?2 AND sent = 0 ORDER BY id DESC LIMIT 1",
            params![job_id, kind.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    Ok(content.and_then(|c| serde_json::from_str(&c).ok()))
}

pub fn save(conn: &Connection, job_id: i64, d: &mut Draft) -> AppResult<()> {
    d.updated_at = crate::jobs::dates::now();
    let content = serde_json::to_string(d).map_err(|e| AppError::Other(e.to_string()))?;
    let id: Option<i64> = conn
        .query_row(
            "SELECT id FROM drafts WHERE job_id = ?1 AND kind = ?2 AND sent = 0 ORDER BY id DESC LIMIT 1",
            params![job_id, d.kind.as_str()],
            |r| r.get(0),
        )
        .optional()?;
    match id {
        Some(id) => conn.execute(
            "UPDATE drafts SET content = ?2 WHERE id = ?1",
            params![id, content],
        )?,
        None => conn.execute(
            "INSERT INTO drafts (job_id, kind, content) VALUES (?1, ?2, ?3)",
            params![job_id, d.kind.as_str(), content],
        )?,
    };
    Ok(())
}

/// The writer opens once a job is saved, so drafts go only to jobs the user wants.
pub fn require_saved(conn: &Connection, job_id: i64) -> AppResult<()> {
    let state: Option<String> = conn
        .query_row("SELECT state FROM jobs WHERE id = ?1", [job_id], |r| {
            r.get(0)
        })
        .optional()?;
    match state.as_deref().map(JobState::parse) {
        Some(JobState::Saved) => Ok(()),
        Some(_) => Err(AppError::Other(
            "Save this job first to write an application.".into(),
        )),
        None => Err(AppError::Other(
            "That job is no longer in your feed.".into(),
        )),
    }
}

pub fn state(conn: &Connection, job_id: i64, kind: DraftKind) -> AppResult<WriterState> {
    let ctx = Context::load(conn, job_id)?;
    let opts = options(&ctx.profile);
    let suggested = suggested(&opts, &format!("{}\n{}", ctx.title, ctx.description));
    Ok(WriterState {
        draft: load(conn, job_id, kind)?,
        options: opts,
        suggested,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Achievement, Contact, Project, Skill};

    fn profile() -> Profile {
        Profile {
            contact: Contact {
                name: "Sam Rivera".into(),
                ..Default::default()
            },
            roles: vec![
                Role {
                    id: "r1".into(),
                    title: "Senior Frontend Engineer".into(),
                    company: "Brook".into(),
                    dates: "2021 – Present".into(),
                    achievements: vec![
                        Achievement {
                            id: "a1".into(),
                            text: "Led the TypeScript migration of a 200k-line React app.".into(),
                            lines: vec![4],
                        },
                        Achievement {
                            id: "a2".into(),
                            text: "Responsible for accessibility audits".into(),
                            lines: vec![5],
                        },
                    ],
                    lines: vec![3],
                    ..Default::default()
                },
                Role {
                    id: "r2".into(),
                    title: "Frontend Engineer".into(),
                    company: "Kite".into(),
                    dates: "2018 – 2021".into(),
                    lines: vec![7],
                    ..Default::default()
                },
            ],
            skills: ["React", "TypeScript", "GraphQL"]
                .iter()
                .enumerate()
                .map(|(i, s)| Skill {
                    id: format!("s{i}"),
                    name: (*s).into(),
                    lines: vec![10],
                })
                .collect(),
            projects: vec![Project {
                id: "p1".into(),
                name: "Tidy".into(),
                description: "A CLI that formats CSV files".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    fn ctx() -> Context {
        Context::new(
            "Lead Frontend Engineer".into(),
            "Acme".into(),
            "We need React and TypeScript. You will lead a migration.".into(),
            profile(),
            &["React".into(), "TypeScript".into()],
        )
    }

    fn texts(d: &Draft) -> Vec<String> {
        d.paragraphs
            .iter()
            .flat_map(|p| p.shown().sentences.iter().map(|s| s.text.clone()))
            .collect()
    }

    #[test]
    fn template_drafts_are_fully_backed() {
        let c = ctx();
        for kind in [DraftKind::CoverLetter, DraftKind::Email] {
            for length in [DraftLength::Short, DraftLength::Standard] {
                let d = from_rules(&c, kind, length, &["a1".into(), "a2".into(), "p1".into()]);
                for p in &d.paragraphs {
                    for s in &p.shown().sentences {
                        assert_ne!(
                            s.backing,
                            Backing::Unbacked,
                            "{:?} {:?}: {}",
                            kind,
                            length,
                            s.text
                        );
                    }
                }
                assert_eq!(d.open_flags(), 0);
            }
        }
        let d = from_rules(
            &c,
            DraftKind::CoverLetter,
            DraftLength::Standard,
            &["a1".into(), "a2".into()],
        );
        let t = texts(&d);
        assert!(
            t.contains(
                &"At Brook, I led the TypeScript migration of a 200k-line React app.".to_string()
            ),
            "{t:?}"
        );
        assert!(
            t.contains(
                &"One example from Brook: responsible for accessibility audits.".to_string()
            ),
            "{t:?}"
        );
        assert!(
            t.iter().any(|s| s.contains("about 8 years")
                || s.contains("about 5 years")
                || s.contains("years of experience")),
            "{t:?}"
        );
        assert!(
            t.contains(&"I work with React and TypeScript, which the role asks for.".to_string()),
            "{t:?}"
        );
        assert_eq!(d.greeting, "Dear [hiring manager name],");
        assert_eq!(d.sign_off, "Best,\nSam Rivera");
    }

    #[test]
    fn rewrite_keeps_earlier_drafts_and_runs_out() {
        let c = ctx();
        let mut d = from_rules(
            &c,
            DraftKind::CoverLetter,
            DraftLength::Short,
            &["a1".into()],
        );
        let first = version_text(d.paragraphs[0].shown());
        rewrite_from_rules(&c, &mut d, 0).unwrap();
        rewrite_from_rules(&c, &mut d, 0).unwrap();
        assert_eq!(d.paragraphs[0].versions.len(), 3);
        assert_eq!(d.paragraphs[0].current, 2);
        assert_eq!(version_text(&d.paragraphs[0].versions[0]), first);
        assert!(rewrite_from_rules(&c, &mut d, 0).is_err());
    }

    #[test]
    fn typed_sentences_are_checked() {
        let c = ctx();
        let mut d = from_rules(
            &c,
            DraftKind::CoverLetter,
            DraftLength::Short,
            &["a1".into()],
        );
        let kept = d.paragraphs[1].shown().sentences[0].text.clone();
        edit_paragraph(
            &c,
            &mut d,
            1,
            &format!("{kept} I also ran the Kubernetes platform for 5 years."),
        )
        .unwrap();
        let s = &d.paragraphs[1].shown().sentences;
        assert_eq!(s[0].backing, Backing::Backed);
        assert_eq!(s[1].backing, Backing::Unbacked);
        assert_eq!(d.open_flags(), 1);
        assert_eq!(d.paragraphs[1].versions.len(), 2);

        resolve(&mut d, 1, 1, true).unwrap();
        assert_eq!(d.open_flags(), 0);
        assert_eq!(
            d.paragraphs[1].shown().sentences[1].backing,
            Backing::Confirmed
        );
    }

    #[test]
    fn remove_drops_the_sentence() {
        let c = ctx();
        let mut d = from_rules(&c, DraftKind::Email, DraftLength::Short, &[]);
        edit_paragraph(&c, &mut d, 0, "I have run 3 marathons.").unwrap();
        assert_eq!(d.open_flags(), 1);
        resolve(&mut d, 0, 0, false).unwrap();
        assert_eq!(d.open_flags(), 0);
    }

    #[test]
    fn ai_drafts_are_checked() {
        let c = ctx();
        let answer = json!({
            "subject": "",
            "greeting": "Dear [hiring manager name],",
            "paragraphs": [
                { "role": "intro", "sentences": [
                    { "text": "I’m applying for the Lead Frontend Engineer role at Acme.", "evidence": [] },
                    { "text": "I led the TypeScript migration of a 200k-line React app.", "evidence": ["a1"] }
                ]},
                { "role": "highlights", "sentences": [
                    { "text": "I cut build times by 50%.", "evidence": ["a1"] },
                    { "text": "I have deep Kubernetes experience.", "evidence": [] }
                ]}
            ]
        });
        let d =
            validate_draft(&c, &answer, DraftKind::CoverLetter, DraftLength::Short, &[]).unwrap();
        let s: Vec<Backing> = d
            .paragraphs
            .iter()
            .flat_map(|p| p.shown().sentences.iter().map(|s| s.backing))
            .collect();
        assert_eq!(
            s,
            [
                Backing::Plain,
                Backing::Backed,
                Backing::Unbacked,
                Backing::Unbacked
            ]
        );
        assert_eq!(d.source, DraftSource::Ai);

        let made_up = json!({ "subject": "", "greeting": "", "paragraphs": [
            { "role": "intro", "sentences": [{ "text": "x", "evidence": ["zz9"] }] }
        ]});
        assert!(
            validate_draft(
                &c,
                &made_up,
                DraftKind::CoverLetter,
                DraftLength::Short,
                &[]
            )
            .is_err()
        );
    }

    #[test]
    fn suggests_highlights_matching_the_job() {
        let opts = options(&profile());
        assert_eq!(opts.len(), 3);
        let s = suggested(&opts, "We need someone who led a TypeScript migration.");
        assert_eq!(s[0], "a1");
    }

    #[test]
    fn stores_the_working_draft() {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO jobs (canonical_key, title, company) VALUES ('k', 'T', 'C')",
            [],
        )
        .unwrap();
        assert!(require_saved(&conn, 1).is_err());
        conn.execute("UPDATE jobs SET state = 'saved'", []).unwrap();
        require_saved(&conn, 1).unwrap();

        let mut d = from_rules(&ctx(), DraftKind::Email, DraftLength::Short, &[]);
        save(&conn, 1, &mut d).unwrap();
        d.greeting = "Hi Ana,".into();
        save(&conn, 1, &mut d).unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM drafts", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        assert_eq!(
            load(&conn, 1, DraftKind::Email).unwrap().unwrap().greeting,
            "Hi Ana,"
        );
        assert!(load(&conn, 1, DraftKind::CoverLetter).unwrap().is_none());
    }
}

/// Real Claude calls. `cargo test live_writer -- --ignored --nocapture`
#[cfg(test)]
mod live {
    use super::*;

    #[test]
    #[ignore]
    fn live_writer() {
        let body = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/sources/remotive.json"
        ))
        .unwrap();
        let job = &crate::jobs::sources::parse_remotive(&body).unwrap()[0];
        let profile = crate::apply::writer::tests_profile();
        let ctx = Context::new(
            job.title.clone(),
            job.company.clone(),
            job.description.clone(),
            profile,
            &[],
        );
        let dir = tempfile::tempdir().unwrap();
        let never = AtomicBool::new(false);
        let start = std::time::Instant::now();
        let mut d = from_ai(
            Provider::Claude,
            &ctx,
            DraftKind::CoverLetter,
            DraftLength::Short,
            &["a1".into()],
            dir.path(),
            &never,
        )
        .unwrap();
        println!("{:?}\n{}\n", start.elapsed(), d.greeting);
        let show = |d: &Draft| {
            for p in &d.paragraphs {
                for s in &p.shown().sentences {
                    println!(
                        "  [{:?}] {} ← {:?} {}",
                        s.backing,
                        s.text,
                        s.evidence.iter().map(|e| &e.id).collect::<Vec<_>>(),
                        s.note
                    );
                }
                println!();
            }
        };
        show(&d);
        let start = std::time::Instant::now();
        rewrite_with_ai(Provider::Claude, &ctx, &mut d, 0, dir.path(), &never).unwrap();
        println!("rewrite {:?}", start.elapsed());
        show(&d);
    }
}

#[cfg(test)]
pub(crate) fn tests_profile() -> Profile {
    use crate::profile::{Achievement, Skill};
    Profile {
        contact: crate::profile::Contact {
            name: "Sam Rivera".into(),
            headline: "Frontend Engineer".into(),
            ..Default::default()
        },
        roles: vec![Role {
            id: "r1".into(),
            title: "Senior Frontend Engineer".into(),
            company: "Brook".into(),
            dates: "2019 – Present".into(),
            achievements: vec![
                Achievement {
                    id: "a1".into(),
                    text: "Led the TypeScript migration of a 200k-line React app.".into(),
                    lines: vec![4],
                },
                Achievement {
                    id: "a2".into(),
                    text: "Cut checkout load time by 40%.".into(),
                    lines: vec![5],
                },
            ],
            lines: vec![3],
            ..Default::default()
        }],
        skills: ["React", "TypeScript", "Node.js"]
            .iter()
            .enumerate()
            .map(|(i, s)| Skill {
                id: format!("s{i}"),
                name: (*s).into(),
                lines: vec![10],
            })
            .collect(),
        ..Default::default()
    }
}
