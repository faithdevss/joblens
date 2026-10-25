//! Application tracker: one row per job the user saved or applied to, with what was
//! sent and when to follow up.
//!
//! Saving a job starts tracking it; unsaving it stops tracking unless the user has
//! moved past "saved". Dates the user sets are plain `YYYY-MM-DD` days.

use chrono::{Days, NaiveDate};
use rusqlite::{Connection, OptionalExtension, Row, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    apply::writer::DraftKind,
    error::{AppError, AppResult},
};

/// Days after applying (or following up) until the next follow-up.
pub const FOLLOW_UP_DAYS: u64 = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export)]
pub enum AppStatus {
    Saved,
    Applied,
    Interviewing,
    Offer,
    Rejected,
    Closed,
}

impl AppStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::Applied => "applied",
            Self::Interviewing => "interviewing",
            Self::Offer => "offer",
            Self::Rejected => "rejected",
            Self::Closed => "closed",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Saved => "Saved",
            Self::Applied => "Applied",
            Self::Interviewing => "Interviewing",
            Self::Offer => "Offer",
            Self::Rejected => "Rejected",
            Self::Closed => "Closed",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "applied" => Self::Applied,
            "interviewing" => Self::Interviewing,
            "offer" => Self::Offer,
            "rejected" => Self::Rejected,
            "closed" => Self::Closed,
            _ => Self::Saved,
        }
    }

    /// Still in progress: follow-ups make sense.
    pub fn active(self) -> bool {
        matches!(self, Self::Applied | Self::Interviewing | Self::Offer)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Application {
    #[ts(type = "number")]
    pub job_id: i64,
    pub title: String,
    pub company: String,
    /// The original posting.
    pub url: Option<String>,
    pub status: AppStatus,
    pub applied_at: Option<String>,
    /// The resume file sent, e.g. "cv.pdf · read Oct 24".
    pub resume_version: Option<String>,
    /// The letter or email sent, e.g. "Cover letter · Oct 25".
    pub letter: Option<String>,
    /// Where the user applied, if not the posting (e.g. a careers portal).
    pub link: String,
    pub notes: String,
    pub follow_up_at: Option<String>,
    pub updated_at: String,
}

/// What the user can edit on an application.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ApplicationEdit {
    pub status: AppStatus,
    pub applied_at: Option<String>,
    pub link: String,
    pub notes: String,
    pub follow_up_at: Option<String>,
}

const COLUMNS: &str = "a.job_id, j.title, j.company, j.url, a.status, a.applied_at,
    a.resume_version, d.kind, d.created_at, a.link, a.notes, a.follow_up_at, a.updated_at";

fn row(r: &Row) -> rusqlite::Result<Application> {
    let kind: Option<String> = r.get(7)?;
    let sent_at: Option<String> = r.get(8)?;
    let letter = kind.map(|k| {
        let label = if k == DraftKind::Email.as_str() {
            DraftKind::Email.label()
        } else {
            DraftKind::CoverLetter.label()
        };
        match sent_at
            .as_deref()
            .and_then(|s| s.get(..10))
            .and_then(short_day)
        {
            Some(day) => format!("{label} · {day}"),
            None => label.to_string(),
        }
    });
    Ok(Application {
        job_id: r.get(0)?,
        title: r.get(1)?,
        company: r.get(2)?,
        url: r.get(3)?,
        status: AppStatus::parse(&r.get::<_, String>(4)?),
        applied_at: r.get(5)?,
        resume_version: r.get(6)?,
        letter,
        link: r.get::<_, Option<String>>(9)?.unwrap_or_default(),
        notes: r.get(10)?,
        follow_up_at: r.get(11)?,
        updated_at: r.get(12)?,
    })
}

/// "Oct 25" from "2026-10-25".
pub fn short_day(day: &str) -> Option<String> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .map(|d| d.format("%b %-d").to_string())
}

fn add_days(day: &str, n: u64) -> Option<String> {
    NaiveDate::parse_from_str(day, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_add_days(Days::new(n)))
        .map(|d| d.format("%Y-%m-%d").to_string())
}

/// Today on this computer's clock.
pub fn today() -> String {
    chrono::Local::now()
        .date_naive()
        .format("%Y-%m-%d")
        .to_string()
}

fn valid_day(day: &Option<String>) -> AppResult<Option<String>> {
    match day.as_deref().map(str::trim).filter(|d| !d.is_empty()) {
        None => Ok(None),
        Some(d) if NaiveDate::parse_from_str(d, "%Y-%m-%d").is_ok() => Ok(Some(d.to_string())),
        Some(d) => Err(AppError::Other(format!("“{d}” isn’t a date."))),
    }
}

pub fn list(conn: &Connection) -> AppResult<Vec<Application>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM applications a
         JOIN jobs j ON j.id = a.job_id
         LEFT JOIN drafts d ON d.id = a.letter_draft_id
         ORDER BY a.updated_at DESC, a.job_id DESC"
    ))?;
    let rows = stmt.query_map([], row)?;
    Ok(rows.collect::<Result<_, _>>()?)
}

pub fn get(conn: &Connection, job_id: i64) -> AppResult<Application> {
    conn.query_row(
        &format!(
            "SELECT {COLUMNS} FROM applications a
             JOIN jobs j ON j.id = a.job_id
             LEFT JOIN drafts d ON d.id = a.letter_draft_id
             WHERE a.job_id = ?1"
        ),
        [job_id],
        row,
    )
    .optional()?
    .ok_or_else(|| AppError::Other("That application is no longer tracked.".into()))
}

/// Keeps tracking in step with the job's saved state.
pub fn on_job_state(conn: &Connection, job_id: i64, saved: bool) -> AppResult<()> {
    if saved {
        conn.execute(
            "INSERT INTO applications (job_id, status) VALUES (?1, 'saved')
             ON CONFLICT (job_id) DO NOTHING",
            [job_id],
        )?;
    } else {
        conn.execute(
            "DELETE FROM applications WHERE job_id = ?1 AND status = 'saved'",
            [job_id],
        )?;
    }
    Ok(())
}

/// Freezes the newest working draft (cover letter first) as the version sent.
fn snapshot_letter(conn: &Connection, job_id: i64) -> AppResult<Option<i64>> {
    let newest: Option<(String, String)> = conn
        .query_row(
            "SELECT kind, content FROM drafts WHERE job_id = ?1 AND sent = 0
             ORDER BY kind = 'cover_letter' DESC, id DESC LIMIT 1",
            [job_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    let Some((kind, content)) = newest else {
        return Ok(None);
    };
    conn.execute(
        "INSERT INTO drafts (job_id, kind, content, sent) VALUES (?1, ?2, ?3, 1)",
        params![job_id, kind, content],
    )?;
    Ok(Some(conn.last_insert_rowid()))
}

fn resume_version(conn: &Connection) -> AppResult<Option<String>> {
    let row: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT resume_file_name, parsed_at FROM profile WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(match row {
        Some((Some(name), at)) => Some(
            match at.as_deref().and_then(|a| a.get(..10)).and_then(short_day) {
                Some(day) => format!("{name} · read {day}"),
                None => name,
            },
        ),
        _ => None,
    })
}

/// Saves the user's edits. Moving past "saved" for the first time records what was
/// sent (resume file, latest letter) and sets the applied date and a follow-up.
pub fn update(
    conn: &Connection,
    job_id: i64,
    edit: &ApplicationEdit,
    today: &str,
) -> AppResult<Application> {
    let current = get(conn, job_id)?;
    let mut applied_at = valid_day(&edit.applied_at)?;
    let mut follow_up_at = valid_day(&edit.follow_up_at)?;
    let first_sent = current.status == AppStatus::Saved && edit.status != AppStatus::Saved;
    let mut resume = current.resume_version.clone();
    let mut letter_id: Option<i64> = conn.query_row(
        "SELECT letter_draft_id FROM applications WHERE job_id = ?1",
        [job_id],
        |r| r.get(0),
    )?;
    if first_sent {
        applied_at.get_or_insert_with(|| today.to_string());
        if resume.is_none() {
            resume = resume_version(conn)?;
        }
        if letter_id.is_none() {
            letter_id = snapshot_letter(conn, job_id)?;
        }
        if follow_up_at.is_none() && edit.status.active() {
            follow_up_at = applied_at
                .as_deref()
                .and_then(|d| add_days(d, FOLLOW_UP_DAYS));
        }
    }
    // Nothing to follow up on once it's over.
    if matches!(edit.status, AppStatus::Rejected | AppStatus::Closed) {
        follow_up_at = None;
    }
    conn.execute(
        "UPDATE applications SET status = ?2, applied_at = ?3, resume_version = ?4,
            letter_draft_id = ?5, link = ?6, notes = ?7, follow_up_at = ?8,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE job_id = ?1",
        params![
            job_id,
            edit.status.as_str(),
            applied_at,
            resume,
            letter_id,
            edit.link.trim(),
            edit.notes,
            follow_up_at
        ],
    )?;
    get(conn, job_id)
}

/// "Mark applied" from the job pane: tracks the job (saving it) and marks it applied.
pub fn mark_applied(conn: &Connection, job_id: i64, today: &str) -> AppResult<Application> {
    conn.execute("UPDATE jobs SET state = 'saved' WHERE id = ?1", [job_id])?;
    on_job_state(conn, job_id, true)?;
    let a = get(conn, job_id)?;
    if a.status != AppStatus::Saved {
        return Ok(a);
    }
    update(
        conn,
        job_id,
        &ApplicationEdit {
            status: AppStatus::Applied,
            applied_at: None,
            link: a.link,
            notes: a.notes,
            follow_up_at: None,
        },
        today,
    )
}

/// Logs a follow-up in the notes and sets the next one a week out.
pub fn followed_up(conn: &Connection, job_id: i64, today: &str) -> AppResult<Application> {
    let a = get(conn, job_id)?;
    let line = format!(
        "Followed up {}.",
        short_day(today).unwrap_or_else(|| today.to_string())
    );
    let notes = if a.notes.trim().is_empty() {
        line
    } else {
        format!("{}\n{line}", a.notes.trim_end())
    };
    conn.execute(
        "UPDATE applications SET notes = ?2, follow_up_at = ?3,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
         WHERE job_id = ?1",
        params![job_id, notes, add_days(today, FOLLOW_UP_DAYS)],
    )?;
    get(conn, job_id)
}

fn csv_cell(s: &str) -> String {
    // A leading = + - @ makes spreadsheets run the cell as a formula.
    let s = if s.starts_with(['=', '+', '-', '@']) {
        format!("'{s}")
    } else {
        s.to_string()
    };
    format!("\"{}\"", s.replace('"', "\"\""))
}

pub fn csv(apps: &[Application]) -> String {
    let mut out = String::from(
        "Role,Company,Status,Applied,Resume sent,Letter sent,Follow-up,Posting,Applied at,Notes\n",
    );
    for a in apps {
        let cells = [
            a.title.as_str(),
            &a.company,
            a.status.label(),
            a.applied_at.as_deref().unwrap_or(""),
            a.resume_version.as_deref().unwrap_or(""),
            a.letter.as_deref().unwrap_or(""),
            a.follow_up_at.as_deref().unwrap_or(""),
            a.url.as_deref().unwrap_or(""),
            &a.link,
            &a.notes,
        ];
        out.push_str(
            &cells
                .iter()
                .map(|c| csv_cell(c))
                .collect::<Vec<_>>()
                .join(","),
        );
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        conn.execute(
            "INSERT INTO jobs (canonical_key, title, company, url) VALUES ('k', 'Frontend Engineer', 'Acme', 'https://acme.dev/1')",
            [],
        )
        .unwrap();
        (dir, conn)
    }

    fn edit(a: &Application, status: AppStatus) -> ApplicationEdit {
        ApplicationEdit {
            status,
            applied_at: a.applied_at.clone(),
            link: a.link.clone(),
            notes: a.notes.clone(),
            follow_up_at: a.follow_up_at.clone(),
        }
    }

    #[test]
    fn saving_a_job_tracks_it_and_unsaving_untracks_it() {
        let (_d, conn) = db();
        on_job_state(&conn, 1, true).unwrap();
        on_job_state(&conn, 1, true).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 1);
        on_job_state(&conn, 1, false).unwrap();
        assert!(list(&conn).unwrap().is_empty());
    }

    #[test]
    fn applying_records_what_was_sent() {
        let (_d, conn) = db();
        conn.execute(
            "UPDATE profile SET resume_file_name = 'cv.pdf', parsed_at = '2026-10-24T09:00:00Z'",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO profile (id, data, resume_file_name, parsed_at) VALUES (1, '{}', 'cv.pdf', '2026-10-24T09:00:00Z')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO drafts (job_id, kind, content, created_at) VALUES (1, 'cover_letter', '{\"v\":1}', '2026-10-25T08:00:00Z')",
            [],
        )
        .unwrap();
        let a = mark_applied(&conn, 1, "2026-10-25").unwrap();
        assert_eq!(a.status, AppStatus::Applied);
        assert_eq!(a.applied_at.as_deref(), Some("2026-10-25"));
        assert_eq!(a.follow_up_at.as_deref(), Some("2026-11-01"));
        assert_eq!(a.resume_version.as_deref(), Some("cv.pdf · read Oct 24"));
        assert!(
            a.letter.as_deref().unwrap().starts_with("Cover letter · "),
            "{:?}",
            a.letter
        );
        let state: String = conn
            .query_row("SELECT state FROM jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "saved");

        // The sent letter is frozen; later edits change only the working draft.
        let sent: i64 = conn
            .query_row("SELECT COUNT(*) FROM drafts WHERE sent = 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(sent, 1);
        assert!(crate::apply::writer::load(&conn, 1, DraftKind::CoverLetter).is_ok());

        // Unsaving no longer drops it.
        on_job_state(&conn, 1, false).unwrap();
        assert_eq!(list(&conn).unwrap().len(), 1);

        let r = update(&conn, 1, &edit(&a, AppStatus::Rejected), "2026-10-30").unwrap();
        assert_eq!(r.follow_up_at, None);
        assert_eq!(r.applied_at.as_deref(), Some("2026-10-25"));
    }

    #[test]
    fn follow_ups_are_logged_and_rescheduled() {
        let (_d, conn) = db();
        mark_applied(&conn, 1, "2026-10-18").unwrap();
        let a = followed_up(&conn, 1, "2026-10-25").unwrap();
        assert_eq!(a.notes, "Followed up Oct 25.");
        assert_eq!(a.follow_up_at.as_deref(), Some("2026-11-01"));
    }

    #[test]
    fn rejects_bad_dates() {
        let (_d, conn) = db();
        on_job_state(&conn, 1, true).unwrap();
        let a = get(&conn, 1).unwrap();
        let mut e = edit(&a, AppStatus::Saved);
        e.follow_up_at = Some("next week".into());
        assert!(update(&conn, 1, &e, "2026-10-25").is_err());
    }

    #[test]
    fn csv_quotes_and_defuses_formulas() {
        let (_d, conn) = db();
        mark_applied(&conn, 1, "2026-10-25").unwrap();
        let mut a = get(&conn, 1).unwrap();
        a.notes = "=HYPERLINK(\"x\")\nsecond line".into();
        let out = csv(&[a]);
        let mut lines = out.lines();
        assert!(lines.next().unwrap().starts_with("Role,Company,Status"));
        assert!(
            out.contains("\"Frontend Engineer\",\"Acme\",\"Applied\",\"2026-10-25\""),
            "{out}"
        );
        assert!(out.contains("\"'=HYPERLINK(\"\"x\"\")"), "{out}");
    }
}
