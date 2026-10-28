//! Reading the feed: eligibility, keyword filters and the match score are worked out
//! here against the current profile and search settings.

use std::collections::HashMap;

use chrono::Datelike;
use rusqlite::{Connection, OptionalExtension, Row, params};

use super::{
    Feed, JobDetail, JobListing, JobState, JobSummary, MatchBreakdown, Salary, SalaryPeriod,
    Weights,
    eligibility::{self, UserPlace},
    score::MatchContext,
    text::Phrases,
};
use crate::{
    ai::analysis::{self, JobAnalysis},
    db::store,
    error::{AppError, AppResult},
    profile::SearchSettings,
};

const JOB_COLUMNS: &str = "id, title, company, url, location_text, salary_min, salary_max,
    salary_currency, salary_period, salary_text, posted_at, first_seen_at, state, description, tags";

struct JobRow {
    id: i64,
    title: String,
    company: String,
    url: Option<String>,
    location: Option<String>,
    salary: Option<Salary>,
    posted_at: Option<String>,
    first_seen_at: String,
    state: JobState,
    description: String,
    tags: Vec<String>,
}

fn job_row(r: &Row) -> rusqlite::Result<JobRow> {
    let min: Option<f64> = r.get(5)?;
    let max: Option<f64> = r.get(6)?;
    let currency: Option<String> = r.get(7)?;
    let period: Option<String> = r.get(8)?;
    let text: Option<String> = r.get(9)?;
    let salary = (text.is_some() || min.is_some()).then(|| Salary {
        min,
        max,
        currency,
        period: period.as_deref().and_then(SalaryPeriod::parse),
        text: text.unwrap_or_default(),
    });
    Ok(JobRow {
        id: r.get(0)?,
        title: r.get(1)?,
        company: r.get(2)?,
        url: r.get(3)?,
        location: r.get::<_, Option<String>>(4)?.filter(|l| !l.is_empty()),
        salary,
        posted_at: r.get(10)?,
        first_seen_at: r.get(11)?,
        state: JobState::parse(&r.get::<_, String>(12)?),
        description: r.get(13)?,
        tags: serde_json::from_str(&r.get::<_, String>(14)?).unwrap_or_default(),
    })
}

/// The user's side of every comparison, read once per request.
struct Viewer {
    matcher: MatchContext,
    place: UserPlace,
    weights: Weights,
    /// Hash of the profile, when AI is on; cached analyses must match it.
    ai_profile: Option<String>,
}

impl Viewer {
    fn load(conn: &Connection, search: &SearchSettings) -> AppResult<Self> {
        let profile = store::get_profile(conn)?
            .map(|p| p.profile)
            .unwrap_or_default();
        let weights = store::get_weights(conn)?;
        let ai_on = store::get_ai_settings(conn)?.provider.is_some();
        Ok(Self {
            matcher: MatchContext::new(&profile, search, weights, chrono::Utc::now().year()),
            place: UserPlace::new(&search.country, &search.timezone),
            weights,
            ai_profile: ai_on.then(|| analysis::profile_hash(&analysis::profile_items(&profile))),
        })
    }
}

/// Everything worked out for one job.
struct Judged {
    summary: JobSummary,
    breakdown: MatchBreakdown,
    passes_keywords: bool,
}

fn judge(
    row: JobRow,
    listings: Vec<JobListing>,
    viewer: &Viewer,
    ai: Option<&JobAnalysis>,
) -> (Judged, String, Vec<String>) {
    let text = format!("{}\n{}", row.title, row.description);
    let phrases = Phrases::new(&text);
    let passes_keywords = viewer
        .matcher
        .passes_keywords(&phrases, &text.to_lowercase());
    let eligibility =
        eligibility::classify(row.location.as_deref().unwrap_or(""), &text, &viewer.place);
    let rules = viewer
        .matcher
        .breakdown(&row.title, &row.description, &row.tags, &phrases);
    let breakdown = match ai {
        Some(a) => analysis::combine(&rules, a, &viewer.weights),
        None => rules,
    };
    let summary = JobSummary {
        id: row.id,
        title: row.title,
        company: row.company,
        url: row.url,
        location: row.location,
        salary: row.salary,
        posted_at: row.posted_at,
        first_seen_at: row.first_seen_at,
        state: row.state,
        eligibility,
        score: breakdown.score,
        ai_checked: ai.is_some(),
        listings,
    };
    (
        Judged {
            summary,
            breakdown,
            passes_keywords,
        },
        row.description,
        row.tags,
    )
}

fn listings(conn: &Connection, job_id: Option<i64>) -> AppResult<HashMap<i64, Vec<JobListing>>> {
    let mut stmt = conn.prepare(
        "SELECT js.job_id, js.source_id, s.name, js.url
         FROM job_sources js JOIN sources s ON s.id = js.source_id
         WHERE ?1 IS NULL OR js.job_id = ?1
         ORDER BY js.rowid",
    )?;
    let mut out: HashMap<i64, Vec<JobListing>> = HashMap::new();
    let rows = stmt.query_map([job_id], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            JobListing {
                source_id: r.get(1)?,
                source_name: r.get(2)?,
                url: r.get(3)?,
            },
        ))
    })?;
    for row in rows {
        let (id, listing) = row?;
        let job = out.entry(id).or_default();
        // A source listing the job once per location is still one place to see it.
        if !job.iter().any(|l| l.source_id == listing.source_id) {
            job.push(listing);
        }
    }
    Ok(out)
}

/// The feed for the active search.
pub fn feed(conn: &Connection) -> AppResult<Feed> {
    feed_for(conn, &store::get_search_settings(conn)?)
}

/// Jobs listed by an enabled source, plus saved jobs. Dismissed jobs are included so the
/// UI can show them on request; keyword-filtered jobs are left out and counted.
pub fn feed_for(conn: &Connection, search: &SearchSettings) -> AppResult<Feed> {
    let viewer = Viewer::load(conn, search)?;
    let analyses = match &viewer.ai_profile {
        Some(hash) => analysis::load_all(conn, hash)?,
        None => HashMap::new(),
    };
    let mut all_listings = listings(conn, None)?;
    let mut stmt = conn.prepare(&format!(
        "SELECT {JOB_COLUMNS} FROM jobs
         WHERE state = 'saved'
            OR EXISTS (SELECT 1 FROM job_sources js JOIN sources s ON s.id = js.source_id
                       WHERE js.job_id = jobs.id AND s.enabled = 1)"
    ))?;
    let rows: Vec<JobRow> = stmt.query_map([], job_row)?.collect::<Result<_, _>>()?;

    let mut jobs = Vec::with_capacity(rows.len());
    let mut hidden_by_keywords = 0;
    for row in rows {
        let l = all_listings.remove(&row.id).unwrap_or_default();
        let ai = analyses.get(&row.id);
        let (judged, _, _) = judge(row, l, &viewer, ai);
        if judged.passes_keywords || judged.summary.state == JobState::Saved {
            jobs.push(judged.summary);
        } else {
            hidden_by_keywords += 1;
        }
    }
    jobs.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| b.posted_at.cmp(&a.posted_at))
    });

    let (last_updated_at, fetched): (Option<String>, i64) = conn.query_row(
        "SELECT MAX(last_ok_at), COUNT(last_run_at) FROM sources WHERE enabled = 1 AND kind != 'manual'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    Ok(Feed {
        jobs,
        hidden_by_keywords,
        last_updated_at,
        never_refreshed: fetched == 0,
    })
}

pub fn job_detail(conn: &Connection, id: i64) -> AppResult<JobDetail> {
    let row = conn
        .query_row(
            &format!("SELECT {JOB_COLUMNS} FROM jobs WHERE id = ?1"),
            [id],
            job_row,
        )
        .optional()?
        .ok_or_else(|| AppError::Other("That job is no longer in your feed.".into()))?;
    let viewer = Viewer::load(conn, &store::get_search_settings(conn)?)?;
    let latest = match &viewer.ai_profile {
        Some(hash) => analysis::load_latest(conn, id, hash)?,
        None => None,
    };
    let fresh = latest.as_ref().filter(|(_, fresh)| *fresh).map(|(a, _)| a);
    let l = listings(conn, Some(id))?.remove(&id).unwrap_or_default();
    let (judged, description, tags) = judge(row, l, &viewer, fresh);
    let analysis_stale = latest.as_ref().is_some_and(|(_, fresh)| !fresh);
    Ok(JobDetail {
        job: judged.summary,
        description,
        tags,
        breakdown: judged.breakdown,
        analysis: latest.map(|(a, _)| a),
        analysis_stale,
    })
}

pub fn set_state(conn: &Connection, id: i64, state: JobState) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE jobs SET state = ?2 WHERE id = ?1",
        params![id, state.as_str()],
    )?;
    if n == 0 {
        return Err(AppError::Other(
            "That job is no longer in your feed.".into(),
        ));
    }
    crate::tracker::on_job_state(conn, id, state == JobState::Saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        jobs::{EligibilityStatus, RawJob, refresh},
        profile::{Profile, SearchSettings, Skill},
    };

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn add(
        conn: &Connection,
        source: &str,
        id: &str,
        title: &str,
        location: &str,
        description: &str,
    ) -> i64 {
        let raw = RawJob {
            external_id: id.into(),
            title: title.into(),
            company: format!("Co {id}"),
            location: location.into(),
            description: description.into(),
            posted_at: Some(format!("2026-10-{:02}T00:00:00Z", 10 + id.len())),
            ..Default::default()
        };
        refresh::upsert_job(conn, source, &raw).unwrap().0
    }

    #[test]
    fn scores_filters_and_classifies() {
        let (_d, conn) = fresh();
        store::save_profile(
            &conn,
            &Profile {
                skills: vec![Skill {
                    name: "React".into(),
                    ..Default::default()
                }],
                ..Default::default()
            },
        )
        .unwrap();
        store::save_search_settings(
            &conn,
            &SearchSettings {
                titles: vec!["Frontend Engineer".into()],
                avoid: vec!["crypto".into()],
                country: "PT".into(),
                ..Default::default()
            },
        )
        .unwrap();
        add(
            &conn,
            "remotive",
            "a",
            "Frontend Engineer",
            "Worldwide",
            "We use React.",
        );
        add(
            &conn,
            "remotive",
            "bb",
            "Accountant",
            "USA Only",
            "Spreadsheets.",
        );
        add(
            &conn,
            "remotive",
            "ccc",
            "Frontend Engineer",
            "",
            "React for a crypto exchange.",
        );

        let feed = feed(&conn).unwrap();
        assert_eq!(feed.hidden_by_keywords, 1);
        assert_eq!(feed.jobs.len(), 2);
        assert_eq!(feed.jobs[0].title, "Frontend Engineer");
        assert_eq!(feed.jobs[0].eligibility.status, EligibilityStatus::Eligible);
        assert_eq!(
            feed.jobs[1].eligibility.status,
            EligibilityStatus::Restricted
        );
        assert!(feed.jobs[0].score > feed.jobs[1].score);
        assert!(feed.never_refreshed);
    }

    #[test]
    fn disabled_sources_hide_their_jobs_unless_saved() {
        let (_d, conn) = fresh();
        let a = add(&conn, "wwr", "a", "Designer", "", "");
        add(&conn, "wwr", "bb", "Writer", "", "");
        refresh::set_enabled(&conn, "wwr", false).unwrap();
        assert!(feed(&conn).unwrap().jobs.is_empty());
        set_state(&conn, a, JobState::Saved).unwrap();
        let jobs = feed(&conn).unwrap().jobs;
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].state, JobState::Saved);
    }

    #[test]
    fn detail_lists_every_source() {
        let (_d, conn) = fresh();
        let id = add(&conn, "remotive", "a", "Designer", "", "");
        let same = RawJob {
            external_id: "z".into(),
            title: "Designer".into(),
            company: "Co a".into(),
            ..Default::default()
        };
        refresh::upsert_job(&conn, "wwr", &same).unwrap();
        let d = job_detail(&conn, id).unwrap();
        let names: Vec<&str> = d
            .job
            .listings
            .iter()
            .map(|l| l.source_name.as_str())
            .collect();
        assert_eq!(names, ["Remotive", "We Work Remotely"]);
        assert!(d.breakdown.short_description);
        assert!(job_detail(&conn, 999).is_err());
    }
}
