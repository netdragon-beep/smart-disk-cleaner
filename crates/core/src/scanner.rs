use crate::models::{FileRecord, PathIssue, ScanResult};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

/// Progress information emitted during directory scanning.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanProgress {
    /// Current phase: "walking", "done"
    pub phase: String,
    /// Number of files discovered so far
    pub files_found: usize,
    /// Number of directories visited so far
    pub dirs_visited: usize,
    /// Cumulative byte size found so far
    pub bytes_found: u64,
    /// Current path being processed (if available)
    pub current_path: Option<PathBuf>,
}

pub fn scan_directory(root: &Path) -> Result<ScanResult> {
    scan_directory_with_progress(root, |_| {}, Arc::new(AtomicBool::new(false)))
}

/// Scan a directory, calling `on_progress` periodically with progress updates.
/// Set `cancel` to true to abort the scan early.
pub fn scan_directory_with_progress<F>(
    root: &Path,
    on_progress: F,
    cancel: Arc<AtomicBool>,
) -> Result<ScanResult>
where
    F: Fn(&ScanProgress),
{
    if !root.exists() {
        bail!("scan path does not exist: {}", root.display());
    }
    if !root.is_dir() {
        bail!("scan path is not a directory: {}", root.display());
    }

    let mut files = Vec::new();
    let mut empty_dirs = Vec::new();
    let mut failures = Vec::new();
    let mut dirs_visited = 0_usize;
    let mut bytes_found = 0_u64;
    let mut counter = 0_u64;

    for entry in WalkDir::new(root).follow_links(false) {
        if cancel.load(Ordering::Relaxed) {
            bail!("scan cancelled by user");
        }

        match entry {
            Ok(entry) => {
                let path = entry.path();
                match entry.metadata() {
                    Ok(metadata) if metadata.is_file() => {
                        bytes_found += metadata.len();
                        files.push(FileRecord {
                            path: path.to_path_buf(),
                            size: metadata.len(),
                            extension: path
                                .extension()
                                .and_then(|value| value.to_str())
                                .map(|value| value.to_ascii_lowercase()),
                            modified_at: metadata
                                .modified()
                                .ok()
                                .map(DateTime::<Utc>::from),
                            is_empty: metadata.len() == 0,
                        });
                    }
                    Ok(metadata) if metadata.is_dir() => {
                        dirs_visited += 1;
                        match fs::read_dir(path) {
                            Ok(mut entries) => {
                                if entries.next().is_none() {
                                    empty_dirs.push(path.to_path_buf());
                                }
                            }
                            Err(err) => failures.push(PathIssue {
                                path: path.to_path_buf(),
                                message: format!("failed to read directory: {err}"),
                            }),
                        }
                    }
                    Ok(_) => {}
                    Err(err) => failures.push(PathIssue {
                        path: path.to_path_buf(),
                        message: format!("failed to read metadata: {err}"),
                    }),
                }

                counter += 1;
                if counter % 100 == 0 {
                    on_progress(&ScanProgress {
                        phase: "walking".to_string(),
                        files_found: files.len(),
                        dirs_visited,
                        bytes_found,
                        current_path: Some(path.to_path_buf()),
                    });
                }
            }
            Err(err) => failures.push(PathIssue {
                path: err.path().unwrap_or(root).to_path_buf(),
                message: err.to_string(),
            }),
        }
    }

    on_progress(&ScanProgress {
        phase: "done".to_string(),
        files_found: files.len(),
        dirs_visited,
        bytes_found,
        current_path: None,
    });

    Ok(ScanResult {
        root: root.to_path_buf(),
        files,
        empty_dirs,
        failures,
    })
}
