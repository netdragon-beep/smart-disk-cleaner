use anyhow::Result;
use chrono::Utc;
use clap::Parser;
use smart_disk_cleaner_core::ai_advisor::{build_advice, AdvisorConfig};
use smart_disk_cleaner_core::analyzer::{analyze, build_scan_modules, AnalyzerOptions};
use smart_disk_cleaner_core::dedup::find_duplicates;
use smart_disk_cleaner_core::diagnostics::probe_path;
use smart_disk_cleaner_core::executor::{execute_from_report, ExecuteOptions};
use smart_disk_cleaner_core::models::ScanReport;
use smart_disk_cleaner_core::reporter::{
    render_diagnosis, render_summary, write_operation_log, write_report,
};
use smart_disk_cleaner_core::scanner::scan_directory;
use tracing_subscriber::EnvFilter;

mod cli;
use cli::{Cli, Commands};

#[tokio::main]
async fn main() -> Result<()> {
    init_tracing();
    let cli = Cli::parse();

    match cli.command {
        Commands::Analyze {
            path,
            large_file_threshold_mb,
            max_ai_items,
            output,
            api_key,
            ai_base_url,
            ai_model,
        } => {
            let scan = scan_directory(&path)?;
            let analysis = analyze(
                &scan,
                AnalyzerOptions {
                    large_file_threshold_bytes: large_file_threshold_mb * 1024 * 1024,
                },
            );
            let dedup = find_duplicates(&scan.files)?;
            let advisor = build_advice(
                &scan,
                &analysis,
                &dedup,
                &AdvisorConfig {
                    api_key,
                    base_url: ai_base_url,
                    model: ai_model,
                    max_items: max_ai_items,
                    strict_file_ai_remote_only: false,
                },
            )
            .await?;

            let report = ScanReport {
                generated_at: Utc::now(),
                root: scan.root.clone(),
                modules: build_scan_modules(&analysis, &dedup),
                analysis,
                dedup,
                advisor,
                failures: scan.failures,
            };

            write_report(&output, &report)?;
            println!("{}", render_summary(&report));
            println!("JSON report written to: {}", output.display());
        }
        Commands::Execute {
            report,
            mode,
            paths,
            target_dir,
            log,
            apply,
        } => {
            let logs = execute_from_report(&ExecuteOptions {
                report_path: report,
                mode: mode.into(),
                paths,
                target_dir,
                dry_run: !apply,
            })?;
            write_operation_log(&log, &logs)?;
            for entry in &logs {
                println!(
                    "[{}] {} -> {}",
                    if entry.success { "OK" } else { "ERR" },
                    entry.path.display(),
                    entry.detail
                );
                if let Some(diagnosis) = &entry.diagnosis {
                    println!("{}", render_diagnosis(diagnosis));
                }
            }
            println!("operation log written to: {}", log.display());
        }
        Commands::Diagnose { path, operation } => {
            let diagnosis = probe_path(&path, operation.into());
            println!("{}", render_diagnosis(&diagnosis));
        }
    }

    Ok(())
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
