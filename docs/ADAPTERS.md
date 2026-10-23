# MNSE Sensory Adapters

Sensory adapters observe environment state changes and publish typed events to the kernel via shared memory (`SlotBus`) and the local UNIX socket.

---

## 1. Hyprland OS Adapter (`mnse-os-adapter`)

- **Binary:** `~/.local/bin/mnse-os-adapter`
- **Source:** `adapters/os/`
- **Systemd Unit:** `mnse-os-adapter.service`
- **Capabilities:**
  - Connects to Hyprland's IPC socket (`$XDG_RUNTIME_DIR/hypr/$HYPRLAND_INSTANCE_SIGNATURE/.socket2.sock`).
  - Emits `AppFocusChanged` on window focus transitions.
  - Emits `WorkspaceSwitched` on active workspace changes.
  - Automatically captures active window titles, classes, and process IDs.

---

## 2. Filesystem & Git Adapter (`mnse-fs-adapter`)

- **Binary:** `~/.local/bin/mnse-fs-adapter`
- **Source:** `adapters/fs/`
- **Systemd Unit:** `mnse-fs-adapter.service`
- **Parameters:** Defaults to watching `~/Applications/`
- **Capabilities:**
  - Inotify directory monitoring with event debouncing (150ms).
  - Emits `FileChanged` and `FileCreated` events.
  - Performs dynamic upward parent directory traversal to locate `.git/HEAD` and resolve the active branch for any nested sub-project (e.g. `my-project`, `gracemo-vira`).

---

## 3. Shell Telemetry Adapter (`mnse-shell-adapter`)

- **Binary:** `~/.local/bin/mnse-shell-adapter`
- **Source:** `adapters/shell/`
- **Hooks Installed:** `~/.config/fish/conf.d/mnse.fish`
- **Capabilities:**
  - Hooks directly into shell post-execution (`fish_postexec`, `precmd`, `DEBUG`).
  - Emits `CommandExecuted` events containing:
    - Executed command string.
    - Exit code (e.g., 0 for success, 101 for Rust compile failure).
    - Execution duration in milliseconds.
    - Working directory (`cwd`).
    - Shell session ID.
  - Emits `DirectoryChanged` (`cd`) events on working directory transitions.
  - Uses non-blocking background dispatch (`>/dev/null 2>&1 &`) to guarantee 0ms terminal input lag.

### Installing Hooks Manually
To generate hook scripts for a specific shell:
```bash
mnse-shell-adapter install-hooks --shell fish
mnse-shell-adapter install-hooks --shell zsh
mnse-shell-adapter install-hooks --shell bash
```
