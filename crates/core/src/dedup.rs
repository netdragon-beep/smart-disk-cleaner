use crate::analyzer::choose_keep_candidate;
use crate::models::{DedupResult, DuplicateGroup, FileRecord, PathIssue};
use anyhow::Result;
use blake3::Hasher;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Progress information emitted during duplicate detection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DedupProgress {
    /// Current phase: "hashing", "done"
    pub phase: String,
    /// Number of files hashed so far
    pub files_hashed: usize,
    /// Total number of files to hash
    pub files_total: usize,
    /// Current file being hashed
    pub current_path: Option<std::path::PathBuf>,
}

pub fn find_duplicates(files: &[FileRecord]) -> Result<DedupResult> {
    find_duplicates_with_progress(files, |_| {}, Arc::new(AtomicBool::new(false)))
}

/// Find duplicate files, calling `on_progress` periodically with progress updates.
pub fn find_duplicates_with_progress<F>(
    files: &[FileRecord],
    on_progress: F,
    cancel: Arc<AtomicBool>,
) -> Result<DedupResult>
where
    F: Fn(&DedupProgress),
{
    let mut by_size: HashMap<u64, Vec<&FileRecord>> = HashMap::new();
    for file in files.iter().filter(|file| file.size > 0) {
        by_size.entry(file.size).or_default().push(file);
    }

    let candidates: Vec<&FileRecord> = by_size
        .into_values()
        .filter(|bucket| bucket.len() > 1)
        .flatten()
        .collect();
    let total = candidates.len();

    let mut by_hash: HashMap<String, Vec<FileRecord>> = HashMap::new();
    let mut failures = Vec::new();
    let mut hashed = 0_usize;

    for file in candidates {
        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("dedup cancelled by user");
        }

        match hash_file(&file.path) {
            Ok(hash) => by_hash.entry(hash).or_default().push(file.clone()),
            Err(err) => failures.push(PathIssue {
                path: file.path.clone(),
                message: format!("failed to hash file: {err}"),
            }),
        }

        hashed += 1;
        if hashed % 50 == 0 || hashed == total {
            on_progress(&DedupProgress {
                phase: "hashing".to_string(),
                files_hashed: hashed,
                files_total: total,
                current_path: Some(file.path.clone()),
            });
        }
    }

    let mut groups: Vec<DuplicateGroup> = by_hash
        .into_iter()
        .filter_map(|(hash, files)| {
            if files.len() < 2 {
                return None;
            }
            let keep = choose_keep_candidate(&files);
            let total_size = files.iter().map(|file| file.size).sum();
            Some(DuplicateGroup {
                hash,
                total_size,
                suggested_keep: keep.map(|file| file.path),
                files,
            })
        })
        .collect();

    groups.sort_by(|left, right| right.total_size.cmp(&left.total_size));

    on_progress(&DedupProgress {
        phase: "done".to_string(),
        files_hashed: hashed,
        files_total: total,
        current_path: None,
    });

    Ok(DedupResult { groups, failures })
}

fn hash_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Hasher::new();
    let mut buffer = [0_u8; 8192];

    loop {
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}
