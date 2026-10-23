//! Reads and writes for the single-row profile/search tables and app settings.

use rusqlite::{Connection, OptionalExtension, params};

use crate::{
    error::{AppError, AppResult},
    profile::{Profile, ResumeMeta, SearchSettings, SetupStatus, StoredProfile},
};

const SETUP_STATUS_KEY: &str = "setup_status";

fn to_json<T: serde::Serialize>(value: &T) -> AppResult<String> {
    serde_json::to_string(value).map_err(|e| AppError::Other(e.to_string()))
}

fn from_json<T: serde::de::DeserializeOwned>(s: &str) -> AppResult<T> {
    serde_json::from_str(s).map_err(|e| AppError::Other(format!("stored data is unreadable: {e}")))
}

pub fn get_profile(conn: &Connection) -> AppResult<Option<StoredProfile>> {
    let row = conn
        .query_row(
            "SELECT data, resume_file_name, resume_text, parsed_at, resume_size_bytes, resume_pages
             FROM profile WHERE id = 1",
            [],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, Option<String>>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, Option<String>>(3)?,
                    r.get::<_, Option<i64>>(4)?,
                    r.get::<_, Option<u32>>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((data, file_name, text, parsed_at, size, pages)) = row else {
        return Ok(None);
    };
    let resume = match (file_name, parsed_at) {
        (Some(file_name), Some(imported_at)) => Some(ResumeMeta {
            file_name,
            size_bytes: size.unwrap_or(0) as u64,
            pages,
            imported_at,
        }),
        _ => None,
    };
    Ok(Some(StoredProfile {
        profile: from_json(&data)?,
        resume,
        resume_lines: text
            .map(|t| t.lines().map(str::to_string).collect())
            .unwrap_or_default(),
    }))
}

/// Saves user edits. Leaves the stored resume text untouched.
pub fn save_profile(conn: &Connection, profile: &Profile) -> AppResult<()> {
    conn.execute(
        "INSERT INTO profile (id, data) VALUES (1, ?1)
         ON CONFLICT (id) DO UPDATE SET
            data = excluded.data,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![to_json(profile)?],
    )?;
    Ok(())
}

/// Replaces the profile with a freshly parsed resume.
pub fn save_import(
    conn: &Connection,
    profile: &Profile,
    lines: &[String],
    file_name: &str,
    size_bytes: u64,
    pages: Option<u32>,
) -> AppResult<()> {
    conn.execute(
        "INSERT INTO profile (id, data, resume_file_name, resume_text, parsed_at, resume_size_bytes, resume_pages)
         VALUES (1, ?1, ?2, ?3, strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), ?4, ?5)
         ON CONFLICT (id) DO UPDATE SET
            data = excluded.data,
            resume_file_name = excluded.resume_file_name,
            resume_text = excluded.resume_text,
            parsed_at = excluded.parsed_at,
            resume_size_bytes = excluded.resume_size_bytes,
            resume_pages = excluded.resume_pages,
            updated_at = excluded.parsed_at",
        params![
            to_json(profile)?,
            file_name,
            lines.join("\n"),
            size_bytes as i64,
            pages
        ],
    )?;
    Ok(())
}

pub fn get_search_settings(conn: &Connection) -> AppResult<SearchSettings> {
    let data: Option<String> = conn
        .query_row("SELECT data FROM search_settings WHERE id = 1", [], |r| {
            r.get(0)
        })
        .optional()?;
    data.map_or_else(|| Ok(SearchSettings::default()), |d| from_json(&d))
}

pub fn save_search_settings(conn: &Connection, settings: &SearchSettings) -> AppResult<()> {
    conn.execute(
        "INSERT INTO search_settings (id, data) VALUES (1, ?1)
         ON CONFLICT (id) DO UPDATE SET
            data = excluded.data,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![to_json(settings)?],
    )?;
    Ok(())
}

fn get_setting<T: serde::de::DeserializeOwned>(
    conn: &Connection,
    key: &str,
) -> AppResult<Option<T>> {
    let value: Option<String> = conn
        .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
            r.get(0)
        })
        .optional()?;
    value.map(|v| from_json(&v)).transpose()
}

fn set_setting<T: serde::Serialize>(conn: &Connection, key: &str, value: &T) -> AppResult<()> {
    conn.execute(
        "INSERT INTO settings (key, value) VALUES (?1, ?2)
         ON CONFLICT (key) DO UPDATE SET
            value = excluded.value,
            updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
        params![key, to_json(value)?],
    )?;
    Ok(())
}

pub fn get_setup_status(conn: &Connection) -> AppResult<SetupStatus> {
    Ok(get_setting(conn, SETUP_STATUS_KEY)?.unwrap_or(SetupStatus::Pending))
}

pub fn set_setup_status(conn: &Connection, status: SetupStatus) -> AppResult<()> {
    set_setting(conn, SETUP_STATUS_KEY, &status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Role, Seniority};

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = crate::db::open(&dir.path().join("t.db")).unwrap();
        (dir, conn)
    }

    #[test]
    fn profile_is_absent_until_saved() {
        let (_d, conn) = fresh();
        assert!(get_profile(&conn).unwrap().is_none());
    }

    #[test]
    fn import_then_edit_keeps_resume_lines() {
        let (_d, conn) = fresh();
        let mut profile = Profile::default();
        profile.contact.name = "Sam".into();
        let lines = vec!["Sam".to_string(), "Engineer".to_string()];
        save_import(&conn, &profile, &lines, "cv.pdf", 1234, Some(2)).unwrap();

        profile.roles.push(Role {
            id: "r1".into(),
            title: "Engineer".into(),
            ..Default::default()
        });
        save_profile(&conn, &profile).unwrap();

        let stored = get_profile(&conn).unwrap().unwrap();
        assert_eq!(stored.profile, profile);
        assert_eq!(stored.resume_lines, lines);
        let meta = stored.resume.unwrap();
        assert_eq!(meta.file_name, "cv.pdf");
        assert_eq!(meta.size_bytes, 1234);
        assert_eq!(meta.pages, Some(2));
    }

    #[test]
    fn manual_profile_has_no_resume() {
        let (_d, conn) = fresh();
        save_profile(&conn, &Profile::default()).unwrap();
        let stored = get_profile(&conn).unwrap().unwrap();
        assert!(stored.resume.is_none());
        assert!(stored.resume_lines.is_empty());
    }

    #[test]
    fn search_settings_round_trip() {
        let (_d, conn) = fresh();
        assert_eq!(
            get_search_settings(&conn).unwrap(),
            SearchSettings::default()
        );
        let s = SearchSettings {
            titles: vec!["Frontend Engineer".into()],
            country: "PT".into(),
            timezone: "UTC+00:00".into(),
            min_salary: Some(60000),
            currency: "EUR".into(),
            seniority: vec![Seniority::Senior],
            ..Default::default()
        };
        save_search_settings(&conn, &s).unwrap();
        assert_eq!(get_search_settings(&conn).unwrap(), s);
    }

    #[test]
    fn setup_status_defaults_to_pending() {
        let (_d, conn) = fresh();
        assert_eq!(get_setup_status(&conn).unwrap(), SetupStatus::Pending);
        set_setup_status(&conn, SetupStatus::Skipped).unwrap();
        assert_eq!(get_setup_status(&conn).unwrap(), SetupStatus::Skipped);
        set_setup_status(&conn, SetupStatus::Done).unwrap();
        assert_eq!(get_setup_status(&conn).unwrap(), SetupStatus::Done);
    }
}
