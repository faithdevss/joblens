//! Job sources, normalization and the rules-only job feed.
//!
//! Sources produce [`RawJob`]s. They are stored once per real-world job (dedupe), and
//! eligibility, keyword filters and the match score are worked out when the feed is
//! read, so changing search settings or the profile applies right away.

pub mod dates;
pub mod dedupe;
pub mod eligibility;
pub mod feed;
pub mod geo;
pub mod refresh;
pub mod salary;
pub mod score;
pub mod sources;
pub mod still_open;
pub mod text;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// One posting as a source returns it, already converted to plain text.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RawJob {
    /// Stable ID within the source.
    pub external_id: String,
    pub title: String,
    pub company: String,
    /// Link to the posting on the source (for attribution and "open original").
    pub url: String,
    pub description: String,
    /// The source's location field as written, e.g. "USA, Canada" or "Anywhere in the World".
    pub location: String,
    pub salary: Option<Salary>,
    /// RFC 3339 UTC.
    pub posted_at: Option<String>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SalaryPeriod {
    Year,
    Month,
    Hour,
}

impl SalaryPeriod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Year => "year",
            Self::Month => "month",
            Self::Hour => "hour",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "year" => Some(Self::Year),
            "month" => Some(Self::Month),
            "hour" => Some(Self::Hour),
            _ => None,
        }
    }
}

/// Salary as stated. Numbers are only set when the text could be read with confidence.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Salary {
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub currency: Option<String>,
    pub period: Option<SalaryPeriod>,
    /// What the posting said, for display.
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum EligibilityStatus {
    Eligible,
    Restricted,
    NotStated,
}

/// The "Can I apply?" badge and the sentence that explains it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Eligibility {
    pub status: EligibilityStatus,
    pub note: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum JobState {
    New,
    Saved,
    Dismissed,
}

impl JobState {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Saved => "saved",
            Self::Dismissed => "dismissed",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "saved" => Self::Saved,
            "dismissed" => Self::Dismissed,
            _ => Self::New,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SourceKind {
    Board,
    Watchlist,
    Manual,
}

impl SourceKind {
    pub fn parse(s: &str) -> Self {
        match s {
            "watchlist" => Self::Watchlist,
            "manual" => Self::Manual,
            _ => Self::Board,
        }
    }
}

/// A source shown in Settings → Job sources.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceStatus {
    pub id: String,
    pub kind: SourceKind,
    pub name: String,
    pub enabled: bool,
    /// Last fetch attempt, RFC 3339.
    pub last_run_at: Option<String>,
    /// Last successful fetch.
    pub last_ok_at: Option<String>,
    /// Why the last fetch failed, in plain words. Cleared on success.
    pub last_error: Option<String>,
    pub job_count: u32,
    /// Where the jobs come from, e.g. "Greenhouse · gitlab". Shown under the name.
    pub detail: Option<String>,
}

/// Where a job was listed. The first one is where we found it first.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobListing {
    pub source_id: String,
    pub source_name: String,
    pub url: Option<String>,
}

/// How much each factor counts toward the match score. Any non-negative numbers;
/// the score divides by their sum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Weights {
    pub skills: u32,
    pub experience: u32,
    pub role: u32,
    pub seniority: u32,
}

impl Default for Weights {
    /// The prototype's defaults.
    fn default() -> Self {
        Self {
            skills: 40,
            experience: 25,
            role: 20,
            seniority: 15,
        }
    }
}

impl Weights {
    /// Weighted average of four 0–100 factors. All-zero weights count equally.
    pub fn score(&self, skills: u32, experience: u32, role: u32, seniority: u32) -> u32 {
        let w = if self.skills + self.experience + self.role + self.seniority == 0 {
            Self {
                skills: 1,
                experience: 1,
                role: 1,
                seniority: 1,
            }
        } else {
            *self
        };
        let total = w.skills + w.experience + w.role + w.seniority;
        let sum =
            skills * w.skills + experience * w.experience + role * w.role + seniority * w.seniority;
        (sum as f64 / total as f64).round() as u32
    }
}

/// Match breakdown, each part 0–100.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MatchBreakdown {
    pub score: u32,
    pub skills: u32,
    pub experience: u32,
    pub role: u32,
    pub seniority: u32,
    /// Profile skills the posting mentions, with the resume lines they came from.
    pub matched_skills: Vec<MatchedSkill>,
    /// Skills the posting mentions that aren't on the profile.
    pub missing_skills: Vec<String>,
    /// Years the posting asks for, if it says.
    pub years_required: Option<u32>,
    /// Years the profile shows, if role dates can be read.
    pub years_on_profile: Option<u32>,
    pub job_seniority: Option<String>,
    pub word_count: u32,
    /// Under ~150 words: the score is a rough guess.
    pub short_description: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MatchedSkill {
    pub name: String,
    pub lines: Vec<u32>,
}

/// One row in the job feed list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobSummary {
    #[ts(type = "number")]
    pub id: i64,
    pub title: String,
    pub company: String,
    pub url: Option<String>,
    pub location: Option<String>,
    pub salary: Option<Salary>,
    pub posted_at: Option<String>,
    pub first_seen_at: String,
    pub state: JobState,
    pub eligibility: Eligibility,
    pub score: u32,
    /// The score uses a cached AI analysis.
    pub ai_checked: bool,
    pub listings: Vec<JobListing>,
}

/// The selected job in the detail pane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct JobDetail {
    pub job: JobSummary,
    pub description: String,
    pub tags: Vec<String>,
    #[serde(rename = "match")]
    pub breakdown: MatchBreakdown,
    /// AI requirement analysis, when AI is on and this job was analyzed.
    pub analysis: Option<crate::ai::analysis::JobAnalysis>,
    /// The analysis was made before the profile last changed; it isn't in the score.
    pub analysis_stale: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Feed {
    pub jobs: Vec<JobSummary>,
    /// Hidden by must-have / avoid keywords.
    pub hidden_by_keywords: u32,
    /// Most recent successful fetch of any enabled source.
    pub last_updated_at: Option<String>,
    /// No enabled source has been fetched yet.
    pub never_refreshed: bool,
}

/// Outcome of one source in a refresh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SourceRun {
    pub source_id: String,
    pub name: String,
    pub outcome: RunOutcome,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "kind")]
#[ts(export)]
pub enum RunOutcome {
    Ok {
        fetched: u32,
        added: u32,
    },
    Failed {
        error: String,
    },
    /// Fetched recently; skipped to respect the source's limits.
    Skipped {
        #[serde(rename = "nextAt")]
        next_at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RefreshReport {
    pub runs: Vec<SourceRun>,
}

/// A job pasted by hand. Either a supported link, or a description with title + company.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct ManualJobInput {
    pub url: String,
    pub title: String,
    pub company: String,
    pub location: String,
    pub description: String,
}
