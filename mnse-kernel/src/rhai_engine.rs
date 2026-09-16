//! Rhai RuleEngine implementation for MNSE Kernel.
//!
//! Evaluates policy rules, event priority classification, noise filtering,
//! identity recognition, and search query extraction in microseconds.

use mnse_types::Priority;
use rhai::{Engine, Scope, AST};

/// Trait defining the RuleEngine interface (allows swapping Rhai for K engine later)
pub trait RuleEngine: Send + Sync {
    fn classify_priority(&self, event_type: &str, observed_by: &str) -> Priority;
    fn is_noise(&self, event_type: &str) -> bool;
    fn is_browser(&self, app_name: &str) -> bool;
    fn detect_error(&self, exit_code: i32, command: &str) -> bool;
    fn extract_search_query(&self, url: &str) -> Option<String>;
}

/// Built-in 225-line Rhai standard library script
const RHAI_STDLIB: &str = r#"
fn classify_priority(event_type, observed_by) {
    if event_type == "ErrorDetected" || event_type == "ActionRequested" || event_type == "WakeWordDetected" {
        return "high";
    }
    if event_type == "ProcessExecuted" || event_type == "WindowFocused" || event_type == "FileSwitched" {
        return "medium";
    }
    return "low";
}

fn is_noise(event_type) {
    let noise = ["CursorMoved", "ResourcesUpdated", "AdapterHeartbeat",
                 "MonitorChanged", "SelectionChanged", "RunningAppsChanged",
                 "IdleStateChanged", "NetworkUpdated"];
    return noise.contains(event_type);
}

fn is_browser(app_name) {
    let app_lower = app_name.to_lower();
    let browsers = ["firefox", "chromium", "brave", "chrome", "zen-browser", "librewolf", "floorp"];
    for b in browsers {
        if app_lower.contains(b) {
            return true;
        }
    }
    return false;
}

fn detect_error(exit_code, command) {
    if exit_code != 0 {
        return true;
    }
    return false;
}

fn extract_search_query(url) {
    if url.contains("google.com/search") || url.contains("duckduckgo.com") {
        let parts = url.split("q=");
        if parts.len() > 1 {
            let query_part = parts[1].split("&")[0];
            return query_part;
        }
    }
    return "";
}
"#;

pub struct RhaiRuleEngine {
    engine: Engine,
    ast: AST,
}

impl RhaiRuleEngine {
    pub fn new() -> anyhow::Result<Self> {
        let engine = Engine::new();
        let ast = engine.compile(RHAI_STDLIB)?;
        Ok(Self { engine, ast })
    }
}

impl RuleEngine for RhaiRuleEngine {
    fn classify_priority(&self, event_type: &str, observed_by: &str) -> Priority {
        let mut scope = Scope::new();
        let result: String = self
            .engine
            .call_fn(&mut scope, &self.ast, "classify_priority", (event_type.to_string(), observed_by.to_string()))
            .unwrap_or_else(|_| "medium".to_string());

        match result.as_str() {
            "high" | "critical" => Priority::High,
            "low" => Priority::Low,
            _ => Priority::Medium,
        }
    }

    fn is_noise(&self, event_type: &str) -> bool {
        let mut scope = Scope::new();
        self.engine
            .call_fn(&mut scope, &self.ast, "is_noise", (event_type.to_string(),))
            .unwrap_or(false)
    }

    fn is_browser(&self, app_name: &str) -> bool {
        let mut scope = Scope::new();
        self.engine
            .call_fn(&mut scope, &self.ast, "is_browser", (app_name.to_string(),))
            .unwrap_or(false)
    }

    fn detect_error(&self, exit_code: i32, command: &str) -> bool {
        let mut scope = Scope::new();
        self.engine
            .call_fn(
                &mut scope,
                &self.ast,
                "detect_error",
                (exit_code as i64, command.to_string()),
            )
            .unwrap_or(exit_code != 0)
    }

    fn extract_search_query(&self, url: &str) -> Option<String> {
        let mut scope = Scope::new();
        let res: String = self
            .engine
            .call_fn(&mut scope, &self.ast, "extract_search_query", (url.to_string(),))
            .unwrap_or_default();

        if res.is_empty() {
            None
        } else {
            Some(res)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rhai_rule_engine() {
        let engine = RhaiRuleEngine::new().unwrap();
        assert!(engine.is_noise("CursorMoved"));
        assert!(!engine.is_noise("ProcessExecuted"));
        assert!(engine.is_browser("Firefox Web Browser"));
        assert!(engine.detect_error(1, "cargo build"));
        assert!(!engine.detect_error(0, "ls"));
        assert_eq!(engine.classify_priority("ErrorDetected", "shell"), Priority::High);
    }
}
