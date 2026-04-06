use crate::models::{
    AdvisorOutput, AiInsightTargetKind, AnalysisResult, DedupResult, FileAiInsight, FileRecord,
    FileSuggestion, RiskLevel, ScanReport, ScanResult, SuggestedAction,
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use tracing::warn;

#[derive(Debug, Clone)]
pub struct AdvisorConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub max_items: usize,
    pub strict_file_ai_remote_only: bool,
}

pub async fn build_advice(
    scan: &ScanResult,
    analysis: &AnalysisResult,
    dedup: &DedupResult,
    config: &AdvisorConfig,
) -> Result<AdvisorOutput> {
    let suggestions = build_rule_based_suggestions(analysis, dedup);

    if let Some(api_key) = config
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        match request_ai_review(scan, analysis, dedup, &suggestions, config, api_key).await {
            Ok(ai_output) => {
                return Ok(AdvisorOutput {
                    source: format!("remote:{}", config.model),
                    summary: ai_output.summary,
                    suggestions: ai_output.suggestions,
                });
            }
            Err(err) => {
                warn!("AI summary failed, falling back to local rules: {err}");
            }
        }
    }

    Ok(AdvisorOutput {
        source: "local_rules".to_string(),
        summary: build_display_summary(analysis, dedup, &suggestions),
        suggestions,
    })
}

pub fn build_local_advice(analysis: &AnalysisResult, dedup: &DedupResult) -> AdvisorOutput {
    let suggestions = build_rule_based_suggestions(analysis, dedup);
    AdvisorOutput {
        source: "local_rules".to_string(),
        summary: build_display_summary(analysis, dedup, &suggestions),
        suggestions,
    }
}

pub async fn explain_file(
    report: &ScanReport,
    path: &Path,
    config: &AdvisorConfig,
) -> Result<FileAiInsight> {
    let resolved_path = resolve_report_file_path(report, path);
    let file = report
        .scanned_files
        .iter()
        .find(|item| item.path == resolved_path)
        .ok_or_else(|| anyhow!("未在最近一次扫描结果中找到该文件。"))?;

    let local_insight = build_local_file_insight(report, file);

    let Some(api_key) = config
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    else {
        if config.strict_file_ai_remote_only {
            return Err(anyhow!(
                "当前未配置可用的 AI API Key，且已启用“仅允许远程 AI，不允许静默回退”。"
            ));
        }
        return Ok(local_insight);
    };

    match request_file_insight(report, file, &local_insight, config, api_key).await {
        Ok(insight) => Ok(insight),
        Err(err) => {
            warn!("AI file insight failed, falling back to local rules: {err}");
            if config.strict_file_ai_remote_only {
                return Err(anyhow!("远程 AI 调用失败：{err}"));
            }
            let mut fallback = local_insight;
            fallback.remote_attempted = true;
            fallback.used_fallback = true;
            fallback.fallback_reason = Some(err.to_string());
            Ok(fallback)
        }
    }
}

fn resolve_report_file_path(report: &ScanReport, path: &Path) -> PathBuf {
    if path.is_absolute() {
        return path.to_path_buf();
    }
    report.root.join(path)
}

pub async fn explain_path(
    report: &ScanReport,
    path: &Path,
    config: &AdvisorConfig,
) -> Result<FileAiInsight> {
    let resolved_path = resolve_report_file_path(report, path);
    let directory_files = directory_files(report, &resolved_path);
    let is_known_empty_dir = report
        .analysis
        .empty_dirs
        .iter()
        .any(|item| item == &resolved_path);

    let local_insight = if let Some(file) = report
        .scanned_files
        .iter()
        .find(|item| item.path == resolved_path)
    {
        build_local_file_insight(report, file)
    } else if !directory_files.is_empty() || is_known_empty_dir || resolved_path == report.root {
        build_local_directory_insight(report, &resolved_path, &directory_files, is_known_empty_dir)
    } else {
        return Err(anyhow!("未在最近一次扫描结果中找到该路径。"));
    };

    let Some(api_key) = config
        .api_key
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    else {
        if config.strict_file_ai_remote_only {
            return Err(anyhow!(
                "当前未配置可用的 AI API Key，且已启用“仅允许远程 AI，不允许静默回退”。"
            ));
        }
        return Ok(local_insight);
    };

    match if local_insight.target_kind == AiInsightTargetKind::File {
        let file = report
            .scanned_files
            .iter()
            .find(|item| item.path == resolved_path)
            .ok_or_else(|| anyhow!("未在最近一次扫描结果中找到该文件。"))?;
        request_file_insight(report, file, &local_insight, config, api_key).await
    } else {
        request_directory_insight(
            report,
            &resolved_path,
            &directory_files,
            is_known_empty_dir,
            &local_insight,
            config,
            api_key,
        )
        .await
    } {
        Ok(insight) => Ok(insight),
        Err(err) => {
            warn!("AI path insight failed, falling back to local rules: {err}");
            if config.strict_file_ai_remote_only {
                return Err(anyhow!("远程 AI 调用失败：{err}"));
            }
            let mut fallback = local_insight;
            fallback.remote_attempted = true;
            fallback.used_fallback = true;
            fallback.fallback_reason = Some(err.to_string());
            Ok(fallback)
        }
    }
}

fn directory_files<'a>(report: &'a ScanReport, dir_path: &Path) -> Vec<&'a FileRecord> {
    report
        .scanned_files
        .iter()
        .filter(|item| item.path.starts_with(dir_path) && item.path != dir_path)
        .collect()
}

fn build_rule_based_suggestions(
    analysis: &AnalysisResult,
    dedup: &DedupResult,
) -> Vec<FileSuggestion> {
    let mut suggestions: HashMap<std::path::PathBuf, RankedSuggestion> = HashMap::new();

    for file in &analysis.empty_files {
        upsert_suggestion(
            &mut suggestions,
            FileSuggestion {
                path: file.path.clone(),
                action: SuggestedAction::Delete,
                risk: RiskLevel::Low,
                reason: "空文件通常可以在快速确认后安全清理。".to_string(),
            },
            30,
        );
    }

    for path in &analysis.empty_dirs {
        upsert_suggestion(
            &mut suggestions,
            FileSuggestion {
                path: path.clone(),
                action: SuggestedAction::Delete,
                risk: RiskLevel::Low,
                reason: "空目录不包含有效数据，适合作为优先清理目标。".to_string(),
            },
            30,
        );
    }

    for file in &analysis.temporary_files {
        upsert_suggestion(
            &mut suggestions,
            FileSuggestion {
                path: file.path.clone(),
                action: SuggestedAction::Delete,
                risk: RiskLevel::Low,
                reason: "临时文件通常是下载中间态、调试残留或缓存副本，适合优先清理。".to_string(),
            },
            40,
        );
    }

    for file in analysis.large_files.iter().take(20) {
        let action = if looks_archive_or_installer(&file.path) && !is_document_like(file) {
            SuggestedAction::Move
        } else {
            SuggestedAction::Review
        };
        let reason = if is_document_like(file) {
            "文档类文件即使体积较大，也可能是唯一资料，默认仅建议人工复核。"
        } else if action == SuggestedAction::Move {
            "大型安装包和压缩包更适合移动到归档目录，而不是继续占用常用磁盘空间。"
        } else {
            "该大文件占用了较多空间，建议在删除或移动前先人工确认。"
        };
        upsert_suggestion(
            &mut suggestions,
            FileSuggestion {
                path: file.path.clone(),
                action,
                risk: RiskLevel::Medium,
                reason: reason.to_string(),
            },
            10,
        );
    }

    for group in &dedup.groups {
        let cautious_duplicate = should_require_manual_review(group);
        let auto_cleanup_duplicate = should_auto_cleanup_duplicate(group);
        for file in &group.files {
            let action = if cautious_duplicate {
                SuggestedAction::Review
            } else if auto_cleanup_duplicate {
                if Some(&file.path) == group.suggested_keep.as_ref() {
                    SuggestedAction::Keep
                } else {
                    SuggestedAction::Delete
                }
            } else if Some(&file.path) == group.suggested_keep.as_ref() {
                SuggestedAction::Keep
            } else {
                SuggestedAction::Review
            };
            let risk = if cautious_duplicate {
                RiskLevel::High
            } else if auto_cleanup_duplicate {
                if is_in_transient_location(file) {
                    RiskLevel::Low
                } else {
                    RiskLevel::Medium
                }
            } else if action == SuggestedAction::Keep {
                RiskLevel::Low
            } else {
                RiskLevel::Medium
            };
            let reason = if cautious_duplicate {
                "该重复文件属于源码、配置、文档或项目元数据，即使内容相同，也可能分别服务于不同项目，默认改为人工复核。".to_string()
            } else if auto_cleanup_duplicate && action == SuggestedAction::Keep {
                "这一份是重复安装包或压缩包中的保留副本。".to_string()
            } else if auto_cleanup_duplicate && is_in_transient_location(file) {
                "该文件位于下载、临时或缓存目录，且属于重复安装包或压缩包，适合在保留一份后清理其余副本。".to_string()
            } else if auto_cleanup_duplicate {
                "该文件属于重复安装包或压缩包，通常只需保留一份，其余副本可清理。".to_string()
            } else if action == SuggestedAction::Keep {
                "这一份更适合作为重复文件组中的保留副本，其余副本暂不自动清理。".to_string()
            } else {
                "该文件虽然与其他副本内容一致，但不属于可直接清理的安装包、压缩包或临时文件，建议人工复核。".to_string()
            };
            upsert_suggestion(
                &mut suggestions,
                FileSuggestion {
                    path: file.path.clone(),
                    action,
                    risk,
                    reason,
                },
                50,
            );
        }
    }

    let mut values: Vec<FileSuggestion> = suggestions
        .into_values()
        .map(|entry| entry.suggestion)
        .collect();
    values.sort_by(|left, right| left.path.cmp(&right.path));
    values
}

fn should_require_manual_review(group: &crate::models::DuplicateGroup) -> bool {
    group.files.iter().any(|file| {
        is_project_sensitive_duplicate(file)
            || is_document_like(file)
            || has_cross_workspace_footprint(group)
    })
}

fn should_auto_cleanup_duplicate(group: &crate::models::DuplicateGroup) -> bool {
    group.files.iter().all(is_archive_or_installer_file)
}

fn is_archive_or_installer_file(file: &crate::models::FileRecord) -> bool {
    looks_archive_or_installer(&file.path)
}

fn is_document_like(file: &crate::models::FileRecord) -> bool {
    matches!(
        file.extension.as_deref().map(|value| value.to_ascii_lowercase()),
        Some(ext)
            if matches!(
                ext.as_str(),
                "pdf" | "ppt" | "pptx" | "doc" | "docx" | "xls" | "xlsx" | "wps" | "md"
            )
    )
}

fn is_project_sensitive_duplicate(file: &crate::models::FileRecord) -> bool {
    let lower_name = file
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if file
        .path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(|value| value.to_ascii_lowercase())
        .any(|value| {
            matches!(
                value.as_str(),
                ".git"
                    | ".svn"
                    | ".hg"
                    | "node_modules"
                    | "target"
                    | "vendor"
                    | "build"
                    | "dist"
                    | ".next"
            )
        })
    {
        return true;
    }

    if matches!(
        lower_name.as_str(),
        "readme"
            | "readme.md"
            | "license"
            | "license.md"
            | "copying"
            | "dockerfile"
            | "makefile"
            | "cmakelists.txt"
            | ".gitignore"
            | ".gitattributes"
            | ".editorconfig"
            | ".env"
            | "agents.md"
    ) {
        return true;
    }

    matches!(
        file.extension.as_deref().map(|value| value.to_ascii_lowercase()),
        Some(ext)
            if matches!(
                ext.as_str(),
                "rs"
                    | "toml"
                    | "md"
                    | "txt"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "ini"
                    | "cfg"
                    | "conf"
                    | "env"
                    | "xml"
                    | "js"
                    | "jsx"
                    | "ts"
                    | "tsx"
                    | "vue"
                    | "java"
                    | "kt"
                    | "py"
                    | "go"
                    | "rb"
                    | "php"
                    | "c"
                    | "cc"
                    | "cpp"
                    | "h"
                    | "hpp"
                    | "cs"
                    | "swift"
                    | "scala"
                    | "sql"
                    | "html"
                    | "css"
                    | "scss"
                    | "less"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "ps1"
                    | "bat"
                    | "cmd"
                    | "sample"
            )
    )
}

fn is_in_transient_location(file: &crate::models::FileRecord) -> bool {
    file.path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(|value| value.to_ascii_lowercase())
        .any(|value| {
            matches!(
                value.as_str(),
                "downloads"
                    | "download"
                    | "temp"
                    | "tmp"
                    | "cache"
                    | "caches"
                    | "export"
                    | "exports"
                    | "exported"
            )
        })
}

fn has_cross_workspace_footprint(group: &crate::models::DuplicateGroup) -> bool {
    let mut roots = std::collections::HashSet::new();
    for file in &group.files {
        let components = file
            .path
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .take(3)
            .map(|value| value.to_ascii_lowercase())
            .collect::<Vec<_>>();
        if !components.is_empty() {
            roots.insert(components.join("/"));
        }
    }
    roots.len() > 1 && group.files.iter().any(is_project_sensitive_duplicate)
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

#[allow(dead_code)]
fn build_local_summary(
    analysis: &AnalysisResult,
    dedup: &DedupResult,
    suggestions: &[FileSuggestion],
) -> String {
    let delete_count = suggestions
        .iter()
        .filter(|item| item.action == SuggestedAction::Delete)
        .count();
    let move_count = suggestions
        .iter()
        .filter(|item| item.action == SuggestedAction::Move)
        .count();
    let review_count = suggestions
        .iter()
        .filter(|item| item.action == SuggestedAction::Review)
        .count();

    format!(
        "本次共扫描 {} 个文件，总大小 {} 字节。发现 {} 个空文件、{} 个空目录以及 {} 组重复文件。当前建议：删除 {} 项、移动 {} 项、人工复核 {} 项。",
        analysis.total_files,
        format_bytes_human(analysis.total_size),
        analysis.empty_files.len(),
        analysis.empty_dirs.len(),
        dedup.groups.len(),
        delete_count,
        move_count,
        review_count
    )
}

fn build_display_summary(
    analysis: &AnalysisResult,
    dedup: &DedupResult,
    suggestions: &[FileSuggestion],
) -> String {
    let delete_count = suggestions
        .iter()
        .filter(|item| item.action == SuggestedAction::Delete)
        .count();
    let move_count = suggestions
        .iter()
        .filter(|item| item.action == SuggestedAction::Move)
        .count();
    let review_count = suggestions
        .iter()
        .filter(|item| item.action == SuggestedAction::Review)
        .count();

    format!(
        "本次共扫描 {} 个文件，总大小 {}。发现 {} 个空文件、{} 个空目录以及 {} 组重复文件。当前建议：删除 {} 项、移动 {} 项、人工复核 {} 项。",
        analysis.total_files,
        format_bytes_human(analysis.total_size),
        analysis.empty_files.len(),
        analysis.empty_dirs.len(),
        dedup.groups.len(),
        delete_count,
        move_count,
        review_count
    )
}

fn build_local_file_insight(report: &ScanReport, file: &FileRecord) -> FileAiInsight {
    let baseline = baseline_suggestion_for_file(report, file);
    let duplicate_group = report
        .dedup
        .groups
        .iter()
        .find(|group| group.files.iter().any(|item| item.path == file.path));
    let mut details = Vec::new();

    details.push(format!(
        "这是一个{}文件，大小约 {}。",
        describe_file_kind(file),
        format_bytes_human(file.size)
    ));

    if report
        .analysis
        .temporary_files
        .iter()
        .any(|item| item.path == file.path)
    {
        details.push("它被本地规则识别为临时/缓存类文件。".to_string());
    }

    if report
        .analysis
        .archive_files
        .iter()
        .any(|item| item.path == file.path)
    {
        details.push("它也属于压缩包、镜像或安装包一类。".to_string());
    }

    if report
        .analysis
        .large_files
        .iter()
        .any(|item| item.path == file.path)
    {
        details.push("它属于本次扫描中的较大文件，删除前最好再确认用途。".to_string());
    }

    if let Some(group) = duplicate_group {
        details.push(format!(
            "它位于一组重复文件中，这组一共 {} 个副本。",
            group.files.len()
        ));
    }

    if details.len() == 1 {
        details.push("当前本地规则没有把它列为高优先级自动清理对象。".to_string());
    }

    FileAiInsight {
        path: file.path.clone(),
        target_kind: AiInsightTargetKind::File,
        source: "local_rules".to_string(),
        summary: details.join(""),
        suggested_action: baseline.action,
        risk: baseline.risk,
        reason: baseline.reason,
        remote_attempted: false,
        used_fallback: false,
        fallback_reason: None,
    }
}

fn build_local_directory_insight(
    report: &ScanReport,
    dir_path: &Path,
    files: &[&FileRecord],
    is_known_empty_dir: bool,
) -> FileAiInsight {
    let total_size: u64 = files.iter().map(|item| item.size).sum();
    let file_count = files.len();
    let duplicate_count = report
        .dedup
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .filter(|item| item.path.starts_with(dir_path))
        .count();
    let temporary_count = report
        .analysis
        .temporary_files
        .iter()
        .filter(|item| item.path.starts_with(dir_path))
        .count();
    let archive_count = report
        .analysis
        .archive_files
        .iter()
        .filter(|item| item.path.starts_with(dir_path))
        .count();
    let large_count = report
        .analysis
        .large_files
        .iter()
        .filter(|item| item.path.starts_with(dir_path))
        .count();

    let mut details = Vec::new();
    details.push(format!(
        "这是一个目录，共包含 {} 个文件，总大小约 {}。",
        file_count,
        format_bytes_human(total_size)
    ));
    if is_known_empty_dir {
        details.push("它在本次扫描中被识别为空目录。".to_string());
    }
    if temporary_count > 0 {
        details.push(format!("其中有 {} 个临时/缓存类文件。", temporary_count));
    }
    if archive_count > 0 {
        details.push(format!("其中有 {} 个压缩包或安装包。", archive_count));
    }
    if duplicate_count > 0 {
        details.push(format!("目录内涉及 {} 个重复文件副本。", duplicate_count));
    }
    if large_count > 0 {
        details.push(format!(
            "其中有 {} 个较大文件，删除前应先确认用途。",
            large_count
        ));
    }

    let (action, risk, reason) = baseline_suggestion_for_directory(
        files,
        is_known_empty_dir,
        temporary_count,
        archive_count,
        duplicate_count,
        large_count,
    );

    FileAiInsight {
        path: dir_path.to_path_buf(),
        target_kind: AiInsightTargetKind::Directory,
        source: "local_rules".to_string(),
        summary: details.join(""),
        suggested_action: action,
        risk,
        reason,
        remote_attempted: false,
        used_fallback: false,
        fallback_reason: None,
    }
}

fn baseline_suggestion_for_directory(
    files: &[&FileRecord],
    is_known_empty_dir: bool,
    temporary_count: usize,
    archive_count: usize,
    duplicate_count: usize,
    large_count: usize,
) -> (SuggestedAction, RiskLevel, String) {
    if is_known_empty_dir || files.is_empty() {
        return (
            SuggestedAction::Delete,
            RiskLevel::Low,
            "该目录为空目录，可优先删除。".to_string(),
        );
    }

    if temporary_count == files.len() && !files.is_empty() {
        return (
            SuggestedAction::Delete,
            RiskLevel::Low,
            "该目录几乎完全由临时/缓存文件组成，通常可以整体清理。".to_string(),
        );
    }

    if archive_count == files.len() && !files.is_empty() {
        return (
            SuggestedAction::Move,
            RiskLevel::Medium,
            "该目录主要由压缩包或安装包组成，更适合整体归档而不是直接删除。".to_string(),
        );
    }

    if duplicate_count > 0 && duplicate_count == files.len() {
        return (
            SuggestedAction::Review,
            RiskLevel::Medium,
            "该目录里大多是重复文件，但是否能整目录删除仍建议人工复核。".to_string(),
        );
    }

    if large_count > 0 {
        return (
            SuggestedAction::Review,
            RiskLevel::Medium,
            "该目录包含较大文件，删除前应确认是否仍在使用。".to_string(),
        );
    }

    (
        SuggestedAction::Review,
        RiskLevel::Medium,
        "该目录包含多类文件，建议先确认用途，再决定是否整体删除或只清理其中部分文件。".to_string(),
    )
}

fn baseline_suggestion_for_file(report: &ScanReport, file: &FileRecord) -> FileSuggestion {
    if let Some(item) = report
        .advisor
        .suggestions
        .iter()
        .find(|item| item.path == file.path)
    {
        return item.clone();
    }

    if file.is_empty {
        return FileSuggestion {
            path: file.path.clone(),
            action: SuggestedAction::Delete,
            risk: RiskLevel::Low,
            reason: "该文件为空文件，通常可以删除。".to_string(),
        };
    }

    if report
        .analysis
        .temporary_files
        .iter()
        .any(|item| item.path == file.path)
    {
        return FileSuggestion {
            path: file.path.clone(),
            action: SuggestedAction::Delete,
            risk: RiskLevel::Low,
            reason: "该文件被识别为临时/缓存类文件，通常可以清理。".to_string(),
        };
    }

    if let Some(group) = report
        .dedup
        .groups
        .iter()
        .find(|group| group.files.iter().any(|item| item.path == file.path))
    {
        if group.suggested_keep.as_ref() == Some(&file.path) {
            return FileSuggestion {
                path: file.path.clone(),
                action: SuggestedAction::Keep,
                risk: RiskLevel::Low,
                reason: "该文件是当前重复文件组中建议保留的副本。".to_string(),
            };
        }

        if should_auto_cleanup_duplicate(group) {
            return FileSuggestion {
                path: file.path.clone(),
                action: SuggestedAction::Delete,
                risk: if is_in_transient_location(file) {
                    RiskLevel::Low
                } else {
                    RiskLevel::Medium
                },
                reason: "该文件属于重复的压缩包或安装包，通常保留一份即可。".to_string(),
            };
        }

        return FileSuggestion {
            path: file.path.clone(),
            action: SuggestedAction::Review,
            risk: if should_require_manual_review(group) {
                RiskLevel::High
            } else {
                RiskLevel::Medium
            },
            reason: "该文件处于重复文件组中，但默认仍建议人工复核。".to_string(),
        };
    }

    if report
        .analysis
        .archive_files
        .iter()
        .any(|item| item.path == file.path)
    {
        return FileSuggestion {
            path: file.path.clone(),
            action: SuggestedAction::Move,
            risk: RiskLevel::Medium,
            reason: "该文件属于压缩包或安装包，更适合移动归档而不是直接删除。".to_string(),
        };
    }

    if report
        .analysis
        .large_files
        .iter()
        .any(|item| item.path == file.path)
    {
        return FileSuggestion {
            path: file.path.clone(),
            action: SuggestedAction::Review,
            risk: RiskLevel::Medium,
            reason: "该文件体积较大，但当前没有自动删除依据，建议人工复核。".to_string(),
        };
    }

    FileSuggestion {
        path: file.path.clone(),
        action: SuggestedAction::Review,
        risk: if is_document_like(file) || is_project_sensitive_duplicate(file) {
            RiskLevel::High
        } else {
            RiskLevel::Medium
        },
        reason: "当前本地规则没有给出自动清理结论，建议先了解用途再决定。".to_string(),
    }
}

fn describe_file_kind(file: &FileRecord) -> String {
    match file
        .extension
        .as_deref()
        .map(|value| value.to_ascii_lowercase())
    {
        Some(ext) if ext.is_empty() => "无扩展名".to_string(),
        Some(ext) if matches!(ext.as_str(), "zip" | "7z" | "rar" | "iso") => {
            "压缩包/镜像".to_string()
        }
        Some(ext) if matches!(ext.as_str(), "exe" | "msi" | "bat" | "cmd" | "ps1") => {
            "可执行/脚本".to_string()
        }
        Some(ext)
            if matches!(
                ext.as_str(),
                "pdf" | "doc" | "docx" | "ppt" | "pptx" | "xls" | "xlsx" | "txt" | "md"
            ) =>
        {
            "文档".to_string()
        }
        Some(ext)
            if matches!(
                ext.as_str(),
                "rs" | "toml"
                    | "json"
                    | "yaml"
                    | "yml"
                    | "ini"
                    | "cfg"
                    | "conf"
                    | "ts"
                    | "tsx"
                    | "js"
                    | "jsx"
                    | "vue"
                    | "py"
                    | "java"
                    | "go"
                    | "c"
                    | "cpp"
            ) =>
        {
            "代码/配置".to_string()
        }
        Some(ext) => format!(".{ext} 文件"),
        None => "普通文件".to_string(),
    }
}

async fn request_file_insight(
    report: &ScanReport,
    file: &FileRecord,
    local_insight: &FileAiInsight,
    config: &AdvisorConfig,
    api_key: &str,
) -> Result<FileAiInsight> {
    let duplicate_group = report
        .dedup
        .groups
        .iter()
        .find(|group| group.files.iter().any(|item| item.path == file.path));
    let duplicate_peers = duplicate_group
        .map(|group| {
            group
                .files
                .iter()
                .filter(|item| item.path != file.path)
                .take(5)
                .map(|item| item.path.clone())
                .collect::<Vec<PathBuf>>()
        })
        .unwrap_or_default();

    let payload = json!({
        "root": report.root,
        "file": file,
        "local_summary": local_insight.summary,
        "local_suggestion": {
            "action": local_insight.suggested_action,
            "risk": local_insight.risk,
            "reason": local_insight.reason,
        },
        "context": {
            "is_large_file": report.analysis.large_files.iter().any(|item| item.path == file.path),
            "is_temporary_file": report.analysis.temporary_files.iter().any(|item| item.path == file.path),
            "is_archive_file": report.analysis.archive_files.iter().any(|item| item.path == file.path),
            "duplicate_group_size": duplicate_group.map(|group| group.files.len()).unwrap_or(0),
            "duplicate_peers": duplicate_peers,
            "suggested_keep": duplicate_group.and_then(|group| group.suggested_keep.clone()),
        }
    });

    let content = send_chat_completion(
        config,
        api_key,
        "You are a disk cleanup assistant. Reply with JSON only. Explain what the target file likely is, whether it can be deleted, and keep the answer concise in Chinese. Respect safety constraints: project/source/config/document files should stay review-only unless the local suggestion is already delete or move.",
        &format!(
            "请分析这个单独文件，并只返回一个 JSON 对象，格式为 {{\"summary\":\"中文说明，回答它是什么、是否建议删除\",\"suggestedAction\":\"delete|move|review|keep\",\"risk\":\"low|medium|high\",\"reason\":\"中文理由\"}}。不要输出 Markdown。输入数据：{}",
            serde_json::to_string(&payload)?
        ),
    )
    .await?;

    let response = parse_ai_file_insight_response(&content)?;
    let baseline = FileSuggestion {
        path: file.path.clone(),
        action: local_insight.suggested_action,
        risk: local_insight.risk,
        reason: local_insight.reason.clone(),
    };
    let ai_item = FileSuggestion {
        path: file.path.clone(),
        action: response.suggested_action,
        risk: response.risk,
        reason: response.reason.clone(),
    };

    Ok(FileAiInsight {
        path: file.path.clone(),
        target_kind: AiInsightTargetKind::File,
        source: format!("remote:{}", config.model),
        summary: if response.summary.trim().is_empty() {
            local_insight.summary.clone()
        } else {
            response.summary.trim().to_string()
        },
        suggested_action: sanitize_ai_action(&baseline, &ai_item),
        risk: max_risk(local_insight.risk, response.risk),
        reason: if response.reason.trim().is_empty() {
            local_insight.reason.clone()
        } else {
            response.reason.trim().to_string()
        },
        remote_attempted: true,
        used_fallback: false,
        fallback_reason: None,
    })
}

async fn request_directory_insight(
    report: &ScanReport,
    dir_path: &Path,
    files: &[&FileRecord],
    is_known_empty_dir: bool,
    local_insight: &FileAiInsight,
    config: &AdvisorConfig,
    api_key: &str,
) -> Result<FileAiInsight> {
    let payload = build_directory_ai_payload(
        report,
        dir_path,
        files,
        is_known_empty_dir,
        config.max_items,
    );
    let content = send_chat_completion(
        config,
        api_key,
        "You are a disk cleanup assistant. Reply with JSON only. Analyze the target directory from metadata only. Do not assume file contents. Explain what this directory is likely used for, whether deleting the whole directory may break something, and whether only some files inside should be removed. Keep the answer concise in Chinese.",
        &format!(
            "请分析这个目录，并且只基于目录摘要、文件名、扩展名、大小和扫描规则输出结论，不要假装读过文件正文。只返回一个 JSON 对象，格式为 {{\"summary\":\"中文说明，回答这个目录大致用途、整目录删除可能影响\",\"suggestedAction\":\"delete|move|review|keep\",\"risk\":\"low|medium|high\",\"reason\":\"中文理由，说明是否建议整目录删除，还是只建议清理其中某些文件\"}}。输入数据：{}",
            serde_json::to_string(&payload)?
        ),
    )
    .await?;

    let response = parse_ai_file_insight_response(&content)?;
    let baseline = FileSuggestion {
        path: dir_path.to_path_buf(),
        action: local_insight.suggested_action,
        risk: local_insight.risk,
        reason: local_insight.reason.clone(),
    };
    let ai_item = FileSuggestion {
        path: dir_path.to_path_buf(),
        action: response.suggested_action,
        risk: response.risk,
        reason: response.reason.clone(),
    };

    Ok(FileAiInsight {
        path: dir_path.to_path_buf(),
        target_kind: AiInsightTargetKind::Directory,
        source: format!("remote:{}", config.model),
        summary: if response.summary.trim().is_empty() {
            local_insight.summary.clone()
        } else {
            response.summary.trim().to_string()
        },
        suggested_action: sanitize_ai_action(&baseline, &ai_item),
        risk: max_risk(local_insight.risk, response.risk),
        reason: if response.reason.trim().is_empty() {
            local_insight.reason.clone()
        } else {
            response.reason.trim().to_string()
        },
        remote_attempted: true,
        used_fallback: false,
        fallback_reason: None,
    })
}

fn build_directory_ai_payload(
    report: &ScanReport,
    dir_path: &Path,
    files: &[&FileRecord],
    is_known_empty_dir: bool,
    max_items: usize,
) -> serde_json::Value {
    let total_size: u64 = files.iter().map(|item| item.size).sum();
    let sample_limit = max_items.clamp(3, 12);
    let mut top_files = files
        .iter()
        .map(|item| {
            json!({
                "relative_path": path_relative_to(dir_path, &item.path),
                "size": item.size,
                "extension": item.extension,
            })
        })
        .collect::<Vec<_>>();
    top_files.sort_by(|left, right| right["size"].as_u64().cmp(&left["size"].as_u64()));
    top_files.truncate(sample_limit);

    let temporary_files = report
        .analysis
        .temporary_files
        .iter()
        .filter(|item| item.path.starts_with(dir_path))
        .take(sample_limit)
        .map(|item| path_relative_to(dir_path, &item.path))
        .collect::<Vec<_>>();
    let archive_files = report
        .analysis
        .archive_files
        .iter()
        .filter(|item| item.path.starts_with(dir_path))
        .take(sample_limit)
        .map(|item| path_relative_to(dir_path, &item.path))
        .collect::<Vec<_>>();
    let duplicate_files = report
        .dedup
        .groups
        .iter()
        .flat_map(|group| group.files.iter())
        .filter(|item| item.path.starts_with(dir_path))
        .take(sample_limit)
        .map(|item| path_relative_to(dir_path, &item.path))
        .collect::<Vec<_>>();

    json!({
        "root": report.root,
        "target_directory": dir_path,
        "is_empty_directory": is_known_empty_dir,
        "file_count": files.len(),
        "total_size": total_size,
        "top_level_entries": summarize_directory_children(dir_path, files, sample_limit),
        "top_extensions": summarize_directory_extensions(files, sample_limit),
        "largest_file_samples": top_files,
        "temporary_file_count": report.analysis.temporary_files.iter().filter(|item| item.path.starts_with(dir_path)).count(),
        "temporary_file_samples": temporary_files,
        "archive_file_count": report.analysis.archive_files.iter().filter(|item| item.path.starts_with(dir_path)).count(),
        "archive_file_samples": archive_files,
        "duplicate_file_count": report.dedup.groups.iter().flat_map(|group| group.files.iter()).filter(|item| item.path.starts_with(dir_path)).count(),
        "duplicate_file_samples": duplicate_files,
        "local_summary": local_directory_summary(report, dir_path, files, is_known_empty_dir),
        "local_suggestion": {
            "action": local_insight_action_for_directory(report, dir_path, files, is_known_empty_dir),
        }
    })
}

fn path_relative_to(base: &Path, path: &Path) -> String {
    path.strip_prefix(base)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| relative.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| path.to_string_lossy().replace('\\', "/"))
}

fn summarize_directory_extensions(files: &[&FileRecord], limit: usize) -> Vec<serde_json::Value> {
    let mut extension_counts = HashMap::<String, (usize, u64)>::new();

    for file in files {
        let key = file
            .extension
            .as_deref()
            .map(|value| value.to_ascii_lowercase())
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| "(none)".to_string());
        let entry = extension_counts.entry(key).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += file.size;
    }

    let mut rows = extension_counts.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .1
             .0
            .cmp(&left.1 .0)
            .then(right.1 .1.cmp(&left.1 .1))
            .then(left.0.cmp(&right.0))
    });
    rows.truncate(limit);

    rows.into_iter()
        .map(|(extension, (file_count, total_size))| {
            json!({
                "extension": extension,
                "file_count": file_count,
                "total_size": total_size,
            })
        })
        .collect()
}

fn summarize_directory_children(
    dir_path: &Path,
    files: &[&FileRecord],
    limit: usize,
) -> Vec<serde_json::Value> {
    let mut children = HashMap::<String, (bool, usize, u64)>::new();

    for file in files {
        let Ok(relative) = file.path.strip_prefix(dir_path) else {
            continue;
        };
        let mut parts = relative.components();
        let Some(first_part) = parts.next() else {
            continue;
        };
        let child_name = first_part.as_os_str().to_string_lossy().to_string();
        let is_direct_file = parts.next().is_none();
        let entry = children
            .entry(child_name)
            .or_insert((!is_direct_file, 0, 0));
        if !is_direct_file {
            entry.0 = true;
        }
        entry.1 += 1;
        entry.2 += file.size;
    }

    let mut rows = children.into_iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .1
             .0
            .cmp(&left.1 .0)
            .then(right.1 .2.cmp(&left.1 .2))
            .then(left.0.cmp(&right.0))
    });
    rows.truncate(limit);

    rows.into_iter()
        .map(|(name, (is_directory, file_count, total_size))| {
            json!({
                "name": name,
                "kind": if is_directory { "directory" } else { "file" },
                "file_count": file_count,
                "total_size": total_size,
            })
        })
        .collect()
}

fn local_directory_summary(
    report: &ScanReport,
    dir_path: &Path,
    files: &[&FileRecord],
    is_known_empty_dir: bool,
) -> String {
    build_local_directory_insight(report, dir_path, files, is_known_empty_dir).summary
}

fn local_insight_action_for_directory(
    report: &ScanReport,
    dir_path: &Path,
    files: &[&FileRecord],
    is_known_empty_dir: bool,
) -> String {
    let insight = build_local_directory_insight(report, dir_path, files, is_known_empty_dir);
    match insight.suggested_action {
        SuggestedAction::Delete => "delete",
        SuggestedAction::Move => "move",
        SuggestedAction::Review => "review",
        SuggestedAction::Keep => "keep",
    }
    .to_string()
}

pub async fn test_connection(config: &AdvisorConfig) -> Result<String> {
    let api_key = configured_api_key(config)?;
    let content = send_chat_completion(
        config,
        api_key,
        "You are a connectivity probe for a disk cleanup app. Reply in concise Chinese plain text.",
        "请只返回一句中文，说明 AI 连接测试成功，并带上当前模型名称。",
    )
    .await?;
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return Err(anyhow!("AI 返回了空响应。"));
    }
    Ok(trimmed.to_string())
}

pub async fn fetch_models(config: &AdvisorConfig) -> Result<Vec<String>> {
    let api_key = configured_api_key(config)?;
    let client = Client::new();
    let response = client
        .get(build_compatible_endpoint(&config.base_url, "models"))
        .bearer_auth(api_key)
        .send()
        .await?
        .error_for_status()?;

    let body: ModelsResponse = response.json().await?;
    let models = normalize_model_ids(body.data);

    if models.is_empty() {
        return Err(anyhow!("AI 服务没有返回可用模型列表"));
    }

    Ok(models)
}

async fn request_ai_review(
    scan: &ScanResult,
    analysis: &AnalysisResult,
    dedup: &DedupResult,
    suggestions: &[FileSuggestion],
    config: &AdvisorConfig,
    api_key: &str,
) -> Result<ResolvedAiOutput> {
    let review_candidates: Vec<&FileSuggestion> = suggestions
        .iter()
        .filter(|item| item.action != SuggestedAction::Keep)
        .take(config.max_items)
        .collect();
    let payload = json!({
        "root": scan.root,
        "total_files": analysis.total_files,
        "total_size": analysis.total_size,
        "temporary_files": analysis.temporary_files.len(),
        "archive_files": analysis.archive_files.len(),
        "empty_files": analysis.empty_files.len(),
        "empty_dirs": analysis.empty_dirs.len(),
        "duplicate_groups": dedup.groups.len(),
        "large_files": analysis.large_files.iter().take(config.max_items).collect::<Vec<_>>(),
        "suggestions": review_candidates,
    });
    let content = send_chat_completion(
        config,
        api_key,
        "You are a disk cleanup assistant. You must reply with a JSON object only. Follow these safety rules strictly: project/source/config/document files must stay review-only; duplicate installers and archives may keep one copy and delete the rest; generic duplicates should be conservative; summary must be Chinese.",
        &format!(
            "请阅读下面的扫描结果 JSON，并只返回一个 JSON 对象，格式为 {{\"summary\":\"中文摘要\",\"suggestions\":[{{\"path\":\"绝对路径\",\"action\":\"delete|move|review\",\"risk\":\"low|medium|high\",\"reason\":\"中文理由\"}}]}}。不要输出 Markdown。不要为未出现在 suggestions 数组里的路径生成结果。对于源码、配置、文档、项目元数据，一律只能给 review。输入数据：{}",
            serde_json::to_string(&payload)?
        ),
    )
    .await?;
    let ai_response = parse_ai_review_response(&content)?;
    let summary = if ai_response.summary.trim().is_empty() {
        build_display_summary(analysis, dedup, suggestions)
    } else {
        ai_response.summary.trim().to_string()
    };

    Ok(ResolvedAiOutput {
        summary,
        suggestions: merge_ai_suggestions(suggestions, &ai_response.suggestions),
    })
}

async fn send_chat_completion(
    config: &AdvisorConfig,
    api_key: &str,
    system_prompt: &str,
    user_prompt: &str,
) -> Result<String> {
    let client = Client::new();
    let response = client
        .post(build_compatible_endpoint(
            &config.base_url,
            "chat/completions",
        ))
        .bearer_auth(api_key)
        .json(&json!({
            "model": config.model,
            "temperature": 0.2,
            "messages": [
                {
                    "role": "system",
                    "content": system_prompt
                },
                {
                    "role": "user",
                    "content": user_prompt
                }
            ]
        }))
        .send()
        .await?
        .error_for_status()?;

    let body: ChatCompletionResponse = response.json().await?;
    body.choices
        .into_iter()
        .next()
        .map(|item| item.message.content.trim().to_string())
        .filter(|content| !content.is_empty())
        .ok_or_else(|| anyhow!("AI 没有返回有效内容。"))
}

fn configured_api_key(config: &AdvisorConfig) -> Result<&str> {
    config
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("请先配置 AI API Key。"))
}

fn build_compatible_endpoint(base_url: &str, endpoint: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/{endpoint}")
    } else {
        format!("{trimmed}/v1/{endpoint}")
    }
}

fn normalize_model_ids(entries: Vec<ModelEntry>) -> Vec<String> {
    let mut values = BTreeSet::new();
    for entry in entries {
        if let Some(id) = entry.into_id() {
            let trimmed = id.trim();
            if !trimmed.is_empty() {
                values.insert(trimmed.to_string());
            }
        }
    }
    values.into_iter().collect()
}

fn parse_ai_review_response(content: &str) -> Result<AiReviewResponse> {
    let trimmed = content.trim();
    if let Ok(parsed) = serde_json::from_str::<AiReviewResponse>(trimmed) {
        return Ok(parsed);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_slice = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str::<AiReviewResponse>(json_slice) {
            return Ok(parsed);
        }
    }

    Err(anyhow!("AI 返回内容不是可解析的 JSON。"))
}

fn parse_ai_file_insight_response(content: &str) -> Result<AiFileInsightResponse> {
    let trimmed = content.trim();
    if let Ok(parsed) = serde_json::from_str::<AiFileInsightResponse>(trimmed) {
        return Ok(parsed);
    }

    if let (Some(start), Some(end)) = (trimmed.find('{'), trimmed.rfind('}')) {
        let json_slice = &trimmed[start..=end];
        if let Ok(parsed) = serde_json::from_str::<AiFileInsightResponse>(json_slice) {
            return Ok(parsed);
        }
    }

    Err(anyhow!("AI 返回内容不是可解析的 JSON。"))
}

fn merge_ai_suggestions(
    local_suggestions: &[FileSuggestion],
    ai_suggestions: &[FileSuggestion],
) -> Vec<FileSuggestion> {
    let ai_map: HashMap<_, _> = ai_suggestions
        .iter()
        .map(|item| (item.path.clone(), item))
        .collect();

    local_suggestions
        .iter()
        .map(|local| {
            let Some(ai_item) = ai_map.get(&local.path) else {
                return local.clone();
            };

            let action = sanitize_ai_action(local, ai_item);
            let risk = max_risk(local.risk, ai_item.risk);
            let reason = if ai_item.reason.trim().is_empty() {
                local.reason.clone()
            } else {
                ai_item.reason.clone()
            };

            FileSuggestion {
                path: local.path.clone(),
                action,
                risk,
                reason,
            }
        })
        .collect()
}

fn sanitize_ai_action(local: &FileSuggestion, ai_item: &FileSuggestion) -> SuggestedAction {
    if local.action == SuggestedAction::Keep {
        return SuggestedAction::Keep;
    }
    if local.action == SuggestedAction::Review && local.risk == RiskLevel::High {
        return SuggestedAction::Review;
    }

    match local.action {
        SuggestedAction::Delete => match ai_item.action {
            SuggestedAction::Delete | SuggestedAction::Move | SuggestedAction::Review => {
                ai_item.action
            }
            SuggestedAction::Keep => SuggestedAction::Delete,
        },
        SuggestedAction::Move => match ai_item.action {
            SuggestedAction::Delete | SuggestedAction::Move | SuggestedAction::Review => {
                ai_item.action
            }
            SuggestedAction::Keep => SuggestedAction::Move,
        },
        SuggestedAction::Review => match ai_item.action {
            SuggestedAction::Move | SuggestedAction::Review => ai_item.action,
            SuggestedAction::Delete | SuggestedAction::Keep => SuggestedAction::Review,
        },
        SuggestedAction::Keep => SuggestedAction::Keep,
    }
}

fn max_risk(left: RiskLevel, right: RiskLevel) -> RiskLevel {
    if risk_rank(left) >= risk_rank(right) {
        left
    } else {
        right
    }
}

fn risk_rank(value: RiskLevel) -> u8 {
    match value {
        RiskLevel::Low => 1,
        RiskLevel::Medium => 2,
        RiskLevel::High => 3,
    }
}

fn looks_archive_or_installer(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase()),
        Some(ext) if matches!(ext.as_str(), "zip" | "7z" | "rar" | "iso" | "exe" | "msi" | "dmg")
    )
}

#[derive(Debug, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

#[derive(Debug, Deserialize)]
struct ModelsResponse {
    #[serde(default, alias = "models")]
    data: Vec<ModelEntry>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum ModelEntry {
    WithId { id: String },
    WithName { name: String },
    Plain(String),
}

impl ModelEntry {
    fn into_id(self) -> Option<String> {
        match self {
            Self::WithId { id } => Some(id),
            Self::WithName { name } => Some(name),
            Self::Plain(value) => Some(value),
        }
    }
}

#[derive(Debug)]
struct ResolvedAiOutput {
    summary: String,
    suggestions: Vec<FileSuggestion>,
}

#[derive(Debug, Deserialize)]
struct AiReviewResponse {
    summary: String,
    #[serde(default)]
    suggestions: Vec<FileSuggestion>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AiFileInsightResponse {
    #[serde(default)]
    summary: String,
    suggested_action: SuggestedAction,
    risk: RiskLevel,
    #[serde(default)]
    reason: String,
}

#[derive(Debug)]
struct RankedSuggestion {
    priority: u8,
    suggestion: FileSuggestion,
}

fn upsert_suggestion(
    suggestions: &mut HashMap<std::path::PathBuf, RankedSuggestion>,
    suggestion: FileSuggestion,
    priority: u8,
) {
    let path = suggestion.path.clone();
    match suggestions.get(&path) {
        Some(existing) if existing.priority > priority => {}
        _ => {
            suggestions.insert(
                path,
                RankedSuggestion {
                    priority,
                    suggestion,
                },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_compatible_endpoint, build_display_summary, build_rule_based_suggestions,
        format_bytes_human, merge_ai_suggestions, normalize_model_ids, resolve_report_file_path,
        ModelEntry,
    };
    use crate::models::{
        AnalysisResult, DedupResult, DuplicateGroup, FileRecord, FileSuggestion, PathIssue,
        RiskLevel, SuggestedAction,
    };
    use std::path::{Path, PathBuf};

    fn file(path: &str, extension: Option<&str>) -> FileRecord {
        FileRecord {
            path: PathBuf::from(path),
            size: 10,
            extension: extension.map(str::to_string),
            modified_at: None,
            is_empty: false,
        }
    }

    fn suggestion(
        path: &str,
        action: SuggestedAction,
        risk: RiskLevel,
        reason: &str,
    ) -> FileSuggestion {
        FileSuggestion {
            path: PathBuf::from(path),
            action,
            risk,
            reason: reason.to_string(),
        }
    }

    fn empty_analysis() -> AnalysisResult {
        AnalysisResult {
            total_files: 0,
            total_size: 0,
            empty_files: Vec::new(),
            empty_dirs: Vec::new(),
            large_files: Vec::new(),
            temporary_files: Vec::new(),
            archive_files: Vec::new(),
            type_breakdown: Vec::new(),
        }
    }

    #[test]
    fn marks_project_duplicates_as_review_only() {
        let dedup = DedupResult {
            groups: vec![DuplicateGroup {
                hash: "same".to_string(),
                total_size: 20,
                files: vec![
                    file(r"E:\agent\数学建模工作流\AGENTS.md", Some("md")),
                    file(r"E:\agent\自动炒股\AGENTS.md", Some("md")),
                ],
                suggested_keep: Some(PathBuf::from(r"E:\agent\自动炒股\AGENTS.md")),
            }],
            failures: Vec::<PathIssue>::new(),
        };

        let suggestions = build_rule_based_suggestions(&empty_analysis(), &dedup);
        assert_eq!(suggestions.len(), 2);
        assert!(suggestions
            .iter()
            .all(|item| item.action == SuggestedAction::Review && item.risk == RiskLevel::High));
    }

    #[test]
    fn keeps_binary_duplicate_cleanup_behavior() {
        let dedup = DedupResult {
            groups: vec![DuplicateGroup {
                hash: "same".to_string(),
                total_size: 20,
                files: vec![
                    file(r"E:\archive\installer_old.zip", Some("zip")),
                    file(r"E:\archive\installer_new.zip", Some("zip")),
                ],
                suggested_keep: Some(PathBuf::from(r"E:\archive\installer_new.zip")),
            }],
            failures: Vec::<PathIssue>::new(),
        };

        let suggestions = build_rule_based_suggestions(&empty_analysis(), &dedup);
        assert!(suggestions.iter().any(|item| {
            item.path == PathBuf::from(r"E:\archive\installer_new.zip")
                && item.action == SuggestedAction::Keep
        }));
        assert!(suggestions.iter().any(|item| {
            item.path == PathBuf::from(r"E:\archive\installer_old.zip")
                && item.action == SuggestedAction::Delete
        }));
    }

    #[test]
    fn reviews_duplicate_documents_instead_of_deleting() {
        let dedup = DedupResult {
            groups: vec![DuplicateGroup {
                hash: "same".to_string(),
                total_size: 20,
                files: vec![
                    file(r"E:\Downloads\课程课件.pdf", Some("pdf")),
                    file(r"E:\Documents\课程课件.pdf", Some("pdf")),
                ],
                suggested_keep: Some(PathBuf::from(r"E:\Documents\课程课件.pdf")),
            }],
            failures: Vec::<PathIssue>::new(),
        };

        let suggestions = build_rule_based_suggestions(&empty_analysis(), &dedup);
        assert!(suggestions
            .iter()
            .all(|item| item.action == SuggestedAction::Review && item.risk == RiskLevel::High));
    }

    #[test]
    fn reviews_generic_duplicates_outside_safe_cleanup_categories() {
        let dedup = DedupResult {
            groups: vec![DuplicateGroup {
                hash: "same".to_string(),
                total_size: 20,
                files: vec![
                    file(r"E:\Videos\clip_a.mp4", Some("mp4")),
                    file(r"E:\Backup\clip_a.mp4", Some("mp4")),
                ],
                suggested_keep: Some(PathBuf::from(r"E:\Backup\clip_a.mp4")),
            }],
            failures: Vec::<PathIssue>::new(),
        };

        let suggestions = build_rule_based_suggestions(&empty_analysis(), &dedup);
        assert!(suggestions.iter().any(|item| {
            item.path == PathBuf::from(r"E:\Backup\clip_a.mp4")
                && item.action == SuggestedAction::Keep
        }));
        assert!(suggestions.iter().any(|item| {
            item.path == PathBuf::from(r"E:\Videos\clip_a.mp4")
                && item.action == SuggestedAction::Review
                && item.risk == RiskLevel::Medium
        }));
    }

    #[test]
    fn ai_cannot_override_high_risk_review_to_delete() {
        let merged = merge_ai_suggestions(
            &[suggestion(
                r"E:\project\README.md",
                SuggestedAction::Review,
                RiskLevel::High,
                "本地规则",
            )],
            &[suggestion(
                r"E:\project\README.md",
                SuggestedAction::Delete,
                RiskLevel::Low,
                "AI 想删",
            )],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].action, SuggestedAction::Review);
        assert_eq!(merged[0].risk, RiskLevel::High);
    }

    #[test]
    fn ai_can_downgrade_delete_to_review() {
        let merged = merge_ai_suggestions(
            &[suggestion(
                r"E:\Downloads\old_installer.zip",
                SuggestedAction::Delete,
                RiskLevel::Low,
                "本地规则",
            )],
            &[suggestion(
                r"E:\Downloads\old_installer.zip",
                SuggestedAction::Review,
                RiskLevel::Medium,
                "AI 建议再确认",
            )],
        );

        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].action, SuggestedAction::Review);
        assert_eq!(merged[0].risk, RiskLevel::Medium);
        assert_eq!(merged[0].reason, "AI 建议再确认");
    }

    #[test]
    fn builds_compatible_endpoint_for_root_base_url() {
        assert_eq!(
            build_compatible_endpoint("https://api.openai.com", "models"),
            "https://api.openai.com/v1/models"
        );
    }

    #[test]
    fn builds_compatible_endpoint_for_v1_base_url() {
        assert_eq!(
            build_compatible_endpoint("https://www.packyapi.com/v1/", "chat/completions"),
            "https://www.packyapi.com/v1/chat/completions"
        );
    }

    #[test]
    fn normalizes_model_ids_from_mixed_entries() {
        let models = normalize_model_ids(vec![
            ModelEntry::WithId {
                id: "gpt-4.1-mini".to_string(),
            },
            ModelEntry::WithName {
                name: "custom-model".to_string(),
            },
            ModelEntry::Plain("gpt-4.1-mini".to_string()),
            ModelEntry::Plain(" ".to_string()),
        ]);

        assert_eq!(
            models,
            vec!["custom-model".to_string(), "gpt-4.1-mini".to_string()]
        );
    }

    #[test]
    fn formats_bytes_with_human_readable_units() {
        assert_eq!(format_bytes_human(999), "999 B");
        assert_eq!(format_bytes_human(2048), "2.0 KB");
        assert_eq!(format_bytes_human(399145), "389.8 KB");
        assert_eq!(format_bytes_human(5 * 1024 * 1024), "5.0 MB");
    }

    #[test]
    fn local_summary_uses_human_readable_size() {
        let summary = build_display_summary(
            &AnalysisResult {
                total_files: 2,
                total_size: 399145,
                empty_files: Vec::new(),
                empty_dirs: Vec::new(),
                large_files: Vec::new(),
                temporary_files: Vec::new(),
                archive_files: Vec::new(),
                type_breakdown: Vec::new(),
            },
            &DedupResult {
                groups: Vec::new(),
                failures: Vec::new(),
            },
            &[
                suggestion(
                    r"E:\Downloads\foo.tmp",
                    SuggestedAction::Delete,
                    RiskLevel::Low,
                    "test",
                ),
                suggestion(
                    r"E:\Downloads\bar.tmp",
                    SuggestedAction::Delete,
                    RiskLevel::Low,
                    "test",
                ),
            ],
        );

        assert!(summary.contains("389.8 KB"));
    }

    #[test]
    fn resolves_relative_file_path_against_scan_root() {
        let report = crate::models::ScanReport {
            generated_at: chrono::Utc::now(),
            scan_duration_ms: 0,
            root: PathBuf::from(r"E:\ScanRoot"),
            scanned_files: Vec::new(),
            analysis: empty_analysis(),
            dedup: DedupResult {
                groups: Vec::new(),
                failures: Vec::new(),
            },
            modules: Vec::new(),
            advisor: crate::models::AdvisorOutput {
                source: "local_rules".to_string(),
                summary: String::new(),
                suggestions: Vec::new(),
            },
            failures: Vec::new(),
        };

        assert_eq!(
            resolve_report_file_path(&report, Path::new("App/lib/demo.dll")),
            PathBuf::from(r"E:\ScanRoot\App\lib\demo.dll")
        );
    }
}
