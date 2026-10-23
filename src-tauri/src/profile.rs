//! Profile and search settings shapes, shared with the UI through ts-rs.
//!
//! Every profile item keeps `lines`: indexes into the stored resume lines it was read
//! from. Later phases cite these as evidence, so parsing must never invent them.
//! Items added by hand in the UI have no lines.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Profile {
    pub contact: Contact,
    pub summary: String,
    pub summary_lines: Vec<u32>,
    pub roles: Vec<Role>,
    pub skills: Vec<Skill>,
    pub education: Vec<Education>,
    pub projects: Vec<Project>,
    pub languages: Vec<Language>,
    pub certifications: Vec<Certification>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Contact {
    pub name: String,
    pub headline: String,
    pub email: String,
    pub phone: String,
    pub location: String,
    pub website: String,
    pub github: String,
    pub linkedin: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Role {
    pub id: String,
    pub title: String,
    pub company: String,
    /// Free text as written on the resume, e.g. "Mar 2021 – Present".
    pub dates: String,
    /// User pressed "Looks right" on a date warning.
    pub dates_confirmed: bool,
    pub achievements: Vec<Achievement>,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Achievement {
    pub id: String,
    pub text: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Skill {
    pub id: String,
    pub name: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Education {
    pub id: String,
    pub school: String,
    pub degree: String,
    pub dates: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub link: String,
    pub description: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Language {
    pub id: String,
    pub name: String,
    /// "Native" | "Fluent" | "Professional" | "Basic", or empty when the resume doesn't say.
    pub level: String,
    pub lines: Vec<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct Certification {
    pub id: String,
    pub name: String,
    pub issuer: String,
    pub year: String,
    pub lines: Vec<u32>,
}

/// The uploaded resume file the profile was read from.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ResumeMeta {
    pub file_name: String,
    #[ts(type = "number")]
    pub size_bytes: u64,
    pub pages: Option<u32>,
    pub imported_at: String,
}

/// What the Resume view needs: the editable profile plus the resume lines it cites.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StoredProfile {
    pub profile: Profile,
    pub resume: Option<ResumeMeta>,
    pub resume_lines: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum Seniority {
    Junior,
    Mid,
    Senior,
    Staff,
    Lead,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct SearchSettings {
    pub titles: Vec<String>,
    pub must_have: Vec<String>,
    pub avoid: Vec<String>,
    /// ISO 3166-1 alpha-2 code, or empty when not set.
    pub country: String,
    /// Fixed offset like "UTC+05:30", or empty when not set.
    pub timezone: String,
    pub min_salary: Option<u32>,
    pub currency: String,
    pub seniority: Vec<Seniority>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum SetupStatus {
    Pending,
    Done,
    Skipped,
}
