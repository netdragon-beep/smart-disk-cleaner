use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::{MigrationExecutionRecord, OperationLogEntry, ScanReport};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone, Default)]
pub struct ScanAsyncStatus {
    pub dedup_pending: bool,
    pub phase: String,
    pub message: String,
    pub error: Option<String>,
}

/// Shared application state managed by Tauri.
pub struct AppState {
    /// The most recent scan report (if any).
    pub last_report: Arc<Mutex<Option<ScanReport>>>,
    /// Async status for background scan work such as dedup continuation.
    pub scan_async_status: Arc<Mutex<ScanAsyncStatus>>,
    /// Monotonic id to prevent stale background scan tasks from overwriting newer results.
    pub scan_epoch: Arc<AtomicU64>,
    /// Accumulated operation history across the session.
    pub history: Arc<Mutex<Vec<OperationLogEntry>>>,
    /// Recorded migration runs across the session.
    pub migration_runs: Arc<Mutex<Vec<MigrationExecutionRecord>>>,
    /// Path to the persistent config file.
    pub config_path: PathBuf,
    /// Cancellation flag for the currently running scan.
    pub cancel_flag: Arc<AtomicBool>,
}

impl AppState {
    pub fn new() -> Self {
        let config_dir = dirs_config_path();
        let config_path = config_dir.join("config.toml");
        let migration_history_path = config_dir.join("runtime").join("migration-runs.json");
        let migration_runs = fs::read_to_string(&migration_history_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<MigrationExecutionRecord>>(&text).ok())
            .unwrap_or_default();
        Self {
            last_report: Arc::new(Mutex::new(None)),
            scan_async_status: Arc::new(Mutex::new(ScanAsyncStatus::default())),
            scan_epoch: Arc::new(AtomicU64::new(0)),
            history: Arc::new(Mutex::new(Vec::new())),
            migration_runs: Arc::new(Mutex::new(migration_runs)),
            config_path,
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn load_config(&self) -> AppConfig {
        AppConfig::load(&self.config_path).unwrap_or_default()
    }

    pub fn runtime_dir(&self) -> PathBuf {
        self.config_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("runtime")
    }

    pub fn migration_history_path(&self) -> PathBuf {
        self.runtime_dir().join("migration-runs.json")
    }
}

fn dirs_config_path() -> PathBuf {
    if let Some(config) = dirs_next::config_dir() {
        config.join("smart-disk-cleaner")
    } else {
        PathBuf::from(".")
    }
}
