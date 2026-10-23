use std::path::Path;

use serde::Serialize;
use tauri::{State, ipc::InvokeBody};
use ts_rs::TS;

use crate::{
    db::{self, store},
    error::{AppError, AppResult},
    profile::{Profile, SearchSettings, SetupStatus, StoredProfile},
    resume,
    state::AppState,
};

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AppInfo {
    pub version: String,
    pub data_dir: String,
    pub db_path: String,
    pub schema_version: u32,
    #[ts(type = "number")]
    pub db_size_bytes: u64,
}

#[tauri::command]
pub fn app_info(app: tauri::AppHandle, state: State<'_, AppState>) -> AppResult<AppInfo> {
    let conn = lock(&state);
    let schema_version = db::schema_version(&conn)?;
    let db_size_bytes = std::fs::metadata(&state.db_path)
        .map(|m| m.len())
        .unwrap_or(0);
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_dir: state.data_dir.display().to_string(),
        db_path: state.db_path.display().to_string(),
        schema_version,
        db_size_bytes,
    })
}

fn lock(state: &AppState) -> std::sync::MutexGuard<'_, rusqlite::Connection> {
    state.db.lock().expect("db mutex poisoned")
}

#[tauri::command]
pub fn get_profile(state: State<'_, AppState>) -> AppResult<Option<StoredProfile>> {
    store::get_profile(&lock(&state))
}

#[tauri::command]
pub fn save_profile(state: State<'_, AppState>, profile: Profile) -> AppResult<()> {
    store::save_profile(&lock(&state), &profile)
}

/// Takes the raw file bytes as the request body and its name in `x-file-name`
/// (URI-encoded). Parses offline, keeps a copy of the file, replaces the profile.
#[tauri::command]
pub async fn import_resume(
    state: State<'_, AppState>,
    request: tauri::ipc::Request<'_>,
) -> AppResult<StoredProfile> {
    let InvokeBody::Raw(bytes) = request.body() else {
        return Err(AppError::Other(
            "expected the resume file as raw bytes".into(),
        ));
    };
    let file_name = request
        .headers()
        .get("x-file-name")
        .and_then(|v| v.to_str().ok())
        .map(percent_decode)
        .unwrap_or_default();
    let file_name = Path::new(&file_name)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .unwrap_or("resume")
        .to_string();

    let bytes = bytes.clone();
    let (parsed, bytes, file_name) = tauri::async_runtime::spawn_blocking(move || {
        resume::read(&file_name, &bytes).map(|p| (p, bytes, file_name))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;

    // Keep only the latest resume file, for backups and re-reading later.
    let dir = state.data_dir.join("resume");
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join(&file_name), &bytes)?;

    let conn = lock(&state);
    store::save_import(
        &conn,
        &parsed.profile,
        &parsed.lines,
        &file_name,
        bytes.len() as u64,
        parsed.pages,
    )?;
    store::get_profile(&conn)?.ok_or_else(|| AppError::Other("profile was not saved".into()))
}

#[tauri::command]
pub fn get_search_settings(state: State<'_, AppState>) -> AppResult<SearchSettings> {
    store::get_search_settings(&lock(&state))
}

#[tauri::command]
pub fn save_search_settings(state: State<'_, AppState>, settings: SearchSettings) -> AppResult<()> {
    store::save_search_settings(&lock(&state), &settings)
}

#[tauri::command]
pub fn get_setup_status(state: State<'_, AppState>) -> AppResult<SetupStatus> {
    store::get_setup_status(&lock(&state))
}

#[tauri::command]
pub fn set_setup_status(state: State<'_, AppState>, status: SetupStatus) -> AppResult<()> {
    store::set_setup_status(&lock(&state), status)
}

/// Decodes `encodeURIComponent` output. Invalid escapes are kept as-is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s
                .get(i + 1..i + 3)
                .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::percent_decode;

    #[test]
    fn decodes_uri_components() {
        assert_eq!(percent_decode("S%C3%A1m%20CV.pdf"), "Sám CV.pdf");
        assert_eq!(percent_decode("100%"), "100%");
    }
}
