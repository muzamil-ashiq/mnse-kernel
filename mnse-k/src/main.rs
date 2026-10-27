//! `k` — CLI Shell, Creator Control & REPL for MNSE Kernel.
//!
//! Interactive shell mode when invoked without arguments: `k`
//! One-shot query mode when given code: `k "current.state"`
//! Real-time creator tap stream: `k tap [adapter] [--raw] [--json]`
//! Time-travel event replay: `k replay --from "3pm" [--to "4pm"] [--speed 5x]`

use clap::Parser as ClapParser;
use mnse_k::{execute, KContext, Lexer, Parser as KParser, Planner, Runtime};
use mnse_types::Event;
use rustyline::DefaultEditor;
use std::path::PathBuf;

#[derive(ClapParser, Debug)]
#[command(name = "k", author = "Muzamil", version = "2.0.0", about = "K Language CLI Shell & Creator Control for MNSE Kernel")]
struct Cli {
    /// Historical point-in-time state query (e.g., "3pm", "yesterday at 4pm", "30m ago")
    #[arg(long, short = 'a')]
    at: Option<String>,

    /// Raw unredacted event streaming / output
    #[arg(long)]
    raw: bool,

    /// Machine-readable NDJSON streaming / output
    #[arg(long)]
    json: bool,

    /// Time-travel replay start point (e.g. "3pm", "1 hour ago")
    #[arg(long)]
    from: Option<String>,

    /// Time-travel replay end point (e.g. "4pm", "now")
    #[arg(long)]
    to: Option<String>,

    /// Replay speed multiplier (e.g. "1x", "2x", "5x", "10x")
    #[arg(long, default_value = "2x")]
    speed: String,

    /// Optional K expression or command to evaluate
    #[arg(trailing_var_arg = true)]
    code: Vec<String>,
}

fn format_event_lean(event: &Event) -> String {
    let time_str = event.timestamp.format("%H:%M:%S").to_string();
    let obs = &event.observed_by;
    match &event.event_type {
        mnse_types::EventType::ProcessExecuted {
            command,
            exit_code,
            duration_ms,
            ..
        } => {
            let status_mark = if *exit_code == 0 { "ok" } else { "fail" };
            format!(
                "[{}] [{}] ProcessExecuted \"{}\" exit:{} ({}) [{}ms]",
                time_str, obs, command, exit_code, status_mark, duration_ms
            )
        }
        mnse_types::EventType::ErrorDetected {
            command,
            exit_code,
            error_pattern,
            ..
        } => {
            let pat = error_pattern.as_deref().unwrap_or("Error");
            format!(
                "[{}] [{}] ErrorDetected \"{}\" exit:{} [{}]",
                time_str, obs, command, exit_code, pat
            )
        }
        mnse_types::EventType::WindowFocused {
            app_name,
            window_title,
            workspace,
            ..
        } => {
            let ws_str = workspace
                .map(|w| format!(" [ws: {}]", w))
                .unwrap_or_default();
            format!(
                "[{}] [{}] WindowFocused \"{}\"{} \"{}\"",
                time_str, obs, app_name, ws_str, window_title
            )
        }
        mnse_types::EventType::WorkspaceChanged {
            workspace_id,
            workspace_count,
            ..
        } => {
            let count_str = workspace_count
                .map(|c| format!(" ({} total)", c))
                .unwrap_or_default();
            format!(
                "[{}] [{}] WorkspaceChanged workspace {}{}",
                time_str, obs, workspace_id, count_str
            )
        }
        mnse_types::EventType::FileSwitched {
            file_path,
            language,
            git_branch,
            ..
        } => {
            let lang_str = language
                .as_deref()
                .map(|l| format!(" [{}]", l))
                .unwrap_or_default();
            let branch_str = git_branch
                .as_deref()
                .map(|b| format!(" (branch: {})", b))
                .unwrap_or_default();
            format!(
                "[{}] [{}] FileSwitched \"{}\"{}{}",
                time_str, obs, file_path, lang_str, branch_str
            )
        }
        mnse_types::EventType::DirectoryChanged {
            new_dir,
            previous_dir,
            ..
        } => {
            let prev_str = previous_dir.as_deref().unwrap_or("");
            format!(
                "[{}] [{}] DirectoryChanged \"{}\" (from \"{}\")",
                time_str, obs, new_dir, prev_str
            )
        }
        mnse_types::EventType::DiagnosticsUpdated {
            file_path,
            error_count,
            warning_count,
        } => {
            format!(
                "[{}] [{}] DiagnosticsUpdated \"{}\" ({} errors, {} warnings)",
                time_str, obs, file_path, error_count, warning_count
            )
        }
        mnse_types::EventType::AdapterConnected {
            adapter_name,
            protocol_version,
            ..
        } => {
            format!(
                "[{}] [{}] AdapterConnected \"{}\" (v{})",
                time_str, obs, adapter_name, protocol_version
            )
        }
        mnse_types::EventType::AdapterDisconnected {
            adapter_name,
            reason,
        } => {
            format!(
                "[{}] [{}] AdapterDisconnected \"{}\" ({:?})",
                time_str, obs, adapter_name, reason
            )
        }
        other => {
            format!("[{}] [{}] {}", time_str, obs, other.type_name())
        }
    }
}

async fn run_tap(adapter: Option<&str>, raw: bool, json_mode: bool) -> anyhow::Result<()> {
    let socket = mnse_sdk::SocketClient::default_local();
    match socket.tap_stream(adapter, raw).await {
        Ok(mut lines) => {
            eprintln!(
                "[tap:active] listening on kernel.sock (filter: {}, raw: {}, json: {})...",
                adapter.unwrap_or("all"),
                raw,
                json_mode
            );
            while let Ok(Some(line)) = lines.next_line().await {
                if line.trim().is_empty() {
                    continue;
                }
                if json_mode {
                    println!("{}", line);
                } else if let Ok(event) = serde_json::from_str::<Event>(&line) {
                    println!("{}", format_event_lean(&event));
                } else {
                    println!("{}", line);
                }
            }
            Ok(())
        }
        Err(e) => {
            eprintln!("Error connecting to kernel tap stream: {}", e);
            std::process::exit(1);
        }
    }
}

async fn run_replay(
    from_expr: &str,
    to_expr: Option<&str>,
    speed_str: &str,
) -> anyhow::Result<()> {
    let from_dt = mnse_types::parse_natural_time(from_expr)
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let to_dt = if let Some(to_s) = to_expr {
        mnse_types::parse_natural_time(to_s)
            .map_err(|e| anyhow::anyhow!("{}", e))?
    } else {
        chrono::Utc::now()
    };

    let speed_mult: f64 = speed_str
        .trim_end_matches('x')
        .trim_end_matches('X')
        .parse::<f64>()
        .unwrap_or(2.0)
        .max(0.1f64);

    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    let ledger_path = home.join(".mnse/ledger.db");
    let ledger = mnse_ledger::Ledger::open_read_only(&ledger_path)?;

    let events = ledger.query_events_in_range(from_dt.timestamp(), to_dt.timestamp())?;

    if events.is_empty() {
        println!(
            "[replay] No events found between {} and {}",
            from_dt.format("%H:%M:%S UTC"),
            to_dt.format("%H:%M:%S UTC")
        );
        return Ok(());
    }

    println!(
        "[replay:started] Replaying {} events from {} to {} at {}x speed...",
        events.len(),
        from_dt.format("%H:%M:%S UTC"),
        to_dt.format("%H:%M:%S UTC"),
        speed_mult
    );

    let mut last_ts = events[0].timestamp;
    for (idx, ev) in events.iter().enumerate() {
        if idx > 0 {
            let diff_sec = (ev.timestamp - last_ts).num_milliseconds() as f64 / 1000.0;
            if diff_sec > 0.0 {
                let scaled_delay = (diff_sec / speed_mult).min(1.5);
                tokio::time::sleep(tokio::time::Duration::from_secs_f64(scaled_delay)).await;
            }
        }
        last_ts = ev.timestamp;
        println!("[replay] {}", format_event_lean(ev));
    }

    println!("[replay:complete] Finished replaying {} events.", events.len());
    Ok(())
}

fn tokenize_args(s: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut current = String::new();
    let mut in_quote = None;
    let mut escape = false;

    for c in s.chars() {
        if escape {
            current.push(c);
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if let Some(q) = in_quote {
            if c == q {
                in_quote = None;
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c.is_whitespace() {
            if !current.is_empty() {
                tokens.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        tokens.push(current);
    }
    tokens
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    // 1. Handle explicit CLI flag: `k state --at "3pm"` or `k --at "3pm"`
    if let Some(at_val) = &cli.at {
        let client = mnse_sdk::HttpClient::default_local();
        match client.get_snapshot_toon_at("rich", Some(at_val)).await {
            Ok(output) => {
                println!("{}", output);
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error querying historical state at '{}': {}", at_val, e);
                std::process::exit(1);
            }
        }
    }

    // 2. Handle replay mode: `k replay --from "3pm" [--to "4pm"] [--speed 5x]`
    if let Some(from_val) = &cli.from {
        return run_replay(from_val, cli.to.as_deref(), &cli.speed).await;
    }

    if !cli.code.is_empty() {
        let joined = cli.code.join(" ").trim().to_string();
        let tokens = tokenize_args(&joined);

        if !tokens.is_empty() {
            let first = &tokens[0];

            // 3. Handle `k tap [adapter] [--raw] [--json]`
            if first == "tap" {
                let is_raw = cli.raw || tokens.iter().any(|c| c == "--raw");
                let is_json = cli.json || tokens.iter().any(|c| c == "--json");
                let adapter = tokens
                    .iter()
                    .skip(1)
                    .find(|c| !c.starts_with('-'))
                    .map(|s| s.as_str());
                return run_tap(adapter, is_raw, is_json).await;
            }

            // 4. Handle `k replay ...`
            if first == "replay" {
                let mut from_val = cli.from.as_deref();
                let mut to_val = cli.to.as_deref();
                let mut speed_val = cli.speed.as_str();

                let mut i = 1;
                while i < tokens.len() {
                    match tokens[i].as_str() {
                        "--from" => {
                            if i + 1 < tokens.len() {
                                from_val = Some(&tokens[i + 1]);
                                i += 1;
                            }
                        }
                        "--to" => {
                            if i + 1 < tokens.len() {
                                to_val = Some(&tokens[i + 1]);
                                i += 1;
                            }
                        }
                        "--speed" => {
                            if i + 1 < tokens.len() {
                                speed_val = &tokens[i + 1];
                                i += 1;
                            }
                        }
                        val if !val.starts_with('-') && from_val.is_none() => {
                            from_val = Some(val);
                        }
                        _ => {}
                    }
                    i += 1;
                }

                let final_from = from_val.unwrap_or("30m ago");
                return run_replay(final_from, to_val, speed_val).await;
            }

            // 5. Handle inline `--at` expressions: `k "state --at 3pm"` or `k 'state --at "1 hour ago"'`
            if let Some(pos) = tokens.iter().position(|t| t == "--at") {
                if pos + 1 < tokens.len() {
                    let at_val = &tokens[pos + 1];
                    let client = mnse_sdk::HttpClient::default_local();
                    match client.get_snapshot_toon_at("rich", Some(at_val)).await {
                        Ok(output) => {
                            println!("{}", output);
                            return Ok(());
                        }
                        Err(e) => {
                            eprintln!("Error querying historical state at '{}': {}", at_val, e);
                            std::process::exit(1);
                        }
                    }
                }
            }
        }

        // One-shot evaluation or file execution (strictly .k files for script execution)
        let input = if cli.code.len() == 1 && cli.code[0].ends_with(".k") {
            let p = std::path::Path::new(&cli.code[0]);
            if p.is_file() {
                std::fs::read_to_string(p)?
            } else {
                return Err(anyhow::anyhow!("K script file not found: {}", cli.code[0]));
            }
        } else {
            joined
        };

        match execute(&input).await {
            Ok(result) => {
                if let serde_json::Value::String(s) = &result {
                    println!("{}", s);
                } else if !result.is_null() {
                    println!("{}", serde_json::to_string_pretty(&result)?);
                }
            }
            Err(e) => {
                eprintln!("Error evaluating K expression: {}", e);
                std::process::exit(1);
            }
        }
        return Ok(());
    }

    // Interactive REPL Shell Mode
    println!("K Language Shell v2.0.0 (MNSE Kernel)");
    println!("Commands: 'help', 'care', 'state', 'tap [adapter]', 'replay [from]', or 'exit'\n");

    let mut rl = DefaultEditor::new()?;
    let runtime = Runtime::default_local();
    let mut ctx = KContext::default();

    loop {
        let readline = rl.readline("k> ");
        match readline {
            Ok(line) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                if trimmed == "exit" || trimmed == "quit" {
                    break;
                }

                let _ = rl.add_history_entry(trimmed);

                // REPL tap command
                if trimmed.starts_with("tap") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let adapter = parts.get(1).copied().filter(|s| !s.starts_with('-'));
                    let is_raw = trimmed.contains("--raw");
                    let is_json = trimmed.contains("--json");
                    let _ = run_tap(adapter, is_raw, is_json).await;
                    continue;
                }

                // REPL replay command
                if trimmed.starts_with("replay") {
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    let from_expr = parts.get(1).copied().unwrap_or("30m ago");
                    let _ = run_replay(from_expr, None, "2x").await;
                    continue;
                }

                let mut lexer = Lexer::new(trimmed);
                match lexer.tokenize() {
                    Ok(tokens) => {
                        let mut parser = KParser::new(tokens);
                        match parser.parse_program() {
                            Ok(program) => match Planner::plan(&program) {
                                Ok(ops) => match runtime.eval_ops(&ops, &mut ctx).await {
                                    Ok(res) => {
                                        if let serde_json::Value::String(s) = &res {
                                            println!("{}", s);
                                        } else if !res.is_null() {
                                            println!("{}", serde_json::to_string_pretty(&res)?);
                                        }
                                    }
                                    Err(e) => eprintln!("Runtime Error: {}", e),
                                },
                                Err(e) => eprintln!("Planner Error: {}", e),
                            },
                            Err(e) => eprintln!("Parse Error: {}", e),
                        }
                    }
                    Err(e) => eprintln!("Lexer Error: {}", e),
                }
            }
            Err(_) => break,
        }
    }

    Ok(())
}
