use crate::state::AppState;
use smart_disk_cleaner_core::ai_advisor::{fetch_models, test_connection, AdvisorConfig};
use smart_disk_cleaner_core::config::AppConfig;
use tauri::State;

#[tauri::command]
pub async fn load_config(state: State<'_, AppState>) -> Result<AppConfig, String> {
    AppConfig::load(&state.config_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn save_config(config: AppConfig, state: State<'_, AppState>) -> Result<(), String> {
    config.save(&state.config_path).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn test_ai_config(config: AppConfig) -> Result<String, String> {
    test_connection(&AdvisorConfig {
        api_key: config.api_key,
        base_url: config.ai_base_url,
        model: config.ai_model,
        max_items: config.max_ai_items,
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_ai_models(config: AppConfig) -> Result<Vec<String>, String> {
    fetch_models(&AdvisorConfig {
        api_key: config.api_key,
        base_url: config.ai_base_url,
        model: config.ai_model,
        max_items: config.max_ai_items,
    })
    .await
    .map_err(|e| e.to_string())
}
