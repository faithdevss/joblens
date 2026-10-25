mod ai;
mod alerts;
mod apply;
mod commands;
mod data;
mod db;
mod error;
mod jobs;
mod profile;
#[cfg(test)]
mod qa_tests;
mod resume;
mod searches;
mod state;
mod tracker;

use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            app.manage(state::AppState::init(data_dir)?);
            alerts::start(app.handle().clone());
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
            commands::get_feed,
            commands::get_job,
            commands::set_job_state,
            commands::list_sources,
            commands::set_source_enabled,
            commands::refresh_sources,
            commands::add_watchlist_company,
            commands::remove_source,
            commands::add_manual_job,
            commands::get_weights,
            commands::save_weights,
            commands::get_ai_settings,
            commands::get_ai_overview,
            commands::set_ai_provider,
            commands::analyze_job,
            commands::cancel_analysis,
            commands::get_gap_check,
            commands::decide_edit,
            commands::suggest_edits,
            commands::get_writer,
            commands::write_draft,
            commands::rewrite_paragraph,
            commands::set_paragraph_version,
            commands::edit_paragraph,
            commands::resolve_sentence,
            commands::update_draft_text,
            commands::export_draft,
            commands::cancel_writing,
            commands::list_applications,
            commands::update_application,
            commands::mark_applied,
            commands::mark_followed_up,
            commands::export_applications_csv,
            commands::get_data_overview,
            commands::create_backup,
            commands::restore_backup,
            commands::delete_all_data,
            commands::get_interview_prep,
            commands::set_topic_prepared,
            commands::check_still_open,
            commands::list_searches,
            commands::create_search,
            commands::rename_search,
            commands::delete_search,
            commands::set_active_search,
            commands::set_search_alerts,
            commands::get_alert_settings,
            commands::save_alert_settings,
            commands::send_test_alert,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
