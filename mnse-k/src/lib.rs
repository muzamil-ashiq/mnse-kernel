//! MNSE K Language Compiler & Behavior Engine.

pub mod ast;
pub mod capabilities;
pub mod lexer;
pub mod parser;
pub mod planner;
pub mod runtime;

pub use ast::{KExpr, KProgram, KStatement};
pub use capabilities::dispatch_capability;
pub use lexer::{Lexer, Token};
pub use parser::Parser;
pub use planner::{KernelOp, Planner};
pub use runtime::{KContext, Runtime};

/// Execute a raw K language code string
pub async fn execute(code: &str) -> anyhow::Result<serde_json::Value> {
    let mut ctx = KContext::default();
    execute_with_ctx(code, &mut ctx).await
}

/// Execute a raw K language code string with a given context
pub async fn execute_with_ctx(code: &str, ctx: &mut KContext) -> anyhow::Result<serde_json::Value> {
    let mut lexer = Lexer::new(code);
    let tokens = lexer.tokenize()?;
    let mut parser = Parser::new(tokens);
    let program = parser.parse_program()?;
    let ops = Planner::plan(&program)?;

    let runtime = Runtime::default_local();
    runtime.eval_ops(&ops, ctx).await
}
