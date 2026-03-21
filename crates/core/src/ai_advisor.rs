use crate::models::{
    AdvisorOutput, AnalysisResult, DedupResult, FileSuggestion, RiskLevel, ScanResult,
    SuggestedAction,
};
use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use serde_json::json;
use std::collections::{BTreeSet, HashMap};
use std::path::Path;
use tracing::warn;

#[derive(Debug, Clone)]
pub struct AdvisorConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub model: String,
    pub max_items: usize,
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
        summary: build_local_summary(analysis, dedup, &suggestions),
        suggestions,
    })
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
        analysis.total_size,
        analysis.empty_files.len(),
        analysis.empty_dirs.len(),
        dedup.groups.len(),
        delete_count,
        move_count,
        review_count
    )
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
        build_local_summary(analysis, dedup, suggestions)
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
        build_compatible_endpoint, build_rule_based_suggestions, merge_ai_suggestions,
        normalize_model_ids, ModelEntry,
    };
    use crate::models::{
        AnalysisResult, DedupResult, DuplicateGroup, FileRecord, FileSuggestion, PathIssue,
        RiskLevel, SuggestedAction,
    };
    use std::path::PathBuf;

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
}
