//! Background refresh while the app is open, and desktop alerts for great new matches.
//!
//! Every few minutes the app checks whether a refresh is due. After a refresh, each
//! search with alerts on is scored against new jobs; jobs that clear the bar get one
//! notification, once. Jobs seen after a manual refresh are recorded too, so they
//! aren't announced later.

use std::{collections::HashMap, time::Duration};

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use ts_rs::TS;

use crate::{
    db::store,
    error::{AppError, AppResult},
    jobs::{EligibilityStatus, JobState, dates, feed, refresh},
    searches,
    state::AppState,
};

const KEY: &str = "alerts";
const CHECK_EVERY: Duration = Duration::from_secs(5 * 60);
/// Let the window load before the first check.
const STARTUP_DELAY: Duration = Duration::from_secs(20);
pub const FEED_UPDATED_EVENT: &str = "feed-updated";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
#[ts(export)]
pub struct AlertSettings {
    /// Refresh sources in the background while the app is open.
    pub auto_refresh: bool,
    pub every_hours: u32,
    /// Show a desktop notification for great new matches.
    pub notify: bool,
    pub min_score: u32,
    /// Jobs first seen before alerts were turned on aren't announced.
    pub since: Option<String>,
}

impl Default for AlertSettings {
    fn default() -> Self {
        Self {
            auto_refresh: true,
            every_hours: 3,
            notify: false,
            min_score: 80,
            since: None,
        }
    }
}

/// One job worth announcing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Alert {
    #[ts(type = "number")]
    pub job_id: i64,
    pub title: String,
    pub company: String,
    pub score: u32,
    /// Name of the search it matched best.
    pub search: String,
}

/// Sent to the window after a background refresh.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct FeedUpdated {
    pub alerts: Vec<Alert>,
}

fn now() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

pub fn get_settings(conn: &Connection) -> AppResult<AlertSettings> {
    Ok(store::get_setting(conn, KEY)?.unwrap_or_default())
}

/// Saves the settings. Turning alerts on starts them from now, so jobs already in
/// the feed aren't announced.
pub fn save_settings(conn: &Connection, next: &AlertSettings) -> AppResult<AlertSettings> {
    if !(1..=24).contains(&next.every_hours) {
        return Err(AppError::Other("Refresh every 1 to 24 hours.".into()));
    }
    if !(50..=100).contains(&next.min_score) {
        return Err(AppError::Other("Alert at a match of 50 to 100.".into()));
    }
    let prev = get_settings(conn)?;
    let mut saved = next.clone();
    saved.since = match (prev.notify, next.notify) {
        (_, false) => None,
        (false, true) => Some(now()),
        (true, true) => prev.since.or_else(|| Some(now())),
    };
    store::set_setting(conn, KEY, &saved)?;
    Ok(saved)
}

/// A background refresh is due: auto refresh is on and no source fetched in a while.
pub fn due(conn: &Connection, settings: &AlertSettings) -> AppResult<bool> {
    if !settings.auto_refresh {
        return Ok(false);
    }
    let last: Option<String> = conn.query_row(
        "SELECT MAX(last_run_at) FROM sources WHERE enabled = 1 AND kind != 'manual'",
        [],
        |r| r.get(0),
    )?;
    let interval = i64::from(settings.every_hours) * 3600;
    Ok(last
        .as_deref()
        .and_then(dates::age_secs)
        .is_none_or(|age| age >= interval))
}

/// New jobs that clear the bar for any search with alerts on, best match first.
/// Each is recorded so it's returned once; `notified` says whether it was announced.
pub fn new_matches(
    conn: &Connection,
    settings: &AlertSettings,
    notified: bool,
) -> AppResult<Vec<Alert>> {
    let Some(since) = settings.since.as_deref() else {
        return Ok(Vec::new());
    };
    let mut best: HashMap<i64, (i64, Alert)> = HashMap::new();
    for (search_id, name, search) in searches::with_alerts(conn)? {
        for job in feed::feed_for(conn, &search)?.jobs {
            let wanted = job.state == JobState::New
                && job.eligibility.status != EligibilityStatus::Restricted
                && job.score >= settings.min_score
                && job.first_seen_at.as_str() >= since;
            if !wanted || best.get(&job.id).is_some_and(|(_, a)| a.score >= job.score) {
                continue;
            }
            best.insert(
                job.id,
                (
                    search_id,
                    Alert {
                        job_id: job.id,
                        title: job.title,
                        company: job.company,
                        score: job.score,
                        search: name.clone(),
                    },
                ),
            );
        }
    }
    let mut out = Vec::new();
    for (search_id, alert) in best.into_values() {
        let added = conn.execute(
            "INSERT OR IGNORE INTO job_alerts (job_id, search_id, score, notified)
             VALUES (?1, ?2, ?3, ?4)",
            params![alert.job_id, search_id, alert.score, notified],
        )?;
        if added > 0 {
            out.push(alert);
        }
    }
    out.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.job_id.cmp(&b.job_id)));
    Ok(out)
}

/// Notification title and body for a batch of alerts.
pub fn message(alerts: &[Alert]) -> Option<(String, String)> {
    let top = alerts.first()?;
    Some(match alerts.len() {
        1 => (
            format!("Great match: {}", top.title),
            format!("{} · {}% match", top.company, top.score),
        ),
        n => (
            format!("{n} great new matches"),
            format!(
                "{} at {} ({}%), and {} more.",
                top.title,
                top.company,
                top.score,
                n - 1
            ),
        ),
    })
}

pub fn notify(app: &AppHandle, title: &str, body: &str) -> AppResult<()> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| AppError::Other(format!("Couldn't show a notification: {e}")))
}

async fn tick(app: &AppHandle) -> AppResult<()> {
    let state = app.state::<AppState>();
    let settings = {
        let conn = state.db.lock().expect("db mutex poisoned");
        let settings = get_settings(&conn)?;
        if !due(&conn, &settings)? {
            return Ok(());
        }
        settings
    };
    // Skip this round if the user is refreshing right now.
    let Ok(_running) = state.refresh_lock.try_lock() else {
        return Ok(());
    };
    refresh::refresh(&state.db, &state.http, None).await?;
    let alerts = {
        let conn = state.db.lock().expect("db mutex poisoned");
        new_matches(&conn, &settings, settings.notify)?
    };
    if settings.notify
        && let Some((title, body)) = message(&alerts)
    {
        notify(app, &title, &body)?;
    }
    let _ = app.emit(FEED_UPDATED_EVENT, FeedUpdated { alerts });
    Ok(())
}

/// Starts the background loop. It runs only while the app is open.
pub fn start(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(STARTUP_DELAY).await;
        let mut every = tokio::time::interval(CHECK_EVERY);
        loop {
            every.tick().await;
            if let Err(e) = tick(&app).await {
                eprintln!("background refresh: {e}");
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        jobs::RawJob,
        profile::{Profile, SearchSettings, Skill},
    };

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn setup(conn: &Connection) {
        let skill = |n: &str| Skill {
            name: n.into(),
            ..Default::default()
        };
        store::save_profile(
            conn,
            &Profile {
                skills: vec![skill("React"), skill("TypeScript"), skill("GraphQL")],
                ..Default::default()
            },
        )
        .unwrap();
        searches::save_active(
            conn,
            &SearchSettings {
                titles: vec!["Frontend Engineer".into()],
                ..Default::default()
            },
        )
        .unwrap();
    }

    fn add(conn: &Connection, id: &str, title: &str, description: &str) -> i64 {
        let raw = RawJob {
            external_id: id.into(),
            title: title.into(),
            company: format!("Co {id}"),
            location: "Remote, worldwide".into(),
            description: description.into(),
            ..Default::default()
        };
        refresh::upsert_job(conn, "remotive", &raw).unwrap().0
    }

    fn on(min_score: u32) -> AlertSettings {
        AlertSettings {
            notify: true,
            min_score,
            ..Default::default()
        }
    }

    #[test]
    fn turning_alerts_on_starts_from_now() {
        let (_d, conn) = fresh();
        assert_eq!(get_settings(&conn).unwrap(), AlertSettings::default());
        let saved = save_settings(&conn, &on(80)).unwrap();
        let since = saved.since.clone().expect("since is set");
        // Saving again keeps the start; turning off clears it.
        let again = save_settings(
            &conn,
            &AlertSettings {
                every_hours: 6,
                ..saved
            },
        )
        .unwrap();
        assert_eq!(again.since.as_deref(), Some(since.as_str()));
        let off = save_settings(
            &conn,
            &AlertSettings {
                notify: false,
                ..again
            },
        )
        .unwrap();
        assert_eq!(off.since, None);
        assert!(
            save_settings(
                &conn,
                &AlertSettings {
                    every_hours: 0,
                    ..off.clone()
                }
            )
            .is_err()
        );
        assert!(
            save_settings(
                &conn,
                &AlertSettings {
                    min_score: 20,
                    ..off
                }
            )
            .is_err()
        );
    }

    #[test]
    fn announces_each_great_new_match_once() {
        let (_d, conn) = fresh();
        setup(&conn);
        let mut settings = on(80);
        settings.since = Some("2000-01-01T00:00:00Z".into());
        let great = add(
            &conn,
            "1",
            "Senior Frontend Engineer",
            "React, TypeScript and GraphQL every day.",
        );
        add(&conn, "2", "Accountant", "Ledgers and audits.");
        let saved = add(
            &conn,
            "3",
            "Frontend Engineer",
            "React, TypeScript, GraphQL.",
        );
        feed::set_state(&conn, saved, JobState::Saved).unwrap();

        let alerts = new_matches(&conn, &settings, true).unwrap();
        assert_eq!(alerts.iter().map(|a| a.job_id).collect::<Vec<_>>(), [great]);
        assert_eq!(alerts[0].search, "My search");
        assert!(new_matches(&conn, &settings, true).unwrap().is_empty());
    }

    #[test]
    fn skips_jobs_from_before_alerts_were_on_and_searches_with_alerts_off() {
        let (_d, conn) = fresh();
        setup(&conn);
        add(
            &conn,
            "1",
            "Frontend Engineer",
            "React, TypeScript, GraphQL.",
        );
        let mut settings = on(80);
        settings.since = Some("2999-01-01T00:00:00Z".into());
        assert!(new_matches(&conn, &settings, true).unwrap().is_empty());

        settings.since = Some("2000-01-01T00:00:00Z".into());
        let id = searches::active_id(&conn).unwrap().unwrap();
        searches::set_alerts(&conn, id, false).unwrap();
        assert!(new_matches(&conn, &settings, true).unwrap().is_empty());
        assert!(
            new_matches(&conn, &AlertSettings::default(), false)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn refresh_is_due_when_sources_are_stale() {
        let (_d, conn) = fresh();
        let settings = AlertSettings::default();
        assert!(due(&conn, &settings).unwrap(), "never fetched");
        conn.execute("UPDATE sources SET last_run_at = ?1", [now()])
            .unwrap();
        assert!(!due(&conn, &settings).unwrap());
        conn.execute(
            "UPDATE sources SET last_run_at = '2026-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
        assert!(due(&conn, &settings).unwrap());
        let off = AlertSettings {
            auto_refresh: false,
            ..settings
        };
        assert!(!due(&conn, &off).unwrap());
    }

    #[test]
    fn writes_one_or_many_alerts() {
        let alert = |id, score| Alert {
            job_id: id,
            title: "Frontend Engineer".into(),
            company: "Lumen".into(),
            score,
            search: "My search".into(),
        };
        assert_eq!(message(&[]), None);
        assert_eq!(
            message(&[alert(1, 91)]).unwrap(),
            (
                "Great match: Frontend Engineer".into(),
                "Lumen · 91% match".into()
            )
        );
        assert_eq!(
            message(&[alert(1, 91), alert(2, 84), alert(3, 80)]).unwrap(),
            (
                "3 great new matches".into(),
                "Frontend Engineer at Lumen (91%), and 2 more.".into()
            )
        );
    }
}
