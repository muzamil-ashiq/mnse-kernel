# MNSE — Machine Native State Engine

> *"Linux Kernel is to Hardware as MNSE Kernel is to Human Intent & Context"*

A local-first **Context Operating Layer** — a lightweight Rust daemon that observes everything a developer does (shell, OS, system), stores it as a causal knowledge graph, and exposes it through a typed domain-specific language called **K**.

**Version:** v2.0.0 (Clean Rebuild)  
**Author:** Muzamil  
**License:** MIT

---

## Quick Start

```bash
# Start the kernel daemon
mnse-kernel

# Query your context
k "current.app"         # what app is focused?
k "recent errors"       # last shell errors
k "history today"       # what did I do today?

# Interactive shell
k
```

---

## What It Does

MNSE runs 24/7 in the background. It:
1. **Observes** — adapters watch your OS, shell, and system
2. **Stores** — events go into an append-only SQLite ledger
3. **Reasons** — causal graph links commands to errors to files
4. **Serves** — K language lets you query everything instantly

---

## Architecture

```
User Actions (keyboard, focus, commands)
    ↓
Adapters (OS, Shell, System)
    ↓
MNSE Kernel (EventBus → Ledger + Graph + Snapshot)
    ↓
K Language (query, behaviors, automation)
```

See [docs/architecture.md](docs/architecture.md) for full detail.
