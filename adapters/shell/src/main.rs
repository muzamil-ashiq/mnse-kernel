use anyhow::Result;
use clap::{Parser, Subcommand};
use mnse_sdk::SocketClient;
use mnse_shell_adapter::buffer::EventBuffer;
use mnse_types::{Event, EventType};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mnse-shell-adapter")]
#[command(about = "MNSE Shell Telemetry Adapter for Fish, Zsh, and Bash")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Record command execution telemetry
    RecordExec {
        #[arg(long)]
        cmd: String,
        #[arg(long)]
        exit: i32,
        #[arg(long, default_value_t = 0)]
        duration_ms: u64,
        #[arg(long)]
        cwd: String,
        #[arg(long)]
        session: Option<u32>,
        #[arg(long)]
        stdout: Option<String>,
        #[arg(long)]
        stderr: Option<String>,
    },

    /// Record directory change (cd)
    RecordCd {
        #[arg(long)]
        new_dir: String,
        #[arg(long)]
        prev_dir: Option<String>,
        #[arg(long)]
        session: Option<u32>,
    },

    /// Output shell hook scripts for Fish, Zsh, and Bash
    InstallHooks {
        #[arg(long, default_value = "all")]
        shell: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let socket_path = get_socket_path();
    let client = SocketClient::new(&socket_path);
    let buffer = EventBuffer::new();

    match cli.command {
        Commands::RecordExec {
            cmd,
            exit,
            duration_ms,
            cwd,
            session: _,
            stdout,
            stderr,
        } => {
            let stdout_tail = stdout.map(|s| cap_string(&s, 500));
            let stderr_tail = stderr.map(|s| cap_string(&s, 500));

            // Build ProcessExecuted event
            let exec_event = Event::new(
                EventType::ProcessExecuted {
                    command: cmd.clone(),
                    exit_code: exit,
                    duration_ms,
                    working_dir: cwd.clone(),
                    stdout_tail,
                    stderr_tail,
                },
                "shell",
            );

            // Attempt IPC emit to socket, buffer on failure
            if let Err(_) = client.emit(&exec_event).await {
                let _ = buffer.push(&Event::new(
                    EventType::ProcessExecuted {
                        command: cmd.clone(),
                        exit_code: exit,
                        duration_ms,
                        working_dir: cwd.clone(),
                        stdout_tail: None,
                        stderr_tail: None,
                    },
                    "shell",
                ));
            } else {
                // If socket is online, flush any pending buffered events
                let _ = buffer.flush(&client, 50).await;
            }

            // If command failed (exit != 0), emit derived ErrorDetected event
            if exit != 0 {
                let pattern = classify_error(&cmd, exit);
                let err_event = Event::new(
                    EventType::ErrorDetected {
                        command: cmd,
                        exit_code: exit,
                        error_pattern: pattern,
                        working_dir: cwd,
                    },
                    "shell",
                );
                let _ = client.emit(&err_event).await;
            }
        }

        Commands::RecordCd {
            new_dir,
            prev_dir,
            session: _,
        } => {
            let cd_event = Event::new(
                EventType::DirectoryChanged {
                    new_dir,
                    previous_dir: prev_dir,
                },
                "shell",
            );

            if let Err(_) = client.emit(&cd_event).await {
                let _ = buffer.push(&Event::new(
                    EventType::DirectoryChanged {
                        new_dir: get_cwd_string(),
                        previous_dir: None,
                    },
                    "shell",
                ));
            }
        }

        Commands::InstallHooks { shell } => match shell.as_str() {
            "fish" => print!("{}", include_str!("../hooks/fish.fish")),
            "zsh" => print!("{}", include_str!("../hooks/zsh.zsh")),
            "bash" => print!("{}", include_str!("../hooks/bash.bash")),
            _ => {
                println!("# MNSE Shell Adapter Hooks");
                println!("# Fish hook:");
                print!("{}", include_str!("../hooks/fish.fish"));
                println!("\n# Zsh hook:");
                print!("{}", include_str!("../hooks/zsh.zsh"));
                println!("\n# Bash hook:");
                print!("{}", include_str!("../hooks/bash.bash"));
            }
        },
    }

    Ok(())
}

fn get_socket_path() -> String {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".mnse/kernel.sock").to_string_lossy().to_string()
}

fn get_cwd_string() -> String {
    std::env::current_dir()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| "/tmp".into())
}

fn cap_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        s[s.len() - max_len..].to_string()
    }
}

fn classify_error(cmd: &str, exit_code: i32) -> Option<String> {
    if cmd.starts_with("cargo ") {
        Some("RustCompile".into())
    } else if cmd.starts_with("git ") {
        Some("GitConflict".into())
    } else if exit_code == 127 {
        Some("NotFound".into())
    } else if exit_code == 126 {
        Some("PermissionDenied".into())
    } else {
        Some(format!("ExitCode{}", exit_code))
    }
}
