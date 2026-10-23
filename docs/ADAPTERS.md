# MNSE Sensory Adapters

Sensory adapters observe environment state changes and publish typed events to the kernel via shared memory (`SlotBus`) and the local UNIX socket (`~/.mnse/kernel.sock`).

---

## 1. Hyprland OS Adapter (`mnse-os-adapter`)

- **Binary:** `~/.local/bin/mnse-os-adapter`
- **Source:** `adapters/os/`
- **Systemd Unit:** `mnse-os-adapter.service`
- **Implementation:**
  - Queries Hyprland state via `hyprctl activewindow -j` and `hyprctl workspaces -j` with a 500ms polling interval.
  - Emits `EventType::WindowFocused` whenever the active window address changes, capturing app class, window title, address ID, and active workspace.
  - Emits `EventType::WorkspaceChanged` whenever the total workspace count changes.

---

## 2. Filesystem & Git Adapter (`mnse-fs-adapter`)

- **Binary:** `~/.local/bin/mnse-fs-adapter`
- **Source:** `adapters/fs/`
- **Systemd Unit:** `mnse-fs-adapter.service`
- **Default Argument:** `--path %h/Applications`
- **Implementation:**
  - Watches directory trees recursively using the `notify` inotify watcher.
  - Debounces identical path events within 300ms (`--debounce-ms`).
  - Filters out noise directories (`target/`, `.git/`, `node_modules/`, `.cache/`, etc.).
  - Detects source code extensions (`.rs`, `.py`, `.ts`, `.js`, `.go`, `.k`, `.c`, `.cpp`, `.md`, etc.) and determines language.
  - Dynamically searches upward parent directories for `.git/HEAD` to resolve the current Git branch of any sub-project.
  - Emits `EventType::FileSwitched` with `file_path`, `language`, and `git_branch`.

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
  - Emits `EventType::ProcessExecuted` on command completion.
  - When `exit_code != 0`, emits a causally linked `EventType::ErrorDetected` with error pattern classification (`RustCompile`, `PythonTraceback`, etc.) referencing `parent_event_id`.
  - Emits `EventType::DirectoryChanged` on directory switches.
  - **Offline Buffering:** If the kernel socket is offline, events are buffered in memory and flushed automatically once connectivity is restored.
  - Uses asynchronous background dispatch (`>/dev/null 2>&1 &`) in shell hooks to prevent terminal latency.

### Generating Hooks
To generate hooks for any shell:
```bash
mnse-shell-adapter install-hooks --shell fish
mnse-shell-adapter install-hooks --shell zsh
mnse-shell-adapter install-hooks --shell bash
```
