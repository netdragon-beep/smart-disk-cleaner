use crate::models::{OperationLogEntry, ScanReport};
use anyhow::{Context, Result};
use humansize::{format_size, DECIMAL};
use std::fs;
use std::path::Path;

pub fn render_summary(report: &ScanReport) -> String {
    let duplicate_file_count: usize = report.dedup.groups.iter().map(|group| group.files.len()).sum();
    format!(
        "Root: {}\nTotal files: {}\nTotal size: {}\nLarge files: {}\nEmpty files: {}\nEmpty directories: {}\nDuplicate groups: {}\nDuplicate files involved: {}\nAdvice source: {}\nSummary: {}",
        report.root.display(),
        report.analysis.total_files,
        format_size(report.analysis.total_size, DECIMAL),
        report.analysis.large_files.len(),
        report.analysis.empty_files.len(),
        report.analysis.empty_dirs.len(),
        report.dedup.groups.len(),
        duplicate_file_count,
        report.advisor.source,
        report.advisor.summary
    )
}

pub fn write_report(path: &Path, report: &ScanReport) -> Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    fs::write(path, json).with_context(|| format!("failed to write report: {}", path.display()))
}

pub fn write_operation_log(path: &Path, entries: &[OperationLogEntry]) -> Result<()> {
    let json = serde_json::to_string_pretty(entries)?;
    fs::write(path, json).with_context(|| format!("failed to write operation log: {}", path.display()))
}

pub fn render_diagnosis(entry: &crate::models::PathDiagnosis) -> String {
    let mut output = String::new();
    output.push_str(&format!("Path: {}\n", entry.path.display()));
    output.push_str(&format!("Operation: {}\n", entry.operation));
    output.push_str(&format!("Code: {:?}\n", entry.code));
    output.push_str(&format!("Severity: {:?}\n", entry.severity));
    output.push_str(&format!("Summary: {}\n", entry.summary));
    if !entry.details.is_empty() {
        output.push_str("Details:\n");
        for detail in &entry.details {
            output.push_str(&format!("- {detail}\n"));
        }
    }
    if !entry.possible_related_apps.is_empty() {
        output.push_str("Possible related apps:\n");
        for item in &entry.possible_related_apps {
            output.push_str(&format!("- {item}\n"));
        }
    }
    if !entry.suggestions.is_empty() {
        output.push_str("Suggestions:\n");
        for suggestion in &entry.suggestions {
            output.push_str(&format!("- {suggestion}\n"));
        }
    }
    output
}
