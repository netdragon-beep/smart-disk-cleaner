use crate::models::{FileRecord, PathIssue, ScanResult};
use anyhow::{bail, Result};
use chrono::{DateTime, Utc};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use walkdir::WalkDir;

const BUILTIN_EXCLUDE_PATTERNS: &[&str] = &[
    "**/.git/**",
    "**/.svn/**",
    "**/.hg/**",
    "**/node_modules/**",
    "**/target/**",
    "**/.cargo-target/**",
    "**/.vscode/extensions/**",
    "**/AppData/Local/Temp/**",
    "**/AppData/Local/**/Cache/**",
    "**/AppData/Local/**/Caches/**",
];

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

#[derive(Debug, Clone, Default)]
pub struct ScanOptions {
    pub exclude_patterns: Vec<String>,
}

pub fn scan_directory(root: &Path) -> Result<ScanResult> {
    scan_directory_with_options(root, &ScanOptions::default())
}

pub fn scan_directory_with_options(root: &Path, options: &ScanOptions) -> Result<ScanResult> {
    scan_directory_with_progress_and_options(
        root,
        |_| {},
        Arc::new(AtomicBool::new(false)),
        options,
    )
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
    scan_directory_with_progress_and_options(root, on_progress, cancel, &ScanOptions::default())
}

pub fn scan_directory_with_progress_and_options<F>(
    root: &Path,
    on_progress: F,
    cancel: Arc<AtomicBool>,
    options: &ScanOptions,
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

    let exclude_matcher = build_exclude_matcher(&options.exclude_patterns)?;
    let mut files = Vec::new();
    let mut empty_dirs = Vec::new();
    let mut failures = Vec::new();
    let mut dirs_visited = 0_usize;
    let mut bytes_found = 0_u64;
    let mut counter = 0_u64;

    let walker = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| !is_excluded(entry.path(), root, &exclude_matcher));

    for entry in walker {
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
                            modified_at: metadata.modified().ok().map(DateTime::<Utc>::from),
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

fn build_exclude_matcher(patterns: &[String]) -> Result<GlobSet> {
    let mut builder = GlobSetBuilder::new();
    for pattern in BUILTIN_EXCLUDE_PATTERNS {
        builder.add(Glob::new(pattern)?);
    }
    for pattern in patterns
        .iter()
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    {
        builder.add(Glob::new(pattern)?);
    }
    Ok(builder.build()?)
}

fn is_excluded(path: &Path, root: &Path, matcher: &GlobSet) -> bool {
    if path == root {
        return false;
    }

    matcher.is_match(path)
        || path
            .strip_prefix(root)
            .map(|relative| matcher.is_match(relative))
            .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::{scan_directory_with_options, ScanOptions};
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let path = PathBuf::from("E:/agent").join(format!("sdc-scanner-{label}-{unique}"));
        fs::create_dir_all(&path).expect("test dir should be created");
        path
    }

    #[test]
    fn skips_builtin_cache_and_build_directories() {
        let root = test_dir("builtin");
        let keep_dir = root.join("docs");
        let excluded_dir = root.join(".cargo-target").join("smart-disk-cleaner");

        fs::create_dir_all(&keep_dir).expect("keep dir should exist");
        fs::create_dir_all(&excluded_dir).expect("excluded dir should exist");
        fs::write(keep_dir.join("keep.txt"), b"keep").expect("keep file should exist");
        fs::write(excluded_dir.join("artifact.lib"), b"artifact")
            .expect("excluded file should exist");

        let scan = scan_directory_with_options(&root, &ScanOptions::default())
            .expect("scan should succeed");

        assert_eq!(scan.files.len(), 1);
        assert!(scan
            .files
            .iter()
            .all(|file| file.path.ends_with("keep.txt")));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn skips_user_defined_patterns() {
        let root = test_dir("custom");
        let keep_dir = root.join("notes");
        let excluded_dir = root.join("archive");

        fs::create_dir_all(&keep_dir).expect("keep dir should exist");
        fs::create_dir_all(&excluded_dir).expect("excluded dir should exist");
        fs::write(keep_dir.join("keep.txt"), b"keep").expect("keep file should exist");
        fs::write(excluded_dir.join("old.zip"), b"zip").expect("excluded file should exist");

        let scan = scan_directory_with_options(
            &root,
            &ScanOptions {
                exclude_patterns: vec!["**/archive/**".to_string()],
            },
        )
        .expect("scan should succeed");

        assert_eq!(scan.files.len(), 1);
        assert!(scan
            .files
            .iter()
            .all(|file| file.path.ends_with("keep.txt")));

        let _ = fs::remove_dir_all(root);
    }
}
