use crate::analyzer::choose_keep_candidate;
use crate::models::{DedupResult, DuplicateGroup, FileRecord, PathIssue};
use anyhow::Result;
use blake3::Hasher;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

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
    pub current_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashCacheEntry {
    pub path: PathBuf,
    pub size: u64,
    pub modified_at: Option<DateTime<Utc>>,
    pub hash: String,
    pub cached_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HashCache {
    pub entries: Vec<HashCacheEntry>,
}

pub fn find_duplicates(files: &[FileRecord]) -> Result<DedupResult> {
    let (result, _) = find_duplicates_with_progress_and_cache(
        files,
        &HashCache::default(),
        |_| {},
        Arc::new(AtomicBool::new(false)),
    )?;
    Ok(result)
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
    let (result, _) =
        find_duplicates_with_progress_and_cache(files, &HashCache::default(), on_progress, cancel)?;
    Ok(result)
}

pub fn find_duplicates_with_progress_and_cache<F>(
    files: &[FileRecord],
    cache: &HashCache,
    on_progress: F,
    cancel: Arc<AtomicBool>,
) -> Result<(DedupResult, HashCache)>
where
    F: Fn(&DedupProgress),
{
    let mut by_size: HashMap<u64, Vec<FileRecord>> = HashMap::new();
    for file in files.iter().filter(|file| file.size > 0) {
        by_size.entry(file.size).or_default().push(file.clone());
    }

    let candidates: Vec<FileRecord> = by_size
        .into_values()
        .filter(|bucket| bucket.len() > 1)
        .flatten()
        .collect();
    let total = candidates.len();

    if total == 0 {
        on_progress(&DedupProgress {
            phase: "done".to_string(),
            files_hashed: 0,
            files_total: 0,
            current_path: None,
        });
        return Ok((
            DedupResult {
                groups: Vec::new(),
                failures: Vec::new(),
            },
            HashCache::default(),
        ));
    }

    let worker_count = std::thread::available_parallelism()
        .map(|value| value.get().min(4))
        .unwrap_or(2)
        .min(total.max(1));
    let queue = Arc::new(Mutex::new(VecDeque::from(candidates.clone())));
    let cache_map = Arc::new(build_cache_lookup(cache));
    let (tx, rx) = mpsc::channel::<WorkerMessage>();

    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&queue);
            let tx = tx.clone();
            let cancel = Arc::clone(&cancel);
            let cache_map = Arc::clone(&cache_map);
            scope.spawn(move || loop {
                if cancel.load(Ordering::Relaxed) {
                    let _ = tx.send(WorkerMessage::Finished);
                    break;
                }

                let Some(file) = pop_job(&queue) else {
                    let _ = tx.send(WorkerMessage::Finished);
                    break;
                };

                let result = if let Some(entry) = cache_map.get(&file.path) {
                    if cache_matches(entry, &file) {
                        Ok(HashCacheEntry {
                            path: file.path.clone(),
                            size: file.size,
                            modified_at: file.modified_at,
                            hash: entry.hash.clone(),
                            cached_at: Utc::now(),
                        })
                    } else {
                        hash_file_entry(&file)
                    }
                } else {
                    hash_file_entry(&file)
                };

                let _ = tx.send(WorkerMessage::Processed { file, result });
            });
        }

        drop(tx);

        let mut by_hash: HashMap<String, Vec<FileRecord>> = HashMap::new();
        let mut failures = Vec::new();
        let mut cache_entries = Vec::new();
        let mut hashed = 0_usize;
        let mut finished_workers = 0_usize;

        while finished_workers < worker_count {
            match rx.recv() {
                Ok(WorkerMessage::Finished) => {
                    finished_workers += 1;
                }
                Ok(WorkerMessage::Processed { file, result }) => {
                    hashed += 1;
                    match result {
                        Ok(entry) => {
                            by_hash
                                .entry(entry.hash.clone())
                                .or_default()
                                .push(file.clone());
                            cache_entries.push(entry);
                        }
                        Err(err) => failures.push(PathIssue {
                            path: file.path.clone(),
                            message: format!("failed to hash file: {err}"),
                        }),
                    }

                    if hashed % 50 == 0 || hashed == total {
                        on_progress(&DedupProgress {
                            phase: "hashing".to_string(),
                            files_hashed: hashed,
                            files_total: total,
                            current_path: Some(file.path.clone()),
                        });
                    }
                }
                Err(_) => break,
            }
        }

        if cancel.load(Ordering::Relaxed) {
            anyhow::bail!("dedup cancelled by user");
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

        Ok((
            DedupResult { groups, failures },
            HashCache {
                entries: cache_entries,
            },
        ))
    })
}

fn pop_job(queue: &Arc<Mutex<VecDeque<FileRecord>>>) -> Option<FileRecord> {
    queue.lock().ok()?.pop_front()
}

fn build_cache_lookup(cache: &HashCache) -> HashMap<PathBuf, HashCacheEntry> {
    cache
        .entries
        .iter()
        .cloned()
        .map(|entry| (entry.path.clone(), entry))
        .collect()
}

fn cache_matches(entry: &HashCacheEntry, file: &FileRecord) -> bool {
    entry.size == file.size && entry.modified_at == file.modified_at
}

fn hash_file_entry(file: &FileRecord) -> Result<HashCacheEntry> {
    Ok(HashCacheEntry {
        path: file.path.clone(),
        size: file.size,
        modified_at: file.modified_at,
        hash: hash_file(&file.path)?,
        cached_at: Utc::now(),
    })
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

enum WorkerMessage {
    Processed {
        file: FileRecord,
        result: Result<HashCacheEntry>,
    },
    Finished,
}
