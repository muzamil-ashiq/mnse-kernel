//! Abstract Syntax Tree (AST) definitions for K Language.

use mnse_types::ActionPayload;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KProgram {
    pub statements: Vec<KStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KStatement {
    /// Pure expression or query statement
    Expr(KExpr),

    /// Variable binding: `let $x = expr`
    Let {
        name: String,
        val: KExpr,
    },

    /// Conditional statement: `if cond { ... } else { ... }`
    If {
        cond: KExpr,
        then_branch: Vec<KStatement>,
        else_branch: Option<Vec<KStatement>>,
    },

    /// Error handling: `try { ... } catch (err) { ... }`
    TryCatch {
        try_body: Vec<KStatement>,
        catch_var: String,
        catch_body: Vec<KStatement>,
    },

    /// Persistent behavior: `behavior name { sense trigger -> { ... } }`
    Behavior {
        name: String,
        triggers: Vec<BehaviorTrigger>,
    },

    /// System health check (`k care`)
    Care,

    /// Daily summary query (`k summary today`)
    Summary {
        range: String,
    },

    /// Adapter management (`k adapter on os`, `k adapter list`, `k adapter off shell`)
    Adapter {
        action: String,
        name: Option<String>,
    },

    /// Action dispatch (`speak "hello"`, `look "up"`)
    Action(ActionPayload),

    /// Help command (`k help`)
    Help,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BehaviorTrigger {
    pub event_type_name: String,
    pub body: Vec<KStatement>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum KExpr {
    /// Domain 1: Current State query (current.file, current.app, current.state)
    Current(String),

    /// Domain 2: Recent Activity query (recent.errors, recent.files, recent.commands)
    Recent(String),

    /// Domain 2: Historical range query (history.today, history.yesterday)
    History(String),

    /// Domain 3: Causal link query (cause.latest_error, related "query")
    Causal {
        op: String,
        target: String,
    },

    /// Help reference
    Help,

    /// Variable reference ($it, $x)
    Var(String),

    /// Literal value (string, number, boolean, object, list)
    Literal(serde_json::Value),

    /// Chained pipeline: expr | expr | expr
    Pipeline(Vec<KExpr>),

    /// Data filter in pipeline: `where $it.exit_code > 0`
    Where(Box<KExpr>),

    /// Field projection in pipeline: `select field1, field2`
    Select(Vec<String>),

    /// Binary comparison or arithmetic (`$x == 10`, `$a + $b`)
    Binary {
        op: String,
        left: Box<KExpr>,
        right: Box<KExpr>,
    },
}
