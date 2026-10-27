//! Manifest-Driven Dynamic Capability Loader for MNSE Kernel & K Language.
//!
//! Loads user and system capability manifests from `~/.mnse/capabilities.d/*.toml`
//! (and custom `MNSE_CAPABILITIES_DIR`), allowing declarative extension of K language
//! capabilities without modifying or recompiling source code.

use super::evidence::{inspect_file_evidence, EngineInfo, StructuredEvidence};
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{OnceLock, RwLock};
use std::time::Instant;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityManifest {
    pub capability: CapabilityMeta,
    #[serde(default)]
    pub operations: HashMap<String, OperationDef>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilityMeta {
    pub namespace: String,
    pub description: Option<String>,
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationDef {
    pub description: Option<String>,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    pub parse_json: Option<bool>,
}

pub struct ManifestRegistry {
    manifests: RwLock<HashMap<String, CapabilityManifest>>,
}

impl ManifestRegistry {
    pub fn new() -> Self {
        Self {
            manifests: RwLock::new(HashMap::new()),
        }
    }

    /// Load manifest from TOML string
    pub fn load_manifest_str(&self, content: &str) -> Result<String> {
        let manifest: CapabilityManifest = toml::from_str(content)
            .map_err(|e| anyhow!("Failed to parse capability manifest TOML: {}", e))?;
        let ns = manifest.capability.namespace.clone();
        let mut map = self.manifests.write().map_err(|e| anyhow!("Lock poisoned: {}", e))?;
        map.insert(ns.clone(), manifest);
        Ok(ns)
    }

    /// Load manifest from a file
    pub fn load_manifest_file<P: AsRef<Path>>(&self, path: P) -> Result<String> {
        let content = std::fs::read_to_string(path.as_ref())?;
        self.load_manifest_str(&content)
    }

    /// Discover and load all `*.toml` manifests in a directory
    pub fn load_from_dir<P: AsRef<Path>>(&self, dir: P) -> usize {
        let d = dir.as_ref();
        if !d.is_dir() {
            return 0;
        }

        let mut loaded = 0;
        if let Ok(entries) = std::fs::read_dir(d) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("toml") {
                    if let Ok(ns) = self.load_manifest_file(&path) {
                        tracing::info!("Registered dynamic capability namespace: '{}' from {:?}", ns, path);
                        loaded += 1;
                    }
                }
            }
        }
        loaded
    }

    /// Check if a namespace is registered dynamically
    pub fn has_namespace(&self, ns: &str) -> bool {
        self.manifests
            .read()
            .map(|m| m.contains_key(ns))
            .unwrap_or(false)
    }

    /// Get registered manifest for a namespace
    pub fn get_manifest(&self, ns: &str) -> Option<CapabilityManifest> {
        self.manifests.read().ok()?.get(ns).cloned()
    }

    /// Dispatch invocation to dynamic capability handler
    pub fn dispatch(&self, namespace: &str, operation: &str, args: &[Value]) -> Result<Option<StructuredEvidence>> {
        let manifest = match self.get_manifest(namespace) {
            Some(m) => m,
            None => return Ok(None),
        };

        let op_def = manifest
            .operations
            .get(operation)
            .ok_or_else(|| anyhow!("Unknown operation '{}' on capability namespace '{}'", operation, namespace))?;

        let start_time = Instant::now();

        // Interpolate arguments into command template
        let mut rendered_args = Vec::new();
        let mut used_indices = std::collections::HashSet::new();

        for arg_template in &op_def.args {
            let mut arg_val = arg_template.clone();
            // Check for positional placeholders: {0}, {1}, {2}, etc.
            for (i, val) in args.iter().enumerate() {
                let placeholder = format!("{{{}}}", i);
                if arg_val.contains(&placeholder) {
                    let s = match val {
                        Value::String(s) => s.clone(),
                        _ => val.to_string(),
                    };
                    arg_val = arg_val.replace(&placeholder, &s);
                    used_indices.insert(i);
                }
            }
            // Named alias placeholders: {input} -> args[0], {output} -> args.last()
            if arg_val.contains("{input}") && !args.is_empty() {
                let s = args[0].as_str().unwrap_or("").to_string();
                arg_val = arg_val.replace("{input}", &s);
                used_indices.insert(0);
            }
            if arg_val.contains("{output}") && !args.is_empty() {
                let last_idx = args.len() - 1;
                let s = args[last_idx].as_str().unwrap_or("").to_string();
                arg_val = arg_val.replace("{output}", &s);
                used_indices.insert(last_idx);
            }
            rendered_args.push(arg_val);
        }

        // If template did not specify positional placeholders, append remaining unused args
        for (i, val) in args.iter().enumerate() {
            if !used_indices.contains(&i) {
                match val {
                    Value::String(s) => rendered_args.push(s.clone()),
                    _ => rendered_args.push(val.to_string()),
                }
            }
        }

        // Collect file evidence on potential input file if first argument points to a file on disk
        let input_ev = args
            .first()
            .and_then(|v| v.as_str())
            .and_then(inspect_file_evidence);

        // Execute command safely without shell expansion
        let output = Command::new(&op_def.command)
            .args(&rendered_args)
            .output()
            .map_err(|e| anyhow!("Failed to execute dynamic capability '{}': {}", op_def.command, e))?;

        let duration_ms = start_time.elapsed().as_millis() as u64;
        let stdout_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr_str = String::from_utf8_lossy(&output.stderr).trim().to_string();

        if !output.status.success() {
            return Err(anyhow!(
                "Capability '{}.{}' failed (exit code {:?}): {}",
                namespace,
                operation,
                output.status.code(),
                if stderr_str.is_empty() { &stdout_str } else { &stderr_str }
            ));
        }

        let output_ev = args
            .last()
            .and_then(|v| v.as_str())
            .and_then(inspect_file_evidence);

        let data = if op_def.parse_json != Some(false) {
            if let Ok(json_val) = serde_json::from_str::<Value>(&stdout_str) {
                json_val
            } else {
                json!({ "output": stdout_str })
            }
        } else {
            json!({ "output": stdout_str })
        };

        let mut warnings = Vec::new();
        if !stderr_str.is_empty() {
            warnings.push(stderr_str);
        }

        Ok(Some(StructuredEvidence {
            operation_id: Uuid::new_v4(),
            status: "success".to_string(),
            engine: EngineInfo {
                name: format!("dynamic:{}", namespace),
                version: manifest.capability.version,
            },
            input: input_ev,
            output: output_ev,
            data,
            warnings,
            duration_ms,
        }))
    }
}

static GLOBAL_REGISTRY: OnceLock<ManifestRegistry> = OnceLock::new();

pub fn global_registry() -> &'static ManifestRegistry {
    GLOBAL_REGISTRY.get_or_init(|| {
        let reg = ManifestRegistry::new();
        let custom_dir = std::env::var("MNSE_CAPABILITIES_DIR").ok().map(PathBuf::from);
        let default_dir = dirs::home_dir().map(|h| h.join(".mnse/capabilities.d"));

        if let Some(dir) = custom_dir.or(default_dir) {
            if dir.is_dir() {
                reg.load_from_dir(dir);
            }
        }
        reg
    })
}

/// Check if namespace is known (built-in or manifest-loaded)
pub fn is_known_namespace(ns: &str) -> bool {
    matches!(ns, "pdf" | "doc" | "git" | "file" | "proc" | "net") || global_registry().has_namespace(ns)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_load_manifest_and_dispatch() {
        let registry = ManifestRegistry::new();
        let toml_content = r#"
[capability]
namespace = "sysutil"
description = "System utility tools"
version = "1.0.0"

[operations.echo_test]
description = "Echoes inputs back"
command = "echo"
args = ["-n", "{0}"]
"#;

        let ns = registry.load_manifest_str(toml_content).unwrap();
        assert_eq!(ns, "sysutil");
        assert!(registry.has_namespace("sysutil"));

        let res = registry
            .dispatch("sysutil", "echo_test", &[json!("MNSE Dynamic Capability")])
            .unwrap();

        assert!(res.is_some());
        let ev = res.unwrap();
        assert_eq!(ev.status, "success");
        assert_eq!(ev.engine.name, "dynamic:sysutil");
        assert_eq!(ev.data["output"], "MNSE Dynamic Capability");
    }

    #[test]
    fn test_load_from_dir() {
        let dir = tempdir().unwrap();
        let manifest_path = dir.path().join("date_util.toml");
        std::fs::write(
            &manifest_path,
            r#"
[capability]
namespace = "timeutil"
version = "2.0"

[operations.year]
command = "date"
args = ["+%Y"]
"#,
        )
        .unwrap();

        let registry = ManifestRegistry::new();
        let loaded = registry.load_from_dir(dir.path());
        assert_eq!(loaded, 1);
        assert!(registry.has_namespace("timeutil"));

        let res = registry.dispatch("timeutil", "year", &[]).unwrap();
        assert!(res.is_some());
        let ev = res.unwrap();
        assert_eq!(ev.status, "success");
    }

    #[test]
    fn test_unknown_operation_error() {
        let registry = ManifestRegistry::new();
        registry
            .load_manifest_str(
                r#"
[capability]
namespace = "foo"
[operations.bar]
command = "true"
"#,
            )
            .unwrap();

        let err = registry.dispatch("foo", "non_existent", &[]);
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("Unknown operation"));
    }
}
