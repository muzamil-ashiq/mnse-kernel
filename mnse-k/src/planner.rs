//! Execution Planner for K Language.
//!
//! Maps KProgram AST nodes into an executable sequence of KernelOps.

use crate::ast::*;
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
    Help,
    Literal(serde_json::Value),
    Var(String),
    Pipeline(Vec<KernelOp>),
    Where(Box<KernelOp>),
    Select(Vec<String>),
    Binary {
        op: String,
        left: Box<KernelOp>,
        right: Box<KernelOp>,
    },
    Sh(String),
    Index {
        target: Box<KernelOp>,
        index: Box<KernelOp>,
    },
    Slice {
        target: Box<KernelOp>,
        start: Option<Box<KernelOp>>,
        end: Option<Box<KernelOp>>,
    },
    FieldAccess {
        target: Box<KernelOp>,
        field: String,
    },
    Call {
        namespace: String,
        operation: String,
        args: Vec<KernelOp>,
    },
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
            KExpr::Binary { op, left, right } => Ok(KernelOp::Binary {
                op: op.clone(),
                left: Box::new(Self::plan_expr(left)?),
                right: Box::new(Self::plan_expr(right)?),
            }),
            KExpr::Sh(cmd) => Ok(KernelOp::Sh(cmd.clone())),
            KExpr::Index { target, index } => Ok(KernelOp::Index {
                target: Box::new(Self::plan_expr(target)?),
                index: Box::new(Self::plan_expr(index)?),
            }),
            KExpr::Slice { target, start, end } => Ok(KernelOp::Slice {
                target: Box::new(Self::plan_expr(target)?),
                start: start.as_ref().map(|s| Self::plan_expr(s).map(Box::new)).transpose()?,
                end: end.as_ref().map(|e| Self::plan_expr(e).map(Box::new)).transpose()?,
            }),
            KExpr::FieldAccess { target, field } => Ok(KernelOp::FieldAccess {
                target: Box::new(Self::plan_expr(target)?),
                field: field.clone(),
            }),
            KExpr::Call { namespace, operation, args } => {
                let planned_args = args.iter().map(Self::plan_expr).collect::<anyhow::Result<Vec<_>>>()?;
                Ok(KernelOp::Call {
                    namespace: namespace.clone(),
                    operation: operation.clone(),
                    args: planned_args,
                })
            }
        }
    }
}
