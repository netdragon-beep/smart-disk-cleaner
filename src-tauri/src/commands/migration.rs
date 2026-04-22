use crate::state::AppState;
use chrono::Utc;
use serde::Deserialize;
use serde_yaml::{Mapping, Sequence, Value};
use smart_disk_cleaner_core::ai_advisor::AdvisorConfig;
use smart_disk_cleaner_core::migration_advisor::build_migration_advice;
use smart_disk_cleaner_core::migration_planner::refine_plan_with_ai;
use smart_disk_cleaner_core::models::{
    DiagnosticCode, DiagnosticSeverity, ExecutionMode, MigrationAction, MigrationActionKind,
    MigrationAdvisorOutput, MigrationCheckpoint, MigrationCheckpointKind, MigrationExecutionRecord,
    MigrationPlan, MigrationPlanReview, MigrationRollbackAction, MigrationRunStatus,
    OperationLogEntry, PathDiagnosis,
};
use smart_disk_cleaner_core::platform::{
    is_macos_system_sensitive_path, is_windows_system_sensitive_path,
};
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tauri::State;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlanExecutionRequest {
    plan: MigrationPlan,
    selected_action_ids: Vec<String>,
    dry_run: bool,
}

#[tauri::command]
pub async fn get_migration_advice(
    target_root: Option<String>,
    state: State<'_, AppState>,
) -> Result<MigrationAdvisorOutput, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or("当前没有扫描报告，请先执行扫描。")?;

    let target_root = target_root
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from);

    Ok(build_migration_advice(report, target_root.as_deref()))
}

#[tauri::command]
pub async fn execute_migration_plan(
    request: MigrationPlanExecutionRequest,
    state: State<'_, AppState>,
) -> Result<MigrationExecutionRecord, String> {
    let started_at = Utc::now();
    let selected_ids = if request.selected_action_ids.is_empty() {
        request
            .plan
            .actions
            .iter()
            .filter(|action| action.required || action.enabled_by_default)
            .map(|action| action.id.clone())
            .collect::<Vec<_>>()
    } else {
        request.selected_action_ids.clone()
    };

    let mut logs = Vec::new();
    let mut checkpoints = Vec::new();
    let mut rollback_actions = Vec::new();
    let mut failure_reason = None;

    for action in request
        .plan
        .actions
        .iter()
        .filter(|action| action.required || selected_ids.contains(&action.id))
    {
        match execute_action(
            action,
            request.dry_run,
            &mut checkpoints,
            &mut rollback_actions,
        ) {
            Ok(log) => {
                let success = log.success;
                logs.push(log);
                if !success {
                    failure_reason = logs.last().map(|item| item.detail.clone());
                    break;
                }
            }
            Err(error) => {
                failure_reason = Some(error.clone());
                logs.push(OperationLogEntry {
                    at: Utc::now(),
                    path: request.plan.source_path.clone(),
                    mode: ExecutionMode::Move,
                    dry_run: request.dry_run,
                    success: false,
                    detail: error,
                    diagnosis: Some(simple_diagnosis(
                        &request.plan.source_path,
                        "execute_plan",
                        DiagnosticCode::Unknown,
                        DiagnosticSeverity::Warning,
                        "迁移计划执行中断。".to_string(),
                    )),
                });
                break;
            }
        }
    }

    let status = if request.dry_run {
        MigrationRunStatus::DryRun
    } else if failure_reason.is_some() {
        MigrationRunStatus::Failed
    } else {
        MigrationRunStatus::Succeeded
    };

    let record = MigrationExecutionRecord {
        run_id: format!("run-{}", started_at.timestamp_millis()),
        plan_id: request.plan.id.clone(),
        title: request.plan.title.clone(),
        started_at,
        finished_at: Utc::now(),
        dry_run: request.dry_run,
        status,
        checkpoints,
        rollback_actions,
        logs,
        failure_reason,
    };

    store_record(&state, &record).await?;
    Ok(record)
}

#[tauri::command]
pub async fn refine_migration_plan_with_ai(
    plan: MigrationPlan,
    state: State<'_, AppState>,
) -> Result<MigrationPlanReview, String> {
    let history = {
        let runs = state.migration_runs.lock().await;
        runs.clone()
    };
    let config = state.load_config();

    refine_plan_with_ai(
        &plan,
        &history,
        &AdvisorConfig {
            api_key: config.api_key,
            base_url: config.ai_base_url,
            model: config.ai_model,
            max_items: config.max_ai_items,
            strict_file_ai_remote_only: config.strict_file_ai_remote_only,
        },
    )
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub async fn get_migration_run_history(
    state: State<'_, AppState>,
) -> Result<Vec<MigrationExecutionRecord>, String> {
    let runs = state.migration_runs.lock().await;
    Ok(runs.clone())
}

#[tauri::command]
pub async fn rollback_migration_run(
    run_id: String,
    state: State<'_, AppState>,
) -> Result<MigrationExecutionRecord, String> {
    let original = {
        let runs = state.migration_runs.lock().await;
        runs.iter()
            .find(|item| item.run_id == run_id)
            .cloned()
            .ok_or("未找到指定的迁移执行记录。".to_string())?
    };

    if original.dry_run {
        return Err("模拟执行记录不需要回滚。".to_string());
    }
    if original.status == MigrationRunStatus::RolledBack {
        return Err("该迁移记录已经回滚过了。".to_string());
    }

    let started_at = Utc::now();
    let mut logs = Vec::new();
    let mut failure_reason = None;

    for action in original.rollback_actions.iter().rev() {
        match apply_rollback_action(action) {
            Ok(log) => {
                let success = log.success;
                logs.push(log);
                if !success {
                    failure_reason = logs.last().map(|item| item.detail.clone());
                    break;
                }
            }
            Err(error) => {
                failure_reason = Some(error.clone());
                logs.push(OperationLogEntry {
                    at: Utc::now(),
                    path: PathBuf::from("."),
                    mode: ExecutionMode::Move,
                    dry_run: false,
                    success: false,
                    detail: error,
                    diagnosis: None,
                });
                break;
            }
        }
    }

    let status = if failure_reason.is_some() {
        MigrationRunStatus::Failed
    } else {
        MigrationRunStatus::RolledBack
    };

    let record = MigrationExecutionRecord {
        run_id: format!("rollback-{}", started_at.timestamp_millis()),
        plan_id: original.plan_id.clone(),
        title: format!("回滚 {}", original.title),
        started_at,
        finished_at: Utc::now(),
        dry_run: false,
        status,
        checkpoints: Vec::new(),
        rollback_actions: Vec::new(),
        logs,
        failure_reason,
    };

    if status == MigrationRunStatus::RolledBack {
        let mut runs = state.migration_runs.lock().await;
        if let Some(item) = runs.iter_mut().find(|item| item.run_id == original.run_id) {
            item.status = MigrationRunStatus::RolledBack;
        }
    }

    store_record(&state, &record).await?;
    persist_records(&state).await?;
    Ok(record)
}

fn execute_action(
    action: &MigrationAction,
    dry_run: bool,
    checkpoints: &mut Vec<MigrationCheckpoint>,
    rollback_actions: &mut Vec<MigrationRollbackAction>,
) -> Result<OperationLogEntry, String> {
    match action.kind {
        MigrationActionKind::StopProcess => {
            let process_name = action_param_str(action, "processName")?;
            close_process_by_name(process_name, dry_run)
        }
        MigrationActionKind::MovePath => {
            let source_path = PathBuf::from(action_param_str(action, "sourcePath")?);
            let target_path = PathBuf::from(action_param_str(action, "targetPath")?);
            let allow_special_paths = action
                .params
                .get("allowSpecialPaths")
                .and_then(|value| value.as_bool())
                .unwrap_or(false);

            if !dry_run {
                checkpoints.push(MigrationCheckpoint {
                    key: format!("path-state:{}", source_path.display()),
                    kind: MigrationCheckpointKind::PathState,
                    target: source_path.to_string_lossy().to_string(),
                    snapshot: serde_json::json!({
                        "sourceExists": source_path.exists(),
                        "targetExists": target_path.exists(),
                    }),
                });
                rollback_actions.push(MigrationRollbackAction {
                    id: format!("rollback:move:{}", source_path.display()),
                    kind: MigrationActionKind::MovePath,
                    title: "恢复原始路径".to_string(),
                    detail: "把迁移后的路径移回原位置。".to_string(),
                    params: serde_json::json!({
                        "sourcePath": target_path,
                        "targetPath": source_path,
                        "allowSpecialPaths": allow_special_paths,
                    }),
                });
            }

            move_path_exact(&source_path, &target_path, dry_run, allow_special_paths)
        }
        MigrationActionKind::UpdateYamlList => {
            let path = resolve_user_path(action_param_str(action, "path")?);
            let key = action_param_str(action, "key")?;
            let value = action_param_str(action, "value")?;
            update_yaml_list_file(&path, key, value, dry_run, checkpoints, rollback_actions)
        }
        MigrationActionKind::SetEnvVar => {
            let name = action_param_str(action, "name")?;
            let value = action_param_str(action, "value")?;
            set_user_env_var(name, value, dry_run, checkpoints, rollback_actions)
        }
        MigrationActionKind::CreateJunction => {
            let source_path = PathBuf::from(action_param_str(action, "sourcePath")?);
            let target_path = PathBuf::from(action_param_str(action, "targetPath")?);
            if !dry_run {
                rollback_actions.push(MigrationRollbackAction {
                    id: format!("rollback:junction:{}", source_path.display()),
                    kind: MigrationActionKind::CreateJunction,
                    title: "移除兼容链接".to_string(),
                    detail: "删除迁移后创建的兼容链接。".to_string(),
                    params: serde_json::json!({
                        "removePath": source_path,
                    }),
                });
            }
            create_compat_link(&source_path, &target_path, dry_run)
        }
        MigrationActionKind::VerifyPathExists => {
            let path = PathBuf::from(action_param_str(action, "path")?);
            verify_path_exists(&path, dry_run)
        }
    }
}

fn apply_rollback_action(action: &MigrationRollbackAction) -> Result<OperationLogEntry, String> {
    match action.kind {
        MigrationActionKind::MovePath => {
            let source_path = PathBuf::from(rollback_param_str(action, "sourcePath")?);
            let target_path = PathBuf::from(rollback_param_str(action, "targetPath")?);
            let allow_special_paths = action
                .params
                .get("allowSpecialPaths")
                .and_then(|value| value.as_bool())
                .unwrap_or(true);
            move_path_exact(&source_path, &target_path, false, allow_special_paths)
        }
        MigrationActionKind::UpdateYamlList => {
            let path = resolve_user_path(rollback_param_str(action, "path")?);
            restore_file_snapshot(&path, action.params.get("content").cloned())
        }
        MigrationActionKind::SetEnvVar => {
            let name = rollback_param_str(action, "name")?;
            restore_env_var(name, action.params.get("value").cloned())
        }
        MigrationActionKind::CreateJunction => {
            let remove_path = PathBuf::from(rollback_param_str(action, "removePath")?);
            remove_compat_link(&remove_path)
        }
        MigrationActionKind::StopProcess | MigrationActionKind::VerifyPathExists => {
            Err("当前 rollback action 不支持该动作类型。".to_string())
        }
    }
}

fn action_param_str<'a>(action: &'a MigrationAction, key: &str) -> Result<&'a str, String> {
    action
        .params
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("迁移动作参数缺少字段 {key}。"))
}

fn rollback_param_str<'a>(
    action: &'a MigrationRollbackAction,
    key: &str,
) -> Result<&'a str, String> {
    action
        .params
        .get(key)
        .and_then(|value| value.as_str())
        .ok_or_else(|| format!("回滚动作参数缺少字段 {key}。"))
}

fn close_process_by_name(process_name: &str, dry_run: bool) -> Result<OperationLogEntry, String> {
    let sanitized = sanitize_process_name(process_name)?;
    if dry_run {
        return Ok(success_log(
            Path::new("."),
            true,
            format!("模拟执行：会在迁移前关闭进程 {sanitized}。"),
        ));
    }

    #[cfg(target_os = "windows")]
    let output = {
        let script = r#"
$name = $args[0]
$base = [System.IO.Path]::GetFileNameWithoutExtension($name)
$procs = @(Get-Process -Name $base -ErrorAction SilentlyContinue)
if ($procs.Count -eq 0) {
  Write-Output "not-running"
  exit 0
}
$procs | Stop-Process -Force -ErrorAction Stop
Write-Output "terminated"
"#;
        Command::new("powershell")
            .args(["-NoProfile", "-Command", script, &sanitized])
            .output()
            .map_err(|error| format!("关闭进程 {sanitized} 失败：{error}"))?
    };

    #[cfg(target_os = "macos")]
    let output = Command::new("sh")
        .args([
            "-lc",
            &format!(
                "pkill -x {} >/dev/null 2>&1 || true; echo terminated",
                shell_escape(&sanitized)
            ),
        ])
        .output()
        .map_err(|error| format!("关闭进程 {sanitized} 失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Ok(failure_log(
            Path::new("."),
            false,
            if stderr.is_empty() {
                format!("关闭进程 {sanitized} 失败。")
            } else {
                format!("关闭进程 {sanitized} 失败：{stderr}")
            },
            Some(simple_diagnosis(
                Path::new("."),
                "terminate_process",
                DiagnosticCode::InUseByAnotherProcess,
                DiagnosticSeverity::Warning,
                format!("无法自动关闭进程 {sanitized}。"),
            )),
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stdout.contains("not-running") {
        format!("迁移前检查：进程 {sanitized} 当前未运行。")
    } else {
        format!("迁移前已自动关闭进程 {sanitized}。")
    };
    Ok(success_log(Path::new("."), false, detail))
}

fn update_yaml_list_file(
    path: &Path,
    key: &str,
    value: &str,
    dry_run: bool,
    checkpoints: &mut Vec<MigrationCheckpoint>,
    rollback_actions: &mut Vec<MigrationRollbackAction>,
) -> Result<OperationLogEntry, String> {
    if dry_run {
        return Ok(success_log(
            path,
            true,
            format!("模拟执行：会更新 {} 中的 {}。", path.display(), key),
        ));
    }

    let previous = fs::read_to_string(path).ok();
    checkpoints.push(MigrationCheckpoint {
        key: format!("file:{}", path.display()),
        kind: MigrationCheckpointKind::FileContent,
        target: path.to_string_lossy().to_string(),
        snapshot: serde_json::json!({ "content": previous }),
    });
    rollback_actions.push(MigrationRollbackAction {
        id: format!("rollback:file:{}", path.display()),
        kind: MigrationActionKind::UpdateYamlList,
        title: "恢复配置文件".to_string(),
        detail: "恢复迁移前的配置文件内容。".to_string(),
        params: serde_json::json!({
            "path": path.to_string_lossy(),
            "content": previous,
        }),
    });

    let mut root = if let Some(text) = previous.as_deref() {
        serde_yaml::from_str::<Value>(text).unwrap_or_else(|_| Value::Mapping(Mapping::new()))
    } else {
        Value::Mapping(Mapping::new())
    };
    let mapping = root
        .as_mapping_mut()
        .ok_or("配置文件不是可编辑的 YAML 映射结构。".to_string())?;

    let mut values = Sequence::new();
    let key_value = Value::String(key.to_string());
    if let Some(Value::Sequence(existing)) = mapping.get(&key_value) {
        for item in existing {
            if let Value::String(text) = item {
                if text != value {
                    values.push(Value::String(text.clone()));
                }
            }
        }
    }
    values.insert(0, Value::String(value.to_string()));
    mapping.insert(key_value, Value::Sequence(values));

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建配置目录失败：{error}"))?;
    }
    let serialized = serde_yaml::to_string(&root).map_err(|error| error.to_string())?;
    fs::write(path, serialized).map_err(|error| format!("写入配置文件失败：{error}"))?;
    Ok(success_log(
        path,
        false,
        format!("已更新配置文件 {}。", path.display()),
    ))
}

fn set_user_env_var(
    name: &str,
    value: &str,
    dry_run: bool,
    checkpoints: &mut Vec<MigrationCheckpoint>,
    rollback_actions: &mut Vec<MigrationRollbackAction>,
) -> Result<OperationLogEntry, String> {
    if dry_run {
        return Ok(success_log(
            Path::new("."),
            true,
            format!("模拟执行：会设置用户环境变量 {name}={value}。"),
        ));
    }

    let previous = env::var(name).ok();
    checkpoints.push(MigrationCheckpoint {
        key: format!("env:{name}"),
        kind: MigrationCheckpointKind::EnvVar,
        target: name.to_string(),
        snapshot: serde_json::json!({ "value": previous }),
    });
    rollback_actions.push(MigrationRollbackAction {
        id: format!("rollback:env:{name}"),
        kind: MigrationActionKind::SetEnvVar,
        title: format!("恢复环境变量 {name}"),
        detail: "恢复迁移前的用户环境变量。".to_string(),
        params: serde_json::json!({
            "name": name,
            "value": previous,
        }),
    });

    let output = set_user_env_var_platform(name, Some(value))
        .map_err(|error| format!("设置环境变量 {name} 失败：{error}"))?;
    if !output.success() {
        return Ok(failure_log(
            Path::new("."),
            false,
            format!("设置环境变量 {name} 失败。"),
            None,
        ));
    }

    env::set_var(name, value);
    Ok(success_log(
        Path::new("."),
        false,
        format!("已更新用户环境变量 {name}。"),
    ))
}

fn move_path_exact(
    source_path: &Path,
    target_path: &Path,
    dry_run: bool,
    allow_special_paths: bool,
) -> Result<OperationLogEntry, String> {
    if !allow_special_paths && !is_safe_generic_move_path(source_path) {
        return Ok(failure_log(
            source_path,
            dry_run,
            "该路径不在通用迁移执行器允许的安全范围内。".to_string(),
            None,
        ));
    }
    if !source_path.exists() {
        return Ok(failure_log(
            source_path,
            dry_run,
            format!("源路径不存在：{}", source_path.display()),
            None,
        ));
    }
    if target_path.exists() {
        return Ok(failure_log(
            source_path,
            dry_run,
            format!("目标路径已存在：{}", target_path.display()),
            None,
        ));
    }

    if dry_run {
        return Ok(success_log(
            source_path,
            true,
            format!(
                "模拟执行：会把 {} 移动到 {}。",
                source_path.display(),
                target_path.display()
            ),
        ));
    }

    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建目标目录失败：{error}"))?;
    }

    match fs::rename(source_path, target_path) {
        Ok(_) => Ok(success_log(
            source_path,
            false,
            format!("已移动到 {}。", target_path.display()),
        )),
        Err(_) => {
            fallback_move_exact(source_path, target_path)
                .map_err(|error| format!("移动失败：{error}"))?;
            Ok(success_log(
                source_path,
                false,
                format!("已移动到 {}。", target_path.display()),
            ))
        }
    }
}

fn fallback_move_exact(source_path: &Path, target_path: &Path) -> io::Result<()> {
    if source_path.is_file() {
        fs::copy(source_path, target_path)?;
        fs::remove_file(source_path)?;
        return Ok(());
    }
    if source_path.is_dir() {
        copy_dir_recursive(source_path, target_path)?;
        fs::remove_dir_all(source_path)?;
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "不支持的文件系统对象",
    ))
}

fn copy_dir_recursive(source: &Path, target: &Path) -> io::Result<()> {
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let path = entry.path();
        let target_child = target.join(entry.file_name());
        if entry.metadata()?.is_dir() {
            copy_dir_recursive(&path, &target_child)?;
        } else {
            fs::copy(&path, &target_child)?;
        }
    }
    Ok(())
}

fn create_compat_link(
    source_path: &Path,
    target_path: &Path,
    dry_run: bool,
) -> Result<OperationLogEntry, String> {
    if dry_run {
        return Ok(success_log(
            source_path,
            true,
            if cfg!(target_os = "macos") {
                format!(
                    "模拟执行：会在 {} 创建符号链接指向 {}。",
                    source_path.display(),
                    target_path.display()
                )
            } else {
                format!(
                    "模拟执行：会在 {} 创建 Junction 指向 {}。",
                    source_path.display(),
                    target_path.display()
                )
            },
        ));
    }
    if source_path.exists() {
        return Ok(failure_log(
            source_path,
            false,
            if cfg!(target_os = "macos") {
                "原路径仍然存在，无法创建符号链接。".to_string()
            } else {
                "原路径仍然存在，无法创建 Junction。".to_string()
            },
            None,
        ));
    }
    if let Some(parent) = source_path.parent() {
        fs::create_dir_all(parent).map_err(|error| format!("创建原路径父目录失败：{error}"))?;
    }

    create_compat_link_platform(source_path, target_path)?;

    Ok(success_log(
        source_path,
        false,
        if cfg!(target_os = "macos") {
            format!("已创建符号链接指向 {}。", target_path.display())
        } else {
            format!("已创建 Junction 指向 {}。", target_path.display())
        },
    ))
}

fn verify_path_exists(path: &Path, dry_run: bool) -> Result<OperationLogEntry, String> {
    if dry_run {
        return Ok(success_log(
            path,
            true,
            format!("模拟执行：会验证 {} 是否存在。", path.display()),
        ));
    }
    if path.exists() {
        Ok(success_log(
            path,
            false,
            format!("验证通过：{} 存在。", path.display()),
        ))
    } else {
        Ok(failure_log(
            path,
            false,
            format!("验证失败：{} 不存在。", path.display()),
            Some(simple_diagnosis(
                path,
                "verify_path_exists",
                DiagnosticCode::NotFound,
                DiagnosticSeverity::Warning,
                "迁移完成后未找到目标路径。".to_string(),
            )),
        ))
    }
}

fn restore_file_snapshot(
    path: &Path,
    content: Option<serde_json::Value>,
) -> Result<OperationLogEntry, String> {
    match content {
        Some(serde_json::Value::String(text)) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| format!("恢复配置目录失败：{error}"))?;
            }
            fs::write(path, text).map_err(|error| format!("恢复配置文件失败：{error}"))?;
            Ok(success_log(
                path,
                false,
                format!("已恢复配置文件 {}。", path.display()),
            ))
        }
        Some(serde_json::Value::Null) | None => {
            if path.exists() {
                fs::remove_file(path).map_err(|error| format!("删除新增配置文件失败：{error}"))?;
            }
            Ok(success_log(
                path,
                false,
                format!("已移除新增配置文件 {}。", path.display()),
            ))
        }
        _ => Err("配置文件回滚快照格式不正确。".to_string()),
    }
}

fn restore_env_var(
    name: &str,
    value: Option<serde_json::Value>,
) -> Result<OperationLogEntry, String> {
    let value_text = match value {
        Some(serde_json::Value::String(text)) => Some(text),
        Some(serde_json::Value::Null) | None => None,
        _ => return Err("环境变量回滚快照格式不正确。".to_string()),
    };

    let output = set_user_env_var_platform(name, value_text.as_deref())
        .map_err(|error| format!("恢复环境变量 {name} 失败：{error}"))?;
    if !output.success() {
        return Ok(failure_log(
            Path::new("."),
            false,
            format!("恢复环境变量 {name} 失败。"),
            None,
        ));
    }

    match value_text {
        Some(text) => env::set_var(name, text),
        None => env::remove_var(name),
    }
    Ok(success_log(
        Path::new("."),
        false,
        format!("已恢复环境变量 {name}。"),
    ))
}

fn remove_compat_link(path: &Path) -> Result<OperationLogEntry, String> {
    if !path.exists() {
        return Ok(success_log(
            path,
            false,
            if cfg!(target_os = "macos") {
                format!("回滚时未发现符号链接 {}，已跳过。", path.display())
            } else {
                format!("回滚时未发现 Junction {}，已跳过。", path.display())
            },
        ));
    }

    remove_compat_link_platform(path)?;
    Ok(success_log(
        path,
        false,
        if cfg!(target_os = "macos") {
            format!("已移除符号链接 {}。", path.display())
        } else {
            format!("已移除 Junction {}。", path.display())
        },
    ))
}

fn sanitize_process_name(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err("进程名不能为空。".to_string());
    }
    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
    {
        Ok(trimmed.to_string())
    } else {
        Err(format!("检测到非法进程名：{trimmed}"))
    }
}

fn resolve_user_path(value: &str) -> PathBuf {
    if let Some(rest) = value.strip_prefix("~/") {
        if let Some(home) = dirs_next::home_dir() {
            return home.join(rest);
        }
    }
    PathBuf::from(value)
}

fn is_safe_generic_move_path(path: &Path) -> bool {
    !is_windows_system_sensitive_path(path) && !is_macos_system_sensitive_path(path)
}

fn success_log(path: &Path, dry_run: bool, detail: String) -> OperationLogEntry {
    OperationLogEntry {
        at: Utc::now(),
        path: path.to_path_buf(),
        mode: ExecutionMode::Move,
        dry_run,
        success: true,
        detail,
        diagnosis: None,
    }
}

fn failure_log(
    path: &Path,
    dry_run: bool,
    detail: String,
    diagnosis: Option<PathDiagnosis>,
) -> OperationLogEntry {
    OperationLogEntry {
        at: Utc::now(),
        path: path.to_path_buf(),
        mode: ExecutionMode::Move,
        dry_run,
        success: false,
        detail,
        diagnosis,
    }
}

fn simple_diagnosis(
    path: &Path,
    operation: &str,
    code: DiagnosticCode,
    severity: DiagnosticSeverity,
    summary: String,
) -> PathDiagnosis {
    PathDiagnosis {
        path: path.to_path_buf(),
        operation: operation.to_string(),
        code,
        severity,
        summary: summary.clone(),
        details: vec![summary.clone()],
        suggestions: vec!["请检查迁移计划、目标路径和相关程序状态后重试。".to_string()],
        possible_related_apps: Vec::new(),
        error_kind: None,
        raw_os_error: None,
    }
}

async fn store_record(
    state: &State<'_, AppState>,
    record: &MigrationExecutionRecord,
) -> Result<(), String> {
    {
        let mut runs = state.migration_runs.lock().await;
        runs.push(record.clone());
    }
    persist_records(state).await
}

async fn persist_records(state: &State<'_, AppState>) -> Result<(), String> {
    let payload = {
        let runs = state.migration_runs.lock().await;
        serde_json::to_string_pretty(&*runs).map_err(|error| error.to_string())?
    };
    let path = state.migration_history_path();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    fs::write(path, payload).map_err(|error| error.to_string())
}

#[cfg(target_os = "windows")]
fn create_compat_link_platform(source_path: &Path, target_path: &Path) -> Result<(), String> {
    let script = r#"
$link = $args[0]
$target = $args[1]
New-Item -ItemType Junction -Path $link -Target $target -ErrorAction Stop | Out-Null
"#;
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            script,
            &source_path.to_string_lossy(),
            &target_path.to_string_lossy(),
        ])
        .output()
        .map_err(|error| format!("创建 Junction 失败：{error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(if stderr.is_empty() {
            "创建 Junction 失败。".to_string()
        } else {
            format!("创建 Junction 失败：{stderr}")
        })
    }
}

#[cfg(target_os = "macos")]
fn create_compat_link_platform(source_path: &Path, target_path: &Path) -> Result<(), String> {
    std::os::unix::fs::symlink(target_path, source_path)
        .map_err(|error| format!("创建符号链接失败：{error}"))
}

#[cfg(target_os = "windows")]
fn remove_compat_link_platform(path: &Path) -> Result<(), String> {
    fs::remove_dir(path).map_err(|error| format!("移除 Junction 失败：{error}"))
}

#[cfg(target_os = "macos")]
fn remove_compat_link_platform(path: &Path) -> Result<(), String> {
    fs::remove_file(path).map_err(|error| format!("移除符号链接失败：{error}"))
}

#[cfg(target_os = "windows")]
fn set_user_env_var_platform(name: &str, value: Option<&str>) -> Result<ExitStatus, io::Error> {
    if let Some(value) = value {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "[Environment]::SetEnvironmentVariable($args[0], $args[1], 'User')",
                name,
                value,
            ])
            .status()
    } else {
        Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                "[Environment]::SetEnvironmentVariable($args[0], $null, 'User')",
                name,
            ])
            .status()
    }
}

#[cfg(target_os = "macos")]
fn set_user_env_var_platform(name: &str, value: Option<&str>) -> Result<ExitStatus, io::Error> {
    let profile_path = dirs_next::home_dir()
        .unwrap_or_else(|| PathBuf::from("/Users/Shared"))
        .join(".zprofile");
    let marker = format!("# smart-disk-cleaner:{name}");
    let mut lines = fs::read_to_string(&profile_path)
        .unwrap_or_default()
        .lines()
        .filter(|line| !line.contains(&marker) && !line.starts_with(&format!("export {name}=")))
        .map(|line| line.to_string())
        .collect::<Vec<_>>();

    if let Some(value) = value {
        lines.push(marker);
        lines.push(format!("export {name}=\"{}\"", value.replace('"', "\\\"")));
    }

    fs::write(
        &profile_path,
        if lines.is_empty() {
            String::new()
        } else {
            lines.join("\n") + "\n"
        },
    )?;
    Command::new("true").status()
}

#[cfg(target_os = "macos")]
fn shell_escape(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
