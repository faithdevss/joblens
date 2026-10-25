use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
};

use rusqlite::Connection;

use crate::{db, error::AppResult, jobs::sources};

pub struct AppState {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub db: Mutex<Connection>,
    pub http: reqwest::Client,
    /// Cancel flags for AI runs in progress, by run key (e.g. `analysis:12`).
    pub ai_runs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    /// Held while sources are being fetched, so background and manual refreshes don't overlap.
    pub refresh_lock: tokio::sync::Mutex<()>,
}

impl AppState {
    pub fn init(data_dir: PathBuf) -> AppResult<Self> {
        std::fs::create_dir_all(&data_dir)?;
        let db_path = data_dir.join(db::DB_FILE_NAME);
        let conn = db::open(&db_path)?;
        Ok(Self {
            data_dir,
            db_path,
            db: Mutex::new(conn),
            http: sources::client(),
            ai_runs: Mutex::new(HashMap::new()),
            refresh_lock: tokio::sync::Mutex::new(()),
        })
    }
}
