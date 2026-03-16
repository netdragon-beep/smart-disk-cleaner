use crate::diagnostics::{diagnose_io_error, DiagnosticOperation};
use crate::models::{ExecutionMode, OperationLogEntry, PathDiagnosis, ScanReport, SuggestedAction};
use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    pub report_path: PathBuf,
    pub mode: ExecutionMode,
    pub paths: Vec<PathBuf>,
    pub target_dir: Option<PathBuf>,
    pub dry_run: bool,
}

pub fn execute_from_report(options: &ExecuteOptions) -> Result<Vec<OperationLogEntry>> {
    let report_text = fs::read_to_string(&options.report_path)
        .with_context(|| format!("failed to read report: {}", options.report_path.display()))?;
    let report: ScanReport = serde_json::from_str(&report_text)
        .with_context(|| format!("invalid report format: {}", options.report_path.display()))?;

    let mut logs = Vec::new();
    for path in &options.paths {
        let suggestion = report
            .advisor
            .suggestions
            .iter()
            .find(|item| item.path == *path);
        let Some(suggestion) = suggestion else {
            logs.push(OperationLogEntry {
                at: Utc::now(),
                path: path.clone(),
                mode: options.mode,
                dry_run: options.dry_run,
                success: false,
                detail: "该路径不在建议清单中，已拒绝执行。".to_string(),
                diagnosis: None,
            });
            continue;
        };

        if matches!(suggestion.action, SuggestedAction::Keep | SuggestedAction::Review) {
            logs.push(OperationLogEntry {
                at: Utc::now(),
                path: path.clone(),
                mode: options.mode,
                dry_run: options.dry_run,
                success: false,
                detail: "该文件被标记为“保留”或“待审”，系统已禁止直接清理，请先人工确认。".to_string(),
                diagnosis: None,
            });
            continue;
        }

        let result = match options.mode {
            ExecutionMode::Recycle => recycle_path(path, options.dry_run),
            ExecutionMode::Move => move_path(path, options.target_dir.as_deref(), options.dry_run),
        };

        match result {
            Ok(detail) => logs.push(OperationLogEntry {
                at: Utc::now(),
                path: path.clone(),
                mode: options.mode,
                dry_run: options.dry_run,
                success: true,
                detail,
                diagnosis: None,
            }),
            Err(err) => logs.push(OperationLogEntry {
                at: Utc::now(),
                path: path.clone(),
                mode: options.mode,
                dry_run: options.dry_run,
                success: false,
                detail: err.detail,
                diagnosis: Some(err.diagnosis),
            }),
        }
    }

    Ok(logs)
}

fn recycle_path(path: &Path, dry_run: bool) -> std::result::Result<String, ExecutionFailure> {
    if !path.exists() {
        return Err(ExecutionFailure::message_only(
            format!("目标不存在：{}", path.display()),
            path,
            DiagnosticOperation::Recycle,
        ));
    }

    if dry_run {
        return Ok("模拟执行：已演练回收操作".to_string());
    }

    trash::delete(path).map_err(|err| {
        let io_error = classify_recycle_error(&err);
        ExecutionFailure::from_io_error(
            format!("移入回收站失败：{}", err),
            path,
            DiagnosticOperation::Recycle,
            &io_error,
        )
    })?;
    Ok("已移入回收站".to_string())
}

fn move_path(
    path: &Path,
    target_dir: Option<&Path>,
    dry_run: bool,
) -> std::result::Result<String, ExecutionFailure> {
    let target_dir = target_dir.ok_or_else(|| {
        ExecutionFailure::message_only(
            "移动模式需要指定目标目录。".to_string(),
            path,
            DiagnosticOperation::Move,
        )
    })?;
    if !path.exists() {
        return Err(ExecutionFailure::message_only(
            format!("目标不存在：{}", path.display()),
            path,
            DiagnosticOperation::Move,
        ));
    }

    fs::create_dir_all(target_dir).map_err(|err| {
        ExecutionFailure::from_io_error(
            format!("创建目标目录失败：{}", target_dir.display()),
            target_dir,
            DiagnosticOperation::Move,
            &err,
        )
    })?;
    let destination = unique_destination_path(target_dir, path)
        .map_err(|err| ExecutionFailure::message_only(err.to_string(), path, DiagnosticOperation::Move))?;

    if dry_run {
        return Ok(format!("模拟执行：将移动到 {}", destination.display()));
    }

    match fs::rename(path, &destination) {
        Ok(_) => Ok(format!("已移动到 {}", destination.display())),
        Err(rename_err) => match fallback_move(path, &destination) {
            Ok(_) => Ok(format!("已移动到 {}", destination.display())),
            Err(fallback_err) => {
                let io_error = fallback_err
                    .downcast_ref::<io::Error>()
                    .map(copy_io_error)
                    .unwrap_or_else(|| io::Error::other(fallback_err.to_string()));
                Err(ExecutionFailure::from_io_error(
                    format!(
                        "重命名失败且回退移动也失败：{} -> {}（{}）",
                        path.display(),
                        destination.display(),
                        rename_err
                    ),
                    path,
                    DiagnosticOperation::Move,
                    &io_error,
                ))
            }
        },
    }
}

fn unique_destination_path(target_dir: &Path, source: &Path) -> Result<PathBuf> {
    let file_name = source
        .file_name()
        .ok_or_else(|| anyhow!("无效的文件名：{}", source.display()))?;
    let initial = target_dir.join(file_name);
    if !initial.exists() {
        return Ok(initial);
    }

    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| anyhow!("无效的文件名：{}", source.display()))?;
    let extension = source.extension().and_then(|value| value.to_str());

    for index in 1..1000 {
        let candidate = match extension {
            Some(extension) => target_dir.join(format!("{stem}_{index}.{extension}")),
            None => target_dir.join(format!("{stem}_{index}")),
        };
        if !candidate.exists() {
            return Ok(candidate);
        }
    }

    bail!("目标目录中的同名文件过多：{}", target_dir.display())
}

fn fallback_move(source: &Path, destination: &Path) -> Result<()> {
    if source.is_file() {
        fs::copy(source, destination)?;
        fs::remove_file(source)?;
        return Ok(());
    }

    if source.is_dir() {
        copy_dir_recursive(source, destination)?;
        fs::remove_dir_all(source)?;
        return Ok(());
    }

    bail!("不支持的文件系统项：{}", source.display())
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let entry_path = entry.path();
        let child_destination = destination.join(entry.file_name());
        let metadata = entry.metadata()?;

        if metadata.is_dir() {
            copy_dir_recursive(&entry_path, &child_destination)?;
        } else if metadata.is_file() {
            fs::copy(&entry_path, &child_destination)?;
        } else {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                format!("不支持的文件系统项：{}", entry_path.display()),
            )
            .into());
        }
    }

    Ok(())
}

#[derive(Debug)]
struct ExecutionFailure {
    detail: String,
    diagnosis: PathDiagnosis,
}

impl ExecutionFailure {
    fn from_io_error(
        detail: String,
        path: &Path,
        operation: DiagnosticOperation,
        err: &io::Error,
    ) -> Self {
        Self {
            detail,
            diagnosis: diagnose_io_error(path, operation, err),
        }
    }

    fn message_only(detail: String, path: &Path, operation: DiagnosticOperation) -> Self {
        Self {
            diagnosis: PathDiagnosis {
                path: path.to_path_buf(),
                operation: operation.as_str().to_string(),
                code: crate::models::DiagnosticCode::Unknown,
                severity: crate::models::DiagnosticSeverity::Warning,
                summary: detail.clone(),
                details: vec![detail.clone()],
                suggestions: vec!["请确认路径状态后重试该操作。".to_string()],
                possible_related_apps: Vec::new(),
                error_kind: None,
                raw_os_error: None,
            },
            detail,
        }
    }
}

fn classify_recycle_error(error: &trash::Error) -> io::Error {
    let text = error.to_string().to_ascii_lowercase();
    if text.contains("being used by another process") || text.contains("sharing violation") {
        return io::Error::from_raw_os_error(32);
    }
    if text.contains("lock violation") {
        return io::Error::from_raw_os_error(33);
    }
    if text.contains("access is denied") || text.contains("permission denied") {
        return io::Error::new(io::ErrorKind::PermissionDenied, error.to_string());
    }
    if text.contains("not found") || text.contains("cannot find") {
        return io::Error::new(io::ErrorKind::NotFound, error.to_string());
    }
    io::Error::new(io::ErrorKind::Other, error.to_string())
}

fn copy_io_error(error: &io::Error) -> io::Error {
    if let Some(code) = error.raw_os_error() {
        io::Error::from_raw_os_error(code)
    } else {
        io::Error::new(error.kind(), error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{execute_from_report, ExecuteOptions};
    use crate::models::{
        AdvisorOutput, AnalysisResult, DedupResult, ExecutionMode, FileSuggestion, RiskLevel,
        ScanReport, SuggestedAction,
    };
    use chrono::Utc;
    use std::fs;
    use std::path::PathBuf;

    fn write_report_file(report: &ScanReport, name: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!(
            "smart_disk_cleaner_executor_test_{}_{}",
            name,
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("test dir should be created");
        let report_path = base.join("report.json");
        let report_text = serde_json::to_string(report).expect("report should serialize");
        fs::write(&report_path, report_text).expect("report should be written");
        report_path
    }

    fn empty_report(suggestions: Vec<FileSuggestion>) -> ScanReport {
        ScanReport {
            generated_at: Utc::now(),
            root: PathBuf::from(r"E:\test"),
            analysis: AnalysisResult {
                total_files: 0,
                total_size: 0,
                empty_files: Vec::new(),
                empty_dirs: Vec::new(),
                large_files: Vec::new(),
                type_breakdown: Vec::new(),
            },
            dedup: DedupResult {
                groups: Vec::new(),
                failures: Vec::new(),
            },
            advisor: AdvisorOutput {
                source: "local_rules".to_string(),
                summary: String::new(),
                suggestions,
            },
            failures: Vec::new(),
        }
    }

    #[test]
    fn rejects_keep_or_review_suggestions() {
        let kept_path = PathBuf::from(r"E:\test\README.md");
        let report = empty_report(vec![FileSuggestion {
            path: kept_path.clone(),
            action: SuggestedAction::Keep,
            risk: RiskLevel::Low,
            reason: "保留".to_string(),
        }]);
        let report_path = write_report_file(&report, "keep");

        let logs = execute_from_report(&ExecuteOptions {
            report_path,
            mode: ExecutionMode::Recycle,
            paths: vec![kept_path],
            target_dir: None,
            dry_run: true,
        })
        .expect("execution should return logs");

        assert_eq!(logs.len(), 1);
        assert!(!logs[0].success);
        assert!(logs[0].detail.contains("禁止直接清理"));
    }

    #[test]
    fn still_allows_delete_suggestions_in_dry_run() {
        let base = std::env::temp_dir().join(format!(
            "smart_disk_cleaner_executor_actionable_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).expect("test dir should be created");
        let file_path = base.join("cache.bin");
        fs::write(&file_path, b"demo").expect("test file should be created");

        let report = empty_report(vec![FileSuggestion {
            path: file_path.clone(),
            action: SuggestedAction::Delete,
            risk: RiskLevel::Medium,
            reason: "删除".to_string(),
        }]);
        let report_path = write_report_file(&report, "delete");

        let logs = execute_from_report(&ExecuteOptions {
            report_path,
            mode: ExecutionMode::Recycle,
            paths: vec![file_path],
            target_dir: None,
            dry_run: true,
        })
        .expect("execution should return logs");

        assert_eq!(logs.len(), 1);
        assert!(logs[0].success);
        assert!(logs[0].detail.contains("模拟执行"));
    }
}
