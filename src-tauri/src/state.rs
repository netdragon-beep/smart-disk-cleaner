use smart_disk_cleaner_core::config::AppConfig;
use smart_disk_cleaner_core::models::{OperationLogEntry, ScanReport};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Shared application state managed by Tauri.
pub struct AppState {
    /// The most recent scan report (if any).
    pub last_report: Mutex<Option<ScanReport>>,
    /// Accumulated operation history across the session.
    pub history: Mutex<Vec<OperationLogEntry>>,
    /// Path to the persistent config file.
    pub config_path: PathBuf,
    /// Cancellation flag for the currently running scan.
    pub cancel_flag: Arc<AtomicBool>,
}

impl AppState {
    pub fn new() -> Self {
        let config_dir = dirs_config_path();
        Self {
            last_report: Mutex::new(None),
            history: Mutex::new(Vec::new()),
            config_path: config_dir.join("config.toml"),
            cancel_flag: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn load_config(&self) -> AppConfig {
        AppConfig::load(&self.config_path).unwrap_or_default()
    }
}

fn dirs_config_path() -> PathBuf {
    if let Some(config) = dirs_next::config_dir() {
        config.join("smart-disk-cleaner")
    } else {
        PathBuf::from(".")
    }
}
