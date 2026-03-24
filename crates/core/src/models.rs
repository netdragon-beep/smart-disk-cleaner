use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRecord {
    pub path: PathBuf,
    pub size: u64,
    pub extension: Option<String>,
    pub modified_at: Option<DateTime<Utc>>,
    pub is_empty: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathIssue {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub root: PathBuf,
    pub files: Vec<FileRecord>,
    pub empty_dirs: Vec<PathBuf>,
    pub failures: Vec<PathIssue>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TypeStat {
    pub extension: String,
    pub file_count: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalysisResult {
    pub total_files: usize,
    pub total_size: u64,
    pub empty_files: Vec<FileRecord>,
    pub empty_dirs: Vec<PathBuf>,
    pub large_files: Vec<FileRecord>,
    pub temporary_files: Vec<FileRecord>,
    pub archive_files: Vec<FileRecord>,
    pub type_breakdown: Vec<TypeStat>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanModuleKind {
    DuplicateFiles,
    EmptyFiles,
    EmptyDirectories,
    LargeFiles,
    TemporaryFiles,
    ArchiveFiles,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanModuleSummary {
    pub kind: ScanModuleKind,
    pub item_count: usize,
    pub total_size: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicateGroup {
    pub hash: String,
    pub total_size: u64,
    pub files: Vec<FileRecord>,
    pub suggested_keep: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupResult {
    pub groups: Vec<DuplicateGroup>,
    pub failures: Vec<PathIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SuggestedAction {
    Delete,
    Keep,
    Move,
    Review,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AiInsightTargetKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSuggestion {
    pub path: PathBuf,
    pub action: SuggestedAction,
    pub risk: RiskLevel,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdvisorOutput {
    pub source: String,
    pub summary: String,
    pub suggestions: Vec<FileSuggestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileAiInsight {
    pub path: PathBuf,
    pub target_kind: AiInsightTargetKind,
    pub source: String,
    pub summary: String,
    pub suggested_action: SuggestedAction,
    pub risk: RiskLevel,
    pub reason: String,
    pub remote_attempted: bool,
    pub used_fallback: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub generated_at: DateTime<Utc>,
    pub root: PathBuf,
    pub scanned_files: Vec<FileRecord>,
    pub analysis: AnalysisResult,
    pub dedup: DedupResult,
    pub modules: Vec<ScanModuleSummary>,
    pub advisor: AdvisorOutput,
    pub failures: Vec<PathIssue>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionMode {
    Recycle,
    Move,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticSeverity {
    Info,
    Warning,
    Critical,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticCode {
    Ok,
    NotFound,
    PermissionDenied,
    InUseByAnotherProcess,
    LockedRegion,
    ReadOnly,
    AlreadyExists,
    InvalidInput,
    DirectoryNotEmpty,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathDiagnosis {
    pub path: PathBuf,
    pub operation: String,
    pub code: DiagnosticCode,
    pub severity: DiagnosticSeverity,
    pub summary: String,
    pub details: Vec<String>,
    pub suggestions: Vec<String>,
    pub possible_related_apps: Vec<String>,
    pub error_kind: Option<String>,
    pub raw_os_error: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationLogEntry {
    pub at: DateTime<Utc>,
    pub path: PathBuf,
    pub mode: ExecutionMode,
    pub dry_run: bool,
    pub success: bool,
    pub detail: String,
    pub diagnosis: Option<PathDiagnosis>,
}
