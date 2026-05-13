use crate::state::AppState;
use chrono::Utc;
use smart_disk_cleaner_core::models::{
    ExecutionMode, OperationLogEntry, OperationRecordKind, RegistryBackup, RegistryChangePreview,
    RegistryEntryKind, RegistryEntryRecord, RegistryIssue, RegistryIssueKind, RegistryRootKey,
    RegistryRollbackRecord, RegistryValueDataKind, RiskLevel, SuggestedAction,
};
use std::fs;
use std::path::PathBuf;
use tauri::State;

#[cfg(target_os = "windows")]
use std::process::Command;

const RUN_KEY_CURRENT_USER: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_KEY_LOCAL_MACHINE: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

#[tauri::command]
pub async fn list_registry_startup_entries() -> Result<Vec<RegistryEntryRecord>, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }

    #[cfg(target_os = "windows")]
    {
        let mut entries = Vec::new();
        entries.extend(query_run_entries(RegistryRootKey::Hkcu, RUN_KEY_CURRENT_USER)?);
        entries.extend(query_run_entries(RegistryRootKey::Hklm, RUN_KEY_LOCAL_MACHINE)?);
        Ok(entries)
    }
}

#[tauri::command]
pub async fn list_registry_path_issues() -> Result<Vec<RegistryIssue>, String> {
    let entries = list_registry_startup_entries().await?;
    Ok(entries
        .into_iter()
        .flat_map(|entry| build_issues_for_entry(&entry))
        .collect())
}

#[tauri::command]
pub async fn export_registry_backup(
    entry_id: String,
    state: State<'_, AppState>,
) -> Result<RegistryBackup, String> {
    let entry = find_entry_by_id(&entry_id).await?;
    let backup = RegistryBackup {
        backup_id: format!("registry-backup-{}", Utc::now().timestamp_millis()),
        original_value_data: entry.value_data.clone(),
        entry,
        created_at: Utc::now(),
    };

    persist_backup(&state, &backup).await?;
    Ok(backup)
}

#[tauri::command]
pub async fn preview_registry_change(entry_id: String) -> Result<RegistryChangePreview, String> {
    let entry = find_entry_by_id(&entry_id).await?;
    if !entry.safe_to_modify {
        return Err("该注册表项当前不允许直接修改，请先人工复核。".to_string());
    }

    Ok(RegistryChangePreview {
        risk_level: entry.risk_level,
        backup_id: None,
        dry_run_supported: true,
        rollback_supported: true,
        before_value: entry.value_data.clone(),
        after_value: disabled_value(&entry.value_data),
        post_check_items: vec![
            "重启或重新登录后确认该启动项未再自动运行".to_string(),
            "确认相关应用仍可手动正常启动".to_string(),
        ],
        entry,
    })
}

#[tauri::command]
pub async fn apply_registry_change(
    entry_id: String,
    dry_run: bool,
    state: State<'_, AppState>,
) -> Result<RegistryChangePreview, String> {
    let entry = find_entry_by_id(&entry_id).await?;
    if !entry.safe_to_modify {
        return Err("该注册表项风险较高，当前版本不允许直接修改。".to_string());
    }

    let backup = RegistryBackup {
        backup_id: format!("registry-backup-{}", Utc::now().timestamp_millis()),
        original_value_data: entry.value_data.clone(),
        entry: entry.clone(),
        created_at: Utc::now(),
    };
    persist_backup(&state, &backup).await?;

    let after_value = disabled_value(&entry.value_data);
    if !dry_run {
        #[cfg(target_os = "windows")]
        apply_value_change(&entry, &after_value)?;
    }

    persist_history(
        &state,
        OperationLogEntry {
            at: Utc::now(),
            path: PathBuf::from(format!(
                "{}\\{}:{}",
                root_key_name(entry.root_key),
                entry.sub_key,
                entry.value_name
            )),
            mode: ExecutionMode::Move,
            dry_run,
            success: true,
            detail: if dry_run {
                format!("已预演禁用启动项：{}", entry.display_name)
            } else {
                format!("已禁用启动项：{}", entry.display_name)
            },
            record_kind: OperationRecordKind::RegistryChange,
            reason: Some("注册表安全治理：禁用单个启动项".to_string()),
            rollback_reference: Some(backup.backup_id.clone()),
            diagnosis: None,
        },
    )
    .await?;

    Ok(RegistryChangePreview {
        entry,
        risk_level: RiskLevel::Medium,
        backup_id: Some(backup.backup_id),
        dry_run_supported: true,
        rollback_supported: true,
        before_value: backup.original_value_data,
        after_value,
        post_check_items: vec![
            "重启或重新登录后确认该启动项未再自动运行".to_string(),
            "确认相关应用仍可手动正常启动".to_string(),
        ],
    })
}

#[tauri::command]
pub async fn rollback_registry_change(
    backup_id: String,
    state: State<'_, AppState>,
) -> Result<RegistryRollbackRecord, String> {
    let backup = {
        let backups = state.registry_backups.lock().await;
        backups
            .iter()
            .find(|item| item.backup_id == backup_id)
            .cloned()
            .ok_or_else(|| "未找到指定的注册表备份记录。".to_string())?
    };

    #[cfg(target_os = "windows")]
    apply_value_change(&backup.entry, &backup.original_value_data)?;

    let record = RegistryRollbackRecord {
        backup_id: backup.backup_id.clone(),
        restored_at: Utc::now(),
        entry: backup.entry.clone(),
    };

    {
        let mut rollbacks = state.registry_rollbacks.lock().await;
        rollbacks.push(record.clone());
        persist_json(&state.registry_rollback_path(), &*rollbacks)?;
    }

    persist_history(
        &state,
        OperationLogEntry {
            at: Utc::now(),
            path: PathBuf::from(format!(
                "{}\\{}:{}",
                root_key_name(backup.entry.root_key),
                backup.entry.sub_key,
                backup.entry.value_name
            )),
            mode: ExecutionMode::Move,
            dry_run: false,
            success: true,
            detail: format!("已回滚注册表改动：{}", backup.entry.display_name),
            record_kind: OperationRecordKind::RegistryRollback,
            reason: Some("注册表安全治理：回滚单次修改".to_string()),
            rollback_reference: Some(backup.backup_id.clone()),
            diagnosis: None,
        },
    )
    .await?;

    Ok(record)
}

async fn find_entry_by_id(entry_id: &str) -> Result<RegistryEntryRecord, String> {
    list_registry_startup_entries()
        .await?
        .into_iter()
        .find(|item| item.id == entry_id)
        .ok_or_else(|| "未找到指定注册表项。".to_string())
}

async fn persist_backup(state: &State<'_, AppState>, backup: &RegistryBackup) -> Result<(), String> {
    let mut backups = state.registry_backups.lock().await;
    backups.push(backup.clone());
    persist_json(&state.registry_backup_path(), &*backups)
}

async fn persist_history(state: &State<'_, AppState>, entry: OperationLogEntry) -> Result<(), String> {
    let mut history = state.history.lock().await;
    history.push(entry);
    Ok(())
}

fn persist_json<T: serde::Serialize>(path: &PathBuf, value: &T) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let text = serde_json::to_string_pretty(value).map_err(|error| error.to_string())?;
    fs::write(path, text).map_err(|error| error.to_string())
}

fn build_issues_for_entry(entry: &RegistryEntryRecord) -> Vec<RegistryIssue> {
    let mut issues = Vec::new();
    if !entry.exists_on_disk {
        issues.push(RegistryIssue {
            id: format!("issue:{}:missing-target", entry.id),
            entry_id: entry.id.clone(),
            issue_kind: RegistryIssueKind::MissingTarget,
            title: "启动项引用的路径不存在".to_string(),
            summary: "该启动项的目标路径当前无法在磁盘上找到。".to_string(),
            risk_level: RiskLevel::Low,
            suggested_action: SuggestedAction::Review,
            pre_check_items: vec!["确认目标程序不是便携版或位于可移动磁盘".to_string()],
            post_check_items: vec!["如果确认无用，可禁用该启动项并重启验证".to_string()],
        });
    }

    if entry.value_data.contains(' ') && !entry.value_data.trim_start().starts_with('"') {
        issues.push(RegistryIssue {
            id: format!("issue:{}:quote", entry.id),
            entry_id: entry.id.clone(),
            issue_kind: RegistryIssueKind::MissingQuotedPath,
            title: "启动项路径可能缺少引号".to_string(),
            summary: "带空格的命令行未被引号包裹，可能导致路径解析异常。".to_string(),
            risk_level: RiskLevel::Medium,
            suggested_action: SuggestedAction::Review,
            pre_check_items: vec!["确认该命令行是否包含参数".to_string()],
            post_check_items: vec!["修复后重启验证是否仍能正常启动应用".to_string()],
        });
    }

    issues
}

fn disabled_value(current: &str) -> String {
    format!("REM_DISABLED_BY_SDC {}", current.trim())
}

fn root_key_name(key: RegistryRootKey) -> &'static str {
    match key {
        RegistryRootKey::Hkcu => "HKCU",
        RegistryRootKey::Hklm => "HKLM",
    }
}

#[cfg(target_os = "windows")]
fn apply_value_change(entry: &RegistryEntryRecord, value: &str) -> Result<(), String> {
    let status = Command::new("reg")
        .args([
            "add",
            &format!(r"{}\{}", root_key_name(entry.root_key), entry.sub_key),
            "/v",
            &entry.value_name,
            "/t",
            "REG_SZ",
            "/d",
            value,
            "/f",
        ])
        .status()
        .map_err(|error| error.to_string())?;

    if status.success() {
        Ok(())
    } else {
        Err("写入注册表失败，可能是权限不足或目标项被保护。".to_string())
    }
}

#[cfg(target_os = "windows")]
fn query_run_entries(
    root: RegistryRootKey,
    sub_key: &str,
) -> Result<Vec<RegistryEntryRecord>, String> {
    let query_key = format!(r"{}\{}", root_key_name(root), sub_key);
    let output = Command::new("reg")
        .args(["query", &query_key])
        .output()
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        return Ok(Vec::new());
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut items = Vec::new();
    for line in text.lines() {
        if !line.contains("REG_") {
            continue;
        }
        let parts = line.split_whitespace().collect::<Vec<_>>();
        if parts.len() < 3 {
            continue;
        }
        let value_name = parts[0].trim().to_string();
        let value_kind = parts[1].trim().to_string();
        let value_data = parts[2..].join(" ");
        let target_path = extract_path_from_command(&value_data);
        let exists_on_disk = target_path.as_ref().map(|path| path.exists()).unwrap_or(false);

        items.push(RegistryEntryRecord {
            id: format!("{}:{}:{}", root_key_name(root), sub_key, value_name),
            root_key: root,
            sub_key: sub_key.to_string(),
            value_name: value_name.clone(),
            value_kind: if value_kind.eq_ignore_ascii_case("REG_EXPAND_SZ") {
                RegistryValueDataKind::ExpandString
            } else {
                RegistryValueDataKind::String
            },
            value_data: value_data.clone(),
            entry_kind: RegistryEntryKind::Startup,
            display_name: value_name,
            target_path,
            exists_on_disk,
            safe_to_modify: root == RegistryRootKey::Hkcu,
            risk_level: if root == RegistryRootKey::Hkcu {
                RiskLevel::Medium
            } else {
                RiskLevel::High
            },
            tags: vec![
                "启动项".to_string(),
                if root == RegistryRootKey::Hkcu {
                    "当前用户".to_string()
                } else {
                    "系统范围".to_string()
                },
            ],
        });
    }
    Ok(items)
}

#[cfg(target_os = "windows")]
fn extract_path_from_command(command_line: &str) -> Option<PathBuf> {
    let trimmed = command_line.trim();
    if trimmed.is_empty() {
        return None;
    }

    let raw = if let Some(rest) = trimmed.strip_prefix('"') {
        rest.split('"').next().unwrap_or(rest)
    } else {
        trimmed.split_whitespace().next().unwrap_or(trimmed)
    };

    let expanded = std::env::vars().fold(raw.to_string(), |acc, (key, value)| {
        acc.replace(&format!("%{}%", key), &value)
    });
    Some(PathBuf::from(expanded))
}
