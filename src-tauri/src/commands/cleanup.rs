use crate::state::AppState;
use smart_disk_cleaner_core::executor::{execute_from_report, ExecuteOptions};
use smart_disk_cleaner_core::models::{ExecutionMode, OperationLogEntry};
use smart_disk_cleaner_core::reporter::write_report;
use std::fs;
use std::path::PathBuf;
use tauri::State;

#[tauri::command]
pub async fn execute_cleanup(
    paths: Vec<String>,
    mode: String,
    target_dir: Option<String>,
    dry_run: bool,
    state: State<'_, AppState>,
) -> Result<Vec<OperationLogEntry>, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or("\u{5F53}\u{524D}\u{6CA1}\u{6709}\u{626B}\u{63CF}\u{62A5}\u{544A}\u{FF0C}\u{8BF7}\u{5148}\u{6267}\u{884C}\u{626B}\u{63CF}\u{3002}")?;

    let temp_report_path = state
        .config_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .join("runtime")
        .join("sdc_temp_report.json");
    if let Some(parent) = temp_report_path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    write_report(&temp_report_path, report).map_err(|e| e.to_string())?;

    let exec_mode = match mode.as_str() {
        "move" => ExecutionMode::Move,
        _ => ExecutionMode::Recycle,
    };

    let options = ExecuteOptions {
        report_path: temp_report_path.clone(),
        mode: exec_mode,
        paths: paths.iter().map(PathBuf::from).collect(),
        target_dir: target_dir.map(PathBuf::from),
        dry_run,
    };

    let logs = execute_from_report(&options).map_err(|e| e.to_string())?;
    let _ = fs::remove_file(&temp_report_path);

    let mut history = state.history.lock().await;
    history.extend(logs.clone());

    Ok(logs)
}

#[tauri::command]
pub async fn get_operation_history(
    state: State<'_, AppState>,
) -> Result<Vec<OperationLogEntry>, String> {
    let history = state.history.lock().await;
    Ok(history.clone())
}
