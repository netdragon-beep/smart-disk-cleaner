use crate::models::{DiagnosticCode, DiagnosticSeverity, PathDiagnosis};
use std::fs;
use std::io;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticOperation {
    Probe,
    Recycle,
    Move,
}

impl DiagnosticOperation {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticOperation::Probe => "probe",
            DiagnosticOperation::Recycle => "recycle",
            DiagnosticOperation::Move => "move",
        }
    }
}

pub fn probe_path(path: &Path, operation: DiagnosticOperation) -> PathDiagnosis {
    let mut details = Vec::new();
    let mut suggestions = Vec::new();
    let app_hints = infer_related_apps(path);

    if !path.exists() {
        suggestions.push("请先确认路径是否正确，并在执行清理前重新扫描。".to_string());
        return PathDiagnosis {
            path: path.to_path_buf(),
            operation: operation.as_str().to_string(),
            code: DiagnosticCode::NotFound,
            severity: DiagnosticSeverity::Warning,
            summary: "目标路径不存在。".to_string(),
            details,
            suggestions,
            possible_related_apps: app_hints,
            error_kind: None,
            raw_os_error: None,
        };
    }

    match fs::metadata(path) {
        Ok(metadata) => {
            details.push(format!(
                "路径类型：{}",
                if metadata.is_dir() { "目录" } else { "文件" }
            ));
            details.push(format!("只读：{}", metadata.permissions().readonly()));
            if metadata.is_file() {
                details.push(format!("文件大小：{} 字节", metadata.len()));
            }
            if metadata.permissions().readonly() {
                suggestions.push("只读文件可能需要先调整权限后才能清理。".to_string());
            }
            suggestions.push(
                "探测模式不会枚举占用该文件的进程；如果后续执行失败，请重新执行清理以获得更具体的诊断信息。"
                    .to_string(),
            );

            PathDiagnosis {
                path: path.to_path_buf(),
                operation: operation.as_str().to_string(),
                code: DiagnosticCode::Ok,
                severity: DiagnosticSeverity::Info,
                summary: "路径可访问，当前未发现明确异常。".to_string(),
                details,
                suggestions,
                possible_related_apps: app_hints,
                error_kind: None,
                raw_os_error: None,
            }
        }
        Err(err) => diagnose_io_error(path, operation, &err),
    }
}

pub fn diagnose_io_error(
    path: &Path,
    operation: DiagnosticOperation,
    err: &io::Error,
) -> PathDiagnosis {
    let raw_os_error = err.raw_os_error();
    let error_kind = Some(format!("{:?}", err.kind()));
    let mut details = vec![format!("操作系统错误：{}", err)];
    let possible_related_apps = infer_related_apps(path);

    let (code, severity, summary, mut suggestions) = match raw_os_error {
        Some(32) => (
            DiagnosticCode::InUseByAnotherProcess,
            DiagnosticSeverity::Critical,
            "该路径正被其他进程占用。".to_string(),
            vec![
                "请关闭可能占用该文件的编辑器、终端、压缩工具、播放器或同步客户端。".to_string(),
                "关闭相关程序后再重试。".to_string(),
            ],
        ),
        Some(33) => (
            DiagnosticCode::LockedRegion,
            DiagnosticSeverity::Critical,
            "某个进程锁定了该文件的一部分。".to_string(),
            vec![
                "请关闭正在读取或写入该文件的应用程序。".to_string(),
                "如果该文件属于数据库、日志或虚拟机镜像，请先停止相关服务。".to_string(),
            ],
        ),
        _ => match err.kind() {
            io::ErrorKind::NotFound => (
                DiagnosticCode::NotFound,
                DiagnosticSeverity::Warning,
                "未找到目标路径。".to_string(),
                vec!["请重新扫描目录，并确认该路径仍然存在。".to_string()],
            ),
            io::ErrorKind::PermissionDenied => (
                if is_readonly(path) {
                    DiagnosticCode::ReadOnly
                } else {
                    DiagnosticCode::PermissionDenied
                },
                DiagnosticSeverity::Critical,
                "当前进程权限不足。".to_string(),
                vec![
                    "请检查文件权限以及路径是否为只读。".to_string(),
                    "仅在确认文件可安全处理后，再考虑使用更高权限重试。".to_string(),
                ],
            ),
            io::ErrorKind::AlreadyExists => (
                DiagnosticCode::AlreadyExists,
                DiagnosticSeverity::Warning,
                "目标位置已存在同名项。".to_string(),
                vec!["请更换目标目录，或重命名目标文件。".to_string()],
            ),
            io::ErrorKind::InvalidInput => (
                DiagnosticCode::InvalidInput,
                DiagnosticSeverity::Warning,
                "提供的路径或参数不适用于当前操作。".to_string(),
                vec!["请检查命令参数和目标路径。".to_string()],
            ),
            io::ErrorKind::DirectoryNotEmpty => (
                DiagnosticCode::DirectoryNotEmpty,
                DiagnosticSeverity::Warning,
                "目录本应为空，但其中仍包含文件。".to_string(),
                vec!["请先检查目录内容，再重试。".to_string()],
            ),
            io::ErrorKind::Unsupported => (
                DiagnosticCode::Unsupported,
                DiagnosticSeverity::Warning,
                "当前文件系统或平台不支持该操作。".to_string(),
                vec!["可以改用“复制后删除”方案，或更换目标文件系统。".to_string()],
            ),
            _ => (
                DiagnosticCode::Unknown,
                DiagnosticSeverity::Warning,
                "该操作因未分类的 I/O 原因失败。".to_string(),
                vec![
                    "请检查原始系统错误以及当前文件系统状态。".to_string(),
                    "确认文件在清理过程中不会被修改后再重试。".to_string(),
                ],
            ),
        },
    };

    if let Some(code_value) = raw_os_error {
        details.push(format!("原始系统错误码={code_value}"));
    }
    details.push(format!("错误类型={:?}", err.kind()));

    if suggestions.is_empty() {
        suggestions.push("请确认路径状态后再重试。".to_string());
    }

    PathDiagnosis {
        path: path.to_path_buf(),
        operation: operation.as_str().to_string(),
        code,
        severity,
        summary,
        details,
        suggestions,
        possible_related_apps,
        error_kind,
        raw_os_error,
    }
}

fn infer_related_apps(path: &Path) -> Vec<String> {
    let mut hints = Vec::new();
    let path_text = path.to_string_lossy().to_ascii_lowercase();

    if path_text.contains("downloads") {
        hints.push("浏览器或下载工具".to_string());
    }
    if path_text.contains("node_modules")
        || path_text.contains("\\target\\")
        || path_text.contains("/target/")
    {
        hints.push("IDE、构建工具或语言服务".to_string());
    }
    if path_text.contains("appdata") || path_text.contains("cache") || path_text.contains("temp") {
        hints.push("桌面应用缓存、浏览器或系统后台进程".to_string());
    }
    if path_text.contains("onedrive") || path_text.contains("dropbox") {
        hints.push("文件同步客户端".to_string());
    }
    if path_text.contains("steam") || path_text.contains("games") {
        hints.push("游戏平台启动器".to_string());
    }

    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        match extension.to_ascii_lowercase().as_str() {
            "zip" | "7z" | "rar" | "iso" => hints.push("压缩工具或下载器".to_string()),
            "pdf" => hints.push("PDF 阅读器".to_string()),
            "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" => {
                hints.push("办公软件".to_string())
            }
            "sqlite" | "db" | "log" => hints.push("数据库客户端、浏览器或 IDE".to_string()),
            "rs" | "toml" | "js" | "ts" | "java" | "py" | "cpp" => {
                hints.push("IDE、终端或构建工具".to_string())
            }
            "mp4" | "mp3" | "mkv" => hints.push("媒体播放器".to_string()),
            _ => {}
        }
    }

    hints.sort();
    hints.dedup();
    hints
}

fn is_readonly(path: &Path) -> bool {
    fs::metadata(path)
        .map(|metadata| metadata.permissions().readonly())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{diagnose_io_error, DiagnosticOperation};
    use crate::models::DiagnosticCode;
    use std::io;
    use std::path::Path;

    #[test]
    fn detects_windows_sharing_violation() {
        let err = io::Error::from_raw_os_error(32);
        let diagnosis = diagnose_io_error(Path::new("demo.zip"), DiagnosticOperation::Recycle, &err);
        assert_eq!(diagnosis.code, DiagnosticCode::InUseByAnotherProcess);
    }

    #[test]
    fn maps_not_found_error() {
        let err = io::Error::new(io::ErrorKind::NotFound, "missing");
        let diagnosis = diagnose_io_error(Path::new("missing.txt"), DiagnosticOperation::Probe, &err);
        assert_eq!(diagnosis.code, DiagnosticCode::NotFound);
    }
}
