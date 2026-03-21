use crate::events::ProgressEvent;
use crate::state::AppState;
use chrono::Utc;
use smart_disk_cleaner_core::ai_advisor::{build_advice, AdvisorConfig};
use smart_disk_cleaner_core::analyzer::{analyze, build_scan_modules, AnalyzerOptions};
use smart_disk_cleaner_core::dedup::find_duplicates_with_progress;
use smart_disk_cleaner_core::diagnostics::{probe_path, DiagnosticOperation};
use smart_disk_cleaner_core::models::{PathDiagnosis, ScanReport};
use smart_disk_cleaner_core::scanner::{scan_directory_with_progress_and_options, ScanOptions};
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use tauri::{ipc::Channel, State};

#[tauri::command]
pub async fn start_scan(
    path: String,
    on_progress: Channel<ProgressEvent>,
    state: State<'_, AppState>,
) -> Result<ScanReport, String> {
    let root = PathBuf::from(&path);
    let config = state.load_config();
    let cancel = state.cancel_flag.clone();
    cancel.store(false, Ordering::Relaxed);

    let cancel_scan = cancel.clone();
    let on_progress_scan = on_progress.clone();

    let scan = tokio::task::spawn_blocking(move || {
        scan_directory_with_progress_and_options(
            &root,
            |p| {
                let _ = on_progress_scan.send(ProgressEvent::from(p));
            },
            cancel_scan,
            &ScanOptions {
                exclude_patterns: config.exclude_patterns.clone(),
            },
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "analyzing".to_string(),
        detail: "\u{6B63}\u{5728}\u{5206}\u{6790}\u{6587}\u{4EF6}\u{7C7B}\u{578B}\u{548C}\u{5927}\u{5C0F}...".to_string(),
    });
    let analysis = analyze(
        &scan,
        AnalyzerOptions {
            large_file_threshold_bytes: config.large_file_threshold_mb * 1024 * 1024,
        },
    );

    let files = scan.files.clone();
    let cancel_dedup = cancel.clone();
    let on_progress_dedup = on_progress.clone();
    let dedup = tokio::task::spawn_blocking(move || {
        find_duplicates_with_progress(
            &files,
            |p| {
                let _ = on_progress_dedup.send(ProgressEvent::from(p));
            },
            cancel_dedup,
        )
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "advising".to_string(),
        detail: "\u{6B63}\u{5728}\u{751F}\u{6210}\u{6E05}\u{7406}\u{5EFA}\u{8BAE}...".to_string(),
    });
    let advisor = build_advice(
        &scan,
        &analysis,
        &dedup,
        &AdvisorConfig {
            api_key: config.api_key.clone(),
            base_url: config.ai_base_url.clone(),
            model: config.ai_model.clone(),
            max_items: config.max_ai_items,
        },
    )
    .await
    .map_err(|e| e.to_string())?;

    let report = ScanReport {
        generated_at: Utc::now(),
        root: scan.root.clone(),
        scanned_files: scan.files.clone(),
        modules: build_scan_modules(&analysis, &dedup),
        analysis,
        dedup,
        advisor,
        failures: scan.failures,
    };

    *state.last_report.lock().await = Some(report.clone());

    let _ = on_progress.send(ProgressEvent::Analyze {
        phase: "done".to_string(),
        detail: "\u{626B}\u{63CF}\u{5B8C}\u{6210}".to_string(),
    });

    Ok(report)
}

#[tauri::command]
pub async fn cancel_scan(state: State<'_, AppState>) -> Result<(), String> {
    state.cancel_flag.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
pub async fn diagnose_path(path: String, operation: String) -> Result<PathDiagnosis, String> {
    let op = match operation.as_str() {
        "recycle" => DiagnosticOperation::Recycle,
        "move" => DiagnosticOperation::Move,
        _ => DiagnosticOperation::Probe,
    };
    Ok(probe_path(&PathBuf::from(path), op))
}
