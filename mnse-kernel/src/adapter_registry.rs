//! AdapterRegistry — Configuration and lifecycle state of adapters.
//!
//! Tracks which adapters are enabled/disabled via `~/.mnse/adapters.toml`.
//! Provides atomic on/off toggling, list discovery, and status introspection.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdapterEntry {
    pub name: String,
    pub enabled: bool,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdapterRegistryConfig {
    pub adapters: HashMap<String, AdapterEntry>,
}

pub struct AdapterRegistry {
    config_path: PathBuf,
    adapters: RwLock<HashMap<String, AdapterEntry>>,
}

impl AdapterRegistry {
    /// Create registry from default `~/.mnse/adapters.toml` or fallback
    pub fn load_default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_path = home.join(".mnse/adapters.toml");
        Self::load_from(config_path)
    }

    /// Load or initialize registry at specific path
    pub fn load_from<P: AsRef<Path>>(path: P) -> Self {
        let path_buf = path.as_ref().to_path_buf();
        let mut initial_adapters = HashMap::new();

        if path_buf.exists() {
            if let Ok(content) = std::fs::read_to_string(&path_buf) {
                if let Ok(cfg) = toml::from_str::<AdapterRegistryConfig>(&content) {
                    initial_adapters = cfg.adapters;
                }
            }
        }

        // Seed core default adapters if empty or missing
        if !initial_adapters.contains_key("shell") {
            initial_adapters.insert(
                "shell".to_string(),
                AdapterEntry {
                    name: "shell".to_string(),
                    enabled: true,
                    description: Some("Shell command and directory tracker".to_string()),
                },
            );
        }
        if !initial_adapters.contains_key("os") {
            initial_adapters.insert(
                "os".to_string(),
                AdapterEntry {
                    name: "os".to_string(),
                    enabled: true,
                    description: Some("Desktop window and workspace tracker".to_string()),
                },
            );
        }
        if !initial_adapters.contains_key("hyprland") {
            initial_adapters.insert(
                "hyprland".to_string(),
                AdapterEntry {
                    name: "hyprland".to_string(),
                    enabled: true,
                    description: Some("Hyprland compositor event tracker".to_string()),
                },
            );
        }
        if !initial_adapters.contains_key("fs") {
            initial_adapters.insert(
                "fs".to_string(),
                AdapterEntry {
                    name: "fs".to_string(),
                    enabled: true,
                    description: Some("Workspace filesystem and git change tracker".to_string()),
                },
            );
        }
        if !initial_adapters.contains_key("browser") {
            initial_adapters.insert(
                "browser".to_string(),
                AdapterEntry {
                    name: "browser".to_string(),
                    enabled: false,
                    description: Some("Browser tab and URL tracker".to_string()),
                },
            );
        }

        let registry = Self {
            config_path: path_buf,
            adapters: RwLock::new(initial_adapters),
        };

        let _ = registry.save();
        registry
    }

    /// Check if a specific adapter is currently enabled
    pub fn is_enabled(&self, name: &str) -> bool {
        if name == "system" || name == "k-script" || name == "unknown" {
            return true;
        }

        if let Ok(guard) = self.adapters.read() {
            if let Some(entry) = guard.get(name) {
                return entry.enabled;
            }
        }
        // Unknown adapters default to allowed
        true
    }

    /// Set an adapter's enabled state (on/off) and persist to disk
    pub fn set_enabled(&self, name: &str, enabled: bool) -> Result<bool> {
        let changed = {
            let mut guard = self.adapters.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            if let Some(entry) = guard.get_mut(name) {
                if entry.enabled != enabled {
                    entry.enabled = enabled;
                    true
                } else {
                    false
                }
            } else {
                guard.insert(
                    name.to_string(),
                    AdapterEntry {
                        name: name.to_string(),
                        enabled,
                        description: None,
                    },
                );
                true
            }
        };

        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    /// Return all registered adapters
    pub fn list(&self) -> Vec<AdapterEntry> {
        self.adapters
            .read()
            .map(|g| g.values().cloned().collect())
            .unwrap_or_default()
    }

    /// Persist current state to `adapters.toml`
    pub fn save(&self) -> Result<()> {
        if let Some(parent) = self.config_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let guard = self.adapters.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        let cfg = AdapterRegistryConfig {
            adapters: guard.clone(),
        };
        let toml_str = toml::to_string_pretty(&cfg)?;
        std::fs::write(&self.config_path, toml_str)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn test_adapter_registry_lifecycle() {
        let temp = NamedTempFile::new().unwrap();
        let registry = AdapterRegistry::load_from(temp.path());

        assert!(registry.is_enabled("os"));
        assert!(registry.is_enabled("shell"));
        assert!(!registry.is_enabled("browser"));

        registry.set_enabled("browser", true).unwrap();
        assert!(registry.is_enabled("browser"));

        registry.set_enabled("shell", false).unwrap();
        assert!(!registry.is_enabled("shell"));
    }
}
