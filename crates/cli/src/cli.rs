use smart_disk_cleaner_core::diagnostics::DiagnosticOperation;
use smart_disk_cleaner_core::models::ExecutionMode;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(name = "smart-disk-cleaner")]
#[command(version, about = "Local disk cleanup and space analysis tool built with Rust and AI")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Analyze {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, default_value_t = 512)]
        large_file_threshold_mb: u64,
        #[arg(long, default_value_t = 20)]
        max_ai_items: usize,
        #[arg(long, default_value = "scan-report.json")]
        output: PathBuf,
        #[arg(long, env = "OPENAI_API_KEY")]
        api_key: Option<String>,
        #[arg(long, default_value = "https://api.openai.com")]
        ai_base_url: String,
        #[arg(long, default_value = "gpt-4.1-mini")]
        ai_model: String,
    },
    Execute {
        #[arg(long)]
        report: PathBuf,
        #[arg(long, value_enum)]
        mode: ModeArg,
        #[arg(long, value_name = "PATH", required = true)]
        paths: Vec<PathBuf>,
        #[arg(long)]
        target_dir: Option<PathBuf>,
        #[arg(long, default_value = "operations-log.json")]
        log: PathBuf,
        #[arg(long, help = "Apply real changes. Without this flag the command stays in dry-run mode.")]
        apply: bool,
    },
    Diagnose {
        #[arg(value_name = "PATH")]
        path: PathBuf,
        #[arg(long, value_enum, default_value = "probe")]
        operation: DiagnoseArg,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum ModeArg {
    Recycle,
    Move,
}

impl From<ModeArg> for ExecutionMode {
    fn from(value: ModeArg) -> Self {
        match value {
            ModeArg::Recycle => ExecutionMode::Recycle,
            ModeArg::Move => ExecutionMode::Move,
        }
    }
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum DiagnoseArg {
    Probe,
    Recycle,
    Move,
}

impl From<DiagnoseArg> for DiagnosticOperation {
    fn from(value: DiagnoseArg) -> Self {
        match value {
            DiagnoseArg::Probe => DiagnosticOperation::Probe,
            DiagnoseArg::Recycle => DiagnosticOperation::Recycle,
            DiagnoseArg::Move => DiagnosticOperation::Move,
        }
    }
}
