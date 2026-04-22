use crate::state::AppState;
use chrono::Utc;
use serde::Deserialize;
use smart_disk_cleaner_core::ai_advisor::AdvisorConfig;
use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::{
    ProcessAiFollowUpAnswer, ProcessAiFollowUpTurn, ProcessAiInsight, ProcessRecord,
};
use smart_disk_cleaner_core::process_advisor::{
    answer_process_follow_up, classify_process_metadata, compute_resource_score, explain_process,
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
pub async fn get_process_monitor_snapshot(
    limit: Option<usize>,
) -> Result<ProcessMonitorSnapshot, String> {
    let limit = limit.unwrap_or(12).clamp(1, MAX_PROCESS_LIMIT);

    tokio::task::spawn_blocking(move || load_monitor_snapshot(limit))
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
pub async fn ask_process_follow_up_with_ai(
    pid: u32,
    question: String,
    history: Option<Vec<ProcessAiFollowUpTurn>>,
    config: Option<AppConfig>,
    state: State<'_, AppState>,
) -> Result<ProcessAiFollowUpAnswer, String> {
    let process = tokio::task::spawn_blocking(move || load_process_by_pid(pid))
        .await
        .map_err(|error| error.to_string())??;

    let config = config.unwrap_or_else(|| state.load_config());
    let advisor_config = AdvisorConfig {
        api_key: config.api_key,
        base_url: config.ai_base_url,
        model: config.ai_model,
        max_items: config.max_ai_items,
        strict_file_ai_remote_only: config.strict_file_ai_remote_only,
    };

    let insight = explain_process(&process, &advisor_config)
        .await
        .map_err(|error| error.to_string())?;
    let history = history.unwrap_or_default();

    answer_process_follow_up(&process, &insight, &question, &history, &advisor_config)
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

fn load_monitor_snapshot(limit: usize) -> Result<ProcessMonitorSnapshot, String> {
    let (summary, top_processes) = std::thread::scope(|scope| {
        let summary_task = scope.spawn(query_system_summary);
        let process_task = scope.spawn(move || load_processes(limit));

        let summary = summary_task
            .join()
            .map_err(|_| "系统摘要采集线程执行失败".to_string())??;
        let top_processes = process_task
            .join()
            .map_err(|_| "进程快照采集线程执行失败".to_string())??;

        Ok::<_, String>((summary, top_processes))
    })?;

    Ok(ProcessMonitorSnapshot {
        collected_at: Utc::now(),
        system_cpu_usage: summary.system_cpu_usage,
        memory_used_bytes: summary.memory_used_bytes,
        memory_total_bytes: summary.memory_total_bytes,
        disk_bytes_per_sec: summary.disk_bytes_per_sec,
        top_processes,
    })
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

    let status = terminate_process_command(pid)?;
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
    #[cfg(target_os = "windows")]
    {
        query_processes_windows()
    }

    #[cfg(target_os = "macos")]
    {
        query_processes_macos()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(Vec::new())
    }
}

fn termination_block_reason(process: &ProcessRecord) -> Option<String> {
    if process.pid == std::process::id() {
        return Some("不能结束当前 smart-disk-cleaner 应用自身进程。".to_string());
    }

    if process.is_critical || process.category == "system_critical" {
        return Some(format!(
            "已阻止结束 {} (PID {})：这是关键系统进程，强制结束可能导致系统不稳定。",
            process.name, process.pid
        ));
    }

    if process.category == "security" {
        return Some(format!(
            "已阻止结束 {} (PID {})：这是安全防护相关进程，结束后可能影响保护能力。",
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

#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessMonitorSnapshot {
    collected_at: chrono::DateTime<chrono::Utc>,
    system_cpu_usage: f32,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
    disk_bytes_per_sec: u64,
    top_processes: Vec<ProcessRecord>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemSummary {
    system_cpu_usage: f32,
    memory_used_bytes: u64,
    memory_total_bytes: u64,
    disk_bytes_per_sec: u64,
}

fn query_system_summary() -> Result<SystemSummary, String> {
    #[cfg(target_os = "windows")]
    {
        query_system_summary_windows()
    }

    #[cfg(target_os = "macos")]
    {
        query_system_summary_macos()
    }

    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        Ok(SystemSummary {
            system_cpu_usage: 0.0,
            memory_used_bytes: 0,
            memory_total_bytes: 0,
            disk_bytes_per_sec: 0,
        })
    }
}

#[cfg(target_os = "windows")]
fn query_processes_windows() -> Result<Vec<ProcessRecord>, String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", PROCESS_QUERY_SCRIPT_WINDOWS])
        .output()
        .map_err(|error| format!("执行 Windows 进程采集脚本失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Windows 进程采集脚本执行失败。".to_string()
        } else {
            format!("Windows 进程采集脚本执行失败：{stderr}")
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

#[cfg(target_os = "macos")]
fn query_processes_macos() -> Result<Vec<ProcessRecord>, String> {
    let output = Command::new("sh")
        .args(["-lc", PROCESS_QUERY_SCRIPT_MACOS])
        .output()
        .map_err(|error| format!("执行 macOS 进程采集脚本失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "macOS 进程采集脚本执行失败。".to_string()
        } else {
            format!("macOS 进程采集脚本执行失败：{stderr}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut rows = Vec::new();

    for line in stdout.lines().filter(|line| !line.trim().is_empty()) {
        let parts = line.split('\t').collect::<Vec<_>>();
        if parts.len() < 9 {
            continue;
        }
        rows.push(ProcessRecord::from(ProcessSample {
            pid: parts[0].trim().parse().unwrap_or(0),
            parent_pid: parts[1].trim().parse().ok(),
            cpu_usage: parts[2].trim().parse().unwrap_or(0.0),
            memory_bytes: parts[3].trim().parse::<u64>().unwrap_or(0) * 1024,
            virtual_memory_bytes: parts[4].trim().parse::<u64>().unwrap_or(0) * 1024,
            run_time_seconds: parse_elapsed_seconds(parts[5].trim()),
            status: parts[6].trim().to_string(),
            name: file_name(parts[7].trim()),
            exe_path: Some(parts[7].trim().to_string()),
            command: if parts[8].trim().is_empty() {
                Vec::new()
            } else {
                vec![parts[8].trim().to_string()]
            },
            disk_read_bytes: 0,
            disk_written_bytes: 0,
        }));
    }

    Ok(rows)
}

#[cfg(target_os = "windows")]
fn query_system_summary_windows() -> Result<SystemSummary, String> {
    let output = Command::new("powershell")
        .args(["-NoProfile", "-Command", SYSTEM_SUMMARY_SCRIPT_WINDOWS])
        .output()
        .map_err(|error| format!("执行 Windows 系统负载采集脚本失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "Windows 系统负载采集脚本执行失败。".to_string()
        } else {
            format!("Windows 系统负载采集脚本执行失败：{stderr}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("Windows 系统负载采集脚本没有返回数据。".to_string());
    }

    serde_json::from_str::<SystemSummary>(trimmed)
        .map_err(|error| format!("解析系统负载采集结果失败：{error}"))
}

#[cfg(target_os = "macos")]
fn query_system_summary_macos() -> Result<SystemSummary, String> {
    let output = Command::new("sh")
        .args(["-lc", SYSTEM_SUMMARY_SCRIPT_MACOS])
        .output()
        .map_err(|error| format!("执行 macOS 系统负载采集脚本失败：{error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            "macOS 系统负载采集脚本执行失败。".to_string()
        } else {
            format!("macOS 系统负载采集脚本执行失败：{stderr}")
        });
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return Err("macOS 系统负载采集脚本没有返回数据。".to_string());
    }

    serde_json::from_str::<SystemSummary>(trimmed)
        .map_err(|error| format!("解析系统负载采集结果失败：{error}"))
}

#[cfg(target_os = "windows")]
fn terminate_process_command(pid: u32) -> Result<std::process::ExitStatus, String> {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!("Stop-Process -Id {pid} -Force -ErrorAction Stop"),
        ])
        .status()
        .map_err(|error| format!("调用 Stop-Process 失败：{error}"))
}

#[cfg(target_os = "macos")]
fn terminate_process_command(pid: u32) -> Result<std::process::ExitStatus, String> {
    Command::new("kill")
        .args(["-9", &pid.to_string()])
        .status()
        .map_err(|error| format!("执行 kill 失败：{error}"))
}

#[cfg(target_os = "macos")]
fn parse_elapsed_seconds(value: &str) -> u64 {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return 0;
    }

    let (days, time_part) = if let Some((days, rest)) = trimmed.split_once('-') {
        (days.parse::<u64>().unwrap_or(0), rest)
    } else {
        (0, trimmed)
    };

    let time_segments = time_part
        .split(':')
        .filter_map(|part| part.parse::<u64>().ok())
        .collect::<Vec<_>>();

    let seconds = match time_segments.as_slice() {
        [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
        [minutes, seconds] => minutes * 60 + seconds,
        [seconds] => *seconds,
        _ => 0,
    };

    days * 24 * 3600 + seconds
}

#[cfg(target_os = "macos")]
fn file_name(value: &str) -> String {
    std::path::Path::new(value)
        .file_name()
        .and_then(|part| part.to_str())
        .map(|part| part.to_string())
        .unwrap_or_else(|| value.to_string())
}

#[cfg(target_os = "windows")]
const PROCESS_QUERY_SCRIPT_WINDOWS: &str = r#"
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
    try { $startTime = $proc.StartTime } catch { $startTime = $null }

    $runTimeSeconds = 0
    if ($null -ne $startTime) {
      $runTimeSeconds = [uint64]([DateTime]::Now - $startTime).TotalSeconds
    }

    $exePath = $null
    try { $exePath = $proc.Path } catch { $exePath = $null }

    $displayName = if ($exePath) { [System.IO.Path]::GetFileName($exePath) } else { "$($proc.ProcessName).exe" }

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

#[cfg(target_os = "windows")]
const SYSTEM_SUMMARY_SCRIPT_WINDOWS: &str = r#"
try {
  $cpu = [double]((Get-Counter '\Processor(_Total)\% Processor Time').CounterSamples | Select-Object -First 1 -ExpandProperty CookedValue)
  $disk = [double]((Get-Counter '\PhysicalDisk(_Total)\Disk Bytes/sec').CounterSamples | Select-Object -First 1 -ExpandProperty CookedValue)
  Add-Type -AssemblyName Microsoft.VisualBasic
  $info = New-Object Microsoft.VisualBasic.Devices.ComputerInfo
  $memoryTotal = [uint64]$info.TotalPhysicalMemory
  $memoryFree = [uint64]$info.AvailablePhysicalMemory
  $memoryUsed = if ($memoryTotal -gt $memoryFree) { $memoryTotal - $memoryFree } else { 0 }

  [pscustomobject]@{
    systemCpuUsage = [Math]::Round($cpu, 1)
    memoryUsedBytes = $memoryUsed
    memoryTotalBytes = $memoryTotal
    diskBytesPerSec = [uint64][Math]::Max(0, $disk)
  } | ConvertTo-Json -Compress
} catch {
  throw $_
}
"#;

#[cfg(target_os = "macos")]
const PROCESS_QUERY_SCRIPT_MACOS: &str = r#"
ps -axo pid=,ppid=,%cpu=,rss=,vsz=,etime=,state=,comm=,command= -ww | awk '{
  pid=$1; ppid=$2; cpu=$3; rss=$4; vsz=$5; etime=$6; state=$7; comm=$8;
  cmd="";
  for (i=9; i<=NF; i++) {
    cmd = cmd (i==9 ? "" : " ") $i;
  }
  printf "%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n", pid, ppid, cpu, rss, vsz, etime, state, comm, cmd;
}'
"#;

#[cfg(target_os = "macos")]
const SYSTEM_SUMMARY_SCRIPT_MACOS: &str = r#"
MEM_TOTAL=$(sysctl -n hw.memsize)
PAGE_SIZE=$(pagesize)
FREE_PAGES=$(vm_stat | awk '/Pages free/ {gsub("\\.","",$3); print $3}')
INACTIVE_PAGES=$(vm_stat | awk '/Pages inactive/ {gsub("\\.","",$3); print $3}')
AVAILABLE=$(( (FREE_PAGES + INACTIVE_PAGES) * PAGE_SIZE ))
USED=$(( MEM_TOTAL - AVAILABLE ))
CPU_COUNT=$(sysctl -n hw.ncpu)
CPU_TOTAL=$(ps -axo %cpu= | awk '{sum+=$1} END {print sum+0}')
CPU_USAGE=$(awk -v total="$CPU_TOTAL" -v count="$CPU_COUNT" 'BEGIN { if (count <= 0) print 0; else printf "%.1f", total / count }')
printf '{"systemCpuUsage":%s,"memoryUsedBytes":%s,"memoryTotalBytes":%s,"diskBytesPerSec":0}' "$CPU_USAGE" "$USED" "$MEM_TOTAL"
"#;
