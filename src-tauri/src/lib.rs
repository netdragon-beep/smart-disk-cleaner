mod commands;
mod events;
mod state;

use state::AppState;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            commands::scan::start_scan,
            commands::scan::cancel_scan,
            commands::scan::diagnose_path,
            commands::cleanup::execute_cleanup,
            commands::cleanup::get_operation_history,
            commands::config::load_config,
            commands::config::save_config,
            commands::config::list_ai_models,
            commands::config::test_ai_config,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
