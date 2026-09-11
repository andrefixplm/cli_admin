# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

**CLI Admin** — a Windows-only Rust binary (`cli_admin`) that manages VMware Workstation VMs, shows a live system/process resource monitor (CPU, RAM, Disk I/O, GPU %, VRAM), and provides a safe process killer. It runs either as a scriptable CLI (`clap` subcommands) or as an interactive TUI (`ratatui` + `crossterm`) when invoked with no subcommand.

## Commands

```bash
cargo build                 # dev build
cargo build --release       # optimized build -> target/release/cli_admin.exe
cargo test                  # run tests/unit_tests.rs
cargo test test_process_shield   # run a single test by name
cargo run -- top -n 15 --sort vram   # run a CLI subcommand via cargo
.\target\release\cli_admin.exe       # launch the interactive TUI (no subcommand)
```

There is no lint/format CI configured in this repo — use `cargo fmt` / `cargo clippy` ad hoc if desired.

Tests live only in `tests/unit_tests.rs` and exercise pure logic (config defaults, `ProcessShield`, `ProcessFilter`, model math, `StartMode`/`StopMode` parsing) — no async/process/IO tests, since those require Windows APIs, `vmrun.exe`, or live processes.

## Architecture

Three independent subsystems live under `src/modules/`, each with its own `mod.rs` re-exporting a small public API. `src/main.rs` (CLI dispatch via `clap`) and `src/tui/app.rs` (`App` struct) are the two consumers — both wire up the same module APIs, so a change to a module's public surface must be checked against both call sites.

- **`modules::vmware`** — wraps VMware's `vmrun.exe` as an async subprocess (`client.rs`, one method per vmrun subcommand: start/stop/pause/unpause/suspend/reset/snapshot). `discovery.rs` does hybrid VM discovery: recursively walks `config.toml`'s `scan_directories` for `*.vmx` files, merges in explicitly configured `[[vmware.vms]]` aliases, then cross-references `vmrun -T ws list` to mark VMs Running vs Stopped. VM identifiers (CLI args, TUI selection) resolve through alias → friendly name → file stem, in that order (`VmDiscovery::resolve_vm_path`).

- **`modules::top`** — process/system metrics. `system.rs` (`SystemMetricsManager`) wraps `sysinfo` for CPU/RAM/per-process disk I/O and is refreshed synchronously on each poll. GPU/VRAM data comes from two independent, optional sources merged by PID: `gpu_pdh.rs` (`GpuPdhCollector`) spawns a **background tokio task** that shells out to `typeperf` every 2s to sample `GPU Engine %` and `GPU Process Memory\Local Usage` per-PID (parses PDH CSV output — brittle, silently returns empty maps on any format mismatch) and caches results behind an `Arc<RwLock<HashMap<pid, metrics>>>` so the TUI/CLI never blocks on it; `gpu_nvml.rs` (`NvidiaCollector`) does a one-shot `nvidia-smi` call for global adapter telemetry (name/temp/power/VRAM totals), silently returning `None` when no NVIDIA driver is present. Both are optional and gated by `config.toml`'s `metrics.enable_gpu_metrics` / `enable_nvml`.

- **`modules::process`** — process listing reuses `SystemMetricsManager::get_top_processes`. `shield.rs` (`ProcessShield`) is the safety layer: it builds a case-insensitive name set from `config.toml`'s `security.protected_processes` plus a hardcoded always-protected NT kernel list (csrss, smss, wininit, services, lsass, winlogon, PID 0/4), and both `.exe`/no-extension variants are indexed so either form matches. Any change to protected-process logic must preserve this shield — it's what stops the TUI's Process Killer tab from letting a user blue-screen the machine. `killer.rs` shells out to Windows `taskkill /PID <pid> [/F] [/T]`. `filter.rs` does fuzzy name/PID matching via `fuzzy-matcher`'s `SkimMatcherV2`.

**Config** (`src/config.rs`): `AppConfig::load()` resolves `config.toml` with precedence `--config` flag > `./config.toml` > `<exe_dir>/config.toml` > `%APPDATA%/cli_admin/config.toml`, writing a default template to `./config.toml` if none is found. All four config sections (`general`, `vmware`, `metrics`, `security`) use `#[serde(default = ...)]` per-field so a partial/hand-edited TOML always parses.

**TUI** (`src/tui/`): `mod.rs` runs the raw-mode/alternate-screen setup and the main draw/event loop (150ms tick); `event.rs` translates crossterm events, filtering to key-press only (Windows repeats key-release events); `app.rs`'s `App` holds all three tabs' state plus a `ModalState` enum (kill confirmation, snapshot creation, help); `ui.rs` renders it. The three tabs (`ActiveTab::Vmware/TopMetrics/ProcessManager`) each own their slice of `App` state and poll their respective module on `App::tick()`.

**Logging** (`src/logger.rs`): file-only by default via `tracing-appender`; stdout logging only turns on when both a CLI subcommand *and* `--debug` are passed (the TUI never logs to stdout, since that would corrupt the alternate-screen render).
