# MNSE Systemd User Automation

MNSE runs natively under `systemd --user`, allowing automatic startup on graphical login, automatic crash restarts, and unified lifecycle management.

---

## 1. Systemd Units Layout

All units are located in `~/.config/systemd/user/`:

| Unit File | Service | Description |
| :--- | :--- | :--- |
| `mnse.target` | Target | Master synchronization target for all MNSE services. |
| `mnse-kernel.service` | Kernel Daemon | Starts `~/.local/bin/mnse-kernel` with Axum HTTP, UNIX socket, and SlotBus. |
| `mnse-os-adapter.service` | OS Sensor | Starts `~/.local/bin/mnse-os-adapter` connected to Hyprland Wayland IPC. |
| `mnse-fs-adapter.service` | FS Sensor | Starts `~/.local/bin/mnse-fs-adapter --path %h/Applications`. |

---

## 2. Service Management Commands

### Check Overall Status
```bash
systemctl --user status mnse.target mnse-kernel mnse-os-adapter mnse-fs-adapter
```

### Restart All Services
```bash
systemctl --user restart mnse.target
```

### Stop All Services
```bash
systemctl --user stop mnse.target
```

### View Live Logs
```bash
# View kernel logs
journalctl --user -u mnse-kernel -f

# View filesystem adapter logs
journalctl --user -u mnse-fs-adapter -f

# View OS adapter logs
journalctl --user -u mnse-os-adapter -f
```

---

## 3. Deployment & Binary Updates

When rebuilding release binaries:
```bash
# Build all release binaries
cargo build --release --workspace

# Install to user binary path
cp target/release/k ~/.local/bin/
cp target/release/mnse-kernel ~/.local/bin/
cp target/release/mnse-os-adapter ~/.local/bin/
cp target/release/mnse-fs-adapter ~/.local/bin/
cp target/release/mnse-shell-adapter ~/.local/bin/

# Restart running daemons
systemctl --user restart mnse.target
```
