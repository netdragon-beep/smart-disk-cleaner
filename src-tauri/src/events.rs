use serde::Serialize;
use smart_disk_cleaner_core::dedup::DedupProgress;
use smart_disk_cleaner_core::scanner::ScanProgress;

/// Unified progress event sent to the frontend via Tauri Channel.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(tag = "kind")]
pub enum ProgressEvent {
    /// Directory walk phase
    #[serde(rename_all = "camelCase")]
    Scan {
        phase: String,
        files_found: usize,
        dirs_visited: usize,
        bytes_found: u64,
        current_path: Option<String>,
    },
    /// Duplicate hashing phase
    #[serde(rename_all = "camelCase")]
    Dedup {
        phase: String,
        files_hashed: usize,
        files_total: usize,
        current_path: Option<String>,
    },
    /// Analysis / advisor phase
    #[serde(rename_all = "camelCase")]
    Analyze { phase: String, detail: String },
}

impl From<&ScanProgress> for ProgressEvent {
    fn from(p: &ScanProgress) -> Self {
        ProgressEvent::Scan {
            phase: p.phase.clone(),
            files_found: p.files_found,
            dirs_visited: p.dirs_visited,
            bytes_found: p.bytes_found,
            current_path: p
                .current_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        }
    }
}

impl From<&DedupProgress> for ProgressEvent {
    fn from(p: &DedupProgress) -> Self {
        ProgressEvent::Dedup {
            phase: p.phase.clone(),
            files_hashed: p.files_hashed,
            files_total: p.files_total,
            current_path: p
                .current_path
                .as_ref()
                .map(|p| p.to_string_lossy().to_string()),
        }
    }
}
