//! The user's data as a whole: what's stored where, backup to one file, restore on
//! another computer, and delete everything.

use std::{
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    db,
    error::{AppError, AppResult},
};

const BACKUP_FORMAT: u32 = 1;
const MANIFEST: &str = "manifest.json";
/// Kept in the data folder when restoring, in case the backup was the wrong one.
pub const BEFORE_RESTORE: &str = "joblens-before-restore.db";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    format: u32,
    app_version: String,
    schema_version: u32,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct DataOverview {
    pub data_dir: String,
    #[ts(type = "number")]
    pub total_bytes: u64,
    #[ts(type = "number")]
    pub db_bytes: u64,
    pub resume_file: Option<String>,
    pub jobs: u32,
    pub applications: u32,
    pub drafts: u32,
}

fn dir_size(p: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(p) else {
        return std::fs::metadata(p).map(|m| m.len()).unwrap_or(0);
    };
    entries
        .flatten()
        .map(|e| {
            let path = e.path();
            if path.is_dir() {
                dir_size(&path)
            } else {
                e.metadata().map(|m| m.len()).unwrap_or(0)
            }
        })
        .sum()
}

fn count(conn: &Connection, sql: &str) -> AppResult<u32> {
    Ok(conn.query_row(sql, [], |r| r.get(0))?)
}

pub fn overview(conn: &Connection, data_dir: &Path, db_path: &Path) -> AppResult<DataOverview> {
    let db_bytes = [
        db_path.to_path_buf(),
        sibling(db_path, "-wal"),
        sibling(db_path, "-shm"),
    ]
    .iter()
    .map(|p| std::fs::metadata(p).map(|m| m.len()).unwrap_or(0))
    .sum();
    Ok(DataOverview {
        data_dir: data_dir.display().to_string(),
        total_bytes: dir_size(data_dir),
        db_bytes,
        resume_file: conn
            .query_row(
                "SELECT resume_file_name FROM profile WHERE id = 1",
                [],
                |r| r.get(0),
            )
            .unwrap_or(None),
        jobs: count(conn, "SELECT COUNT(*) FROM jobs")?,
        applications: count(conn, "SELECT COUNT(*) FROM applications")?,
        drafts: count(conn, "SELECT COUNT(*) FROM drafts")?,
    })
}

fn sibling(db_path: &Path, suffix: &str) -> PathBuf {
    let mut s = db_path.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

fn zip_err(e: zip::result::ZipError) -> AppError {
    AppError::Other(format!("The backup file is damaged: {e}"))
}

/// A consistent copy of the open database, written to `to`.
fn copy_db(conn: &Connection, to: &Path) -> AppResult<()> {
    if to.exists() {
        std::fs::remove_file(to)?;
    }
    conn.execute("VACUUM INTO ?1", [to.display().to_string()])?;
    Ok(())
}

/// Writes one zip with the database, the stored resume file and a manifest.
pub fn backup(conn: &Connection, data_dir: &Path, out: &Path, app_version: &str) -> AppResult<()> {
    let tmp = data_dir.join("backup-tmp.db");
    copy_db(conn, &tmp)?;
    let db_bytes = std::fs::read(&tmp);
    std::fs::remove_file(&tmp).ok();
    let db_bytes = db_bytes?;

    let manifest = Manifest {
        format: BACKUP_FORMAT,
        app_version: app_version.into(),
        schema_version: db::schema_version(conn)?,
        created_at: crate::jobs::dates::now(),
    };
    let mut zip = zip::ZipWriter::new(std::fs::File::create(out)?);
    let opts = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(MANIFEST, opts).map_err(zip_err)?;
    zip.write_all(
        serde_json::to_string_pretty(&manifest)
            .unwrap_or_default()
            .as_bytes(),
    )?;
    zip.start_file(db::DB_FILE_NAME, opts).map_err(zip_err)?;
    zip.write_all(&db_bytes)?;
    if let Ok(entries) = std::fs::read_dir(data_dir.join("resume")) {
        for e in entries.flatten().filter(|e| e.path().is_file()) {
            let name = e.file_name().to_string_lossy().into_owned();
            zip.start_file(format!("resume/{name}"), opts)
                .map_err(zip_err)?;
            zip.write_all(&std::fs::read(e.path())?)?;
        }
    }
    zip.finish().map_err(zip_err)?;
    Ok(())
}

/// A checked backup, unpacked and ready to swap in.
pub struct Restore {
    db_file: PathBuf,
    resume: Vec<(String, Vec<u8>)>,
}

/// Unpacks a backup into `staging` and checks it opens with this app version.
pub fn read_backup(bytes: &[u8], staging: &Path) -> AppResult<Restore> {
    let not_backup = || AppError::Other("That file isn’t a JobLens backup.".into());
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(|_| not_backup())?;
    let manifest: Manifest = {
        let mut f = zip.by_name(MANIFEST).map_err(|_| not_backup())?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        serde_json::from_str(&s).map_err(|_| not_backup())?
    };
    if manifest.format > BACKUP_FORMAT || manifest.schema_version > db::latest_version() {
        return Err(AppError::Other(format!(
            "This backup was made by a newer JobLens ({}). Update the app first.",
            manifest.app_version
        )));
    }

    if staging.exists() {
        std::fs::remove_dir_all(staging)?;
    }
    std::fs::create_dir_all(staging)?;
    let db_file = staging.join(db::DB_FILE_NAME);
    {
        let mut f = zip.by_name(db::DB_FILE_NAME).map_err(|_| not_backup())?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        std::fs::write(&db_file, buf)?;
    }
    // Opening runs any migrations it needs and refuses newer schemas.
    let conn = db::open(&db_file)
        .map_err(|e| AppError::Other(format!("The backup can’t be read: {e}")))?;
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    drop(conn);

    let mut resume = Vec::new();
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(zip_err)?;
        let entry = f.name().map_err(zip_err)?.to_string();
        let Some(rest) = entry.strip_prefix("resume/").map(str::to_string) else {
            continue;
        };
        // Only plain file names: nothing may land outside the resume folder.
        let Some(name) = Path::new(&rest)
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.is_empty() && *n == rest)
            .map(str::to_string)
        else {
            continue;
        };
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        resume.push((name, buf));
    }
    Ok(Restore { db_file, resume })
}

fn remove_db_files(db_path: &Path) -> AppResult<()> {
    for p in [
        db_path.to_path_buf(),
        sibling(db_path, "-wal"),
        sibling(db_path, "-shm"),
    ] {
        if p.exists() {
            std::fs::remove_file(p)?;
        }
    }
    Ok(())
}

fn remove_dir(p: &Path) -> AppResult<()> {
    if p.exists() {
        std::fs::remove_dir_all(p)?;
    }
    Ok(())
}

/// Replaces the open database, in place: with a restored backup, or with a fresh empty
/// one when `restore` is `None` (delete everything).
pub fn swap(
    conn: &mut Connection,
    data_dir: &Path,
    db_path: &Path,
    restore: Option<Restore>,
) -> AppResult<()> {
    if restore.is_some() {
        copy_db(conn, &data_dir.join(BEFORE_RESTORE))?;
    }
    // Close the file before touching it.
    *conn = Connection::open_in_memory()?;
    let result = (|| {
        remove_db_files(db_path)?;
        let resume_dir = data_dir.join("resume");
        remove_dir(&resume_dir)?;
        match &restore {
            Some(r) => {
                std::fs::rename(&r.db_file, db_path)?;
                if !r.resume.is_empty() {
                    std::fs::create_dir_all(&resume_dir)?;
                    for (name, bytes) in &r.resume {
                        std::fs::write(resume_dir.join(name), bytes)?;
                    }
                }
            }
            None => {
                remove_dir(&data_dir.join("ai"))?;
                remove_dir(&data_dir.join("exports"))?;
                std::fs::remove_file(data_dir.join(BEFORE_RESTORE)).ok();
            }
        }
        Ok::<_, AppError>(())
    })();
    // Always reopen, so the app keeps working even if a step failed.
    *conn = db::open(db_path)?;
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (tempfile::TempDir, PathBuf, Connection) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join(db::DB_FILE_NAME);
        let conn = db::open(&db_path).unwrap();
        conn.execute(
            "INSERT INTO jobs (canonical_key, title, company) VALUES ('k', 'Engineer', 'Acme')",
            [],
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("resume")).unwrap();
        std::fs::write(dir.path().join("resume/cv.pdf"), b"%PDF").unwrap();
        (dir, db_path, conn)
    }

    fn jobs(conn: &Connection) -> u32 {
        count(conn, "SELECT COUNT(*) FROM jobs").unwrap()
    }

    #[test]
    fn backup_restores_on_another_computer() {
        let (a, _, conn) = setup();
        let file = a.path().join("backup.zip");
        backup(&conn, a.path(), &file, "0.1.0").unwrap();
        let bytes = std::fs::read(&file).unwrap();

        // A second, empty install.
        let b = tempfile::tempdir().unwrap();
        let b_db = b.path().join(db::DB_FILE_NAME);
        let mut other = db::open(&b_db).unwrap();
        assert_eq!(jobs(&other), 0);
        let r = read_backup(&bytes, &b.path().join("restore")).unwrap();
        swap(&mut other, b.path(), &b_db, Some(r)).unwrap();
        assert_eq!(jobs(&other), 1);
        assert_eq!(
            std::fs::read(b.path().join("resume/cv.pdf")).unwrap(),
            b"%PDF"
        );
        // The data it replaced is kept aside.
        assert!(b.path().join(BEFORE_RESTORE).exists());
        let o = overview(&other, b.path(), &b_db).unwrap();
        assert_eq!(o.jobs, 1);
        assert!(o.total_bytes > 0);
    }

    #[test]
    fn refuses_other_files_and_newer_backups() {
        let dir = tempfile::tempdir().unwrap();
        assert!(read_backup(b"not a zip", dir.path()).is_err());

        let mut buf = Cursor::new(Vec::new());
        {
            let mut z = zip::ZipWriter::new(&mut buf);
            z.start_file(MANIFEST, zip::write::SimpleFileOptions::default())
                .unwrap();
            z.write_all(
                format!(
                    r#"{{"format":1,"appVersion":"9.0.0","schemaVersion":{},"createdAt":""}}"#,
                    db::latest_version() + 1
                )
                .as_bytes(),
            )
            .unwrap();
            z.finish().unwrap();
        }
        let e = read_backup(buf.get_ref(), dir.path()).err().unwrap();
        assert!(e.to_string().contains("newer JobLens"), "{e}");
    }

    #[test]
    fn delete_everything_leaves_a_fresh_db() {
        let (dir, db_path, mut conn) = setup();
        std::fs::create_dir_all(dir.path().join("ai")).unwrap();
        swap(&mut conn, dir.path(), &db_path, None).unwrap();
        assert_eq!(jobs(&conn), 0);
        assert_eq!(db::schema_version(&conn).unwrap(), db::latest_version());
        assert!(!dir.path().join("resume").exists());
        assert!(!dir.path().join("ai").exists());
    }
}
