use crate::models::{AnalysisResult, FileRecord, ScanResult, TypeStat};
use std::collections::HashMap;

#[derive(Debug, Clone, Copy)]
pub struct AnalyzerOptions {
    pub large_file_threshold_bytes: u64,
}

pub fn analyze(scan: &ScanResult, options: AnalyzerOptions) -> AnalysisResult {
    let mut total_size = 0_u64;
    let mut empty_files = Vec::new();
    let mut large_files = Vec::new();
    let mut type_map: HashMap<String, (usize, u64)> = HashMap::new();

    for file in &scan.files {
        total_size += file.size;

        if file.is_empty {
            empty_files.push(file.clone());
        }

        if file.size >= options.large_file_threshold_bytes {
            large_files.push(file.clone());
        }

        let extension = file
            .extension
            .clone()
            .unwrap_or_else(|| "no_extension".to_string());
        let entry = type_map.entry(extension).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += file.size;
    }

    large_files.sort_by(|left, right| right.size.cmp(&left.size));
    empty_files.sort_by(|left, right| left.path.cmp(&right.path));

    let mut type_breakdown: Vec<TypeStat> = type_map
        .into_iter()
        .map(|(extension, (file_count, total_size))| TypeStat {
            extension,
            file_count,
            total_size,
        })
        .collect();
    type_breakdown.sort_by(|left, right| right.total_size.cmp(&left.total_size));

    AnalysisResult {
        total_files: scan.files.len(),
        total_size,
        empty_files,
        empty_dirs: scan.empty_dirs.clone(),
        large_files,
        type_breakdown,
    }
}

pub fn choose_keep_candidate(files: &[FileRecord]) -> Option<FileRecord> {
    files
        .iter()
        .cloned()
        .max_by(|left, right| match (&left.modified_at, &right.modified_at) {
            (Some(left_time), Some(right_time)) => left_time.cmp(right_time),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => left.path.cmp(&right.path),
        })
}

#[cfg(test)]
mod tests {
    use super::choose_keep_candidate;
    use crate::models::FileRecord;
    use chrono::{TimeZone, Utc};
    use std::path::PathBuf;

    #[test]
    fn choose_keep_candidate_prefers_newer_file() {
        let older = FileRecord {
            path: PathBuf::from("old.zip"),
            size: 10,
            extension: Some("zip".to_string()),
            modified_at: Some(Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap()),
            is_empty: false,
        };
        let newer = FileRecord {
            path: PathBuf::from("new.zip"),
            size: 10,
            extension: Some("zip".to_string()),
            modified_at: Some(Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap()),
            is_empty: false,
        };

        let selected = choose_keep_candidate(&[older, newer]).expect("candidate should exist");
        assert_eq!(selected.path, PathBuf::from("new.zip"));
    }
}
