# Changelog

All notable changes to MNSE (Machine Native State Engine) are documented in this file.
The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [2.1.0] - 2026-10-28 (Sensory IPC Hardening & Architecture Realignment)

### Removed
- **Dead Rhai Scripting Engine:** Removed unreferenced `rhai_engine.rs`, trait `RuleEngine`, and `rhai` dependency tree from `mnse-kernel` and workspace dependencies.
- **Obsolete Robotics & Hardware Schemas:** Purged `FallDetected`, `ObstacleDetected`, `EmergencyStop`, `ActionPayload`, `LookDirection`, `WheelDirection`, and `EventType::ActionRequested`.
- **Phantom Sensory Schemas:** Purged unbacked variants `Vision` (`PersonVisible`, `PersonGone`), `Audio` (`VoiceDetected`, `VoiceSilence`, `WakeWordDetected`, `TranscriptionResult`), `Browser` (`UrlChanged`, `TabSwitched`, `TabUpdated`), and `Agent` (`AgentStarted`, `AgentEnded`).
- **Dead Snapshot Fields:** Removed phantom fields (`browser_*`, `person_*`, `voice_*`) from `ContextSnapshot` and TOON serializers.
- **Dummy Kernel Endpoint:** Removed mock `POST /evaluate` returning placeholder response.

### Changed & Improved
- **Direct Shared Memory Sensory Intake (`SlotBus`):**
  - Added `AdaptiveClient` to `mnse-sdk` providing nanosecond-latency lockless shared memory event emission (`/dev/shm/mnse_slotbus`, <500ns) with automatic fallback to UNIX domain socket.
  - Upgraded all three sensory adapters (`mnse-os-adapter`, `mnse-fs-adapter`, `mnse-shell-adapter`) to emit through `AdaptiveClient`.
- **Event-Driven Hyprland Sensor:** Realignment of `mnse-os-adapter` documentation to reflect native push telemetry via Hyprland IPC `socket2.sock` (<100µs latency, 0.00% idle CPU) with one-time state seeding.
- **Inotify Eager Drain Loop (`mnse-fs-adapter`):** Replaced single-event receiver tick with eager `rx.try_recv()` batch draining to prevent queue lag under high-frequency compilation bursts.
- **Git Worktree & Detached HEAD Support (`mnse-fs-adapter`):** Added upward discovery for `.git` file pointers (`gitdir: ...`) and detached HEAD commit hash resolution.
- **K CLI Script Dispatch Hardening:** Restricted file execution strictly to files ending with `.k`, preventing accidental execution of non-script files or syntax errors.

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
  - Live window focus, title, class, and workspace tracking via native Hyprland IPC `socket2.sock` push streaming.
- **K Language Engine v2.0 (`mnse-k`):**
  - Direct execution of `.k` script files (`k script.k`).
  - Object dot traversal on pipeline variables (`$it.exit_code`, `$it.command`).
  - Array projection in `select` stage (`select command, exit_code`).
  - Cross-type numerical comparison in `KernelOp::Binary` (`==`, `!=`, `>`, `<`, `>=`, `<=`) supporting both integer and float representations.
  - Interactive REPL shell mode (`k> `) with readline history.
- **Systemd User Automation:**
  - Automated user units under `~/.config/systemd/user/`: `mnse-kernel.service`, `mnse-os-adapter.service`, `mnse-fs-adapter.service`.
  - Unified user target `mnse.target` enabling seamless autostart on system login and unified lifecycle control (`systemctl --user restart mnse.target`).
- **Native CLI Binary (`k`):**
  - Standalone release binary installed at `~/.local/bin/k` providing high-performance query execution and interactive REPL mode.

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
