//! Named searches. The feed, the settings form and interview prep use the active one;
//! background alerts check every search that has alerts on.

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    db::store,
    error::{AppError, AppResult},
    profile::SearchSettings,
};

const ACTIVE_KEY: &str = "active_search";
const DEFAULT_NAME: &str = "My search";
const MAX_NAME_CHARS: usize = 60;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SavedSearch {
    #[ts(type = "number")]
    pub id: i64,
    pub name: String,
    pub active: bool,
    /// Checked for great new matches after background refreshes.
    pub alerts: bool,
}

/// The active search, falling back to the oldest one if the stored id is gone.
pub fn active_id(conn: &Connection) -> AppResult<Option<i64>> {
    let stored: Option<i64> = store::get_setting(conn, ACTIVE_KEY)?;
    Ok(conn
        .query_row(
            "SELECT id FROM searches ORDER BY (id = ?1) DESC, id LIMIT 1",
            [stored],
            |r| r.get(0),
        )
        .optional()?)
}

pub fn list(conn: &Connection) -> AppResult<Vec<SavedSearch>> {
    let active = active_id(conn)?;
    let mut stmt = conn.prepare("SELECT id, name, alerts FROM searches ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        let id: i64 = r.get(0)?;
        Ok(SavedSearch {
            id,
            name: r.get(1)?,
            active: Some(id) == active,
            alerts: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<Result<_, _>>()?)
}

fn settings_of(conn: &Connection, id: i64) -> AppResult<SearchSettings> {
    let data: String = conn
        .query_row("SELECT data FROM searches WHERE id = ?1", [id], |r| {
            r.get(0)
        })
        .optional()?
        .ok_or_else(|| AppError::Other("That search no longer exists.".into()))?;
    store::from_json(&data)
}

pub fn active(conn: &Connection) -> AppResult<SearchSettings> {
    match active_id(conn)? {
        Some(id) => settings_of(conn, id),
        None => Ok(SearchSettings::default()),
    }
}

/// Saves the active search; the first save creates it.
pub fn save_active(conn: &Connection, settings: &SearchSettings) -> AppResult<()> {
    let data = store::to_json(settings)?;
    match active_id(conn)? {
        Some(id) => {
            conn.execute(
                "UPDATE searches SET data = ?2, updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
                 WHERE id = ?1",
                params![id, data],
            )?;
        }
        None => {
            conn.execute(
                "INSERT INTO searches (name, data) VALUES (?1, ?2)",
                params![DEFAULT_NAME, data],
            )?;
            store::set_setting(conn, ACTIVE_KEY, &conn.last_insert_rowid())?;
        }
    }
    Ok(())
}

/// Searches to check for alerts, with their settings.
pub fn with_alerts(conn: &Connection) -> AppResult<Vec<(i64, String, SearchSettings)>> {
    let mut stmt =
        conn.prepare("SELECT id, name, data FROM searches WHERE alerts = 1 ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter()
        .map(|(id, name, data)| Ok((id, name, store::from_json(&data)?)))
        .collect()
}

fn clean_name(conn: &Connection, name: &str, except: Option<i64>) -> AppResult<String> {
    let name = name.split_whitespace().collect::<Vec<_>>().join(" ");
    if name.is_empty() {
        return Err(AppError::Other("Give the search a name.".into()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(AppError::Other(format!(
            "Keep the name under {MAX_NAME_CHARS} characters."
        )));
    }
    let taken: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM searches WHERE lower(name) = lower(?1) AND id IS NOT ?2)",
        params![name, except],
        |r| r.get(0),
    )?;
    if taken {
        return Err(AppError::Other(format!(
            "You already have a search called “{name}”."
        )));
    }
    Ok(name)
}

/// Starts a new search from a copy of the active one and makes it active.
pub fn create(conn: &Connection, name: &str) -> AppResult<i64> {
    let name = clean_name(conn, name, None)?;
    let data = store::to_json(&active(conn)?)?;
    conn.execute(
        "INSERT INTO searches (name, data) VALUES (?1, ?2)",
        params![name, data],
    )?;
    let id = conn.last_insert_rowid();
    store::set_setting(conn, ACTIVE_KEY, &id)?;
    Ok(id)
}

fn changed(n: usize) -> AppResult<()> {
    if n == 0 {
        return Err(AppError::Other("That search no longer exists.".into()));
    }
    Ok(())
}

pub fn rename(conn: &Connection, id: i64, name: &str) -> AppResult<()> {
    let name = clean_name(conn, name, Some(id))?;
    changed(conn.execute(
        "UPDATE searches SET name = ?2 WHERE id = ?1",
        params![id, name],
    )?)
}

pub fn set_alerts(conn: &Connection, id: i64, on: bool) -> AppResult<()> {
    changed(conn.execute(
        "UPDATE searches SET alerts = ?2 WHERE id = ?1",
        params![id, on],
    )?)
}

pub fn set_active(conn: &Connection, id: i64) -> AppResult<SearchSettings> {
    let settings = settings_of(conn, id)?;
    store::set_setting(conn, ACTIVE_KEY, &id)?;
    Ok(settings)
}

/// Deletes a search. The last one can't be deleted; deleting the active one
/// makes the oldest remaining search active.
pub fn delete(conn: &Connection, id: i64) -> AppResult<()> {
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM searches", [], |r| r.get(0))?;
    if count <= 1 {
        return Err(AppError::Other("You need at least one search.".into()));
    }
    changed(conn.execute("DELETE FROM searches WHERE id = ?1", [id])?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    fn titled(t: &str) -> SearchSettings {
        SearchSettings {
            titles: vec![t.into()],
            country: "PT".into(),
            ..Default::default()
        }
    }

    #[test]
    fn first_save_creates_the_default_search() {
        let (_d, conn) = fresh();
        assert!(list(&conn).unwrap().is_empty());
        assert_eq!(active(&conn).unwrap(), SearchSettings::default());
        save_active(&conn, &titled("Designer")).unwrap();
        let all = list(&conn).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].name, "My search");
        assert!(all[0].active && all[0].alerts);
        assert_eq!(active(&conn).unwrap(), titled("Designer"));
    }

    #[test]
    fn creates_switches_renames_and_deletes() {
        let (_d, conn) = fresh();
        save_active(&conn, &titled("Designer")).unwrap();
        let first = active_id(&conn).unwrap().unwrap();

        // A new search starts as a copy of the active one and becomes active.
        let second = create(&conn, "  Staff   roles ").unwrap();
        assert_eq!(active(&conn).unwrap(), titled("Designer"));
        save_active(&conn, &titled("Staff Engineer")).unwrap();
        let names: Vec<_> = list(&conn)
            .unwrap()
            .into_iter()
            .map(|s| (s.name, s.active))
            .collect();
        assert_eq!(
            names,
            [
                ("My search".to_string(), false),
                ("Staff roles".to_string(), true)
            ]
        );

        assert_eq!(set_active(&conn, first).unwrap(), titled("Designer"));
        assert_eq!(active(&conn).unwrap(), titled("Designer"));

        assert!(create(&conn, "staff ROLES").is_err());
        assert!(create(&conn, "   ").is_err());
        rename(&conn, second, "Staff").unwrap();
        rename(&conn, second, "staff").unwrap(); // its own name, other case
        assert!(rename(&conn, first, "Staff").is_err());

        delete(&conn, first).unwrap();
        assert_eq!(active_id(&conn).unwrap(), Some(second));
        assert_eq!(active(&conn).unwrap(), titled("Staff Engineer"));
        assert!(delete(&conn, second).is_err(), "the last search stays");
    }

    #[test]
    fn alerts_can_be_turned_off_per_search() {
        let (_d, conn) = fresh();
        save_active(&conn, &titled("Designer")).unwrap();
        let second = create(&conn, "Other").unwrap();
        set_alerts(&conn, second, false).unwrap();
        let names: Vec<_> = with_alerts(&conn)
            .unwrap()
            .into_iter()
            .map(|(_, n, _)| n)
            .collect();
        assert_eq!(names, ["My search"]);
        assert!(set_alerts(&conn, 999, true).is_err());
    }

    #[test]
    fn migration_keeps_the_old_search() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("t.db");
        let conn = Connection::open(&path).unwrap();
        // Bring a database up to v7 and store a search the old way.
        for (i, sql) in [
            include_str!("db/migrations/0001_init.sql"),
            include_str!("db/migrations/0002_resume_meta.sql"),
            include_str!("db/migrations/0003_job_feed.sql"),
            include_str!("db/migrations/0004_job_analysis.sql"),
            include_str!("db/migrations/0005_apply.sql"),
            include_str!("db/migrations/0006_tracker.sql"),
            include_str!("db/migrations/0007_prep.sql"),
        ]
        .iter()
        .enumerate()
        {
            conn.execute_batch(sql).unwrap();
            conn.pragma_update(None, "user_version", i as u32 + 1)
                .unwrap();
        }
        conn.execute(
            "INSERT INTO search_settings (id, data) VALUES (1, ?1)",
            [store::to_json(&titled("Designer")).unwrap()],
        )
        .unwrap();
        drop(conn);

        let conn = crate::db::open(&path).unwrap();
        assert_eq!(active(&conn).unwrap(), titled("Designer"));
        assert_eq!(list(&conn).unwrap()[0].name, "My search");
    }
}
