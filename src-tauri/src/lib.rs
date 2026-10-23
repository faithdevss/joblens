mod commands;
mod db;
mod error;
mod profile;
mod resume;
mod state;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(state::AppState::init(data_dir)?);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            commands::get_profile,
            commands::save_profile,
            commands::import_resume,
            commands::get_search_settings,
            commands::save_search_settings,
            commands::get_setup_status,
            commands::set_setup_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
