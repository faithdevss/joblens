use std::{path::PathBuf, sync::Mutex};

use rusqlite::Connection;

use crate::{db, error::AppResult};

pub struct AppState {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub db: Mutex<Connection>,
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
        })
    }
}
