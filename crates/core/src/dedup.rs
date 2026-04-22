use crate::analyzer::choose_keep_candidate;
use crate::models::{DedupResult, DuplicateGroup, FileRecord, PathIssue};
use anyhow::Result;
use blake3::Hasher;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};

const PARTIAL_HASH_WINDOW_BYTES: u64 = 64 * 1024;

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

    let cache_map = build_cache_lookup(cache);
    let mut ready_entries = Vec::new();
    let mut failures = Vec::new();
    let candidates = collect_full_hash_candidates(by_size, &mut ready_entries, &mut failures)?;

    let total = candidates.len();

    if total == 0 {
        let groups = build_duplicate_groups_from_entries(&ready_entries);
        on_progress(&DedupProgress {
            phase: "done".to_string(),
            files_hashed: 0,
            files_total: 0,
            current_path: None,
        });
        return Ok((
            DedupResult { groups, failures },
            HashCache {
                entries: hashed_entries_to_cache(&ready_entries),
            },
        ));
    }

    let worker_count = std::thread::available_parallelism()
        .map(|value| value.get().min(4))
        .unwrap_or(2)
        .min(total.max(1));
    let queue = Arc::new(Mutex::new(VecDeque::from(candidates.clone())));
    let cache_map = Arc::new(cache_map);
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
                        Ok(cached_entry_to_hashed(entry, &file))
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
        let mut all_failures = failures;
        let mut cache_entries = ready_entries;
        let mut hashed = 0_usize;
        let mut finished_workers = 0_usize;

        for entry in &cache_entries {
            by_hash
                .entry(entry.hash.clone())
                .or_default()
                .push(entry.file.clone());
        }

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
                                .push(entry.file.clone());
                            cache_entries.push(entry);
                        }
                        Err(err) => all_failures.push(PathIssue {
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

        let groups = build_duplicate_groups(by_hash);

        on_progress(&DedupProgress {
            phase: "done".to_string(),
            files_hashed: hashed,
            files_total: total,
            current_path: None,
        });

        Ok((
            DedupResult {
                groups,
                failures: all_failures,
            },
            HashCache {
                entries: hashed_entries_to_cache(&cache_entries),
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

fn hash_file_entry(file: &FileRecord) -> Result<HashedFileEntry> {
    Ok(HashedFileEntry {
        file: file.clone(),
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

fn collect_full_hash_candidates(
    by_size: HashMap<u64, Vec<FileRecord>>,
    ready_entries: &mut Vec<HashedFileEntry>,
    failures: &mut Vec<PathIssue>,
) -> Result<Vec<FileRecord>> {
    let mut candidates = Vec::new();

    for bucket in by_size.into_values().filter(|bucket| bucket.len() > 1) {
        let mut exact_by_hash: HashMap<String, Vec<HashedFileEntry>> = HashMap::new();
        let mut partial_by_hash: HashMap<String, Vec<FileRecord>> = HashMap::new();

        for file in bucket {
            match prefilter_file(&file) {
                Ok(PrefilterOutcome::Exact(entry)) => {
                    exact_by_hash
                        .entry(entry.hash.clone())
                        .or_default()
                        .push(entry);
                }
                Ok(PrefilterOutcome::Partial { file, signature }) => {
                    partial_by_hash.entry(signature).or_default().push(file);
                }
                Err(err) => failures.push(PathIssue {
                    path: file.path.clone(),
                    message: format!("failed to sample file for duplicate detection: {err}"),
                }),
            }
        }

        for entries in exact_by_hash.into_values().filter(|items| items.len() > 1) {
            ready_entries.extend(entries);
        }

        for files in partial_by_hash
            .into_values()
            .filter(|items| items.len() > 1)
        {
            candidates.extend(files);
        }
    }

    Ok(candidates)
}

fn prefilter_file(file: &FileRecord) -> Result<PrefilterOutcome> {
    if file.size <= PARTIAL_HASH_WINDOW_BYTES * 2 {
        return Ok(PrefilterOutcome::Exact(hash_file_entry(file)?));
    }

    let signature = partial_hash(&file.path, file.size)?;
    Ok(PrefilterOutcome::Partial {
        file: file.clone(),
        signature,
    })
}

fn partial_hash(path: &Path, size: u64) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Hasher::new();
    let mut head_buffer = vec![0_u8; PARTIAL_HASH_WINDOW_BYTES as usize];
    let mut tail_buffer = vec![0_u8; PARTIAL_HASH_WINDOW_BYTES as usize];

    reader.read_exact(&mut head_buffer)?;
    hasher.update(&head_buffer);
    hasher.update(&size.to_le_bytes());

    reader.seek(SeekFrom::Start(size - PARTIAL_HASH_WINDOW_BYTES))?;
    reader.read_exact(&mut tail_buffer)?;
    hasher.update(&tail_buffer);

    Ok(hasher.finalize().to_hex().to_string())
}

fn build_duplicate_groups_from_entries(entries: &[HashedFileEntry]) -> Vec<DuplicateGroup> {
    let mut by_hash: HashMap<String, Vec<FileRecord>> = HashMap::new();
    for entry in entries {
        by_hash
            .entry(entry.hash.clone())
            .or_default()
            .push(entry.file.clone());
    }
    build_duplicate_groups(by_hash)
}

fn build_duplicate_groups(by_hash: HashMap<String, Vec<FileRecord>>) -> Vec<DuplicateGroup> {
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
    groups
}

#[derive(Debug, Clone)]
enum PrefilterOutcome {
    Exact(HashedFileEntry),
    Partial { file: FileRecord, signature: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct HashedFileEntry {
    file: FileRecord,
    size: u64,
    modified_at: Option<DateTime<Utc>>,
    hash: String,
    cached_at: DateTime<Utc>,
}

impl From<HashCacheEntry> for HashedFileEntry {
    fn from(value: HashCacheEntry) -> Self {
        Self {
            file: FileRecord {
                path: value.path.clone(),
                size: value.size,
                extension: value
                    .path
                    .extension()
                    .and_then(|value| value.to_str())
                    .map(|value| value.to_ascii_lowercase()),
                modified_at: value.modified_at,
                is_empty: value.size == 0,
            },
            size: value.size,
            modified_at: value.modified_at,
            hash: value.hash,
            cached_at: value.cached_at,
        }
    }
}

fn cached_entry_to_hashed(entry: &HashCacheEntry, file: &FileRecord) -> HashedFileEntry {
    HashedFileEntry {
        file: file.clone(),
        size: entry.size,
        modified_at: entry.modified_at,
        hash: entry.hash.clone(),
        cached_at: Utc::now(),
    }
}

fn hashed_entries_to_cache(entries: &[HashedFileEntry]) -> Vec<HashCacheEntry> {
    entries
        .iter()
        .map(|entry| HashCacheEntry {
            path: entry.file.path.clone(),
            size: entry.size,
            modified_at: entry.modified_at,
            hash: entry.hash.clone(),
            cached_at: entry.cached_at,
        })
        .collect()
}

enum WorkerMessage {
    Processed {
        file: FileRecord,
        result: Result<HashedFileEntry>,
    },
    Finished,
}

#[cfg(test)]
mod tests {
    use super::{find_duplicates, PARTIAL_HASH_WINDOW_BYTES};
    use crate::models::FileRecord;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_dir(label: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("sdc-dedup-{label}-{unique}"));
        fs::create_dir_all(&path).expect("test dir should be created");
        path
    }

    fn record(path: PathBuf) -> FileRecord {
        let metadata = fs::metadata(&path).expect("file metadata should exist");
        FileRecord {
            path: path.clone(),
            size: metadata.len(),
            extension: path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase()),
            modified_at: metadata
                .modified()
                .ok()
                .map(chrono::DateTime::<chrono::Utc>::from),
            is_empty: metadata.len() == 0,
        }
    }

    #[test]
    fn finds_large_duplicates_after_partial_prefilter() {
        let root = test_dir("large-dupes");
        let duplicate_a = root.join("a.bin");
        let duplicate_b = root.join("b.bin");
        let unique = root.join("c.bin");

        let mut shared = vec![b'A'; PARTIAL_HASH_WINDOW_BYTES as usize];
        shared.extend(vec![b'B'; PARTIAL_HASH_WINDOW_BYTES as usize]);
        shared.extend(vec![b'C'; PARTIAL_HASH_WINDOW_BYTES as usize]);

        let mut unique_bytes = shared.clone();
        let middle_start = PARTIAL_HASH_WINDOW_BYTES as usize;
        let middle_end = middle_start + PARTIAL_HASH_WINDOW_BYTES as usize;
        unique_bytes[middle_start..middle_end].fill(b'Z');

        fs::write(&duplicate_a, &shared).expect("duplicate a should exist");
        fs::write(&duplicate_b, &shared).expect("duplicate b should exist");
        fs::write(&unique, &unique_bytes).expect("unique file should exist");

        let result = find_duplicates(&[
            record(duplicate_a.clone()),
            record(duplicate_b.clone()),
            record(unique.clone()),
        ])
        .expect("dedup should succeed");

        assert_eq!(result.groups.len(), 1);
        assert_eq!(result.groups[0].files.len(), 2);
        assert!(result.groups[0]
            .files
            .iter()
            .any(|file| file.path == duplicate_a));
        assert!(result.groups[0]
            .files
            .iter()
            .any(|file| file.path == duplicate_b));
        assert!(!result.groups[0]
            .files
            .iter()
            .any(|file| file.path == unique));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn does_not_mark_same_size_large_files_as_duplicates_when_hash_differs() {
        let root = test_dir("same-size");
        let left = root.join("left.bin");
        let right = root.join("right.bin");

        let mut left_bytes = vec![b'A'; (PARTIAL_HASH_WINDOW_BYTES * 3) as usize];
        let mut right_bytes = left_bytes.clone();
        right_bytes[(PARTIAL_HASH_WINDOW_BYTES as usize)..(PARTIAL_HASH_WINDOW_BYTES as usize * 2)]
            .fill(b'Q');

        fs::write(&left, &left_bytes).expect("left file should exist");
        fs::write(&right, &right_bytes).expect("right file should exist");

        let result = find_duplicates(&[record(left), record(right)]).expect("dedup should succeed");
        assert!(result.groups.is_empty());

        let _ = fs::remove_dir_all(root);
        left_bytes.clear();
    }
}
