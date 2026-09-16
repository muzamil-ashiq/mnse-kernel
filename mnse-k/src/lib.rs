//! MNSE K Language Compiler & Behavior Engine.

pub mod ast;
pub mod lexer;
pub mod parser;
pub mod planner;
pub mod runtime;

pub use ast::{KExpr, KProgram, KStatement};
pub use lexer::{Lexer, Token};
pub use parser::Parser;
pub use planner::{KernelOp, Planner};
pub use runtime::{KContext, Runtime};

/// Execute a raw K language code string
pub async fn execute(code: &str) -> anyhow::Result<serde_json::Value> {
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program()?;
    let ops = Planner::plan(&program)?;

    let runtime = Runtime::default_local();
    let mut ctx = KContext::default();
    runtime.eval_ops(&ops, &mut ctx).await
}
