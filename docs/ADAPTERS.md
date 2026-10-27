# MNSE Sensory Adapters

Sensory adapters observe environment state changes and publish typed events to the kernel via lockless shared memory (`SlotBus`, `/dev/shm/mnse_slotbus`, <500ns latency) with automatic fallback to the local UNIX socket (`~/.mnse/kernel.sock`).

---

## 1. Hyprland OS Adapter (`mnse-os-adapter`)

- **Binary:** `~/.local/bin/mnse-os-adapter`
- **Source:** `adapters/os/`
- **Systemd Unit:** `mnse-os-adapter.service`
- **Implementation:**
  - Connects natively to Hyprland's IPC socket2 (`$XDG_RUNTIME_DIR/hypr/*/.socket2.sock`) for event-driven push telemetry (<100µs latency, 0.00% idle CPU).
  - One-time initial seeding via `hyprctl activewindow -j` and `hyprctl workspaces -j` upon adapter startup.
  - Emits `EventType::WindowFocused` on `activewindow` events, capturing app class, window title, address ID, and active workspace.
  - Emits `EventType::WorkspaceChanged` on `workspace`, `createworkspace`, and `destroyworkspace` events.
  - Emits directly to `SlotBusClient` shared memory with automatic `SocketClient` fallback.

---

## 2. Filesystem & Git Adapter (`mnse-fs-adapter`)

- **Binary:** `~/.local/bin/mnse-fs-adapter`
- **Source:** `adapters/fs/`
- **Systemd Unit:** `mnse-fs-adapter.service`
- **Default Argument:** `--path %h/Applications`
- **Implementation:**
  - Watches directory trees recursively using the `notify` inotify watcher with an eager drain loop (`try_recv` batching) to prevent event lag under heavy build bursts.
  - Debounces identical path events within 300ms (`--debounce-ms`).
  - Filters out noise directories (`target/`, `.git/`, `node_modules/`, `.cache/`, `dist/`, `tmp/`, etc.).
  - Detects source code extensions (`.rs`, `.py`, `.ts`, `.js`, `.go`, `.k`, `.c`, `.cpp`, `.md`, etc.) and determines language.
  - Dynamically searches upward parent directories for `.git/HEAD`, supporting standard `.git` directories, git worktree pointers (`gitdir: ...`), and detached HEAD commit hashes.
  - Emits `EventType::FileSwitched` with `file_path`, `language`, and `git_branch` via `AdaptiveClient`.

---

## 3. Shell Telemetry Adapter (`mnse-shell-adapter`)

- **Binary:** `~/.local/bin/mnse-shell-adapter`
- **Source:** `adapters/shell/`
- **Hook Configuration:** `~/.config/fish/conf.d/mnse.fish`
- **Commands:**
  - `record-exec`: Captures command string, exit code, duration in milliseconds, working directory, and stdout/stderr tails.
  - `record-cd`: Captures directory transitions.
  - `install-hooks`: Generates shell hook scripts for Fish, Zsh, or Bash.
- **Event Emission:**
  - Emits directly to `/dev/shm/mnse_slotbus` via `AdaptiveClient` in <500 nanoseconds with automatic socket fallback.
  - Emits `EventType::ProcessExecuted` on command completion.
  - When `exit_code != 0`, emits a causally linked `EventType::ErrorDetected` with error pattern classification (`RustCompile`, `PythonTraceback`, etc.) referencing `parent_event_id`.
  - Emits `EventType::DirectoryChanged` on directory switches.
  - **Offline Buffering:** If kernel IPC is offline, events are buffered in memory and flushed automatically once connectivity is restored.
  - Uses asynchronous background dispatch (`>/dev/null 2>&1 &`) in shell hooks to prevent terminal latency.

### Generating Hooks
To generate hooks for any shell:
```bash
mnse-shell-adapter install-hooks --shell fish
mnse-shell-adapter install-hooks --shell zsh
mnse-shell-adapter install-hooks --shell bash
```
