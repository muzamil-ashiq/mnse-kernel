# MNSE — Machine Native State Engine (v2.0)

[![Tests](https://img.shields.io/badge/tests-100%25%20passing-brightgreen.svg)]()
[![Rust](https://img.shields.io/badge/rust-1.80%2B-blue.svg)]()
[![License](https://img.shields.io/badge/license-MIT-green.svg)]()

MNSE is a low-latency, privacy-first **digital nervous system** that captures operational context across your operating system, editors, shells, and codebases in real time. It unifies sensory telemetry into an operational knowledge graph and provides sub-millisecond retrieval for developers and AI coding agents.

---

## Architecture at a Glance

MNSE operates across four unified layers:
1. **Sensors (Adapters):** Zero-overhead sensory adapters for Hyprland (`mnse-os-adapter`), Filesystem & Git (`mnse-fs-adapter`), and Shells (`mnse-shell-adapter`).
2. **Fast IPC (`mnse-slotbus`):** Lockless ring buffer in POSIX shared memory (`/dev/shm/mnse_slotbus`) enabling zero-copy, sub-microsecond event ingestion.
3. **Kernel Daemon (`mnse-kernel`):** Central nervous system running SQLite persistence (`mnse-ledger`), Hawkes process self-excitation and PageRank graph diffusion (`mnse-graph`), Kalman rate filters, and zero-trust Gatekeeper admission.
4. **Interface (`mnse-k`):** Command-line tool and K Language compiler supporting real-time diagnostics (`k state`), filtering pipelines, `.k` automation scripts, and an interactive REPL.

---

## Workspace Crates

- [`mnse-types`](mnse-types): Shared event schemas, `ContextSnapshot`, Toon Option 3 serializers, and reproducible trace IDs (`[cmd:hash]`, `[err:hash]`).
- [`mnse-slotbus`](mnse-slotbus): High-performance lockless ring buffer in POSIX shared memory.
- [`mnse-ledger`](mnse-ledger): SQLite event storage with FTS indexing, daily statistics, and historical queries.
- [`mnse-graph`](mnse-graph): Causal knowledge graph with Hawkes temporal self-excitation and PageRank diffusion.
- [`mnse-kernel`](mnse-kernel): Async daemon providing Axum HTTP (`127.0.0.1:7780`), UNIX socket (`~/.mnse/kernel.sock`), and policy gatekeeper.
- [`mnse-sdk`](mnse-sdk): Rust SDK providing `AdaptiveClient` (shared memory ring buffer with automatic UNIX socket fallback) and HTTP clients.
- [`mnse-k`](mnse-k): K Language compiler, pipeline planner, `.k` runner, and interactive CLI shell.
- [`adapters/os`](adapters/os): Wayland Hyprland compositor sensor.
- [`adapters/fs`](adapters/fs): Inotify filesystem and upward parent `.git/HEAD` sensor.
- [`adapters/shell`](adapters/shell): Hook generator and telemetry recorder for Fish, Zsh, and Bash.

---

## Quickstart

### Run State Diagnostics
```bash
# Print current window, git branch, active file, recent commands, and errors
k state

# View daemon health and active sensors
k care

# List all loaded adapters
k adapter list
```

### Pipelines & Script Execution
```bash
# Filter recent failed commands
k 'recent.commands | where $it.exit_code == 101 | select command, exit_code'

# Execute a .k workflow script
k examples/failed_commands.k
```

### Interactive REPL
```bash
k
```

---

## Documentation

- [System Architecture](docs/ARCHITECTURE.md)
- [CLI & Language Reference](docs/CLI_REFERENCE.md)
- [Sensory Adapters Guide](docs/ADAPTERS.md)
- [Systemd User Automation](docs/SYSTEMD.md)
- [Changelog](CHANGELOG.md)
