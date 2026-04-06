use crate::state::AppState;
use serde::Deserialize;
use smart_disk_cleaner_core::ai_advisor::AdvisorConfig;
use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::{ProcessAiInsight, ProcessRecord};
use smart_disk_cleaner_core::process_advisor::{
    classify_process_metadata, compute_resource_score, explain_process,
};
use std::path::PathBuf;
use std::process::Command;
use tauri::State;

const DEFAULT_PROCESS_LIMIT: usize = 30;
const MAX_PROCESS_LIMIT: usize = 100;

#[tauri::command]
pub async fn list_top_processes(limit: Option<usize>) -> Result<Vec<ProcessRecord>, String> {
    let limit = limit
        .unwrap_or(DEFAULT_PROCESS_LIMIT)
        .clamp(1, MAX_PROCESS_LIMIT);

    tokio::task::spawn_blocking(move || load_processes(limit))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn explain_process_with_ai(
    pid: u32,
    config: Option<AppConfig>,
    state: State<'_, AppState>,
) -> Result<ProcessAiInsight, String> {
    let process = tokio::task::spawn_blocking(move || load_process_by_pid(pid))
        .await
        .map_err(|error| error.to_string())??;

    let config = config.unwrap_or_else(|| state.load_config());
    explain_process(
        &process,
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
pub async fn terminate_process(pid: u32) -> Result<String, String> {
    tokio::task::spawn_blocking(move || terminate_process_inner(pid))
        .await
        .map_err(|error| error.to_string())?
}

fn load_processes(limit: usize) -> Result<Vec<ProcessRecord>, String> {
    let mut rows = query_processes()?;

    rows.sort_by(|left, right| {
        right
            .resource_score
            .total_cmp(&left.resource_score)
            .then_with(|| right.cpu_usage.total_cmp(&left.cpu_usage))
            .then_with(|| right.memory_bytes.cmp(&left.memory_bytes))
            .then_with(|| left.name.cmp(&right.name))
    });
    rows.truncate(limit);
    Ok(rows)
}

fn load_process_by_pid(pid: u32) -> Result<ProcessRecord, String> {
    query_processes()?
        .into_iter()
        .find(|process| process.pid == pid)
        .ok_or_else(|| format!("未找到 PID 为 {pid} 的进程，可能已经退出。"))
}

fn terminate_process_inner(pid: u32) -> Result<String, String> {
    let process = load_process_by_pid(pid)?;
    if let Some(reason) = termination_block_reason(&process) {
        return Err(reason);
    }

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Stop-Process -Id {pid} -Force -ErrorAction Stop"),
        ])
        .status()
        .map_err(|error| format!("调用 Stop-Process 失败：{error}"))?;

    if !status.success() {
        return Err(format!(
            "结束进程失败：{} (PID {})，可能是权限不足或该进程已经退出。",
            process.name, process.pid
        ));
    }

    Ok(format!(
        "已请求结束进程：{} (PID {})",
        process.name, process.pid
    ))
}

fn query_processes() -> Result<Vec<ProcessRecord>, String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", PROCESS_QUERY_SCRIPT])
        .output()
        .map_err(|error| format!("执行进程采集脚本失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "进程采集脚本执行失败。".to_string()
        } else {
            format!("进程采集脚本执行失败：{stderr}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Ok(Vec::new());
    }

    let samples: Vec<ProcessSample> =
        serde_json::from_str(trimmed).map_err(|error| format!("解析进程采集结果失败：{error}"))?;
    Ok(samples.into_iter().map(ProcessRecord::from).collect())
}

fn termination_block_reason(process: &ProcessRecord) -> Option<String> {
    if process.pid == std::process::id() {
        return Some("不能结束当前 smart-disk-cleaner 应用自身进程。".to_string());
    }

    if process.is_critical || process.category == "system_critical" {
        return Some(format!(
            "已阻止结束 {} (PID {})：这是关键系统进程，强制结束可能导致系统不稳定或直接蓝屏。",
            process.name, process.pid
        ));
    }

    if process.category == "security" {
        return Some(format!(
            "已阻止结束 {} (PID {})：这是安全防护相关进程，结束后可能导致实时防护失效。",
            process.name, process.pid
        ));
    }

    None
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSample {
    pid: u32,
    parent_pid: Option<u32>,
    name: String,
    exe_path: Option<String>,
    command: Vec<String>,
    cpu_usage: f32,
    memory_bytes: u64,
    virtual_memory_bytes: u64,
    disk_read_bytes: u64,
    disk_written_bytes: u64,
    run_time_seconds: u64,
    status: String,
}

impl From<ProcessSample> for ProcessRecord {
    fn from(value: ProcessSample) -> Self {
        let exe_path = value.exe_path.as_deref().map(PathBuf::from);
        let (category, is_critical) =
            classify_process_metadata(&value.name, exe_path.as_deref(), &value.command, value.pid);

        let resource_score = compute_resource_score(
            value.cpu_usage,
            value.memory_bytes,
            value.disk_read_bytes,
            value.disk_written_bytes,
        );

        ProcessRecord {
            pid: value.pid,
            parent_pid: value.parent_pid,
            name: value.name,
            exe_path,
            command: value.command,
            cpu_usage: value.cpu_usage,
            memory_bytes: value.memory_bytes,
            virtual_memory_bytes: value.virtual_memory_bytes,
            disk_read_bytes: value.disk_read_bytes,
            disk_written_bytes: value.disk_written_bytes,
            run_time_seconds: value.run_time_seconds,
            status: value.status,
            category,
            is_critical,
            resource_score,
        }
    }
}

const PROCESS_QUERY_SCRIPT: &str = r#"
try {
  $perfRows = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -ErrorAction Stop |
    Where-Object { $_.IDProcess -gt 0 -and $_.Name -ne '_Total' -and $_.Name -ne 'Idle' }

  $procMap = @{}
  Get-CimInstance Win32_Process -ErrorAction Stop | ForEach-Object {
    $procMap[[string]$_.ProcessId] = $_
  }

  $items = foreach ($row in $perfRows) {
    $detail = $procMap[[string]$row.IDProcess]
    $command = @()
    if ($null -ne $detail -and $detail.CommandLine) {
      $command = @([string]$detail.CommandLine)
    }

    [pscustomobject]@{
      pid = [uint32]$row.IDProcess
      parentPid = if ($null -ne $detail) { [uint32]$detail.ParentProcessId } else { $null }
      name = if ($null -ne $detail -and $detail.Name) { [string]$detail.Name } else { [string]$row.Name }
      exePath = if ($null -ne $detail -and $detail.ExecutablePath) { [string]$detail.ExecutablePath } else { $null }
      command = $command
      cpuUsage = [double]$row.PercentProcessorTime
      memoryBytes = [uint64]$row.WorkingSet
      virtualMemoryBytes = [uint64]$row.VirtualBytes
      diskReadBytes = [uint64]$row.IOReadBytesPersec
      diskWrittenBytes = [uint64]$row.IOWriteBytesPersec
      runTimeSeconds = [uint64]$row.ElapsedTime
      status = 'running'
    }
  }

  @($items) | ConvertTo-Json -Depth 4 -Compress
} catch {
  $sampleWindow = 0.4
  $first = @{}
  Get-Process -ErrorAction SilentlyContinue | ForEach-Object {
    $first[[string]$_.Id] = $_
  }

  Start-Sleep -Milliseconds 400

  $items = foreach ($proc in Get-Process -ErrorAction SilentlyContinue) {
    $prev = $first[[string]$proc.Id]
    $cpuUsage = 0
    if ($null -ne $prev -and $null -ne $prev.CPU -and $null -ne $proc.CPU) {
      $deltaCpu = [Math]::Max(0, [double]$proc.CPU - [double]$prev.CPU)
      $cpuUsage = ($deltaCpu / $sampleWindow / [Environment]::ProcessorCount) * 100
    }

    $startTime = $null
    try {
      $startTime = $proc.StartTime
    } catch {
      $startTime = $null
    }

    $runTimeSeconds = 0
    if ($null -ne $startTime) {
      $runTimeSeconds = [uint64]([DateTime]::Now - $startTime).TotalSeconds
    }

    $exePath = $null
    try {
      $exePath = $proc.Path
    } catch {
      $exePath = $null
    }

    $displayName = if ($exePath) {
      [System.IO.Path]::GetFileName($exePath)
    } else {
      "$($proc.ProcessName).exe"
    }

    [pscustomobject]@{
      pid = [uint32]$proc.Id
      parentPid = $null
      name = [string]$displayName
      exePath = if ($exePath) { [string]$exePath } else { $null }
      command = @()
      cpuUsage = [double]$cpuUsage
      memoryBytes = [uint64]$proc.WorkingSet64
      virtualMemoryBytes = [uint64]$proc.VirtualMemorySize64
      diskReadBytes = [uint64]0
      diskWrittenBytes = [uint64]0
      runTimeSeconds = [uint64]$runTimeSeconds
      status = 'running'
    }
  }

  @($items) | ConvertTo-Json -Depth 4 -Compress
}
"#;
