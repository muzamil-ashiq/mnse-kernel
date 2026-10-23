# MNSE K-CLI & Language Reference

`k` is the primary command-line tool and operational language runtime for the MNSE Kernel.

---

## 1. Quickstart Commands

| Command | Description |
| :--- | :--- |
| `k state` | Display real-time context snapshot (focus, active branch, commands, errors, telemetry). |
| `k care` | Display kernel daemon health, active adapters, and privacy status. |
| `k adapter list` | List status, observed event count, and timestamps for all registered adapters. |
| `k adapter on <name>` | Enable an adapter (e.g., `k adapter on fs`). |
| `k adapter off <name>` | Disable an adapter (e.g., `k adapter off fs`). |
| `k help` | Display built-in help and available query targets. |

---

## 2. State & Entity Queries

```bash
# Query recent command executions
k recent.commands

# Query classified compilation and runtime errors
k recent.errors

# Query recently touched source files
k recent.files

# History and aggregate telemetry
k 'history today'
k 'summary today'

# Root cause analysis of latest error
k 'causes of latest.error'
```

---

## 3. Pipelines & Filtering

The K language supports UNIX-style pipelines with `$it` binding:

### Filtering with `where`
```bash
# Filter recent commands to only those that failed with exit code 101
k 'recent.commands | where $it.exit_code == 101'
```

### Field Projection with `select`
```bash
# Filter failures and extract only the command and exit code
k 'recent.commands | where $it.exit_code == 101 | select command, exit_code'
```

*Note: In interactive shells like Fish or Bash, wrap pipeline queries in single quotes (`'...'`) to prevent the shell from expanding `$it`.*

---

## 4. Script Files (`.k`)

You can save complex pipelines and automation queries into `.k` script files:

Create `failed_commands.k`:
```k
# Find all build failures and project command names
recent.commands | where $it.exit_code == 101 | select command, exit_code
```

Run the script:
```bash
k failed_commands.k
```

Output:
```json
[
  {
    "command": "cargo build --release",
    "exit_code": 101
  },
  {
    "command": "cargo test --bin missing",
    "exit_code": 101
  }
]
```

---

## 5. Interactive REPL Mode

To launch the interactive K-shell with persistent readline history:

```bash
k
```

Inside the REPL, commands run directly without quotes or `k` prefixes:
```text
K Language Shell v2.0.0 (MNSE Kernel)
Type 'help', 'care', 'current.state', or 'exit' to quit.

k> state
k> recent.errors
k> recent.commands | where $it.exit_code == 101
k> exit
```

---

## 6. Global Dispatcher Behavior

The launcher script at `~/.local/bin/k` provides intelligent context-aware dispatching:
1. **Robotics & GRaCEmo Verbs:** If the command is run inside `gracemo-vira` or specifies a robotics verb (`forward`, `backward`, `spin`, `stop`, `arm`, `goto`, `speak`, `scan`, `hud`, `view`, `graph`, `verify`, `reset`, `center`, `reflex`, `explore`, `patrol`, `memory`), execution delegates to GRaCEmo.
2. **Kernel Operations:** Everywhere else, execution routes to the native release binary at `/home/mab/Applications/my-project/target/release/k`.
