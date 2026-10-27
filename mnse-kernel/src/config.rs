//! TOML Configuration Loader for MNSE Kernel.
//!
//! Loads `~/.mnse/kernel.toml` and individual module TOML files from `~/.mnse/modules/`.
//! Zero hardcodes — every operational parameter is configurable.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelConfig {
    pub daemon: DaemonConfig,
    pub event_bus: EventBusConfig,
    pub storage: StorageConfig,
    pub network: NetworkConfig,
    pub redaction: RedactionConfig,
    pub filters: FilterConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub instance_name: String,
    pub hot_reload: bool,
    pub tick_interval_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBusConfig {
    pub capacity: usize,
    pub priority_high_capacity: usize,
    pub priority_low_capacity: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageConfig {
    pub db_path: String,
    pub graph_path: String,
    pub noise_retention_days: u32,
    pub milestone_retention_days: u32,
    pub auto_prune_hours: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub http_bind: String,
    pub socket_path: String,
    pub rate_limit_per_conn: u32,
    pub rate_limit_global: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionConfig {
    pub privacy_mode: bool,
    pub redact_titles: bool,
    pub redact_urls: bool,
    pub redact_clipboard: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterConfig {
    pub noise_events: Vec<String>,
    pub high_priority_events: Vec<String>,
}

impl Default for KernelConfig {
    fn default() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let base_dir = home.join(".mnse");

        Self {
            daemon: DaemonConfig {
                instance_name: "mnse-main".to_string(),
                hot_reload: true,
                tick_interval_ms: 1000,
            },
            event_bus: EventBusConfig {
                capacity: 4096,
                priority_high_capacity: 2048,
                priority_low_capacity: 512,
            },
            storage: StorageConfig {
                db_path: base_dir.join("ledger.db").to_string_lossy().to_string(),
                graph_path: base_dir.join("graph.db").to_string_lossy().to_string(),
                noise_retention_days: 7,
                milestone_retention_days: 30,
                auto_prune_hours: 24,
            },
            network: NetworkConfig {
                http_bind: "127.0.0.1:7780".to_string(),
                socket_path: base_dir.join("kernel.sock").to_string_lossy().to_string(),
                rate_limit_per_conn: 100,
                rate_limit_global: 2000,
            },
            redaction: RedactionConfig {
                privacy_mode: true,
                redact_titles: true,
                redact_urls: true,
                redact_clipboard: true,
            },
            filters: FilterConfig {
                noise_events: vec![
                    "CursorMoved".to_string(),
                    "ResourcesUpdated".to_string(),
                    "AdapterHeartbeat".to_string(),
                    "MonitorChanged".to_string(),
                    "SelectionChanged".to_string(),
                    "IdleStateChanged".to_string(),
                    "RunningAppsChanged".to_string(),
                    "NetworkUpdated".to_string(),
                ],
                high_priority_events: vec![
                    "ErrorDetected".to_string(),
                ],
            },
        }
    }
}

impl KernelConfig {
    /// Load kernel configuration from default file locations or fallback to defaults
    pub fn load() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let config_file = home.join(".mnse/kernel.toml");

        if config_file.exists() {
            if let Ok(content) = std::fs::read_to_string(&config_file) {
                if let Ok(cfg) = toml::from_str::<KernelConfig>(&content) {
                    tracing::info!("Loaded configuration from {:?}", config_file);
                    return cfg;
                }
            }
        }

        let default_cfg = Self::default();
        let _ = default_cfg.save(&config_file);
        default_cfg
    }

    /// Save configuration to file
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
