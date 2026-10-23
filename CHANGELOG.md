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

## Legacy History (v0.0.1 – v0.0.53)

🔜 [0.0.53] — Online Registry
Blocked on: more users / public adoption What: github.com/mnse-k/libs with index.json + all manifests so k install <ns> works without local files. Why deferred: Only consumer is this machine; everything is already installed locally.
[0.0.53] - 2026-07-06 (Modular Architecture + k run + Config Cleanup) 🏗️🚀🧹
Added — Modular Kernel Architecture
trait ConfigProvider + trait RuleEngine: Swappable config/policy interfaces in loader/ module. TOML today, K later — zero kernel changes needed.
16 pluggable module configs: ~/.mnse/modules/ — each module self-contained with config.toml:
event_bus (capacities, priority, debounce) — identity (browser, editor, terminal, agent names)
dispatch (custom event handlers) — redaction (secrets, labels, privacy mode)
network (bind, CORS, auth, ratelimit) — storage (retention, history, validation)
scheduler (18 intervals, crash thresholds) — filters (5 unified lists)
causality (linking rules) — errors (17 tool patterns)
sensory (hear/see/feel mappings) — semantics (K-language event map)
views (3 context views) — privacy (snapshot redaction)
defaults (username, paths, backend) — adapters (expected list, source names, MCP servers)
kernel.toml: Single boot config — which modules to load, paths, hot-reload settings.
Built-in Rhai stdlib: 225L of embedded rules for classification, filtering, identity detection.
Added — K Language
k run <file.k> [args...]: Execute .k script files with named (key=val) and positional ($1, $2) argument bindings.
execute_program_with_context(): New public function accepting initial KContext with bindings.
3 new CLI executor libraries: gh, git, python3.
Changed — All 229 Hardcodes Eliminated
main.rs: 22 values → config (intervals, limits, filters, rehydration params). Loader integration.
event_bus.rs: Capacities/priority/debounce → config-driven with Rhai fallback.
browser.rs/helpers.rs/process_info.rs: Identity names → identity/config.toml.
event_handlers.rs: Duplicate TabSwitched removed, all functions accept optional loader.
inspector_state.rs/http_server.rs/policies.rs: Config-driven parameters.
lib.rs: Refactored execute_program_with_client to accept initial context.
All callers: Pass optional loader (None = hardcoded fallback, backward compatible).
Changed — Version Bump & Config Cleanup
Workspace version: 0.0.28 → 0.0.53 (all crates inherit via version.workspace = true).
MCP servers: Merged mcp_servers.json → adapters/config.toml [[mcp_servers]]. http_server reads TOML first, JSON fallback.
Registry: ~/.k/registry.json deleted. k install auto-discovers from ~/.k/libs/.
Kernel config: mnse.toml kernel-relevant settings merged into kernel.toml.
Build
0 errors, 0 warnings. 429 tests passing (421 mnse-k + 8 mnse-kernel).
[0.0.52] - 2026-07-06 (k test — Embedded Test Runner for K Libraries) 🧪
Added
k test <namespace> / k test <path/manifest.json>: New subcommand that runs tests embedded in K library manifests.
mnse-k/src/test.rs: Test runner module with run_library_tests(), format_test_results(), and helpers for expression building and result comparison.
Test schema: CommandDef.tests: Vec<TestCase> in manifest. Each test has name, args, optional positional, expect, and expect_mode (exact/contains/type).
Graceful offline handling: Tests that fail due to connection errors (MCP backend unavailable) are marked as skipped rather than failed.
Test cases in date manifest: Three passing tests for date.now, date.utc, and date.format.
Changed
manifest.rs: Added tests field to CommandDef, TestCase struct with ExpectMode enum.
lib.rs: Added pub mod test.
tools/k/src/main.rs: Wired up k test subcommand dispatch and updated help text.
module.rs: Fixed CommandDef initializers to include tests field.
[0.0.51] - 2026-07-06 (CLI Executor — Any CLI Tool Becomes a K Library) 🚀
Added
CommandMapping::CliCommand: New maps_to variant for manifest commands. Uses cli_args (static flags) and arg_map (K arg → CLI flag mapping, empty string = positional). Backend must be {"type": "shell", "command": "..."}.
build_cli_args(): New helper in module.rs that maps resolved K arguments to CLI flag strings. Handles booleans (flag-only when true, skipped when false), strings/numbers (flag + value), and positional args.
Stdin piping in ShellCall: When __pipe or $it is set, the pipe value is forwarded as stdin to the CLI process. Enables chains like curl.get ... | jq.query ".name".
Example CLI libraries: curl, jq, and date — all available via k install <ns> from the local registry.
Changed
module.rs: resolve_command now handles CliCommand → ResolvedCommand::ShellCall with backend validation.
planner.rs: Added match arm for ResolvedCommand::ShellCall → KernelOp::ShellCall and SdkCall → helpful error.
runtime.rs: ShellCall now forwards piped stdin data to child processes. Uses spawn() + stdin write instead of .output() for pipe support.
tools/k/src/main.rs: CLI help text updated with new CLI-based libraries (curl, date, jq).
Library Authors
Any CLI tool can now become a K library with a simple manifest:
{
  "namespace": "mytool",
  "backend": { "type": "shell", "command": "mytool" },
  "commands": {
    "cmd": {
      "maps_to": { "cli_args": [], "arg_map": { "arg": "--flag" } }
    }
  }
}
[0.0.50] - 2026-07-06 (k install, Library Package Manager) 📦
Added
k install command: Install K libraries from URLs, local file paths, or namespace names (with registry lookup). Downloads manifest.json, validates it, writes to ~/.k/libs/<namespace>/, and auto-imports. Supports overwrite prompt for existing libraries.
manifest::library_path(): New public function that returns the ~/.k/libs/<namespace>/ directory path, creating it if needed.
Default registry: k install <namespace> looks up https://raw.githubusercontent.com/mnse-k/libs/main/index.json to resolve namespace to manifest URL.
Changed
tools/k/src/main.rs: Added install subcommand dispatch with help text.
tools/k/src/install.rs: New module with handle_install() and print_install_help().
mnse-k/src/manifest.rs: Refactored load_library path logic into reusable library_path() function.
[0.0.49] - 2026-07-06 (Case-Insensitive Commands, Keyword-Safe Named Args, Pipe Type Coercion) 🔧🔄
Added
Case-insensitive command resolution: Registry keys are lowercased at import time so commands like calculator.isPrime resolve regardless of casing. Manifest scans and namespace registry fallback also use case-insensitive matching.
Keyword-safe named arguments: Named args whose names coincide with K keywords (limit, select, where, etc.) are now parsed correctly via Token::Keyword support in parse_any_arg. Both keyword-named args and keyword values work.
Pipeline $it type coercion: When a piped value is passed to a module command, the runtime applies the target command's manifest type definitions (Int, Float, Bool, Duration, Date) to coerce the value. This enables chains like calculator.add a=42 b=58 | calculator.multiply a=$it b=2.
resolve_module_call_args(): New function in module.rs that looks up a command's CompiledCommand by key and applies resolve_arg type coercion to each argument.
NamespaceRegistry::resolve_ignore_case(): Case-insensitive key lookup for the dynamic namespace registry.
ShellCall handler: Added KernelOp::ShellCall execution via tokio::process::Command in run_op.
Changed
mnse-k/src/module.rs: Registry keys lowered at import (import_library line 524). Added resolve_module_call_args() for post-$it-resolution type coercion. Added case-insensitive command_exists/resolve_command tests.
mnse-k/src/planner.rs: KernelOp::ModuleCall now carries key: Option<String> so the runtime can look up type definitions. plan_expr passes Some(op_key) when constructing module calls.
mnse-k/src/runtime.rs: ModuleCall handler now calls resolve_module_call_args after resolve_value to coerce pipe-sourced values against manifest types. Added ShellCall handler. Removed dead extract_number_from_text from resolve.rs.
mnse-k/src/parser.rs: is_arg_op_command manifest scan now checks keys().any(|k| k.to_lowercase() == cmd) instead of contains_key. Namespace registry fallback uses resolve_ignore_case.
mnse-k/src/namespace.rs: Added resolve_ignore_case() method.
docs/components/K_LANGUAGE.md: Added subsections on case-insensitive resolution, keyword-safe named args, pipe variable type coercion, and updated pipeline operators (|, &&, ||, ;).
.muzamil/k_commands.md: Updated with keyword-safe names and case-insensitive resolution notes.
Testing
11 new tests (386 total): case-insensitive namespace resolution, keyword-safe named args (name + value), pipeline keyword arg roundtrip, semicolon + keyword arg, module case-insensitive resolution. All 386 pass.
Verified end-to-end: calculator.nthRoot (camelCase), search.web limit=1 (keyword arg), 4-stage calculator pipeline with $it coercion.
Added
Generic Circuit Breaker: Introduced sys.circuit_breaker and sys.circuit_status to protect the system from cascading failures. Automatically opens the circuit after N consecutive failures within a reset window and resets on success.
Cancellation Tokens & Cooperative Shutdown: Added sys.cancel to gracefully shut down long-running operations and flush SQLite queues before exit. Added cancelled state to KContext and Cancel/CheckCancelled ops.
[0.0.47] - 2026-07-05 (Advanced K Language Primitives — Try/Catch, Records, Lists, Pipeline Filters) 🧩🔥
Added
Try/Catch Error Handling: Implemented robust runtime error catching syntax in the K language. Added try { ... } catch (err) { ... } blocks to cleanly handle failure states without crashing the pipeline.
Structured Data Literals: Added first-class support for Record ({ key: "value" }) and List ([1, 2, 3]) JSON literals directly in the K language syntax.
Pipeline Data Filtering: Introduced structured pipeline operators where (for filtering arrays of records by condition) and select (for mapping and projecting specific fields).
Core Engine Updates: Modified mnse-k/src/ast.rs, lexer.rs, parser.rs, planner.rs, runtime.rs, runtime/control.rs, and lib.rs to support the new AST nodes and execution paths for these language primitives.
[0.0.46] - 2026-07-05 (Capabilities/Permissions Enforcement, sys.deny, Manifest Types/Events, Auto-Import, Enhanced Help) 🛡️🧩📋
Added
PolicyEngine & Access Control: New PolicyEngine with grant_capability, revoke_capability, deny_capability, remove_denial, grant_permission, revoke_permission, set_enforcement for fine-grained command gating. Enforcement is disabled by default (permissive).
check_access() in resolve_command(): Every resolved K library command now evaluates capabilities + permissions + denials before execution.
sys.capabilities: Lists enforcement state, granted capabilities, denied capabilities, and granted permissions.
sys.grant "<capability>": Grants a capability at runtime.
sys.revoke "<capability>": Revokes a capability.
sys.deny "<capability>": Explicitly denies a capability (removes from granted + blocks future grants).
sys.remove_denial "<capability>": Removes an explicit denial, allowing future grants.
Manifest Types + Events schema: KLibraryManifest now supports optional types (exposed data structures) and events (emitted events) for self-describing adapter metadata.
Auto-import (interactive mode): Dotted commands like search.web "query" auto-import their namespace if it's available but not yet imported. Explicit import still required in .k script files.
Enhanced help output: help search.web now shows required capabilities, permissions, argument schema, and usage examples. help search shows commands + types + events.
Usage examples: CommandDef now supports an examples field; get_command_doc includes them in help output.
Companion script detection: import_library logs when a .k companion script is found in the library directory.
Changed
mnse-k/src/module.rs: Added PolicyEngine with denied set, deny_capability(), remove_denial(), get_command_retry(), get_library(), enhanced get_command_doc() (capabilities, permissions, examples, types, events).
mnse-k/src/planner.rs: Added SysDeny, SysRemoveDenial KernelOps + planner handling; auto-import on missing namespace; fallback_to_namespace_registry() extracted helper; wired up retry config via get_command_retry().
mnse-k/src/runtime.rs: Added handlers for SysDeny, SysRemoveDenial; updated SysCapabilities output to include denied list; updated Help namespace output to include types + events.
mnse-k/src/manifest.rs: Added TypeDef, TypeField, EventDef structs with Serialize; added examples field to CommandDef; added types and events to KLibraryManifest.
~/.k/libs/search/manifest.json and ~/.k/libs/browser/manifest.json: Updated with types, events, and command examples.
K_LANGUAGE.md: Updated with auto-import, sys.deny/sys.remove_denial, adapter metadata docs.
.muzamil/k_commands.md: Updated capabilities section with sys.deny/sys.remove_denial, auto-import, and enhanced help.
Testing
13 new tests (up from 42 to 45 total in mnse-k): added test_policy_deny_blocks_even_when_granted, test_policy_deny_grant_revoke, test_command_doc_includes_capabilities_examples, plus enhanced test_manifest_loading checks for types/events/examples.
All 91 workspace tests pass (88 + 3 new).
[0.0.45] - 2026-06-15 (Format Style Refinement — Minimal Removal, JSON Pruning & History Limiting) 🧹🎨
Removed
Minimal Output Format: Removed the minimal formatting style enum option, deleted tools/k/src/formatter/minimal.rs, and cleaned up all references inside the k CLI command-line options and REPL slash commands.
Changed
Refined JSON Output: Configured FormatStyle::Json to recursively prune all fields containing null values, empty maps, or empty arrays.
JSON Size Compression: Limited the historical last_commands_with_times list to the 5 most recent executed commands (instead of printing the full history up to 30), truncated clipboard history content previews to a maximum of 100 characters in the JSON state output, and corrected history traversal to retrieve the newest events first. This compresses environment snapshot payloads to only display populated context, making them extremely clean to read and highly token-efficient for LLM agent context window hydration.
[0.0.44] - 2026-06-15 (Behavior Engine Cooldown Optimization) ⚡⏱️
Changed
Configurable Rule Cooldowns: Replaced the hardcoded 60-second cooldown in mnse-k/src/runtime/behavior.rs with an environment variable lookup MNSE_RULE_COOLDOWN that defaults to 2 seconds, greatly improving the responsiveness of reactive background behaviors.
[0.0.43] - 2026-06-15 (Architectural Focus — Nvim & Search Removal) 🎯🧹
Removed
Neovim RPC Editor Control: Purged the entire mnse-k/src/runtime/nvim.rs module (454 lines), all nvim.* / editor.* K language commands (13 KernelOp variants), and the adapters/linux/nvim/ adapter directory (plugin.lua + README). The kernel's job is to sense the environment, not control editors — AI agents write files natively.
Search Namespace (search.code, search.files): Removed ripgrep and fd wrappers (mnse-k/src/runtime/search.rs, SearchCode + SearchFiles KernelOps, parser recognition). Any AI agent can invoke rg / fd directly — this was not kernel-unique functionality.
Changed
K Language is now a pure kernel query + reactive scripting language with 10 focused commands: current.state, error.latest, sense, watch, behavior, why, causes, audio.speak, let, if, and pipeline operators (|, &&, ||).
Documentation: Updated K_LANGUAGE.md, CLI.md, and .muzamil/k_commands.md to reflect the leaner command set.
OS-level nvim sensing preserved: The adapters/linux/os window title parser still detects which file is open in Neovim (passive sensing) — this is correct kernel behaviour.
[0.0.42] - 2026-06-15 (Manual Memory System & Vector DB Removal) 🧠🧹🗑️
Removed
Manual Note-Taking Memory System: Purged the k memory subcommand CLI handles, REST routes, client SDK methods, and database virtual tables (memory_vectors and nodes_fts).
Legacy associate Expression: Removed keyword, parsing, compilation, and evaluation logic for the associate causal K expression, aligning the runtime with automated history tracing.
MCP Memory Tool: Completely removed the "memory" tool registration and handlers from the Model Context Protocol server.
Changed
Documentation & Command Reference: Updated CHANGELOG.md, README.md, CLI.md, K_LANGUAGE.md, GRAPH_MEMORY.md, INTEGRATION.md, and the K commands cheat sheet.
Rust Warning Cleanup: Removed or deprecated unused helper functions, constants, and validation methods across tools/k/src/util.rs and mnse-mcp-server to keep the workspace compilation warning-free.
[0.0.41] - 2026-06-11 (Memory Optimization, Defensive Sanitation & Subsystem Branching) 🧠🔒📦
Added
Defensive Rehydration Sanitation: Auto-sanitizes legacy bloated terminal buffers (>50MB JSON data) in ledger.db on boot, preventing startup OOM crashes.
Subsystem Branching Layout: Introduced the BRANCHING.md contract and initialized 7 primary development branches: feat/kernel, feat/k-language, feat/graph, feat/ledger, feat/adapters, feat/tooling, and feat/devops.
Changed
Lazy Model Ingest (97% RAM Drop): Replaced eager startup model preloading with lazy-loading inside generate_embedding. The embedding model is loaded only on the first memory recall/search, dropping idle RAM from ~1.0 GB to ~22 MB.
Terminal Output Capping: Capped in-memory TerminalOutput logs to the last 100 lines and 500 characters max per line in event_handlers.rs to prevent context snapshot bloat.
Auth Simplification: Reverted multi-token auth back to single api_token validation, restoring agent self-identification (observed_by) and auto-attribution tagging (agent:<name>).
[0.0.40] - 2026-06-11 (Workspace Modularization & Formatter Cleanup) 📦🧹
Added
mnse-k Runtime Submodule Split: Deconstructed monolithic runtime.rs into focused modules:
runtime/state.rs (State, Hear, See, Feel)
runtime/shell.rs (ShellPipeline execution)
runtime/actions.rs (Bindings, sub/pub, actions, speech)
runtime/control.rs (Pipelines, conditionals, arithmetic)
runtime/graph_ops.rs (Graph associations, reasoning queries)
tools/k Formatter Submodule Split: Deconstructed monolithic formatter.rs into formatter/ module directory:
formatter/mod.rs (FormatStyle enum & dispatcher)
formatter/toon.rs (Toon render dispatcher)
formatter/minimal.rs (Minimal render logic)
formatter/snapshot.rs (Snapshot formatting & reality state translation)
formatter/reality.rs (Reality state and agent-only rendering)
formatter/tree.rs (Causality tree formatting)
Shared Helpers Cleanup: Eradicated duplicate rendering helpers in formatter.rs and converted submodules to import from tools/k/src/util.rs.
[0.0.39] - 2026-06-10 (List Search & Sort) 🔎📋
Added
k memory list --search <text>: Case-insensitive substring match on label and body. Applied after fetch so it correctly respects --limit. Fetches up to 250 nodes when active to ensure enough candidates.
k memory list --sort created|updated: Controls backend ORDER BY (via sort= query param through HTTP → list_memories) and the client-side merge sort. Default: updated (most-recently-touched first). created shows memories in insertion order — useful for timeline view.
Filter summary header on list now always shows active options including sort field.
[0.0.38] - 2026-06-10 (Recall Filtering & Stats Breakdown) 🔍📊
Added
k memory recall --status <val>: Filter semantic search results by task status, single or comma-separated OR-list ("todo,blocked"). Validated before any network requests.
k memory recall --search <text>: Case-insensitive substring match on label and body, applied client-side after the semantic search.
k memory recall score display: Shows Score: 0.847 per result when the backend returns a relevance score.
k memory stats status breakdown: Colour-coded per-status memory count (todo: 2 done: 5 blocked: 1). Gathered in the same properties scan as tags — zero extra DB queries.
Changed
k memory recall --links-depth 0 now explicitly disables links (consistent with list).
k memory recall --limit <non-integer> now errors cleanly instead of silently defaulting.
k memory recall filter header reformatted to consistent (tag: 'x', status: 'y', limit: N) style.
[0.0.37] - 2026-06-10 (CLI Polish & Multi-Status Filtering) ✨🔍
Added
Multi-value --status filtering (k memory list --status "todo,blocked"): CLI fans out one request per status value, merges results client-side, deduplicates by ID, sorts by updated_at desc, and applies --limit after merge.
validate_status_multi() in util.rs: validates each comma-separated token individually before any requests are made.
Changed
--links-depth 0 now explicitly disables link output (was previously silently ignored).
--limit <non-integer> now errors with a clear message instead of silently defaulting to no limit.
--status / --links-depth with no following value now errors with a usage hint instead of silently ignoring the flag.
docs/operation/CLI.md fully rewritten: all flags documented with types, defaults, and valid ranges. Added Section 5 (Output Formats / --format) and Section 6 (Interactive REPL). Multi-value --status examples added.
[0.0.36] - 2026-06-10 (Modular Formatter Restoration) 🎨🔧
Added
Unified Formatter Module (tools/k/src/formatter.rs): Reintroduced clean client-side formatting logic (toon and minimal formats) for K expression outputs. Encapsulated the layouts behind a type-safe FormatStyle enum and format_value function.
CLI Format Configuration (--format): Restored parsing of the command line --format <style> flag in main.rs to dynamically customize printed output layout.
REPL Output Formatting (/format): Restored the dynamic /format <toon|json|minimal> slash command in the interactive Rustyline REPL loop, allowing real-time layout updates.
Temporal State Formatting: Updated handle_state_at_cli (current.state --at) to reconstruct snapshots to reality states using formatter::convert_snapshot_to_reality_state and format the output according to the user's styling preference.
[0.0.35] - 2026-06-09 (K Memory Enhancements & Recursive Traversal) 🧠🌲🔌
Added
k memory read and k memory get: First-class direct memory lookup CLI commands that resolve exact node details by ID. Supports auto-prepending the memory: prefix on short hash inputs.
Recursive Graph Rendering (--links-depth N): Implemented recursive DFS traversal for relationship links on memory recall, list, and read/get. Limits traversal to a maximum depth of 3 and tracks visited nodes to prevent cycles in the graph.
Temporal Context Rehydration (k current.state --at): Added a CLI interceptor to query and render toon-formatted historical snapshots at relative/absolute times.
High-Signal History Filtering: Deduplicates consecutive focus poll logs and strips system load average noise from default k history feeds, adding a --verbose flag for raw logs.
[0.0.34] - 2026-06-06 (Hardening & Script Cleanup) 🧹⚙️
Added
SQLite WAL Mode & Integrity Checks: Hardened both mnse-ledger and mnse-graph SQLite databases to open in WAL (Write-Ahead Logging) mode and run integrity checks on startup to guarantee crash resilience.
POST /graph/associate Endpoint: Exposed a first-class HTTP API endpoint to create/strengthen relational edges between custom entities in the Knowledge Graph.
K Language Version Contract: Implemented #!k version=N stripping in the lexer to allow version-safe future-proofing.
eBPF Sliding Window Buffer: Implemented a 300s temporal ring buffer for raw eBPF OS events.
Changed
Compiler Warnings & Unused Code Cleanup: Marked or resolved all unused helpers, methods, and struct fields with #[allow(dead_code)] in mnse-k and mnse-graph crates, ensuring a 100% warning-free compilation.
K Script Examples: Updated all .k scripts (debugger.k, startup.k, companion.k, test.k, morning_standup.k, tell_screen.k) in the workspace to utilize syntactically valid K v2 commands, preventing runtime parsing exceptions.
[0.0.33] - 2026-06-03 (Production Hardening & K History Extensions) 🧠🔌💾
Added
Generic Python Adapter SDK:
Implemented AdapterClient in adapters/mnse_adapter_sdk.py to consolidate HTTP /emit and /snapshot logic.
Refactored vision_adapter.py and audio_adapter.py to import and utilize the shared AdapterClient class, removing duplicate request handling boilerplate.
K Language HistoryBefore Extension:
Added history before <expr> limit <expr> grammar to the K AST (KExpr::HistoryBefore) and parser rules.
Extended the K runtime to dynamically resolve target expressions to timestamps, with specific support for:
Event objects (Value::Object containing "timestamp").
Event arrays (Value::Array or { "events": [...] } objects like those returned by recent errors), picking the newest event's timestamp.
Symbolic event references (such as latest.error, latest.file, and latest.command) using indexed SQL queries mapping to event_type_name values.
Live system snapshots via current.
Multi-Tier SQLite Database Pruning:
Refactored ledger database pruning in mnse-ledger/src/db.rs to split retention thresholds.
Background noise events (CPU/resources, heartbeats, mouse cursor movements) are archived and deleted after 7 days.
Milestone events (errors, command executions, focus switches) are retained for a full 30 days.
Added transactional integrity and auto-triggered VACUUM queries when more than 10,000 rows are deleted to prevent lock poisoning.
Changed
Parser Fix: Replaced matching on Token::IntLit in parser.rs with Token::Number to correctly parse integer limits following limit keywords.
Documentation Overhaul: Updated CHANGELOG.md, ROADMAP.md, K_LANGUAGE.md, and STATUS_AUDIT_2026.md to reflect these production upgrades and log deferred concepts (SHM IPC, interactive REPL, LLM think).
[0.0.32] - 2026-06-01 (GPU-Accelerated Sensory Perception Stack) 👁️🎙️⚡
Added
GPU-Accelerated Local Perception Stack (feat: GPU-accelerated local perception stack): Converted the entire offline sensory system to run on local GPU/CUDA (NVIDIA GeForce GTX 1650, CUDA 12.1), keeping total VRAM usage under 600MB:
Speech-to-Text (voice_wake.py): Replaced Vosk-only logic with Faster-Whisper (base.en model) running on CUDA (compute_type="float16"). Captured raw PCM audio in-memory, transcribed with 100% offline accuracy, injected keystrokes using Wayland wtype, and instantly restored window focus in ~300ms.
Text-to-Speech (speak_context.py & action_dispatcher.py): Preloaded Kokoro-82M globally on CUDA at startup to eliminate latency. Generates neural, studio-quality speech tensors in <80ms and streams raw 16-bit little-endian PCM bytes directly to speakers via mpv.
High-Precision Face Detection (vision_adapter.py & enroll_face.py): Retired legacy CPU-bound Haar cascades in favor of the lightweight deep learning YuNet CNN face detector (82KB) running in ~5ms per frame. Passed detected face crops to a local LBPH Face Recognizer to identify mab vs stranger with 100% stability.
K Startup Greeting: Modified startup.k to sleep for 3s (allowing webcam warm-up) and check state.person == "mab" to greet the user exactly once at system boot.
Wake Words Cleanup: Cured false triggers by removing over-sensitive phonetic triggers ("integrity", "degrading", etc.) from the active wake word list in voice_wake.py.
Warning-Free Compilation: Resolved all active compiler warnings and lints across mnse-kernel, mnse-k, and adapters/agent, resulting in extremely fast clean builds (0.58s).
Changed
Adapter Setup Documentation: Overhauled ADAPTER_SETUP.md and docs/STATUS_AUDIT_2026.md to provide complete step-by-step setup, verification, and logs inspection for the new GPU/CNN adapters.
[0.0.31] - 2026-03-24 (K Language v2 & Cognitive Runtime) 🧠🐚
Added
K Language v2 (feat(k): K Language v2 — Unified Operational Cognition Language): Introduced the official domain-specific query and behavior language for the context engine:
Core Lexer & Parser: Built native lexer/parser/AST compiler (mnse-k crate) supporting structured statements, Let bindings, if/else conditionals, pipelines, behavior blocks, and selectors (.latest, .current, .active, .all, .recent).
Pipelining & Stdin Routing (|>): Implemented robust shell-style pipelining where the output of one cognition step (e.g., recent errors) flows seamlessly into the next (e.g., think).
Cognitive Senses: Added native keywords for physical and operational interaction (speak, look, follow, move, think, remember, forget, tell).
Behavior Engine: Enabled persistent behavior mounting reacting to live SSE event streams (GET /events/live), executing pipelines, and dispatching derived system events (ActionRequested.*).
ONNX Inference Engine (feat(infer): Integrate ONNX Runtime Inference Engine): Built local machine learning inference directly into the kernel:
Integrated ort ONNX Runtime and tokenizers crates with native AVX2/FMA3 acceleration.
Preloaded a DistilBERT-based ErrorClassifier asynchronously at startup to support the K think operator and automated causal analysis.
Built an automatic HuggingFace model downloader for zero-configuration setup.
Semantic Vector Memory (feat(kernel): Expose Graph Memory to K Language): Statically linked native sqlite-vec virtual tables into mnse-graph to allow 384-dimensional vector embeddings generated via all-MiniLM-L6-v2. Provides near-instant offline cosine similarity search with automatic fallback to keyword searching.
[0.0.30] - 2026-02-07 (System Integrity) 🔒🧠
Added
Archive Before Prune: Events exported to ~/.mnse/archive/events-YYYY-MM.jsonl before deletion.
Memory Correction: action=correct creates linked correction events with original ID references.
Timestamp Trust: kernel_received_at forensic timestamp set when kernel receives events.
Changed
Fact-Only Edges: Renamed INFORMED_EDIT → IN_STM_DURING with confidence 1.0 (no heuristics).
Design: Kernel records facts; agents interpret meaning.
Mutex Safety: Poison-safe locking across all graph module mutexes:
db.rs: acquire_lock() helper (10 calls)
graph_builder.rs: STM recovery (5 calls)
session_manager.rs: Session/Burst recovery (2 calls)
Code Cleanup: Removed deprecated mnse-os-adapter-legacy (dead code) and hardened SDK HTTP client initialization.
OS Adapter Reliability: Fixed startup race conditions and silent failures:
Retries connection to kernel (backoff strategy)
Auto-discovers Hyprland signature on restart
Invalidates connection on send failure to force reconnect
Prevents empty events by checking command exit codes (fixes "Focus empty" bug)
[0.0.29] - 2026-02-06 (Agentic Intelligence) 🧠🤖
Added
Memory Logic v2: Overhauled memory tool for intentional knowledge management.
Metadata: Added tags (Vec), priority (low/medium/high), and expires (auto-cleanup).
Linking: Added references for explicit edges to Traceable IDs (files, commands).
Filtering: Enhanced recall with tags, priority, and date (created_after, expires_before) filters.
Persistence: Implemented update action to modify or append to existing memories.
Contextual Intelligence (LSP): Integrated Code Intelligence into the Core.
Code Snippets: get_definition now returns surrounding source code (~10 lines), not just file paths.
Symbol Resolution: Precise navigation to File, Line, Column via LSP protocol.
Agent Lifecycle: Implemented POST /emit endpoint for AgentStarted/AgentEnded events, enabling identity tracking (User vs Agent).
Socket Reliability: Added 500ms timeouts to socket writes to prevent OS adapter hangs.
Changed
Tool Architecture: Consolidated 15 legacy tools into 6 Core Tools to reduce agent cognitive load.
get_mnse_context (Root Snapshot)
analyze (Analytics & Timeline - replaces get_tree, get_activity_summary)
graph (Traversal - replaces get_node, get_related, get_causes)
ledger (History Search - replaces query_mnse_history)
memory (Knowledge - replaces recall, remember)
get_definition (Code Intel - Native LSP)
API Standardization: All tools now enforce consistent format and limit/offset arguments.
Graph Schema: Updated Memory node storage and references edge creation logic.
[0.0.28] - 2026-01-31 (Infrastructure Hardening) 🦀🛡️
Added
MNSE SDK: Native Rust SDK (mnse-sdk) for consistent adapter development.
KernelClient: Robust socket communication with auto-reconnect.
HttpClient: Fallback state querying.
EventBuilder: Type-safe event construction.
Shell Adapter: Refactored fish.fish to use mnse-shell-adapter binary (no more netcat).
Shell Session ID: Added PID-based differentiation (session field) to support multiple split/tab sessions.
Cognitive VFS: mnse-vfs daemon (Rust) ensuring performant Kernel->File sync.
MCP Client: tools/mnse-mcp-client (Rust) replacing mcp-cli.py for faster testing.
Changed
Documentation: Updated PROTOCOL.md and ARCHITECTURE.md to reflect SDK usage.
[0.0.27] - 2026-01-31 (Tool Enhancements) 🛠️
Added
Activity Limit: get_activity_summary now supports limit parameter (default 5, max 50) to restrict top apps/files output.
Batch Queries: get_node and get_related now support ids array input (e.g., ["id1", "id2"]) for efficient batch retrieval.
CLI JSON: mcp-cli.py adds --args-json for passing complex arguments.
Changed
get_node: When ids array is used, returns a JSON Map { "id": content, ... } instead of a single string.
Systemd: Auto-discovery of Hyprland signature in mnse-os-adapter service.
Fixed
Startup: start-mnse.sh now auto-kills stale processes to prevent "Address already in use".
Connectivity: Fixed mnse-mcp-server default port (3000) and disabled proxy for reliable localhost access.
Formatting: Deduplicated clipboard history in "toon" output.
Lint: Removed unused variables in kernel_client.rs.
[0.0.26.13] - 2026-01-30 (Maintenance & Hardening) 🛠️🛡️
Added
Schema Freeze: SHA256 verification of database structure on boot. Prevents drift and ensures integrity.
Migration Auto-Backup: Creates ledger.v0.0.26.bak before upgrading from legacy versions.
Adapter Handshake: Required mnse-v0.1.0 protocol handshake on Unix socket. Rejects incompatible adapters.
MCP Pagination: Added limit (max 500) and offset support to get_tree and query_mnse_history.
API Contracts: Added _mnse_schema versioning to all MCP tool responses.
Error Contracts: Standardized MNSE-specific error codes (e.g., component unavailable mappings).
Socket Hardening: Enforced 0600 permissions on kernel.sock using umask(0o177).
Panic Protection: Implemented panic hook logging to ~/.mnse/panic.log.
Changed
Health Upgrade: /health endpoint now returns structured JSON with schema_version and privacy_mode.
Tool Logic: Defaulted get_node and get_activity_summary to toon format; removed redundant markdown options.
Versioning Policy: Defined major bump requirement for any future schema changes in docs/VERSIONING.md.
[0.0.26.12] - 2026-01-26 (Enhanced Context & Privacy) 🎨🔒
Added
Toon Format: Added Clock Header (🕒 TIME:) and [WS:X] context tags to history.
Minimal Format: Implemented Strict Action Whitelist (Focus, Edit, Cmd) to eliminate all system noise.
Diagnostics: start-mnse.sh now echoes MNSE_PRIVACY_MODE status on startup.
Fixed
Privacy Override: inspector_state.rs now correctly respects MNSE_PRIVACY_MODE=false for snapshots (Window Title, Clipboard).
Previously, snapshots were hard-redacted even when history was unmasked.
[0.0.26.10] - 2026-01-26 (The Cognitive VFS) 🧠📂
Added
Cognitive VFS: vfs_daemon.py prototype that mounts AI context as a filesystem at ~/.mnse/mnt.
cat ~/.mnse/mnt/context/active_app -> Read current focus.
watch ~/.mnse/mnt/status/health -> Live dashboard.
Slash Command Wrapper: mnse-chat script acting as a bridge between the Shell and MCP Prompts.
Intercepts /summarize, /debug to fully emulate VS Code-style slash commands in the terminal.
[0.0.26.9] - 2026-01-26 (System Standard & Innovation) 🏗️🚀
Added
MCP Prompts: First-class support for prompts/list and prompts/get.
summarize_session: Auto-injects context and timeline.
analyze_error: Auto-injects logs and system state.
VSCode Compatibility: Implemented legacy resources/templates schema to support VS Code Copilot's dialect.
Tool Standardization: All 10 tools now have semantic prefixes ([Context], [History], etc.) for better agent routing.
[0.0.26.8] - 2026-01-26 (Developer Experience) 🐍🛠️
Added
mcp-cli.py: Python-based CLI wrapper for easy testing of MCP server methods (tools/call, prompts/get).
Local Time: Timestamps in get_tree and History now respect system local time (e.g., IST).
[0.0.26.7] - 2026-01-26 (Signal-to-Noise) 🔇
Fixed
OS Filtering: Implemented "Delta Tracking" in OsAdapter to prevent redundant MonitorChanged and ResourcesUpdated events.
Noise Reduction: Reduced event volume by 40% during idle states.
[0.0.26.5] - 2026-01-24 (The Awakening) 📊👁️�
Added
Analyst: get_activity_summary now displays a 🧿 Daily Dashboard with authoritative counts for events, files, commands, and errors.
Prism: MinimalCompiler now supports active_app fallback and (os) mode hints for high-signal situational awareness outside of editors.
Robust Health: get_adapter_status migrated to HTTP Snapshot API (Port 3000) for zero-flicker connectivity reports.
[0.0.26.4] - 2026-01-24 (Causal Reanimation) 🏗️
Fixed
Ingestion: Integrated SessionManager into the Kernel event processor, reanimating the Historian Tree (get_tree).
Graph Indexing: Extended GraphBuilder to handle WindowFocused and FocusChanged events, bridging OS-level activity to the Knowledge Graph.
[0.0.26.3] - 2026-01-24 (Seamless Bridge) 🔗
Improved
ID Enrichment: query_mnse_history now explicitly provides _graph_node_id for Clipboard, Apps, and Custom events.
Guidance: Updated MCP tool schemas to explicitly guide agents toward Concept IDs instead of Event UUIDs for Graph traversal.
[0.0.26.2] - 2026-01-24 (Robust Connectivity) 🔌
Fixed
Socket Stability: Improved error handling for the Unix socket to prevent "Broken Pipe" cascades during high event volume.
HTTP Fallback: Kernel now provides a reliable HTTP backup for all context queries.
[0.0.26.1] - 2026-01-24 (Historian Tree) 🌳
Added
get_tree: New MCP tool for hierarchical temporal visualization (Day -> Session -> Burst -> Event).
Toon Rendering: Support for format="toon" ASCII art trees for instant agent perception of work rhythm.
[0.0.25.6] - 2026-01-24 (Browser Intelligence) 🌐
Added
Browser Graph: Kernel now extracts domains and search queries from URLs to create Domain and Concept nodes in the Knowledge Graph.
Event Linking: BrowserUrl events are now first-class citizens in the causality graph.
[0.0.25.5] - 2026-01-24 (Adapter Health) 🩺
Added
Connectivity: New get_adapter_status tool for real-time sensor health tracking.
Lifecycle Events: AdapterConnected and AdapterDisconnected events are now emitted to the ledger for reliability auditing.
[0.0.25] - 2026-01-24 (Graph Memory) 🧠
Added
Graph Memory & Causality
mnse-graph crate: New SQLite-based graph storage (~/.mnse/mnse_graph.db).
Causal Ingestion: Kernel automatically links events (e.g., Command -> EXECUTED_IN -> Directory, Command -> CAUSED -> Error).
Graph Schema: Nodes (File, Command, Context, Concept) and Weighted Edges (RELATED, INFORMED, MODIFIED).
MCP Tools (Graph)
get_node(id): Inspect low-level graph nodes (properties, timestamps).
get_related(id, relation?): Traverse edges to find what an entity caused or was informed by.
get_causes(id) / get_effects(id): Targeted directional queries (Upstream/Downstream).
Auto-ID Injection: query_mnse_history now returns _graph_node_id (e.g., cmd:<md5>) to bridge the gap between Ledger timestamps and Graph IDs.
Added
mnse-grouping crate: On-demand ActivityBlock grouping (Tasks, Sessions, Clusters).
MCP Tools: get_tasks and get_sessions for AI context.
Improved
MCP Output: Tools now include full events array (not just metadata) for LLM reasoning.
Configurable: include_events parameter (default: true) to control verbosity.
Statistical Layer (Pure Structure, No Interpretation)
get_sessions: stats object with event_types, sources, files_touched, commands_run counts.
get_tasks: files and commands arrays extracted from events.
Timing: delta_from_prev_ms field on all events for temporal gap analysis.
Noise Filtering
Filtered Events: MonitorChanged, NetworkUpdated, ResourcesUpdated, SystemUpdated, AdapterHeartbeat, AdapterConnected, AdapterDisconnected, RunningAppsChanged
Behavior: Noise events stay in ledger but cannot create new tasks/sessions
MCP Response: Events arrays in get_sessions/get_tasks return only meaningful activity
Result: No "ghost sessions" from system context or adapter lifecycle churn
Infrastructure Stability
Heartbeat Timeout: Increased from 60s to 300s (5 minutes) to reduce adapter flapping
Check Interval: Increased from 10s to 30s for efficiency
Result: Stable adapter connections, no 70-second disconnect cycles
Adapters
Shell: Command history (last 10) in payload, DirectoryChanged events on cd.
VSCode: Visible range change tracking (debounced), terminal open/close events.
Browser: URL blacklist filtering (chrome://, analytics, tracking domains).
--------------------------------------------------------------------------------
[0.0.22] - 2026-01-22
Security
Global Rate Limit: Connection cap (50 max) + global event cap (2000/sec) with silent drop.
Connection Tracking: Decrement on disconnect to prevent deadlock.
Transactional Prune: BEGIN IMMEDIATE prevents race with event processor.
Safe Lock Pattern: All 14 lock().unwrap() calls → acquire_lock() (never panics).
Buffer Integrity: Shell events checksummed (md5), tampered events rejected on flush.
Reliability
Panic Isolation: Prune runs in spawn_blocking + catch_unwind — cannot crash kernel.
Conditional VACUUM: Only runs if deleted > 10000 to avoid locking.
Shell Buffer Limit: Max 1000 events, rotates to 500, flush capped at 100.
Unverified Source Handling: _unverified suffix normalized in adapter status tracking.
--------------------------------------------------------------------------------
[0.0.21] - 2026-01-22
Added
Socket Authentication: SO_PEERCRED verification for peer UID. Unverified peers tagged with _unverified suffix.
Rate Limiting: Per-connection limit (100 events/sec) to prevent DoS.
VSCode Heartbeat: Extension sends heartbeat every 30s for reliable status.
Ledger Retention API: Ledger.prune_events(days) to delete old events.
Systemd Service: systemd/mnse-kernel.service for auto-restart on crash with security hardening.
Shell Buffering: Fish adapter queues events to ~/.mnse/event_buffer.jsonl when kernel is offline.
Indexed Event Types: event_type_name column with O(1) exact-match queries (replaces LIKE scans).
Daily Auto-Prune: Background task runs every 24h to delete events older than 30 days.
Security
DoS protection prevents event flooding.
Unverified connections flagged in observed_by field.
Systemd service includes NoNewPrivileges, ProtectSystem, memory limits.
--------------------------------------------------------------------------------
[0.0.20] - 2026-01-21
Fixed
Code Cleanliness: Eliminated all 17 compiler warnings in mnse-os-adapter.
Dead Code: Implementation of IdleTracker and unused HyprlandEvent fields silenced/cleaned.
Unused Imports: Removed PathBuf, serde_json, anyhow unused imports.
Changed
Build Quality: cargo build --release now clean (0 warnings).
--------------------------------------------------------------------------------
[0.0.19] - 2026-01-21
Fixed
Browser Extension Reliability: Fixed bug where tracking disabled itself on browser restart.
Root Cause: Manifest V3 service worker lifecycle race condition.
Fix: Popup now reads directly from chrome.storage.local and background loads state immediately.
VSCode Extension: Verified connectivity and port settings.
--------------------------------------------------------------------------------
[0.0.18] - 2026-01-21
Fixed
OS Adapter Connectivity: Resolved "never connected" status despite active events.
Root Cause: Adapter sent EventSource::System instead of EventSource::OS. kernel mapped System -> "system" but expected "os".
Fix: Added EventSource::OS variant, updated adapter source, updated kernel mapping.
Deployment: Fixed mnse-os-adapter binary build location issue.
Added
Type Definitions: EventSource::OS added to mnse-types.
--------------------------------------------------------------------------------
[0.0.17] - 2026-01-21
Fixed
Ledger Privacy: All PII-containing events now redacted before storage
SystemUpdated: Window titles in array → [REDACTED:TITLE]
TabsUpdated: All tab URLs/titles → [REDACTED:URL]/[REDACTED:TITLE]
SelectionChanged: Browser selection text → [REDACTED:SELECTION]
DomSnapshot: Article content → [REDACTED:DOM]
observed_by Persistence: Added column to SQLite schema, now stored and retrieved correctly
Changed
db.rs: Schema, migration, INSERT, and all 9 SELECT queries updated for observed_by
redaction.rs: 4 new event type handlers for comprehensive PII protection
--------------------------------------------------------------------------------
[0.0.16] - 2026-01-21
Fixed
Snapshot Staleness: snapshot() now sets fresh timestamp at query time (was using event time, causing 14-minute lag)
Privacy by Default: Window titles and URLs now ALWAYS redacted before ledger storage (was policy-dependent)
Changed
inspector_state.rs: window_title always redacted in snapshots
redaction.rs: should_redact_title and should_redact_url now always true
--------------------------------------------------------------------------------
Added
Epistemic Grounding (Paradox Fix)
Provenance Tracking: Added observed_by field to all events (e.g., observed_by="os", "shell", "vscode").
Health Guard: InspectorState now updates adapter health only when events are observed directly by that adapter.
Result: Resolved the "Epistemic Paradox" where OS events masked silent/crashed adapters.
Semantic Causality
Error Detection: High-priority ErrorDetected events now emitted for non-zero shell exit codes.
Causal Linking: Enhanced parent_event_id tracking to bind errors to subsequent recovery actions (searches, fixes).
Zero Trust Privacy
Hardened Redaction: ContextSnapshot now strictly redacts clipboard_preview and editor_selection by default.
Redaction Patterns: Added placeholder logic for URL/Title redaction based on user policies.
Changed
mnse-types: Event struct now includes observed_by with default values.
mnse-ledger: Historical events read from DB default to observed_by="unknown".
mnse-kernel: InspectorState::snapshot() returns redacted data unless explicit permission is granted.
--------------------------------------------------------------------------------
[0.0.14] - 2026-01-20
Added
Privacy & Lifecycle Polish
Lifecycle Events: AdapterConnected and AdapterDisconnected (with reason field) are now first-class events.
Stale Adapter Detection: mark_stale_adapters logic hardened to prevent ghost connections.
Privacy Policy: Foundation for per-field redaction rules in Redactor.
--------------------------------------------------------------------------------
[0.0.13] - 2026-01-20
Fixed
Timestamp Overflow: Resolved TOON compiler bug showing "29483146m ago".
Deployment: Fixed fish shell adapter installation and order.
--------------------------------------------------------------------------------
[0.0.12] - 2026-01-20
Fixed
Reliability & Metrics
Timestamp Integrity: Fixed TOON compiler timestamp overflow bug that showed "29483146m ago".
Duration Tuning: Fixed duration calculation (converted seconds to milliseconds correctly).
Shell Support: Addressed missing shell configurations in kernel defaults.
--------------------------------------------------------------------------------
[0.0.11] - 2026-01-20
Added
Reliability & Trust: Implemented adapter health heartbeats, ErrorDetected base, and snapshot consistency checks.
--------------------------------------------------------------------------------
[0.0.10] - 2026-01-20
Added
Intelligent Noise Filtering: Implemented JSON-aware exclusion filtering in mnse-ledger.
API Filtering: Support for comma-separated exclude_types in HTTP API.
MCP Optimization: Auto-filtering of AdapterHeartbeat and other noise in MCP Tool by default.
--------------------------------------------------------------------------------
[0.0.9] - 2026-01-20
Added
Duration Metrics: New DurationMetrics struct and DurationAnalyzer for tracking focus time and context switches.
Metrics API: Added /metrics endpoint for deep analytics.
CLI Analytics: Added mnse_metrics command to query_kernel.py.
--------------------------------------------------------------------------------
[0.0.8] - 2026-01-20
Added
Redaction Core: Implemented robust redaction system in mnse-kernel/src/redaction.rs.
Browser Intelligence: Automatic extraction of search queries from browser events.
Socket Integration: Tied redaction filters directly into the socket ingestion layer.
--------------------------------------------------------------------------------
[0.0.7] - 2026-01-20
Added
Causality Linking: First-class support for linking Shell, Browser, and Editor events via parent_event_id.
TOON History v1: Fixed and populated chronological history display in TOON format.
Documentation: Initial release of docs/API_REFERENCE.md.
--------------------------------------------------------------------------------
[0.0.6] - 2026-01-20
Added
Terminal Error Detection
Exit Code Detection: terminal_has_error auto-populated from exit_code != 0
Pattern Recognition: detect_error_pattern() for Rust, Node, Python, Git, Make errors
TOON Integration: SNAPSHOT shows err:1, err_type:, err_cmd: when errors detected
Enhanced TOON HISTORY Format
Visual Indicators: ✅ success / ❌ failure for shell commands
Error Messages: └─ pattern: error message for TerminalOutput
Noise Filtering: Removed Heartbeat, ResourcesUpdated, SystemUpdated spam
Clean Sources: SHELL, BROWSER, VSCODE, NEOVIM instead of debug format
Context Events: Added WORKSPACE and FOCUS events
Connected Adapters Display
TUI now shows all connected adapters from kernel heartbeats (Shell, Browser, etc.)
Each adapter shows event count and last seen time
Changed
TOON HISTORY now filters and formats events for agent readability
Long commands truncated to 60 chars for readability
--------------------------------------------------------------------------------
[0.0.5] - 2026-01-20
Added
Phase 5: Sentinel HUD Upgrade
Context View: New TOON narrative display in mnse-control
Async Architecture: Converted TUI to tokio runtime
HTTP Client: Added reqwest for rich data fetching
MCP Format Support
Format Parameter: get_mnse_context now accepts format: toon|json|minimal
Default changed to toon for AI-optimized output
Tool schema exposes format as enum parameter
Fixed
Critical Query Bug (Gemini CLI Review)
Case-Sensitivity: query_mnse_history(source="shell") now works (was returning empty)
Fixed: WHERE LOWER(source) = LOWER(?) in db.rs
Fixed: MCP JSON-to-query-params serialization in kernel_client.rs
Changed
MCP server uses HTTP API for ledger queries (richer filtering)
Unix socket retained for control commands and event ingestion
mnse-control fetches TOON via HTTP, structured data via socket
--------------------------------------------------------------------------------
[0.0.4] - 2026-01-19
Added
TUI Control Panel (mnse-control)
Modular TUI architecture using Ratatui
Process Management: Start/Stop/Restart adapters directly from TUI
Robust Process Spawning: Adapters launch in separate, visible terminal windows (Ghostty/Foot/Kitty support)
Status Dashboard: Live view of connected adapters and event counts
Replaces legacy mnse-tui and sentinel
Browser Context Enrichment
Tab Snapshot: Tracks ALL open tabs (title + URL), not just active tab
Extension Sync: Added "Sync Now" button to browser extension popup
Enhanced Query: query_kernel.py now displays full tab list with pinned indicators
Shell Output Capture
Terminal Output Buffer: Kernel now stores last N lines of terminal output
mnse_run wrapper for fish shell to capture stdout/stderr
Error Detection: Automatic detection of error patterns (error, failed, panic, etc.)
ContextSnapshot fields: terminal_output, terminal_has_error, terminal_error_pattern
Developer Experience
start-mnse.sh: One-click startup script for Kernel + all Adapters
query_kernel.py: Enhanced CLI tool for inspecting kernel state (replaced mnse-debug.sh)
docs/ENHANCEMENTS.md: Roadmap for future improvements
Changed
Refactored mnse-control to be the primary interface
Improved startup reliability with ordered process launching (Kernel first -> 2s delay -> Adapters)
Moved logs to ~/.mnse/logs/ with nohup daemonization support
Fixed
Fixed "Address already in use" crashes by robustly handling zombie processes
Fixed browser extension connection issues with manual sync fallback
--------------------------------------------------------------------------------
[0.0.3] - 2026-01-17
Added
Kernel Event Processing Fix
Added 5 new EventTypes: WindowFocused, WorkspaceChanged, ClipboardChanged, ResourcesUpdated, RunningAppsChanged
Added 12 new ContextSnapshot fields for OS-level data
Kernel now properly processes and stores OS adapter events
Adapter status tracking with heartbeats
VSCode Adapter (adapters/vscode/)
Rust HTTP server on port 3001
TypeScript VSCode extension
Tracks: file path, cursor position, selection, diagnostics, git branch, open files, language
Endpoints: /context, /cursor, /file, /selection, /diagnostics
Debounced cursor events (100ms default)
Browser Adapter (adapters/browser/)
Rust HTTP server on port 3002
Chrome/Firefox extension (Manifest V3)
Tracks: URL, title, domain, tab count, all tabs, selection
Endpoints: /context, /tab, /url, /selection, /tabs
Content script for selection tracking
Changed
OS adapter now sends proper EventTypes instead of Custom events
ContextSnapshot includes: window_id, window_title, window_class, focused_url, current_workspace, total_workspaces, window_count, clipboard_preview, cpu_usage, memory_used_mb, memory_total_mb, running_apps
Fixed
Kernel inspector_state now handles all OS-level events
Types consolidated in mnse-types (removed duplicates from kernel/types.rs)
--------------------------------------------------------------------------------
[0.0.2] - 2026-01-17
Added
HTTP Server with Context Compression
Axum web server on port 3000 (configurable via MNSE_HTTP_PORT)
GET /health - Health check endpoint
GET /snapshot?format=json - Full context (870 bytes)
GET /snapshot?format=toon - TOON compressed (60 bytes, 93% reduction)
GET /snapshot?format=minimal - Ultra-compressed (15 bytes, 98% reduction)
CORS enabled for web clients
Integrated with CompilerRegistry for format selection
Hyprland Event Listener
Real-time event streaming via socket2 (/tmp/hypr/$SIGNATURE/.socket2.sock)
Event types: activewindow, workspace, fullscreen, monitoradded, monitorremoved, windowopened, windowclosed, layoutchanged
Opt-in event mode (MNSE_EVENT_MODE=1)
Fallback polling mode (500ms-10s intervals)
Deep Adapter Router
Intelligent app detection from window class/name
Supported deep adapters:
Neovim/Vim → LSP, diagnostics, treesitter
VSCode/Cursor → file context, extensions
Firefox/Chrome/Brave → URL, tabs, history
Terminal/Ghostty/Kitty → Fish shell adapter
Yazi/Nautilus → file manager selection
Generic → OS-level context only
Routing decision logging
JSON routing instructions with query parameters
OS Adapter for Hyprland
System-level context tracking
Window management: active window, title, class, PID, workspace
Clipboard monitoring via wl-paste
System resource tracking (CPU, memory, disk)
Process list detection
Shared Types Package
mnse-types package for shared type definitions
Resolves circular dependency between kernel and compilers
Types: Event, EventType, EventSource, ContextSnapshot, DiagnosticsCount, AdapterStatus
Changed
Type system refactored to use shared mnse-types package
Event structure updated: added id field, source is now enum, renamed data to payload
OS adapter now includes routing in event payload
Fixed
Hyprland fullscreen type: changed from bool to i32 (Hyprland returns 0/1)
Circular dependency between mnse-kernel and mnse-compilers
Event structure alignment across packages
--------------------------------------------------------------------------------
[0.0.1] - 2026-01-15
Added
Core Kernel Daemon
mnse-kernel Rust daemon with event loop
Event bus (MPSC, ordered, timestamped)
Inspector state (canonical truth, append-only history)
Unix socket server at ~/.mnse/kernel.sock
Commands: ping, snapshot, history
17 EventType variants, 19 ContextSnapshot fields
Neovim Adapter
Lua plugin for comprehensive Neovim integration
Tracking: cursor, file, mode, selection, directory
Enhanced: buffer list, file type, modified state
Git branch detection
LSP diagnostics and symbol tracking
15 autocmd groups for real-time updates
Sentinel HUD
Ratatui TUI for real-time visualization
3-panel layout (header, context, buffers)
100ms refresh rate
Color-coded status indicators
Fish Shell Adapter
Shell command tracking
Exit code capture
Working directory changes
Duration timing
Context Compilers
JSON compiler (full context output)
TOON compiler (77% token reduction)
Minimal compiler (95% reduction)
CompilerRegistry for format selection
Documentation
MNSE Specification (3,500+ words)
Architecture documentation (6-layer system)
Phase specifications (1-4)
Implementation guides
Architecture
Layer A: User (observable actions)
Layer B: Adapters (Neovim, Shell, OS)
Layer C: MNSE Kernel (event bus, state, IPC)
Layer D: Context Compilers (JSON, TOON, Minimal)
Layer E: Executors (planned)
Layer F: Sentinel (TUI visualization)
