# Changelog

All notable changes to MNSE (Machine Native State Engine) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [2.0.0] - 2026-10-19 (Modern Digital Nervous System Architecture)

### Added
- **Zero-Copy Shared Memory IPC (`mnse-slotbus`):**
  - Lockless fixed-size ring buffer implementation in POSIX shared memory (`/dev/shm/mnse_slotbus`).
  - Capable of sustaining sub-microsecond event delivery from sensory adapters to the kernel without heap allocations or system call overhead.
- **Traceable Diagnostic Snapshot Format (`mnse-types`):**
  - Structured, lean Option 3 Diagnostic format with reproducible hexadecimal trace hashes: `[cmd:hash]` for commands and `[err:hash]` for errors.
  - Omission of noise (zero exit codes, trivial cursor events) from context serialization.
  - Full schema support for `ContextSnapshot`, `Event`, `EventType`, `EventPriority`, and `DiagnosticEvent`.
- **Knowledge Graph & Statistical Learning (`mnse-graph` & `mnse-kernel`):**
  - Hawkes process self-excitation and temporal decay for operational event clustering (`ml::hawkes`).
  - PageRank diffusion algorithm for root cause analysis of errors across command and file nodes (`ml::diffusion`).
  - Kalman filter for rate convergence and burst detection of system events (`ml::kalman`).
  - Multilingual compiler error classifier recognizing Rust, Python, Go, Node, and shell tracebacks (`ml::error_classifier`).
- **Dynamic Filesystem & Git Sensor (`adapters/fs`):**
  - Automatic upward directory traversal to detect `.git/HEAD` and current active branches across nested sub-repositories within `~/Applications`.
  - Continuous directory monitoring with debounce and active file focus reporting.
- **Cross-Shell Telemetry Adapter (`adapters/shell`):**
  - Shell telemetry integration with hook generators for Fish, Zsh, and Bash (`mnse-shell-adapter install-hooks`).
  - Offline JSONL buffering for fault tolerance during kernel restarts.
  - Auto-installed Fish integration via `~/.config/fish/conf.d/mnse.fish`.
- **Hyprland Wayland Compositor Sensor (`adapters/os`):**
  - Live window focus, title, class, and workspace tracking via Hyprland IPC socket.
- **K Language Engine v2.0 (`mnse-k`):**
  - Direct execution of `.k` script files (`k script.k`).
  - Object dot traversal on pipeline variables (`$it.exit_code`, `$it.command`).
  - Array projection in `select` stage (`select command, exit_code`).
  - Cross-type numerical comparison in `KernelOp::Binary` (`==`, `!=`, `>`, `<`, `>=`, `<=`) supporting both integer and float representations.
  - Interactive REPL shell mode (`k> `) with readline history.
- **Systemd User Automation:**
  - Automated user units under `~/.config/systemd/user/`: `mnse-kernel.service`, `mnse-os-adapter.service`, `mnse-fs-adapter.service`.
  - Unified user target `mnse.target` enabling seamless autostart on system login and unified lifecycle control (`systemctl --user restart mnse.target`).
- **Context-Aware Global Dispatcher (`~/.local/bin/k`):**
  - Transparent dual dispatching: routes robotics verbs (`forward`, `spin`, `arm`, etc.) or commands inside `gracemo-vira` to the GRaCEmo engine, while routing all operational queries to the high-performance `mnse-k` binary globally.

### Changed
- Complete workspace restructuring into 10 modular, zero-warning crates.
- Eliminated all emoji noise from CLI and REPL outputs in favor of lean diagnostic IDs.
- Hardened Gatekeeper admission policy to drop duplicate telemetry and filter secrets.

### Verification
- 100% test pass rate across all 10 workspace crates (`cargo test --workspace`).

---

## Pre-2.0 Evolution Summary (v0.0.1 – v0.0.53)

The earlier prototypes developed in the legacy `kernel chat` workspace established the foundational research leading to MNSE 2.0:
- **v0.0.1 – v0.0.10:** Initial daemon event bus, early Hyprland and shell event listeners, and basic SQLite event logging.
- **v0.0.11 – v0.0.30:** Initial K-language parser experiments and preliminary knowledge graph schemas.
- **v0.0.31 – v0.0.53:** Pluggable module configs (`~/.mnse/modules/`), Rhai rule evaluation, CLI execution wrappers, and local library manifest testing.

*All unverified legacy marketing claims (such as theoretical compiler compression percentages) were retired in v2.0 in favor of measurable, verifiable telemetry and lean Option 3 Diagnostic formatting.*
