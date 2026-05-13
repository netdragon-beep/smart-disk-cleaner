use crate::state::AppState;
use smart_disk_cleaner_core::ai_advisor::{build_advice, explain_path, AdvisorConfig};
use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::{FileAiInsight, FileSuggestion, ScanResult};
use std::path::PathBuf;
use std::process::Command;
use tauri::State;

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCleanupAppCandidate {
    key: String,
    app_name: String,
    publisher: Option<String>,
    install_location: Option<String>,
    estimated_size: u64,
    reason: String,
    uninstall_available: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiCleanupPlan {
    source: String,
    summary: String,
    file_suggestions: Vec<FileSuggestion>,
    uninstall_candidates: Vec<AiCleanupAppCandidate>,
}

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct InstalledAppCleanupEntry {
    identity_key: String,
    display_name: String,
    publisher: Option<String>,
    install_location: Option<String>,
    uninstall_string: Option<String>,
}

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

#[tauri::command]
pub async fn generate_ai_cleanup_plan(
    config: Option<AppConfig>,
    state: State<'_, AppState>,
) -> Result<AiCleanupPlan, String> {
    let report = state
        .last_report
        .lock()
        .await
        .clone()
        .ok_or_else(|| "请先完成一次扫描，再生成 AI 整理建议。".to_string())?;
    let config = config.unwrap_or_else(|| state.load_config());

    let scan = ScanResult {
        root: report.root.clone(),
        files: report.scanned_files.clone(),
        empty_dirs: report.analysis.empty_dirs.clone(),
        failures: report.failures.clone(),
    };

    let advisor = build_advice(
        &scan,
        &report.analysis,
        &report.dedup,
        &AdvisorConfig {
            api_key: config.api_key,
            base_url: config.ai_base_url,
            model: config.ai_model,
            max_items: config.max_ai_items,
            strict_file_ai_remote_only: config.strict_file_ai_remote_only,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let uninstall_candidates = build_uninstall_candidates(&report)?;

    Ok(AiCleanupPlan {
        source: advisor.source,
        summary: advisor.summary,
        file_suggestions: advisor.suggestions.into_iter().take(200).collect(),
        uninstall_candidates,
    })
}

#[tauri::command]
pub async fn open_apps_and_features() -> Result<(), String> {
    #[cfg(not(target_os = "windows"))]
    {
        Err("当前仅支持在 Windows 上打开系统应用卸载列表。".to_string())
    }

    #[cfg(target_os = "windows")]
    {
        Command::new("cmd")
            .args(["/C", "start", "", "ms-settings:appsfeatures"])
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn build_uninstall_candidates(
    report: &smart_disk_cleaner_core::models::ScanReport,
) -> Result<Vec<AiCleanupAppCandidate>, String> {
    let installed_apps = query_installed_apps_for_cleanup()?;
    let mut candidates = installed_apps
        .into_iter()
        .filter_map(|app| {
            let install_location = app.install_location.as_ref()?;
            let normalized_root = normalize_path_string(install_location);
            let estimated_size = report
                .scanned_files
                .iter()
                .filter(|file| {
                    normalize_path_string(&file.path.to_string_lossy()).starts_with(&normalized_root)
                })
                .map(|file| file.size)
                .sum::<u64>();

            if estimated_size < 256 * 1024 * 1024 {
                return None;
            }

            Some(AiCleanupAppCandidate {
                key: app.identity_key,
                app_name: app.display_name,
                publisher: app.publisher,
                install_location: app.install_location,
                estimated_size,
                reason: format!(
                    "这个软件在当前扫描目录下大约占用 {}。如果你基本不用它，可以考虑卸载而不是只删零散文件。",
                    format_bytes_human(estimated_size)
                ),
                uninstall_available: app
                    .uninstall_string
                    .as_ref()
                    .map(|value| !value.trim().is_empty())
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();

    candidates.sort_by(|left, right| {
        right
            .estimated_size
            .cmp(&left.estimated_size)
            .then_with(|| left.app_name.cmp(&right.app_name))
    });
    candidates.truncate(20);
    Ok(candidates)
}

fn format_bytes_human(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    const GB: f64 = MB * 1024.0;

    if bytes < 1024 {
        return format!("{bytes} B");
    }

    let bytes = bytes as f64;
    if bytes < MB {
        return format!("{:.1} KB", bytes / KB);
    }
    if bytes < GB {
        return format!("{:.1} MB", bytes / MB);
    }
    format!("{:.2} GB", bytes / GB)
}

fn normalize_path_string(value: &str) -> String {
    value.replace('\\', "/").to_ascii_lowercase()
}

fn query_installed_apps_for_cleanup() -> Result<Vec<InstalledAppCleanupEntry>, String> {
    #[cfg(not(target_os = "windows"))]
    {
        Ok(Vec::new())
    }

    #[cfg(target_os = "windows")]
    {
        let script = r#"
$paths = @(
  'HKLM:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKLM:\Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall\*',
  'HKCU:\Software\Microsoft\Windows\CurrentVersion\Uninstall\*'
)

$rows = foreach ($path in $paths) {
  Get-ItemProperty -Path $path -ErrorAction SilentlyContinue | Where-Object { $_.DisplayName } | ForEach-Object {
    [pscustomobject]@{
      identityKey = if ($_.PSChildName) { [string]$_.PSChildName } else { [string]$_.DisplayName }
      displayName = [string]$_.DisplayName
      publisher = if ($_.Publisher) { [string]$_.Publisher } else { $null }
      installLocation = if ($_.InstallLocation) { [string]$_.InstallLocation } else { $null }
      uninstallString = if ($_.UninstallString) { [string]$_.UninstallString } else { $null }
    }
  }
}

@($rows) | ConvertTo-Json -Depth 4 -Compress
"#;

        let output = Command::new("powershell")
            .args(["-NoProfile", "-Command", script])
            .output()
            .map_err(|error| format!("读取已安装软件列表失败：{error}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            return Err(if stderr.is_empty() {
                "读取已安装软件列表失败。".to_string()
            } else {
                format!("读取已安装软件列表失败：{stderr}")
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let trimmed = stdout.trim();
        if trimmed.is_empty() {
            return Ok(Vec::new());
        }

        serde_json::from_str(trimmed).map_err(|error| format!("解析已安装软件列表失败：{error}"))
    }
}
