use std::path::Path;

use rusqlite::Connection;

use crate::error::AppResult;

pub mod store;

pub const DB_FILE_NAME: &str = "joblens.db";

/// Ordered list of migrations. Index + 1 is the schema version it produces.
/// Never edit a shipped migration; append a new one instead.
const MIGRATIONS: &[&str] = &[
    include_str!("migrations/0001_init.sql"),
    include_str!("migrations/0002_resume_meta.sql"),
    include_str!("migrations/0003_job_feed.sql"),
    include_str!("migrations/0004_job_analysis.sql"),
    include_str!("migrations/0005_apply.sql"),
    include_str!("migrations/0006_tracker.sql"),
    include_str!("migrations/0007_prep.sql"),
    include_str!("migrations/0008_searches_alerts.sql"),
    include_str!("migrations/0009_more_sources.sql"),
];

pub fn open(path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(path)?;
    configure(&conn)?;
    migrate(&conn)?;
    Ok(conn)
}

fn configure(conn: &Connection) -> AppResult<()> {
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

pub fn schema_version(conn: &Connection) -> AppResult<u32> {
    Ok(conn.pragma_query_value(None, "user_version", |row| row.get(0))?)
}

pub fn latest_version() -> u32 {
    MIGRATIONS.len() as u32
}

/// Applies pending migrations, each in its own transaction.
pub fn migrate(conn: &Connection) -> AppResult<()> {
    let current = schema_version(conn)?;
    if current > latest_version() {
        return Err(crate::error::AppError::Other(format!(
            "database schema v{current} is newer than this app (v{}); update JobLens",
            latest_version()
        )));
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(current as usize) {
        let tx = conn.unchecked_transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as u32)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let conn = open(&dir.path().join(DB_FILE_NAME)).unwrap();
        (dir, conn)
    }

    #[test]
    fn migrates_fresh_db_to_latest() {
        let (_dir, conn) = fresh();
        assert_eq!(schema_version(&conn).unwrap(), latest_version());
    }

    #[test]
    fn migrate_is_idempotent() {
        let (_dir, conn) = fresh();
        migrate(&conn).unwrap();
        assert_eq!(schema_version(&conn).unwrap(), latest_version());
    }

    #[test]
    fn creates_expected_tables() {
        let (_dir, conn) = fresh();
        let mut stmt = conn
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table' ORDER BY name")
            .unwrap();
        let tables: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        for t in [
            "applications",
            "drafts",
            "job_analysis",
            "job_sources",
            "jobs",
            "profile",
            "searches",
            "settings",
            "sources",
        ] {
            assert!(tables.iter().any(|n| n == t), "missing table {t}");
        }
    }

    #[test]
    fn enforces_foreign_keys() {
        let (_dir, conn) = fresh();
        let res = conn.execute(
            "INSERT INTO applications (job_id, status) VALUES (999, 'saved')",
            [],
        );
        assert!(res.is_err());
    }

    #[test]
    fn rejects_db_from_newer_app() {
        let (_dir, conn) = fresh();
        conn.pragma_update(None, "user_version", latest_version() + 1)
            .unwrap();
        assert!(migrate(&conn).is_err());
    }
}
