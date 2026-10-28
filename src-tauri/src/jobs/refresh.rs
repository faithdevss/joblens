//! Fetching sources and storing what they return, one row per real-world job.
//!
//! Sources run side by side, each with its own timeout. The database is only locked
//! to read the source list and to store each result, never while waiting on the network.

use std::{collections::HashSet, sync::Mutex};

use rusqlite::{Connection, OptionalExtension, params};

use super::{
    RawJob, RefreshReport, RunOutcome, SourceKind, SourceRun, SourceStatus, dates, dedupe,
    sources::{Adapter, FoundBoard, MANUAL_SOURCE_ID},
};
use crate::error::{AppError, AppResult};

/// Board listings not seen for this long are treated as closed.
const BOARD_LISTING_TTL_DAYS: i64 = 30;
const RETRY_AFTER_FAILURE_SECS: i64 = 5 * 60;

pub struct SourceRow {
    pub id: String,
    pub kind: SourceKind,
    pub name: String,
    pub config: String,
    pub enabled: bool,
    pub last_run_at: Option<String>,
    pub last_ok_at: Option<String>,
    pub last_error: Option<String>,
}

fn source_rows(conn: &Connection) -> AppResult<Vec<SourceRow>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, name, config, enabled, last_run_at, last_ok_at, last_error
         FROM sources WHERE kind != 'manual'
         ORDER BY kind = 'watchlist', rowid",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(SourceRow {
                id: r.get(0)?,
                kind: SourceKind::parse(&r.get::<_, String>(1)?),
                name: r.get(2)?,
                config: r.get(3)?,
                enabled: r.get(4)?,
                last_run_at: r.get(5)?,
                last_ok_at: r.get(6)?,
                last_error: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    Ok(rows)
}

fn detail(row: &SourceRow) -> Option<String> {
    match Adapter::for_source(&row.id, &row.config)? {
        Adapter::Remotive => Some("remotive.com".into()),
        Adapter::RemoteOk => Some("remoteok.com".into()),
        Adapter::Wwr => Some("weworkremotely.com".into()),
        Adapter::Jobicy => Some("jobicy.com".into()),
        Adapter::Ats { ats, token } => Some(format!("{} · {token}", ats.label())),
    }
}

pub fn list_sources(conn: &Connection) -> AppResult<Vec<SourceStatus>> {
    let rows = source_rows(conn)?;
    let mut count = conn.prepare("SELECT COUNT(*) FROM job_sources WHERE source_id = ?1")?;
    rows.iter()
        .map(|row| {
            Ok(SourceStatus {
                id: row.id.clone(),
                kind: row.kind,
                name: row.name.clone(),
                enabled: row.enabled,
                last_run_at: row.last_run_at.clone(),
                last_ok_at: row.last_ok_at.clone(),
                last_error: row.last_error.clone(),
                job_count: count.query_row([&row.id], |r| r.get(0))?,
                detail: detail(row),
            })
        })
        .collect()
}

pub fn set_enabled(conn: &Connection, id: &str, enabled: bool) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE sources SET enabled = ?2 WHERE id = ?1 AND kind != 'manual'",
        params![id, enabled],
    )?;
    if n == 0 {
        return Err(AppError::Other("That source no longer exists.".into()));
    }
    Ok(())
}

/// Adds a company found by [`sources::find_board`] and stores its first fetch.
pub fn add_watchlist(conn: &Connection, found: &FoundBoard) -> AppResult<String> {
    let id = format!("{}:{}", found.ats.as_str(), found.token.to_lowercase());
    let exists: Option<String> = conn
        .query_row("SELECT name FROM sources WHERE id = ?1", [&id], |r| {
            r.get(0)
        })
        .optional()?;
    if let Some(name) = exists {
        return Err(AppError::Other(format!(
            "{name} is already on your watchlist."
        )));
    }
    let config = serde_json::json!({ "ats": found.ats.as_str(), "token": found.token }).to_string();
    conn.execute(
        "INSERT INTO sources (id, kind, name, config) VALUES (?1, 'watchlist', ?2, ?3)",
        params![id, found.company, config],
    )?;
    store_run(conn, &id, &found.company, true, Ok(found.jobs.clone()))?;
    Ok(id)
}

/// Removes a watchlist company and the jobs only it listed (saved jobs are kept).
pub fn remove_source(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute(
        "DELETE FROM sources WHERE id = ?1 AND kind = 'watchlist'",
        [id],
    )?;
    if n == 0 {
        return Err(AppError::Other(
            "Only watchlist companies can be removed.".into(),
        ));
    }
    prune_orphans(conn)
}

/// Jobs no source lists anymore, unless the user saved them or is tracking an application.
fn prune_orphans(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "DELETE FROM jobs
         WHERE state != 'saved'
           AND NOT EXISTS (SELECT 1 FROM job_sources js WHERE js.job_id = jobs.id)
           AND NOT EXISTS (SELECT 1 FROM applications a WHERE a.job_id = jobs.id)",
        [],
    )?;
    Ok(())
}

/// Stores one posting from a source. Returns the job's id and whether it's new.
pub fn upsert_job(conn: &Connection, source_id: &str, raw: &RawJob) -> AppResult<(i64, bool)> {
    let now = dates::now();
    let salary = raw.salary.as_ref();
    let (smin, smax) = (salary.and_then(|s| s.min), salary.and_then(|s| s.max));
    let scur = salary.and_then(|s| s.currency.as_deref());
    let sper = salary.and_then(|s| s.period).map(|p| p.as_str());
    let stext = salary.map(|s| s.text.as_str());
    let tags = serde_json::to_string(&raw.tags).unwrap_or_else(|_| "[]".into());
    let url = Some(raw.url.as_str()).filter(|u| !u.is_empty());

    let known: Option<i64> = conn
        .query_row(
            "SELECT job_id FROM job_sources WHERE source_id = ?1 AND external_id = ?2",
            params![source_id, raw.external_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(job_id) = known {
        conn.execute(
            "UPDATE job_sources SET seen_at = ?3, url = ?4 WHERE source_id = ?1 AND external_id = ?2",
            params![source_id, raw.external_id, now, url],
        )?;
        // The source a job was first found on owns its details; edits there flow through.
        // A posting fetched without its description (a SmartRecruiters posting that
        // failed to load) keeps the one already stored.
        conn.execute(
            "UPDATE jobs SET title = ?2, company = ?3,
                description = CASE WHEN ?4 = '' THEN description ELSE ?4 END, url = ?5,
                location_text = ?6, salary_min = ?7, salary_max = ?8, salary_currency = ?9,
                salary_period = ?10, salary_text = ?11, posted_at = COALESCE(?12, posted_at), tags = ?13
             WHERE id = ?1
               AND (SELECT source_id FROM job_sources WHERE job_id = ?1 ORDER BY rowid LIMIT 1) = ?14",
            params![
                job_id, raw.title, raw.company, raw.description, url, raw.location, smin, smax,
                scur, sper, stext, raw.posted_at, tags, source_id
            ],
        )?;
        return Ok((job_id, false));
    }

    let base = dedupe::base_key(&raw.company, &raw.title);
    let mut stmt = conn.prepare_cached(
        "SELECT id, description FROM jobs
         WHERE (canonical_key = ?1 OR substr(canonical_key, 1, length(?1) + 1) = ?1 || '#')
           AND NOT EXISTS (SELECT 1 FROM job_sources js WHERE js.job_id = jobs.id AND js.source_id = ?2)",
    )?;
    let candidates: Vec<(i64, String)> = stmt
        .query_map(params![base, source_id], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    let same = candidates
        .into_iter()
        .find(|(_, d)| dedupe::similar_descriptions(d, &raw.description));

    let (job_id, added) =
        match same {
            Some((id, _)) => (id, false),
            None => {
                let mut key = base.clone();
                let mut n = 2;
                while conn
                    .query_row(
                        "SELECT 1 FROM jobs WHERE canonical_key = ?1",
                        [&key],
                        |_| Ok(()),
                    )
                    .optional()?
                    .is_some()
                {
                    key = format!("{base}#{n}");
                    n += 1;
                }
                conn.execute(
                "INSERT INTO jobs (canonical_key, title, company, description, url, location_text,
                    salary_min, salary_max, salary_currency, salary_period, salary_text, posted_at,
                    tags, first_seen_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![
                    key, raw.title, raw.company, raw.description, url, raw.location, smin, smax,
                    scur, sper, stext, raw.posted_at, tags, now
                ],
            )?;
                (conn.last_insert_rowid(), true)
            }
        };
    conn.execute(
        "INSERT INTO job_sources (job_id, source_id, external_id, url, seen_at) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![job_id, source_id, raw.external_id, url, now],
    )?;
    Ok((job_id, added))
}

/// Records another listing of a stored job on the same source. A listing that was
/// its own job before moves over, and that duplicate is pruned.
fn add_listing(
    conn: &Connection,
    source_id: &str,
    external_id: &str,
    job_id: i64,
    url: &str,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO job_sources (job_id, source_id, external_id, url, seen_at)
         VALUES (?1, ?2, ?3, ?4, ?5)
         ON CONFLICT (source_id, external_id)
         DO UPDATE SET job_id = excluded.job_id, url = excluded.url, seen_at = excluded.seen_at",
        params![
            job_id,
            source_id,
            external_id,
            Some(url).filter(|u| !u.is_empty()),
            dates::now()
        ],
    )?;
    Ok(())
}

/// Saves a fetch result and the source's status. `full_board` means the result is the
/// source's complete list, so listings missing from it have closed.
fn store_run(
    conn: &Connection,
    source_id: &str,
    source_name: &str,
    full_board: bool,
    result: Result<Vec<RawJob>, String>,
) -> AppResult<RunOutcome> {
    let now = dates::now();
    let jobs = match result {
        Ok(jobs) => jobs,
        Err(error) => {
            conn.execute(
                "UPDATE sources SET last_run_at = ?2, last_error = ?3 WHERE id = ?1",
                params![source_id, now, error],
            )?;
            return Ok(RunOutcome::Failed { error });
        }
    };
    let tx = conn.unchecked_transaction()?;
    let mut added = 0;
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for mut raw in jobs {
        if !seen.insert(raw.external_id.clone()) {
            continue;
        }
        // Some boards' postings don't name the company; the watchlist entry does.
        if raw.company.is_empty() {
            raw.company = source_name.to_string();
        }
        unique.push(raw);
    }
    for repost in dedupe::merge_reposts(unique) {
        let (job_id, new) = upsert_job(&tx, source_id, &repost.job)?;
        if new {
            added += 1;
        }
        for (external_id, url) in &repost.also {
            add_listing(&tx, source_id, external_id, job_id, url)?;
        }
    }
    if full_board {
        let mut stmt = tx.prepare("SELECT external_id FROM job_sources WHERE source_id = ?1")?;
        let stale: Vec<String> = stmt
            .query_map([source_id], |r| r.get(0))?
            .collect::<Result<Vec<String>, _>>()?
            .into_iter()
            .filter(|id| !seen.contains(id))
            .collect();
        drop(stmt);
        for id in stale {
            tx.execute(
                "DELETE FROM job_sources WHERE source_id = ?1 AND external_id = ?2",
                params![source_id, id],
            )?;
        }
    } else {
        tx.execute(
            "DELETE FROM job_sources WHERE source_id = ?1
               AND julianday(?2) - julianday(seen_at) > ?3",
            params![source_id, now, BOARD_LISTING_TTL_DAYS],
        )?;
    }
    prune_orphans(&tx)?;
    tx.execute(
        "UPDATE sources SET last_run_at = ?2, last_ok_at = ?2, last_error = NULL, job_count = ?3 WHERE id = ?1",
        params![source_id, now, seen.len() as u32],
    )?;
    tx.commit()?;
    Ok(RunOutcome::Ok {
        fetched: seen.len() as u32,
        added,
    })
}

/// When a source may be fetched again, if it's too soon now.
fn next_allowed(row: &SourceRow, adapter: &Adapter) -> Option<String> {
    let age = dates::age_secs(row.last_run_at.as_deref()?)?;
    // After a failure, a retry soon is fine unless the site asked us to slow down.
    let interval = match &row.last_error {
        Some(e) if !e.contains("(429)") => RETRY_AFTER_FAILURE_SECS,
        _ => adapter.min_interval_secs(),
    };
    let wait = interval - age;
    (wait > 0).then(|| {
        let at = chrono::Utc::now() + chrono::Duration::seconds(wait);
        at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
    })
}

/// Fetches enabled sources (or just `only`), skipping any fetched too recently.
pub async fn refresh(
    db: &Mutex<Connection>,
    client: &reqwest::Client,
    only: Option<Vec<String>>,
) -> AppResult<RefreshReport> {
    let rows = source_rows(&db.lock().expect("db mutex poisoned"))?;
    let mut runs = Vec::new();
    let mut tasks = Vec::new();
    for row in rows {
        let wanted = match &only {
            Some(ids) => ids.contains(&row.id),
            None => row.enabled,
        };
        if !wanted {
            continue;
        }
        let Some(adapter) = Adapter::for_source(&row.id, &row.config) else {
            continue;
        };
        if let Some(next_at) = next_allowed(&row, &adapter) {
            runs.push(SourceRun {
                source_id: row.id,
                name: row.name,
                outcome: RunOutcome::Skipped { next_at },
            });
            continue;
        }
        let client = client.clone();
        let handle = tauri::async_runtime::spawn({
            let adapter = adapter.clone();
            async move { adapter.fetch(&client).await }
        });
        tasks.push((row, adapter, handle));
    }
    for (row, adapter, handle) in tasks {
        let result = handle
            .await
            .unwrap_or_else(|_| Err("The fetch stopped unexpectedly.".into()));
        let full_board = matches!(adapter, Adapter::Ats { .. });
        let conn = db.lock().expect("db mutex poisoned");
        let outcome = store_run(&conn, &row.id, &row.name, full_board, result)?;
        runs.push(SourceRun {
            source_id: row.id,
            name: row.name,
            outcome,
        });
    }
    Ok(RefreshReport { runs })
}

/// Stores a job the user added by hand. It's saved so it stays visible.
pub fn add_manual(conn: &Connection, raw: &RawJob) -> AppResult<i64> {
    let tx = conn.unchecked_transaction()?;
    let (id, _) = upsert_job(&tx, MANUAL_SOURCE_ID, raw)?;
    tx.execute("UPDATE jobs SET state = 'saved' WHERE id = ?1", [id])?;
    crate::tracker::on_job_state(&tx, id, true)?;
    tx.commit()?;
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::jobs::sources::Ats;

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn raw(id: &str, company: &str, title: &str, description: &str) -> RawJob {
        RawJob {
            external_id: id.into(),
            title: title.into(),
            company: company.into(),
            url: format!("https://example.com/{id}"),
            description: description.into(),
            ..Default::default()
        }
    }

    fn count(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn seeds_builtin_sources() {
        let (_d, conn) = fresh();
        let ids: Vec<String> = list_sources(&conn)
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["remotive", "remoteok", "wwr", "jobicy"]);
    }

    #[test]
    fn same_job_on_two_boards_is_one_job() {
        let (_d, conn) = fresh();
        let desc = "Build accessible patient tools with React and TypeScript. ".repeat(10);
        store_run(
            &conn,
            "remotive",
            "Remotive",
            false,
            Ok(vec![raw(
                "1",
                "Lumen Health, Inc.",
                "Sr. Frontend Engineer",
                &desc,
            )]),
        )
        .unwrap();
        let out = store_run(
            &conn,
            "wwr",
            "WWR",
            false,
            Ok(vec![raw(
                "a",
                "Lumen Health",
                "Senior Front-end Engineer (Remote)",
                &desc,
            )]),
        )
        .unwrap();
        assert_eq!(
            out,
            RunOutcome::Ok {
                fetched: 1,
                added: 0
            }
        );
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM job_sources"), 2);
    }

    #[test]
    fn one_source_posting_a_job_per_place_is_one_job() {
        let (_d, conn) = fresh();
        let desc = "Audit business and IT operations across finance and sales teams. ".repeat(8);
        let at = |id: &str, place: &str| RawJob {
            location: place.into(),
            ..raw(id, "ServiceNow", "Senior Auditor", &desc)
        };
        let out = store_run(
            &conn,
            "wwr",
            "WWR",
            true,
            Ok(vec![at("tx", "Texas"), at("fl", "Florida")]),
        )
        .unwrap();
        assert_eq!(
            out,
            RunOutcome::Ok {
                fetched: 2,
                added: 1
            }
        );
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM job_sources"), 2);
        let location: String = conn
            .query_row("SELECT location_text FROM jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(location, "Texas; Florida");
        let job_id: i64 = conn
            .query_row("SELECT id FROM jobs", [], |r| r.get(0))
            .unwrap();

        // The first listing closes; the job lives on through the other.
        store_run(&conn, "wwr", "WWR", true, Ok(vec![at("fl", "Florida")])).unwrap();
        assert_eq!(
            count(
                &conn,
                &format!("SELECT COUNT(*) FROM jobs WHERE id = {job_id}")
            ),
            1
        );
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM job_sources"), 1);
    }

    #[test]
    fn earlier_duplicates_from_one_source_are_folded_in() {
        let (_d, conn) = fresh();
        let desc = "Audit business and IT operations across finance and sales teams. ".repeat(8);
        let at = |id: &str, place: &str| RawJob {
            location: place.into(),
            ..raw(id, "ServiceNow", "Senior Auditor", &desc)
        };
        // Stored one at a time, as before reposts were merged.
        let tx = conn.unchecked_transaction().unwrap();
        upsert_job(&tx, "wwr", &at("tx", "Texas")).unwrap();
        upsert_job(&tx, "wwr", &at("fl", "Florida")).unwrap();
        tx.commit().unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 2);

        store_run(
            &conn,
            "wwr",
            "WWR",
            true,
            Ok(vec![at("tx", "Texas"), at("fl", "Florida")]),
        )
        .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM job_sources"), 2);
    }

    #[test]
    fn different_postings_with_one_title_stay_apart() {
        let (_d, conn) = fresh();
        let a =
            "Own our data pipelines in Python, Spark and Airflow for analytics teams. ".repeat(8);
        let b = "Lead customer onboarding calls and write help center articles for new users. "
            .repeat(8);
        store_run(
            &conn,
            "remotive",
            "Remotive",
            false,
            Ok(vec![raw("1", "Acme", "Engineer", &a)]),
        )
        .unwrap();
        store_run(
            &conn,
            "wwr",
            "WWR",
            false,
            Ok(vec![raw("2", "Acme", "Engineer", &b)]),
        )
        .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 2);
        // Two roles with one title on the same board are never merged either.
        store_run(
            &conn,
            "remoteok",
            "Remote OK",
            false,
            Ok(vec![
                raw("x", "Beta", "Engineer", ""),
                raw("y", "Beta", "Engineer", ""),
            ]),
        )
        .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 4);
    }

    fn auditor(id: &str, place: &str) -> RawJob {
        let desc = "Audit business and IT operations across finance and sales teams. ".repeat(8);
        RawJob {
            location: place.into(),
            ..raw(id, "ServiceNow", "Senior Auditor", &desc)
        }
    }

    fn one_str(conn: &Connection, sql: &str) -> String {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn reposts_and_other_sites_are_one_job_listed_once_per_site() {
        let (_d, conn) = fresh();
        store_run(
            &conn,
            "wwr",
            "WWR",
            true,
            Ok(vec![auditor("tx", "Texas"), auditor("fl", "Florida")]),
        )
        .unwrap();
        store_run(
            &conn,
            "remotive",
            "Remotive",
            false,
            Ok(vec![auditor("r1", "USA")]),
        )
        .unwrap();
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM job_sources"), 3);
        // Each listing keeps its own link.
        assert_eq!(
            one_str(
                &conn,
                "SELECT url FROM job_sources WHERE external_id = 'fl'"
            ),
            "https://example.com/fl"
        );
        let feed = crate::jobs::feed::feed(&conn).unwrap();
        assert_eq!(feed.jobs.len(), 1);
        let sites: Vec<&str> = feed.jobs[0]
            .listings
            .iter()
            .map(|l| l.source_id.as_str())
            .collect();
        assert_eq!(sites, ["wwr", "remotive"]);
    }

    #[test]
    fn reposts_in_a_new_order_stay_the_same_job() {
        let (_d, conn) = fresh();
        let run = |jobs: Vec<RawJob>| store_run(&conn, "wwr", "WWR", true, Ok(jobs)).unwrap();
        run(vec![auditor("tx", "Texas"), auditor("fl", "Florida")]);
        let id = count(&conn, "SELECT id FROM jobs");
        let out = run(vec![auditor("fl", "Florida"), auditor("tx", "Texas")]);
        assert_eq!(
            out,
            RunOutcome::Ok {
                fetched: 2,
                added: 0
            }
        );
        assert_eq!(count(&conn, "SELECT id FROM jobs"), id);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        // A third place shows up on the next fetch.
        run(vec![
            auditor("tx", "Texas"),
            auditor("fl", "Florida"),
            auditor("oh", "Ohio"),
        ]);
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 1);
        assert_eq!(
            one_str(&conn, "SELECT location_text FROM jobs"),
            "Texas; Florida; Ohio"
        );
        // All listings close: the job goes too.
        run(Vec::new());
        assert_eq!(count(&conn, "SELECT COUNT(*) FROM jobs"), 0);
    }

    #[test]
    fn folding_old_duplicates_keeps_a_saved_one() {
        let (_d, conn) = fresh();
        let tx = conn.unchecked_transaction().unwrap();
        upsert_job(&tx, "wwr", &auditor("tx", "Texas")).unwrap();
        let (saved, _) = upsert_job(&tx, "wwr", &auditor("fl", "Florida")).unwrap();
        tx.commit().unwrap();
        conn.execute("UPDATE jobs SET state = 'saved' WHERE id = ?1", [saved])
            .unwrap();
        store_run(
            &conn,
            "wwr",
            "WWR",
            true,
            Ok(vec![auditor("tx", "Texas"), auditor("fl", "Florida")]),
        )
        .unwrap();
        assert_eq!(
            count(
                &conn,
                &format!("SELECT COUNT(*) FROM jobs WHERE id = {saved} AND state = 'saved'")
            ),
            1,
            "the user's saved job is never deleted"
        );
    }

    #[test]
    fn refetch_without_description_keeps_the_stored_one() {
        let (_d, conn) = fresh();
        let run = |desc: &str| {
            store_run(
                &conn,
                "remotive",
                "Remotive",
                false,
                Ok(vec![raw("1", "Acme", "Engineer", desc)]),
            )
            .unwrap()
        };
        run("Full description of the role.");
        run("");
        assert_eq!(
            one_str(&conn, "SELECT description FROM jobs"),
            "Full description of the role."
        );
        run("Updated description.");
        assert_eq!(
            one_str(&conn, "SELECT description FROM jobs"),
            "Updated description."
        );
    }

    #[test]
    fn same_id_twice_in_one_fetch_is_stored_once() {
        let (_d, conn) = fresh();
        let out = store_run(
            &conn,
            "wwr",
            "WWR",
            true,
            Ok(vec![auditor("tx", "Texas"), auditor("tx", "Ohio")]),
        )
        .unwrap();
        assert_eq!(
            out,
            RunOutcome::Ok {
                fetched: 1,
                added: 1
            }
        );
        assert_eq!(one_str(&conn, "SELECT location_text FROM jobs"), "Texas");
    }

    #[test]
    fn refetch_updates_instead_of_duplicating() {
        let (_d, conn) = fresh();
        store_run(
            &conn,
            "remotive",
            "Remotive",
            false,
            Ok(vec![raw("1", "Acme", "Engineer", "old")]),
        )
        .unwrap();
        let out = store_run(
            &conn,
            "remotive",
            "Remotive",
            false,
            Ok(vec![raw("1", "Acme", "Engineer", "new text")]),
        )
        .unwrap();
        assert_eq!(
            out,
            RunOutcome::Ok {
                fetched: 1,
                added: 0
            }
        );
        let d: String = conn
            .query_row("SELECT description FROM jobs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(d, "new text");
    }

    #[test]
    fn failure_is_recorded_and_keeps_jobs() {
        let (_d, conn) = fresh();
        store_run(
            &conn,
            "wwr",
            "WWR",
            false,
            Ok(vec![raw("1", "Acme", "Engineer", "")]),
        )
        .unwrap();
        let out = store_run(
            &conn,
            "wwr",
            "WWR",
            false,
            Err("Timed out after 20 s.".into()),
        )
        .unwrap();
        assert!(matches!(out, RunOutcome::Failed { .. }));
        let s = list_sources(&conn)
            .unwrap()
            .into_iter()
            .find(|s| s.id == "wwr")
            .unwrap();
        assert_eq!(s.last_error.as_deref(), Some("Timed out after 20 s."));
        assert!(s.last_ok_at.is_some());
        assert_eq!(s.job_count, 1);
    }

    #[test]
    fn watchlist_drops_closed_roles_but_keeps_saved_ones() {
        let (_d, conn) = fresh();
        let found = FoundBoard {
            ats: Ats::Lever,
            token: "acme".into(),
            company: "Acme".into(),
            jobs: vec![raw("a", "", "Engineer", ""), raw("b", "", "Designer", "")],
        };
        let id = add_watchlist(&conn, &found).unwrap();
        assert_eq!(id, "lever:acme");
        let company: String = conn
            .query_row("SELECT company FROM jobs LIMIT 1", [], |r| r.get(0))
            .unwrap();
        assert_eq!(company, "Acme");
        assert!(add_watchlist(&conn, &found).is_err());

        conn.execute(
            "UPDATE jobs SET state = 'saved' WHERE title = 'Designer'",
            [],
        )
        .unwrap();
        store_run(&conn, &id, "Acme", true, Ok(vec![])).unwrap();
        let titles: Vec<String> = conn
            .prepare("SELECT title FROM jobs")
            .unwrap()
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(titles, ["Designer"]);

        remove_source(&conn, &id).unwrap();
        assert!(remove_source(&conn, "remotive").is_err());
    }

    #[test]
    fn manual_jobs_are_saved() {
        let (_d, conn) = fresh();
        let id = add_manual(&conn, &raw("m1", "Acme", "Engineer", "Paste")).unwrap();
        let state: String = conn
            .query_row("SELECT state FROM jobs WHERE id = ?1", [id], |r| r.get(0))
            .unwrap();
        assert_eq!(state, "saved");
    }

    #[test]
    fn waits_between_fetches() {
        let row = SourceRow {
            id: "remotive".into(),
            kind: SourceKind::Board,
            name: "Remotive".into(),
            config: "{}".into(),
            enabled: true,
            last_run_at: Some(dates::now()),
            last_ok_at: None,
            last_error: None,
        };
        assert!(next_allowed(&row, &Adapter::Remotive).is_some());
        let old = SourceRow {
            last_run_at: Some("2020-01-01T00:00:00Z".into()),
            ..row
        };
        assert!(next_allowed(&old, &Adapter::Remotive).is_none());
    }

    /// Real fetch → store → feed. `cargo test live_refresh -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_refresh() {
        let (_d, conn) = fresh();
        let db = Mutex::new(conn);
        let client = crate::jobs::sources::client();
        let report = tauri::async_runtime::block_on(refresh(&db, &client, None)).unwrap();
        for r in &report.runs {
            println!("{}: {:?}", r.name, r.outcome);
        }
        let conn = db.lock().unwrap();
        let start = std::time::Instant::now();
        let feed = crate::jobs::feed::feed(&conn).unwrap();
        println!("feed: {} jobs in {:?}", feed.jobs.len(), start.elapsed());
        let multi = feed.jobs.iter().filter(|j| j.listings.len() > 1).count();
        println!("listed on several boards: {multi}");
        drop(conn);
        let again = tauri::async_runtime::block_on(refresh(&db, &client, None)).unwrap();
        assert!(
            again
                .runs
                .iter()
                .all(|r| matches!(r.outcome, RunOutcome::Skipped { .. }))
        );
    }
}
