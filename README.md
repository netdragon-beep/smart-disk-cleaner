# smart-disk-cleaner

Local disk cleanup and space analysis tool built with Rust and AI. This repository currently targets the course-project MVP: analyze first, recommend next, and avoid dangerous actions by default.

## Implemented in this iteration

- Directory scanning for a user-selected root
- Large file detection
- Empty file and empty directory detection
- Duplicate file detection with size pre-filtering and BLAKE3 hashing
- Rule-based cleanup recommendations
- Optional AI-generated summary through an OpenAI-compatible `chat/completions` endpoint
- Safe execution flow with `dry-run`
- Move-to-recycle-bin support
- Move-to-target-directory support
- Failure diagnostics for missing paths, permission problems, and common Windows file-lock cases
- JSON scan reports and operation logs

## Project structure

```text
smart-disk-cleaner/
|- src/
|  |- ai_advisor.rs
|  |- analyzer.rs
|  |- cli.rs
|  |- dedup.rs
|  |- diagnostics.rs
|  |- executor.rs
|  |- lib.rs
|  |- main.rs
|  |- models.rs
|  |- reporter.rs
|  `- scanner.rs
`- Cargo.toml
```

## Quick start

Run an analysis after installing the Rust toolchain:

```bash
cargo run -- analyze "D:/Downloads" --output "./scan-report.json"
```

Enable AI summary generation:

```bash
set OPENAI_API_KEY=your_key
cargo run -- analyze "D:/Downloads" --ai-model "gpt-4.1-mini"
```

Simulate recycling files from the generated report:

```bash
cargo run -- execute --report "./scan-report.json" --mode recycle --paths "D:/Downloads/old.zip"
```

Real filesystem changes require `--apply`. Without it, execution stays in `dry-run` mode.

Move files into an archive directory:

```bash
cargo run -- execute --report "./scan-report.json" --mode move --target-dir "E:/Archive" --paths "D:/Downloads/big.iso" --apply
```

Run a standalone diagnosis before execution:

```bash
cargo run -- diagnose "D:/Downloads/old.zip" --operation recycle
```

## Design choices

- Safe by default: execution stays in `dry-run` unless `--apply` is provided
- Human-in-the-loop: the user chooses what to execute from the report
- AI for explanation: local rules generate suggestions, AI adds a natural-language summary
- Modular structure: scanning, analysis, dedup, advice, execution, and reporting are isolated
- Diagnostics are explainable: execution failures are mapped to structured codes, summaries, suggestions, and likely related applications

## Current gaps

- No GUI yet
- AI does not generate a structured execution plan yet

## Recommended next steps

1. Extend `diagnostics` with Windows-specific process enumeration to identify the exact locking process
2. Introduce `config.toml` for thresholds and AI configuration
3. Add sample datasets and integration tests
4. Prepare architecture diagrams, demo scripts, and benchmark cases for course presentation
