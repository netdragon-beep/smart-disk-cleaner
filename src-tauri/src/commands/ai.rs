use crate::state::AppState;
use smart_disk_cleaner_core::ai_advisor::{explain_path, AdvisorConfig};
use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::FileAiInsight;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn explain_file_with_ai(
    path: String,
    config: Option<AppConfig>,
    state: State<'_, AppState>,
) -> Result<FileAiInsight, String> {
    let report = state
        .last_report
        .lock()
        .await
        .clone()
        .ok_or_else(|| "请先完成一次扫描，再进行单文件 AI 解读。".to_string())?;
    let config = config.unwrap_or_else(|| state.load_config());

    explain_path(
        &report,
        &PathBuf::from(path),
        &AdvisorConfig {
            api_key: config.api_key,
            base_url: config.ai_base_url,
            model: config.ai_model,
            max_items: config.max_ai_items,
            strict_file_ai_remote_only: config.strict_file_ai_remote_only,
        },
    )
    .await
    .map_err(|e| e.to_string())
}
