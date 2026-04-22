use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationCategory {
    LargeFiles,
    DownloadArchives,
    WechatData,
    CondaPackageCache,
    CondaEnvironments,
    CondaInstallation,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationSupportLevel {
    OneClick,
    Guided,
    Manual,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationObjectKind {
    File,
    Directory,
    AppData,
    PackageCache,
    EnvironmentStore,
    InstallationRoot,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationDocSourceKind {
    LocalRule,
    OfficialDoc,
    LocalObservation,
    HistoricalCase,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationActionKind {
    StopProcess,
    MovePath,
    UpdateYamlList,
    SetEnvVar,
    CreateJunction,
    VerifyPathExists,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationCheckpointKind {
    FileContent,
    EnvVar,
    PathState,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MigrationRunStatus {
    DryRun,
    Succeeded,
    Failed,
    RolledBack,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationActionStep {
    pub title: String,
    pub detail: String,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationOpportunity {
    pub id: String,
    pub title: String,
    pub category: MigrationCategory,
    pub support_level: MigrationSupportLevel,
    pub risk: RiskLevel,
    pub estimated_size: u64,
    pub source_path: PathBuf,
    pub recommended_target_dir: PathBuf,
    pub recommended_target_path: PathBuf,
    pub reason: String,
    pub blocked_processes: Vec<String>,
    pub required_steps: Vec<MigrationActionStep>,
    pub one_click_paths: Vec<PathBuf>,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDocSource {
    pub title: String,
    pub kind: MigrationDocSourceKind,
    pub uri: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationDocExcerpt {
    pub title: String,
    pub kind: MigrationDocSourceKind,
    pub uri: Option<String>,
    pub excerpt: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationAction {
    pub id: String,
    pub kind: MigrationActionKind,
    pub title: String,
    pub detail: String,
    pub required: bool,
    pub enabled_by_default: bool,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationCheckpoint {
    pub key: String,
    pub kind: MigrationCheckpointKind,
    pub target: String,
    pub snapshot: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationRollbackAction {
    pub id: String,
    pub kind: MigrationActionKind,
    pub title: String,
    pub detail: String,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlan {
    pub id: String,
    pub title: String,
    pub category: MigrationCategory,
    pub object_kind: MigrationObjectKind,
    pub support_level: MigrationSupportLevel,
    pub risk: RiskLevel,
    pub estimated_size: u64,
    pub source_path: PathBuf,
    pub recommended_target_dir: PathBuf,
    pub recommended_target_path: PathBuf,
    pub summary: String,
    pub rationale: String,
    pub tags: Vec<String>,
    pub doc_sources: Vec<MigrationDocSource>,
    pub actions: Vec<MigrationAction>,
    pub verification_steps: Vec<MigrationActionStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationHistoricalCase {
    pub run_id: String,
    pub title: String,
    pub status: MigrationRunStatus,
    pub dry_run: bool,
    pub action_titles: Vec<String>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationAdvisorOutput {
    pub source: String,
    pub summary: String,
    pub opportunities: Vec<MigrationOpportunity>,
    pub plans: Vec<MigrationPlan>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationExecutionRecord {
    pub run_id: String,
    pub plan_id: String,
    pub title: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: DateTime<Utc>,
    pub dry_run: bool,
    pub status: MigrationRunStatus,
    pub checkpoints: Vec<MigrationCheckpoint>,
    pub rollback_actions: Vec<MigrationRollbackAction>,
    pub logs: Vec<OperationLogEntry>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationPlanReview {
    pub plan: MigrationPlan,
    pub source: String,
    pub remote_attempted: bool,
    pub used_fallback: bool,
    pub fallback_reason: Option<String>,
    pub doc_excerpts: Vec<MigrationDocExcerpt>,
    pub historical_cases: Vec<MigrationHistoricalCase>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProcessSuggestedAction {
    SafeToEnd,
    EndAfterSave,
    Review,
    AvoidEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessRecord {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub exe_path: Option<PathBuf>,
    pub command: Vec<String>,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub virtual_memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_written_bytes: u64,
    pub run_time_seconds: u64,
    pub status: String,
    pub category: String,
    pub is_critical: bool,
    pub resource_score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAiInsight {
    pub pid: u32,
    pub name: String,
    pub source: String,
    pub summary: String,
    pub suggested_action: ProcessSuggestedAction,
    pub risk: RiskLevel,
    pub reason: String,
    pub remote_attempted: bool,
    pub used_fallback: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAiFollowUpAnswer {
    pub pid: u32,
    pub name: String,
    pub question: String,
    pub answer: String,
    pub source: String,
    pub remote_attempted: bool,
    pub used_fallback: bool,
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessAiFollowUpTurn {
    pub question: String,
    pub answer: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanReport {
    pub generated_at: DateTime<Utc>,
    #[serde(default)]
    pub scan_duration_ms: u64,
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
