//! Execution Planner for K Language.
//!
//! Maps KProgram AST nodes into an executable sequence of KernelOps.

use crate::ast::*;
use mnse_types::ActionPayload;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KernelOp {
    QueryCurrent(String),
    QueryRecent(String),
    QueryHistory(String),
    QueryCausal { op: String, target: String },
    Let { name: String, val: Box<KernelOp> },
    If {
        cond: Box<KernelOp>,
        then_branch: Vec<KernelOp>,
        else_branch: Option<Vec<KernelOp>>,
    },
    TryCatch {
        try_body: Vec<KernelOp>,
        catch_var: String,
        catch_body: Vec<KernelOp>,
    },
    Behavior {
        name: String,
        triggers: Vec<BehaviorTriggerOp>,
    },
    Care,
    Summary { range: String },
    Adapter { action: String, name: Option<String> },
    Action(ActionPayload),
    Help,
    Literal(serde_json::Value),
    Var(String),
    Pipeline(Vec<KernelOp>),
    Where(Box<KernelOp>),
    Select(Vec<String>),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BehaviorTriggerOp {
    pub event_type_name: String,
    pub body: Vec<KernelOp>,
}

pub struct Planner;

impl Planner {
    pub fn plan(program: &KProgram) -> anyhow::Result<Vec<KernelOp>> {
        let mut ops = Vec::new();
        for stmt in &program.statements {
            ops.push(Self::plan_statement(stmt)?);
        }
        Ok(ops)
    }

    fn plan_statement(stmt: &KStatement) -> anyhow::Result<KernelOp> {
        match stmt {
            KStatement::Expr(expr) => Self::plan_expr(expr),
            KStatement::Let { name, val } => Ok(KernelOp::Let {
                name: name.clone(),
                val: Box::new(Self::plan_expr(val)?),
            }),
            KStatement::If {
                cond,
                then_branch,
                else_branch,
            } => {
                let planned_then = then_branch
                    .iter()
                    .map(Self::plan_statement)
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let planned_else = else_branch
                    .as_ref()
                    .map(|b| b.iter().map(Self::plan_statement).collect::<anyhow::Result<Vec<_>>>())
                    .transpose()?;

                Ok(KernelOp::If {
                    cond: Box::new(Self::plan_expr(cond)?),
                    then_branch: planned_then,
                    else_branch: planned_else,
                })
            }
            KStatement::TryCatch {
                try_body,
                catch_var,
                catch_body,
            } => {
                let planned_try = try_body
                    .iter()
                    .map(Self::plan_statement)
                    .collect::<anyhow::Result<Vec<_>>>()?;
                let planned_catch = catch_body
                    .iter()
                    .map(Self::plan_statement)
                    .collect::<anyhow::Result<Vec<_>>>()?;

                Ok(KernelOp::TryCatch {
                    try_body: planned_try,
                    catch_var: catch_var.clone(),
                    catch_body: planned_catch,
                })
            }
            KStatement::Behavior { name, triggers } => {
                let planned_triggers = triggers
                    .iter()
                    .map(|t| {
                        let body_ops = t
                            .body
                            .iter()
                            .map(Self::plan_statement)
                            .collect::<anyhow::Result<Vec<_>>>()?;
                        Ok(BehaviorTriggerOp {
                            event_type_name: t.event_type_name.clone(),
                            body: body_ops,
                        })
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;

                Ok(KernelOp::Behavior {
                    name: name.clone(),
                    triggers: planned_triggers,
                })
            }
            KStatement::Care => Ok(KernelOp::Care),
            KStatement::Summary { range } => Ok(KernelOp::Summary {
                range: range.clone(),
            }),
            KStatement::Adapter { action, name } => Ok(KernelOp::Adapter {
                action: action.clone(),
                name: name.clone(),
            }),
            KStatement::Action(payload) => Ok(KernelOp::Action(payload.clone())),
            KStatement::Help => Ok(KernelOp::Help),
        }
    }

    fn plan_expr(expr: &KExpr) -> anyhow::Result<KernelOp> {
        match expr {
            KExpr::Current(target) => Ok(KernelOp::QueryCurrent(target.clone())),
            KExpr::Recent(target) => Ok(KernelOp::QueryRecent(target.clone())),
            KExpr::History(range) => Ok(KernelOp::QueryHistory(range.clone())),
            KExpr::Causal { op, target } => Ok(KernelOp::QueryCausal {
                op: op.clone(),
                target: target.clone(),
            }),
            KExpr::Help => Ok(KernelOp::Help),
            KExpr::Var(v) => Ok(KernelOp::Var(v.clone())),
            KExpr::Literal(val) => Ok(KernelOp::Literal(val.clone())),
            KExpr::Pipeline(stages) => {
                let planned_stages = stages
                    .iter()
                    .map(Self::plan_expr)
                    .collect::<anyhow::Result<Vec<_>>>()?;
                Ok(KernelOp::Pipeline(planned_stages))
            }
            KExpr::Where(pred) => Ok(KernelOp::Where(Box::new(Self::plan_expr(pred)?))),
            KExpr::Select(fields) => Ok(KernelOp::Select(fields.clone())),
            KExpr::Binary { op: _, left: _, right: _ } => {
                // Return literal for now
                Ok(KernelOp::Literal(serde_json::Value::Bool(true)))
            }
        }
    }
}
