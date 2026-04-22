use crate::state::AppState;
use smart_disk_cleaner_core::executor::{execute_from_report, ExecuteOptions};
use smart_disk_cleaner_core::models::{ExecutionMode, OperationLogEntry};
use smart_disk_cleaner_core::reporter::write_report;
use std::fs;
use std::path::{Path, PathBuf};
use tauri::State;

#[cfg(target_os = "windows")]
use std::ffi::OsString;
#[cfg(target_os = "windows")]
use std::mem::MaybeUninit;
#[cfg(target_os = "windows")]
use std::os::windows::ffi::{OsStrExt, OsStringExt};
#[cfg(target_os = "windows")]
use windows_sys::Win32::Foundation::{ERROR_MORE_DATA, FILETIME};
#[cfg(target_os = "windows")]
use windows_sys::Win32::System::RestartManager::{
    RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
    RM_PROCESS_INFO,
};

#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockingProcessInfo {
    pid: u32,
    app_name: String,
    service_name: Option<String>,
    restartable: bool,
    process_start_time: u64,
}

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
        .ok_or("当前没有扫描报告，请先执行扫描。")?;

    let temp_report_path = state
        .config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
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

#[tauri::command]
pub async fn get_path_blockers(path: String) -> Result<Vec<BlockingProcessInfo>, String> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = path;
        Ok(Vec::new())
    }

    #[cfg(target_os = "windows")]
    {
        get_path_blockers_windows(Path::new(&path))
    }
}

#[cfg(target_os = "windows")]
fn get_path_blockers_windows(path: &Path) -> Result<Vec<BlockingProcessInfo>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let mut session_handle = 0_u32;
    let mut session_key = [0u16; CCH_RM_SESSION_KEY as usize + 1];

    let start = unsafe { RmStartSession(&mut session_handle, 0, session_key.as_mut_ptr()) };
    if start != 0 {
        return Err(format!("启动 Restart Manager 会话失败，错误码 {start}"));
    }

    let result = (|| unsafe {
        let file_path = wide_path(path);
        let resources = [file_path.as_ptr()];
        let register = RmRegisterResources(
            session_handle,
            1,
            resources.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        );
        if register != 0 {
            return Err(format!("注册要检测的路径失败，错误码 {register}"));
        }

        let mut needed = 0_u32;
        let mut count = 0_u32;
        let mut reasons = 0_u32;
        let first = RmGetList(
            session_handle,
            &mut needed,
            &mut count,
            std::ptr::null_mut(),
            &mut reasons,
        );

        if first != 0 && first != ERROR_MORE_DATA {
            return Err(format!("获取占用进程列表失败，错误码 {first}"));
        }
        if needed == 0 {
            return Ok(Vec::new());
        }

        let mut entries = vec![MaybeUninit::<RM_PROCESS_INFO>::zeroed(); needed as usize];
        count = needed;
        let second = RmGetList(
            session_handle,
            &mut needed,
            &mut count,
            entries.as_mut_ptr() as *mut RM_PROCESS_INFO,
            &mut reasons,
        );
        if second != 0 {
            return Err(format!("读取占用进程信息失败，错误码 {second}"));
        }

        let processes = entries
            .into_iter()
            .take(count as usize)
            .map(|entry: MaybeUninit<RM_PROCESS_INFO>| entry.assume_init())
            .map(map_rm_process)
            .collect::<Vec<_>>();
        Ok(processes)
    })();

    unsafe {
        let _ = RmEndSession(session_handle);
    }

    result
}

#[cfg(target_os = "windows")]
fn map_rm_process(entry: RM_PROCESS_INFO) -> BlockingProcessInfo {
    BlockingProcessInfo {
        pid: entry.Process.dwProcessId,
        app_name: utf16_to_string(&entry.strAppName)
            .unwrap_or_else(|| format!("PID {}", entry.Process.dwProcessId)),
        service_name: utf16_to_string(&entry.strServiceShortName),
        restartable: entry.bRestartable != 0,
        process_start_time: filetime_to_u64(entry.Process.ProcessStartTime),
    }
}

#[cfg(target_os = "windows")]
fn wide_path(path: &Path) -> Vec<u16> {
    path.as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect()
}

#[cfg(target_os = "windows")]
fn utf16_to_string<const N: usize>(value: &[u16; N]) -> Option<String> {
    let len = value.iter().position(|ch| *ch == 0).unwrap_or(N);
    if len == 0 {
        return None;
    }
    Some(
        OsString::from_wide(&value[..len])
            .to_string_lossy()
            .to_string(),
    )
}

#[cfg(target_os = "windows")]
fn filetime_to_u64(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}
