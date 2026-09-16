//! `k` — CLI Shell & REPL for MNSE Kernel.
//!
//! Interactive shell mode when invoked without arguments: `k`
//! One-shot query mode when given code: `k "current.app"`

use clap::Parser as ClapParser;
use mnse_k::{execute, KContext, Lexer, Parser as KParser, Planner, Runtime};
use rustyline::DefaultEditor;

#[derive(ClapParser, Debug)]
#[command(name = "k", author = "Muzamil", version = "2.0.0", about = "K Language CLI Shell for MNSE Kernel")]
struct Cli {
    /// Optional K expression or command to evaluate
    #[arg(trailing_var_arg = true)]
    code: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    if !cli.code.is_empty() {
        // One-shot evaluation
        let input = cli.code.join(" ");
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
    println!("⚡ K Language Shell v2.0.0 (MNSE Kernel)");
    println!("Type 'help', 'care', 'current.state', or 'exit' to quit.\n");

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
