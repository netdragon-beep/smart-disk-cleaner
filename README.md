# smart-disk-cleaner

`smart-disk-cleaner` is a **local storage governance assistant for Windows power users**.  
It does not treat disk cleanup as a vague “system optimization” problem. Instead, it focuses on:

- finding space-heavy content
- explaining why something is safe or risky to change
- guiding cleanup and migration with `dry-run`
- recording actions and keeping rollback paths visible

## Positioning

This project is now oriented around three product lines:

1. `Space Discovery`
   Scan directories and identify large files, temporary content, duplicate files, app data, and typical disk pressure scenarios.

2. `Risk Explanation`
   Convert raw scan output into governance suggestions with scenario labels, risk levels, pre-check steps, and post-check validation steps.

3. `Safe Execution`
   Support cleanup, migration, registry startup governance, operation history, and rollback-oriented workflows.

## Typical scenarios

- WeChat / QQ / enterprise chat data occupying too much space
- Download folders with large archives, installers, and videos
- Cloud-sync folders consuming local disk unexpectedly
- Large application directories and leftovers that should be reviewed before removal
- Startup registry entries that can be backed up, previewed, disabled, and rolled back safely

## Safety principles

- Safe by default: destructive actions should support `dry-run`
- Explain before execute: every high-value suggestion should tell the user why it exists
- No blind bulk registry cleanup
- No “one-click acceleration” narrative
- Keep rollback visible in both migration and registry governance flows

## Architecture overview

- `crates/core`
  Core scan, analysis, governance suggestion, migration planning, and diagnostics logic
- `src-tauri`
  Tauri command layer for scan, cleanup, migration, process diagnosis, registry governance, and config/state
- `ui`
  Vue + Naive UI desktop frontend organized around discovery, explanation, and safe action

## Local rules vs AI

The project uses **local rules first** and AI as an optional explanation layer.

- local rules generate structured governance suggestions
- AI can refine explanation, not replace safety checks
- when remote AI fails, the app should fall back to local rules instead of returning an empty result

## Current implemented direction

- scan reports now expose governance suggestions in addition to raw cleanup suggestions
- operation history can distinguish file cleanup, migration, registry change, and registry rollback records
- registry governance supports:
  - startup entry listing
  - path issue detection
  - explicit backup export
  - change preview
  - single-entry apply
  - single-entry rollback

## Quick start

Install dependencies and run the desktop app:

```bash
npm --prefix ui install
cargo tauri dev
```

Build checks:

```bash
cargo check -p smart-disk-cleaner-gui
npm --prefix ui run build
```

## Demo flow

For an open-source showcase or graduation demo, the recommended flow is:

1. Scan a user directory or disk root
2. Show governance suggestions for high-impact scenarios
3. Open process diagnosis if a target path is occupied
4. Run a `dry-run` cleanup or migration step
5. Execute a real action
6. Open history and explain rollback references
7. Show registry startup governance preview and rollback

## What this project intentionally does not claim

- it is not a generic “system booster”
- it does not perform blind registry junk cleanup
- it does not promise that all large files are safe to delete
- it does not replace user confirmation for high-risk actions
