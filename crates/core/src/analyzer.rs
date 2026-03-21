use crate::models::{
    AnalysisResult, DedupResult, FileRecord, ScanModuleKind, ScanModuleSummary, ScanResult,
    TypeStat,
};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct AnalyzerOptions {
    pub large_file_threshold_bytes: u64,
}

pub fn analyze(scan: &ScanResult, options: AnalyzerOptions) -> AnalysisResult {
    let mut total_size = 0_u64;
    let mut empty_files = Vec::new();
    let mut large_files = Vec::new();
    let mut temporary_files = Vec::new();
    let mut archive_files = Vec::new();
    let mut type_map: HashMap<String, (usize, u64)> = HashMap::new();

    for file in &scan.files {
        total_size += file.size;

        if file.is_empty {
            empty_files.push(file.clone());
        }

        if file.size >= options.large_file_threshold_bytes {
            large_files.push(file.clone());
        }

        if is_temporary_file(file) {
            temporary_files.push(file.clone());
        }

        if looks_archive_or_installer(&file.path) {
            archive_files.push(file.clone());
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
    temporary_files.sort_by(|left, right| right.size.cmp(&left.size));
    archive_files.sort_by(|left, right| right.size.cmp(&left.size));

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
        temporary_files,
        archive_files,
        type_breakdown,
    }
}

pub fn build_scan_modules(
    analysis: &AnalysisResult,
    dedup: &DedupResult,
) -> Vec<ScanModuleSummary> {
    vec![
        ScanModuleSummary {
            kind: ScanModuleKind::DuplicateFiles,
            item_count: dedup.groups.len(),
            total_size: dedup.groups.iter().map(|group| group.total_size).sum(),
        },
        ScanModuleSummary {
            kind: ScanModuleKind::LargeFiles,
            item_count: analysis.large_files.len(),
            total_size: analysis.large_files.iter().map(|file| file.size).sum(),
        },
        ScanModuleSummary {
            kind: ScanModuleKind::TemporaryFiles,
            item_count: analysis.temporary_files.len(),
            total_size: analysis.temporary_files.iter().map(|file| file.size).sum(),
        },
        ScanModuleSummary {
            kind: ScanModuleKind::ArchiveFiles,
            item_count: analysis.archive_files.len(),
            total_size: analysis.archive_files.iter().map(|file| file.size).sum(),
        },
        ScanModuleSummary {
            kind: ScanModuleKind::EmptyFiles,
            item_count: analysis.empty_files.len(),
            total_size: 0,
        },
        ScanModuleSummary {
            kind: ScanModuleKind::EmptyDirectories,
            item_count: analysis.empty_dirs.len(),
            total_size: 0,
        },
    ]
}

pub fn choose_keep_candidate(files: &[FileRecord]) -> Option<FileRecord> {
    files.iter().cloned().max_by(
        |left, right| match (&left.modified_at, &right.modified_at) {
            (Some(left_time), Some(right_time)) => left_time.cmp(right_time),
            (Some(_), None) => std::cmp::Ordering::Greater,
            (None, Some(_)) => std::cmp::Ordering::Less,
            (None, None) => left.path.cmp(&right.path),
        },
    )
}

fn is_temporary_file(file: &FileRecord) -> bool {
    let extension = file
        .extension
        .as_deref()
        .map(|value| value.to_ascii_lowercase());

    if matches!(
        extension.as_deref(),
        Some("tmp" | "temp" | "cache" | "bak" | "old" | "dmp" | "crdownload" | "part" | "download")
    ) {
        return true;
    }

    let file_name = file
        .path
        .file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .unwrap_or_default();

    if file_name.ends_with(".tmp")
        || file_name.ends_with(".temp")
        || file_name.ends_with(".cache")
        || file_name.ends_with(".bak")
    {
        return true;
    }

    file.path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .map(|value| value.to_ascii_lowercase())
        .any(|value| matches!(value.as_str(), "temp" | "tmp" | "cache" | "caches"))
}

fn looks_archive_or_installer(path: &Path) -> bool {
    matches!(
        path.extension()
            .and_then(|value| value.to_str())
            .map(|value| value.to_ascii_lowercase())
            .as_deref(),
        Some("zip" | "7z" | "rar" | "tar" | "gz" | "bz2" | "xz" | "iso" | "msi" | "exe")
    )
}

#[cfg(test)]
mod tests {
    use super::{analyze, build_scan_modules, choose_keep_candidate, AnalyzerOptions};
    use crate::models::{
        DedupResult, DuplicateGroup, FileRecord, PathIssue, ScanModuleKind, ScanResult,
    };
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

    fn file(path: &str, size: u64) -> FileRecord {
        FileRecord {
            path: PathBuf::from(path),
            size,
            extension: PathBuf::from(path)
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.to_ascii_lowercase()),
            modified_at: None,
            is_empty: size == 0,
        }
    }

    #[test]
    fn analyze_detects_temporary_and_archive_files() {
        let scan = ScanResult {
            root: PathBuf::from("E:/test"),
            files: vec![
                file("E:/test/cache/app.tmp", 128),
                file("E:/test/archive/demo.zip", 2048),
                file("E:/test/docs/report.pdf", 512),
            ],
            empty_dirs: vec![PathBuf::from("E:/test/empty")],
            failures: Vec::<PathIssue>::new(),
        };

        let analysis = analyze(
            &scan,
            AnalyzerOptions {
                large_file_threshold_bytes: 1024,
            },
        );

        assert_eq!(analysis.temporary_files.len(), 1);
        assert_eq!(analysis.archive_files.len(), 1);
        assert_eq!(analysis.large_files.len(), 1);
        assert_eq!(
            analysis.archive_files[0].path,
            PathBuf::from("E:/test/archive/demo.zip")
        );
    }

    #[test]
    fn build_scan_modules_contains_expected_counts() {
        let analysis = analyze(
            &ScanResult {
                root: PathBuf::from("E:/test"),
                files: vec![
                    file("E:/test/cache/app.tmp", 100),
                    file("E:/test/demo.zip", 200),
                ],
                empty_dirs: vec![PathBuf::from("E:/test/empty")],
                failures: Vec::<PathIssue>::new(),
            },
            AnalyzerOptions {
                large_file_threshold_bytes: 150,
            },
        );

        let dedup = DedupResult {
            groups: vec![DuplicateGroup {
                hash: "abc".to_string(),
                total_size: 400,
                files: vec![file("E:/test/a.bin", 200), file("E:/test/b.bin", 200)],
                suggested_keep: None,
            }],
            failures: Vec::<PathIssue>::new(),
        };

        let modules = build_scan_modules(&analysis, &dedup);

        assert!(modules
            .iter()
            .any(|item| item.kind == ScanModuleKind::DuplicateFiles && item.item_count == 1));
        assert!(modules
            .iter()
            .any(|item| item.kind == ScanModuleKind::TemporaryFiles && item.item_count == 1));
        assert!(modules
            .iter()
            .any(|item| item.kind == ScanModuleKind::ArchiveFiles && item.item_count == 1));
    }
}
