# MNSE 2.0 System Architecture

## 1. Overview

MNSE (Machine Native State Engine) is a local, real-time digital nervous system for developers. It continuously captures low-level operating system events (active windows, active files, git branches, shell commands, compilation errors), builds an operational knowledge graph, and provides sub-millisecond context retrieval for human developers and AI coding agents.

---

## 2. Workspace Crate Layout

MNSE v2.0 is partitioned into 10 modular, zero-warning Rust crates:

| Crate | Directory | Purpose |
| :--- | :--- | :--- |
| `mnse-types` | `mnse-types/` | Canonical event schemas, `ContextSnapshot`, Toon diagnostic serializers, and trace IDs (`[cmd:hash]`, `[err:hash]`). |
| `mnse-slotbus` | `mnse-slotbus/` | Lockless fixed-size ring buffer implementation in POSIX shared memory (`/dev/shm/mnse_slotbus`). |
| `mnse-ledger` | `mnse-ledger/` | SQLite event store (`~/.mnse/kernel.db`) with append-only persistence, FTS search, and time-range filtering. |
| `mnse-graph` | `mnse-graph/` | Operational knowledge graph (`~/.mnse/mnse_graph.db`) with Hawkes process self-excitation and PageRank diffusion. |
| `mnse-kernel` | `mnse-kernel/` | Core daemon: Axum HTTP (`127.0.0.1:7780`), UNIX socket (`~/.mnse/kernel.sock`), Gatekeeper admission, Kalman burst filtering, and Rhai policy engine. |
| `mnse-sdk` | `mnse-sdk/` | Client SDK providing synchronous and asynchronous IPC connection bindings to the kernel. |
| `mnse-k` | `mnse-k/` | K Language compiler, interpreter, AST planner, pipeline engine, `.k` file runner, and interactive REPL. |
| `mnse-os-adapter` | `adapters/os/` | Hyprland Wayland compositor sensor tracking active window focus, titles, and workspace switches. |
| `adapters/fs` | `adapters/fs/` | Filesystem sensor watching `~/Applications/`, tracking file modifications, debounce, and upward parent `.git/HEAD` resolution. |
| `adapters/shell` | `adapters/shell/` | Shell telemetry adapter and hook generator for Fish, Zsh, and Bash with offline JSONL fallback. |

---

## 3. Communication & Data Flow

```text
+-------------------+      +-------------------+      +-------------------+
|  Hyprland Socket  |      |  Inotify / FS     |      |  Fish Shell Hook  |
|  (mnse-os-adapter)|      |  (mnse-fs-adapter)|      |  (Shell Adapter)  |
+---------+---------+      +---------+---------+      +---------+---------+
          |                          |                          |
          +--------------------------+--------------------------+
                                     |
                                     v
                       +---------------------------+
                       |  POSIX Shared Memory      |
                       |  /dev/shm/mnse_slotbus    |
                       |  (Lockless Ring Buffer)   |
                       +-------------+-------------+
                                     |
                                     v
                       +---------------------------+
                       |  mnse-kernel Daemon       |
                       |  - Gatekeeper & Redaction |
                       |  - Kalman Burst Filter    |
                       |  - Hawkes Excitation      |
                       +------+-------------+------+
                              |             |
                 +------------+             +------------+
                 v                                       v
    +------------------------+              +------------------------+
    | SQLite Ledger          |              | SQLite Knowledge Graph |
    | (~/.mnse/kernel.db)    |              | (~/.mnse/mnse_graph.db)|
    +------------------------+              +------------------------+
                 ^                                       ^
                 +-------------------+-------------------+
                                     |
                                     v
                       +---------------------------+
                       |  K Language CLI & REPL    |
                       |  (k state / k script.k)   |
                       +---------------------------+
```

### Zero-Copy Shared Memory (`mnse-slotbus`)
Sensory adapters publish events directly into fixed 512-byte slots in `/dev/shm/mnse_slotbus`. This eliminates heap allocations and kernel context switches, allowing continuous sensory streaming without impacting CPU or memory performance.

### Statistical & Graph Inference
- **Hawkes Process (`ml::hawkes`):** Models causal self-excitation between sequential events (e.g., editing a file triggers a compiler invocation, which excites an error event).
- **PageRank Diffusion (`ml::diffusion`):** Propagates error impact backward across command and file nodes to isolate root causes.
- **Kalman Filter (`ml::kalman`):** Smooths incoming event rates to detect sudden bursts of activity or systemic failure loops.

---

## 4. Privacy & Security Invariants

1. **No External Network Egress:** MNSE binds strictly to `127.0.0.1:7780` and local UNIX domain sockets.
2. **Deterministic Redaction:** Sensitive variables (API keys, authorization headers, environment secrets) are scrubbed before admission into the event bus or ledger.
3. **No Line-Drawing Boxes or Emojis in Terminal:** Terminal outputs adhere to the lean Option 3 Diagnostic format with traceable hexadecimal IDs.
