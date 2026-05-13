use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::{
    MigrationExecutionRecord, OperationLogEntry, RegistryBackup, RegistryRollbackRecord, ScanReport,
};
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
    /// Recorded registry backups across the session.
    pub registry_backups: Arc<Mutex<Vec<RegistryBackup>>>,
    /// Recorded registry rollback history across the session.
    pub registry_rollbacks: Arc<Mutex<Vec<RegistryRollbackRecord>>>,
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
        let registry_backup_path = config_dir.join("runtime").join("registry-backups.json");
        let registry_rollback_path = config_dir.join("runtime").join("registry-rollbacks.json");
        let migration_runs = fs::read_to_string(&migration_history_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<MigrationExecutionRecord>>(&text).ok())
            .unwrap_or_default();
        let registry_backups = fs::read_to_string(&registry_backup_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<RegistryBackup>>(&text).ok())
            .unwrap_or_default();
        let registry_rollbacks = fs::read_to_string(&registry_rollback_path)
            .ok()
            .and_then(|text| serde_json::from_str::<Vec<RegistryRollbackRecord>>(&text).ok())
            .unwrap_or_default();
        Self {
            last_report: Arc::new(Mutex::new(None)),
            scan_async_status: Arc::new(Mutex::new(ScanAsyncStatus::default())),
            scan_epoch: Arc::new(AtomicU64::new(0)),
            history: Arc::new(Mutex::new(Vec::new())),
            migration_runs: Arc::new(Mutex::new(migration_runs)),
            registry_backups: Arc::new(Mutex::new(registry_backups)),
            registry_rollbacks: Arc::new(Mutex::new(registry_rollbacks)),
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

    pub fn registry_backup_path(&self) -> PathBuf {
        self.runtime_dir().join("registry-backups.json")
    }

    pub fn registry_rollback_path(&self) -> PathBuf {
        self.runtime_dir().join("registry-rollbacks.json")
    }
}

fn dirs_config_path() -> PathBuf {
    if let Some(config) = dirs_next::config_dir() {
        config.join("smart-disk-cleaner")
    } else {
        PathBuf::from(".")
    }
}
