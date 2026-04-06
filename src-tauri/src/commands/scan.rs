use crate::events::ProgressEvent;
use crate::state::AppState;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use smart_disk_cleaner_core::ai_advisor::build_local_advice;
use smart_disk_cleaner_core::analyzer::{analyze, build_scan_modules, AnalyzerOptions};
use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::dedup::{find_duplicates_with_progress_and_cache, HashCache};
use smart_disk_cleaner_core::diagnostics::{probe_path, DiagnosticOperation};
use smart_disk_cleaner_core::models::{
    AnalysisResult, DuplicateGroup, FileRecord, FileSuggestion, PathDiagnosis, PathIssue,
    ScanModuleSummary, ScanReport,
};
use smart_disk_cleaner_core::scanner::{scan_directory_with_progress_and_options, ScanOptions};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::Ordering;
use std::time::Instant;
use tauri::{ipc::Channel, State};

const FRONTEND_LARGE_FILES_LIMIT: usize = 50;
const FRONTEND_TEMP_FILES_LIMIT: usize = 50;
const FRONTEND_ARCHIVE_FILES_LIMIT: usize = 50;
const FRONTEND_DUPLICATE_GROUPS_LIMIT: usize = 10;
const FRONTEND_DUPLICATE_FILES_PER_GROUP_LIMIT: usize = 20;
const FRONTEND_SUGGESTIONS_LIMIT: usize = 1000;
const DIRECTORY_OVERVIEW_LIMIT: usize = 200;
const FILE_TREE_MATCH_LIMIT: usize = 3000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
struct UsnJournalSnapshot {
    volume: String,
    journal_id: String,
    next_usn: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedScanReport {
    config_signature: String,
    usn_snapshot: UsnJournalSnapshot,
    report: ScanReport,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendAdvisorOutput {
    source: String,
    summary: String,
    suggestions: Vec<FileSuggestion>,
    suggestion_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendDedupResult {
    groups: Vec<DuplicateGroup>,
    failures: Vec<PathIssue>,
    group_count: usize,
    truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FrontendScanReport {
    generated_at: chrono::DateTime<chrono::Utc>,
    scan_duration_ms: u64,
    root: PathBuf,
    scanned_files: Vec<FileRecord>,
    analysis: AnalysisResult,
    dedup: FrontendDedupResult,
    modules: Vec<ScanModuleSummary>,
    advisor: FrontendAdvisorOutput,
    failures: Vec<PathIssue>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DirectoryOverviewItem {
    key: String,
    name: String,
    path: String,
    kind: String,
    file_count: usize,
    total_size: u64,
    preview: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNode {
    key: String,
    name: String,
    path: String,
    kind: String,
    size: u64,
    extension: String,
    file_count: usize,
    children: Option<Vec<FileTreeNode>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeQueryResult {
    matched_count: usize,
    node_count: usize,
    truncated: bool,
    rows: Vec<FileTreeNode>,
}

#[tauri::command]
pub async fn start_scan(
    path: String,
    on_progress: Channel<ProgressEvent>,
    state: State<'_, AppState>,
) -> Result<FrontendScanReport, String> {
    let root = PathBuf::from(&path);
    let started_at = Instant::now();
    let config = state.load_config();
    let cancel = state.cancel_flag.clone();
    cancel.store(false, Ordering::Relaxed);

    if let Some(cached_report) = try_restore_cached_report(&root, &config, &state) {
        let _ = on_progress.send(ProgressEvent::Analyze {
            phase: "cached".to_string(),
            detail: "检测到卷无变化，直接复用上次扫描结果。".to_string(),
        });
        *state.last_report.lock().await = Some(cached_report.clone());
        let _ = on_progress.send(ProgressEvent::Analyze {
            phase: "done".to_string(),
            detail: "扫描完成".to_string(),
        });
        return Ok(summarize_report_for_frontend(&cached_report));
    }

    let cancel_scan = cancel.clone();
    let on_progress_scan = on_progress.clone();
    let scan_root = root.clone();
    let exclude_patterns = config.exclude_patterns.clone();

    let scan = tokio::task::spawn_blocking(move || {
        scan_directory_with_progress_and_options(
            &scan_root,
            |progress| {
                let _ = on_progress_scan.send(ProgressEvent::from(progress));
            },
            cancel_scan,
            &ScanOptions { exclude_patterns },
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "analyzing".to_string(),
        detail: "正在分析文件类型和大小...".to_string(),
    });
    let analysis = analyze(
        &scan,
        AnalyzerOptions {
            large_file_threshold_bytes: config.large_file_threshold_mb * 1024 * 1024,
        },
    );

    let files = scan.files.clone();
    let cancel_dedup = cancel.clone();
    let on_progress_dedup = on_progress.clone();
    let hash_cache = load_hash_cache(&state, &root).unwrap_or_default();
    let dedup_root = root.clone();
    let (dedup, updated_hash_cache) = tokio::task::spawn_blocking(move || {
        find_duplicates_with_progress_and_cache(
            &files,
            &hash_cache,
            |progress| {
                let _ = on_progress_dedup.send(ProgressEvent::from(progress));
            },
            cancel_dedup,
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())?;

    save_hash_cache(&state, &dedup_root, &updated_hash_cache).map_err(|error| error.to_string())?;

    let advisor = build_local_advice(&analysis, &dedup);

    let report = ScanReport {
        generated_at: Utc::now(),
        scan_duration_ms: started_at.elapsed().as_millis() as u64,
        root: scan.root.clone(),
        scanned_files: scan.files.clone(),
        modules: build_scan_modules(&analysis, &dedup),
        analysis,
        dedup,
        advisor,
        failures: scan.failures,
    };

    store_cached_report(&root, &config, &state, &report).map_err(|error| error.to_string())?;
    *state.last_report.lock().await = Some(report.clone());

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "done".to_string(),
        detail: "扫描完成".to_string(),
    });

    Ok(summarize_report_for_frontend(&report))
}

#[tauri::command]
pub async fn get_directory_overview(
    state: State<'_, AppState>,
) -> Result<Vec<DirectoryOverviewItem>, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?;
    Ok(build_directory_overview(report, DIRECTORY_OVERVIEW_LIMIT))
}

#[tauri::command]
pub async fn query_file_tree(
    query: Option<String>,
    category: Option<String>,
    state: State<'_, AppState>,
) -> Result<FileTreeQueryResult, String> {
    let report_guard = state.last_report.lock().await;
    let report = report_guard
        .as_ref()
        .ok_or_else(|| "当前没有扫描报告，请先执行扫描。".to_string())?;
    Ok(build_file_tree_query_result(
        report,
        query.as_deref().unwrap_or_default(),
        category.as_deref().unwrap_or("all"),
        FILE_TREE_MATCH_LIMIT,
    ))
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn diagnose_path(path: String, operation: String) -> Result<PathDiagnosis, String> {
    let op = match operation.as_str() {
        "recycle" => DiagnosticOperation::Recycle,
        "move" => DiagnosticOperation::Move,
        _ => DiagnosticOperation::Probe,
    };
    Ok(probe_path(&PathBuf::from(path), op))
}

fn summarize_report_for_frontend(report: &ScanReport) -> FrontendScanReport {
    FrontendScanReport {
        generated_at: report.generated_at,
        scan_duration_ms: report.scan_duration_ms,
        root: report.root.clone(),
        scanned_files: Vec::new(),
        analysis: AnalysisResult {
            total_files: report.analysis.total_files,
            total_size: report.analysis.total_size,
            empty_files: report
                .analysis
                .empty_files
                .iter()
                .take(100)
                .cloned()
                .collect(),
            empty_dirs: report
                .analysis
                .empty_dirs
                .iter()
                .take(100)
                .cloned()
                .collect(),
            large_files: report
                .analysis
                .large_files
                .iter()
                .take(FRONTEND_LARGE_FILES_LIMIT)
                .cloned()
                .collect(),
            temporary_files: report
                .analysis
                .temporary_files
                .iter()
                .take(FRONTEND_TEMP_FILES_LIMIT)
                .cloned()
                .collect(),
            archive_files: report
                .analysis
                .archive_files
                .iter()
                .take(FRONTEND_ARCHIVE_FILES_LIMIT)
                .cloned()
                .collect(),
            type_breakdown: report.analysis.type_breakdown.clone(),
        },
        dedup: FrontendDedupResult {
            groups: report
                .dedup
                .groups
                .iter()
                .take(FRONTEND_DUPLICATE_GROUPS_LIMIT)
                .map(|group| {
                    let mut group = group.clone();
                    if group.files.len() > FRONTEND_DUPLICATE_FILES_PER_GROUP_LIMIT {
                        group
                            .files
                            .truncate(FRONTEND_DUPLICATE_FILES_PER_GROUP_LIMIT);
                    }
                    group
                })
                .collect(),
            failures: report.dedup.failures.iter().take(100).cloned().collect(),
            group_count: report.dedup.groups.len(),
            truncated: report.dedup.groups.len() > FRONTEND_DUPLICATE_GROUPS_LIMIT,
        },
        modules: report.modules.clone(),
        advisor: FrontendAdvisorOutput {
            source: report.advisor.source.clone(),
            summary: report.advisor.summary.clone(),
            suggestions: report
                .advisor
                .suggestions
                .iter()
                .take(FRONTEND_SUGGESTIONS_LIMIT)
                .cloned()
                .collect(),
            suggestion_count: report.advisor.suggestions.len(),
            truncated: report.advisor.suggestions.len() > FRONTEND_SUGGESTIONS_LIMIT,
        },
        failures: report.failures.iter().take(100).cloned().collect(),
    }
}

fn try_restore_cached_report(
    root: &Path,
    config: &AppConfig,
    state: &AppState,
) -> Option<ScanReport> {
    let current_snapshot = query_usn_journal(root)?;
    let cache_path = report_cache_path(state, root);
    let text = fs::read_to_string(cache_path).ok()?;
    let cached: CachedScanReport = serde_json::from_str(&text).ok()?;

    if cached.config_signature != config_signature(config) {
        return None;
    }

    if cached.usn_snapshot != current_snapshot {
        return None;
    }

    Some(cached.report)
}

fn store_cached_report(
    root: &Path,
    config: &AppConfig,
    state: &AppState,
    report: &ScanReport,
) -> anyhow::Result<()> {
    let Some(snapshot) = query_usn_journal(root) else {
        return Ok(());
    };
    let cache_path = report_cache_path(state, root);
    if let Some(parent) = cache_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let payload = CachedScanReport {
        config_signature: config_signature(config),
        usn_snapshot: snapshot,
        report: report.clone(),
    };
    fs::write(cache_path, serde_json::to_vec(&payload)?)?;
    Ok(())
}

fn load_hash_cache(state: &AppState, root: &Path) -> anyhow::Result<HashCache> {
    let path = hash_cache_path(state, root);
    if !path.exists() {
        return Ok(HashCache::default());
    }
    let text = fs::read_to_string(path)?;
    Ok(serde_json::from_str(&text)?)
}

fn save_hash_cache(state: &AppState, root: &Path, cache: &HashCache) -> anyhow::Result<()> {
    let path = hash_cache_path(state, root);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_vec(cache)?)?;
    Ok(())
}

fn report_cache_path(state: &AppState, root: &Path) -> PathBuf {
    cache_dir(state).join(format!("scan-report-{}.json", root_cache_key(root)))
}

fn hash_cache_path(state: &AppState, root: &Path) -> PathBuf {
    cache_dir(state).join(format!("hash-cache-{}.json", root_cache_key(root)))
}

fn cache_dir(state: &AppState) -> PathBuf {
    state
        .config_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("cache")
}

fn root_cache_key(root: &Path) -> String {
    blake3::hash(root.to_string_lossy().as_bytes())
        .to_hex()
        .to_string()
}

fn config_signature(config: &AppConfig) -> String {
    serde_json::to_string(config).unwrap_or_default()
}

fn build_directory_overview(report: &ScanReport, limit: usize) -> Vec<DirectoryOverviewItem> {
    let mut buckets: HashMap<String, DirectoryOverviewBucket> = HashMap::new();

    for file in &report.scanned_files {
        let parts = relative_parts(&file.path, &report.root);
        if parts.is_empty() {
            continue;
        }

        if parts.len() == 1 {
            buckets.insert(
                format!("file:{}", parts[0]),
                DirectoryOverviewBucket {
                    key: format!("file:{}", parts[0]),
                    name: parts[0].clone(),
                    path: file.path.to_string_lossy().to_string(),
                    kind: "file".to_string(),
                    file_count: 1,
                    total_size: file.size,
                    preview: BTreeSet::new(),
                },
            );
            continue;
        }

        let key = format!("dir:{}", parts[0]);
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| DirectoryOverviewBucket {
                key: key.clone(),
                name: parts[0].clone(),
                path: report.root.join(&parts[0]).to_string_lossy().to_string(),
                kind: "directory".to_string(),
                file_count: 0,
                total_size: 0,
                preview: BTreeSet::new(),
            });
        bucket.file_count += 1;
        bucket.total_size += file.size;
        if let Some(next) = parts.get(1) {
            bucket.preview.insert(next.clone());
        }
    }

    for dir_path in &report.analysis.empty_dirs {
        let parts = relative_parts(dir_path, &report.root);
        if parts.is_empty() {
            continue;
        }

        let key = format!("dir:{}", parts[0]);
        let bucket = buckets
            .entry(key.clone())
            .or_insert_with(|| DirectoryOverviewBucket {
                key: key.clone(),
                name: parts[0].clone(),
                path: report.root.join(&parts[0]).to_string_lossy().to_string(),
                kind: "directory".to_string(),
                file_count: 0,
                total_size: 0,
                preview: BTreeSet::new(),
            });
        if let Some(next) = parts.get(1) {
            bucket.preview.insert(next.clone());
        }
    }

    let mut rows = buckets
        .into_values()
        .map(|item| DirectoryOverviewItem {
            key: item.key,
            name: item.name,
            path: item.path,
            kind: item.kind.clone(),
            file_count: item.file_count,
            total_size: item.total_size,
            preview: if item.kind == "file" {
                "-".to_string()
            } else if item.preview.is_empty() {
                "空目录".to_string()
            } else {
                let values: Vec<String> = item.preview.iter().take(4).cloned().collect();
                let mut preview = values.join("、");
                if item.preview.len() > 4 {
                    preview.push_str(" 等");
                }
                preview
            },
        })
        .collect::<Vec<_>>();

    rows.sort_by(|left, right| {
        if left.kind != right.kind {
            return if left.kind == "directory" {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Greater
            };
        }
        if left.total_size != right.total_size {
            return right.total_size.cmp(&left.total_size);
        }
        left.name.cmp(&right.name)
    });
    rows.truncate(limit);
    rows
}

fn build_file_tree_query_result(
    report: &ScanReport,
    query: &str,
    category: &str,
    limit: usize,
) -> FileTreeQueryResult {
    let query = query.trim().to_ascii_lowercase();
    let mut matched = report
        .scanned_files
        .iter()
        .filter(|file| {
            let ext = file
                .extension
                .as_deref()
                .unwrap_or_default()
                .to_ascii_lowercase();
            let full_path = file.path.to_string_lossy().to_ascii_lowercase();
            let query_matched =
                query.is_empty() || full_path.contains(&query) || ext.contains(&query);
            query_matched && matches_file_category(file, category)
        })
        .cloned()
        .collect::<Vec<_>>();

    matched.sort_by(|left, right| left.path.cmp(&right.path));
    let matched_count = matched.len();
    let truncated = matched_count > limit;
    if truncated {
        matched.truncate(limit);
    }

    let rows = build_file_tree_rows(&report.root, &matched);
    let node_count = count_nodes(&rows);

    FileTreeQueryResult {
        matched_count,
        node_count,
        truncated,
        rows,
    }
}

fn build_file_tree_rows(root: &Path, files: &[FileRecord]) -> Vec<FileTreeNode> {
    #[derive(Clone)]
    struct NodeEntry {
        row: FileTreeNode,
        children: Vec<String>,
    }

    let mut node_map: HashMap<String, NodeEntry> = HashMap::new();
    let mut roots = Vec::new();

    for file in files {
        let parts = relative_parts(&file.path, root);
        if parts.is_empty() {
            continue;
        }

        if parts.len() == 1 {
            let key = format!("file:{}", parts[0]);
            node_map.insert(
                key.clone(),
                NodeEntry {
                    row: FileTreeNode {
                        key: key.clone(),
                        name: parts[0].clone(),
                        path: file.path.to_string_lossy().to_string(),
                        kind: "file".to_string(),
                        size: file.size,
                        extension: file.extension.clone().unwrap_or_default(),
                        file_count: 1,
                        children: None,
                    },
                    children: Vec::new(),
                },
            );
            roots.push(key);
            continue;
        }

        for depth in 0..parts.len() - 1 {
            let key = format!("dir:{}", parts[..=depth].join("/"));
            if !node_map.contains_key(&key) {
                node_map.insert(
                    key.clone(),
                    NodeEntry {
                        row: FileTreeNode {
                            key: key.clone(),
                            name: parts[depth].clone(),
                            path: root
                                .join(parts[..=depth].join("/"))
                                .to_string_lossy()
                                .to_string(),
                            kind: "directory".to_string(),
                            size: 0,
                            extension: String::new(),
                            file_count: 0,
                            children: None,
                        },
                        children: Vec::new(),
                    },
                );
                if depth == 0 {
                    roots.push(key.clone());
                } else {
                    let parent_key = format!("dir:{}", parts[..depth].join("/"));
                    if let Some(parent) = node_map.get_mut(&parent_key) {
                        if !parent.children.contains(&key) {
                            parent.children.push(key.clone());
                        }
                    }
                }
            }
            if let Some(node) = node_map.get_mut(&key) {
                node.row.size += file.size;
                node.row.file_count += 1;
            }
        }

        let file_key = format!("file:{}", parts.join("/"));
        node_map.insert(
            file_key.clone(),
            NodeEntry {
                row: FileTreeNode {
                    key: file_key.clone(),
                    name: parts.last().cloned().unwrap_or_default(),
                    path: file.path.to_string_lossy().to_string(),
                    kind: "file".to_string(),
                    size: file.size,
                    extension: file.extension.clone().unwrap_or_default(),
                    file_count: 1,
                    children: None,
                },
                children: Vec::new(),
            },
        );
        let parent_key = format!("dir:{}", parts[..parts.len() - 1].join("/"));
        if let Some(parent) = node_map.get_mut(&parent_key) {
            if !parent.children.contains(&file_key) {
                parent.children.push(file_key);
            }
        }
    }

    fn build_rows(keys: &[String], node_map: &HashMap<String, NodeEntry>) -> Vec<FileTreeNode> {
        let mut rows = keys
            .iter()
            .filter_map(|key| node_map.get(key))
            .cloned()
            .collect::<Vec<_>>();

        rows.sort_by(|left, right| {
            if left.row.kind != right.row.kind {
                return if left.row.kind == "directory" {
                    std::cmp::Ordering::Less
                } else {
                    std::cmp::Ordering::Greater
                };
            }
            if left.row.size != right.row.size {
                return right.row.size.cmp(&left.row.size);
            }
            left.row.name.cmp(&right.row.name)
        });

        rows.into_iter()
            .map(|entry| FileTreeNode {
                children: if entry.children.is_empty() {
                    None
                } else {
                    Some(build_rows(&entry.children, node_map))
                },
                ..entry.row
            })
            .collect()
    }

    let unique_roots = roots
        .into_iter()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    build_rows(&unique_roots, &node_map)
}

fn count_nodes(rows: &[FileTreeNode]) -> usize {
    rows.iter()
        .map(|row| 1 + row.children.as_deref().map(count_nodes).unwrap_or(0))
        .sum()
}

fn relative_parts(path: &Path, root: &Path) -> Vec<String> {
    path.strip_prefix(root)
        .ok()
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .map(|value| value.to_string())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn matches_file_category(file: &FileRecord, category: &str) -> bool {
    if category == "all" {
        return true;
    }

    let ext = file
        .extension
        .as_deref()
        .unwrap_or_default()
        .to_ascii_lowercase();

    if ext.is_empty() {
        return category == "other";
    }

    let image = matches!(
        ext.as_str(),
        "png"
            | "jpg"
            | "jpeg"
            | "gif"
            | "bmp"
            | "webp"
            | "svg"
            | "ico"
            | "tif"
            | "tiff"
            | "heic"
            | "heif"
            | "raw"
            | "psd"
            | "avif"
    );
    let video = matches!(
        ext.as_str(),
        "mp4"
            | "mkv"
            | "avi"
            | "mov"
            | "wmv"
            | "flv"
            | "webm"
            | "m4v"
            | "mpeg"
            | "mpg"
            | "ts"
            | "3gp"
            | "rmvb"
    );
    let audio = matches!(
        ext.as_str(),
        "mp3"
            | "wav"
            | "flac"
            | "aac"
            | "m4a"
            | "ogg"
            | "wma"
            | "opus"
            | "ape"
            | "amr"
            | "mid"
            | "midi"
    );
    let archive = matches!(
        ext.as_str(),
        "zip"
            | "zipx"
            | "7z"
            | "rar"
            | "tar"
            | "gz"
            | "tgz"
            | "bz2"
            | "xz"
            | "cab"
            | "iso"
            | "img"
            | "dmg"
            | "jar"
    );
    let executable = matches!(
        ext.as_str(),
        "exe"
            | "com"
            | "msi"
            | "msix"
            | "msixbundle"
            | "appx"
            | "appxbundle"
            | "bat"
            | "cmd"
            | "ps1"
            | "vbs"
            | "js"
            | "jar"
            | "scr"
    );
    let document = matches!(
        ext.as_str(),
        "pdf"
            | "doc"
            | "docx"
            | "ppt"
            | "pptx"
            | "xls"
            | "xlsx"
            | "csv"
            | "txt"
            | "md"
            | "rtf"
            | "wps"
            | "odt"
            | "ods"
            | "odp"
    );
    let code = matches!(
        ext.as_str(),
        "rs" | "toml"
            | "json"
            | "yaml"
            | "yml"
            | "xml"
            | "ini"
            | "cfg"
            | "conf"
            | "env"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "vue"
            | "py"
            | "java"
            | "kt"
            | "go"
            | "c"
            | "cc"
            | "cpp"
            | "h"
            | "hpp"
            | "cs"
            | "php"
            | "rb"
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
            | "lock"
    );

    match category {
        "image" => image,
        "video" => video,
        "audio" => audio,
        "archive" => archive,
        "executable" => executable,
        "document" => document,
        "code" => code,
        "other" => !(image || video || audio || archive || executable || document || code),
        _ => true,
    }
}

fn query_usn_journal(root: &Path) -> Option<UsnJournalSnapshot> {
    #[cfg(not(target_os = "windows"))]
    {
        let _ = root;
        None
    }

    #[cfg(target_os = "windows")]
    {
        let volume = windows_volume_arg(root)?;
        let output = Command::new("cmd")
            .args(["/C", "fsutil", "usn", "queryjournal", &volume])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut journal_id = None;
        let mut next_usn = None;

        for line in stdout.lines() {
            let trimmed = line.trim();
            if let Some((key, value)) = trimmed.split_once(':') {
                let key = key.trim();
                let value = value.trim().to_string();
                if key.contains("Journal ID") {
                    journal_id = Some(value);
                } else if key.contains("Next Usn") {
                    next_usn = Some(value);
                }
            }
        }

        Some(UsnJournalSnapshot {
            volume,
            journal_id: journal_id?,
            next_usn: next_usn?,
        })
    }
}

#[cfg(target_os = "windows")]
fn windows_volume_arg(root: &Path) -> Option<String> {
    let text = root.to_string_lossy();
    let bytes = text.as_bytes();
    if bytes.len() < 2 || bytes[1] != b':' {
        return None;
    }
    Some(text[0..2].to_ascii_uppercase())
}

#[derive(Debug, Clone)]
struct DirectoryOverviewBucket {
    key: String,
    name: String,
    path: String,
    kind: String,
    file_count: usize,
    total_size: u64,
    preview: BTreeSet<String>,
}
