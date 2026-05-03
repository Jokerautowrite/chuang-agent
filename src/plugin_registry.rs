use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginRegistry {
    pub plugins: Vec<PluginManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginManifest {
    pub id: String,
    pub kind: PluginKind,
    pub display_name: String,
    pub command: Option<String>,
    pub config_path: Option<String>,
    #[serde(default)]
    pub capabilities: Vec<String>,
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub dry_run_default: bool,
    #[serde(default)]
    pub notes: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginKind {
    Channel,
    SubagentRunner,
    ControlAdapter,
    ActuatorAdapter,
    GenesisAdapter,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginRegistryCheck {
    pub ok: bool,
    pub registry_path: String,
    pub plugin_count: usize,
    pub plugins: Vec<PluginCheckItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginCheckItem {
    pub id: String,
    pub kind: PluginKind,
    pub enabled: bool,
    pub command_state: String,
    pub config_state: String,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginRegistrySummary {
    pub registry_path: String,
    pub available: bool,
    pub ok: bool,
    pub plugin_count: usize,
    pub enabled_count: usize,
    pub issue_count: usize,
}

pub fn load_plugin_registry(path: &Path) -> Result<PluginRegistry, String> {
    let content = fs::read_to_string(path).map_err(|e| {
        format!(
            "plugin_registry_read_failed path={} error={e}",
            path.display()
        )
    })?;
    let registry = serde_json::from_str::<PluginRegistry>(&content).map_err(|e| {
        format!(
            "plugin_registry_parse_failed path={} error={e}",
            path.display()
        )
    })?;
    validate_unique_ids(&registry)?;
    Ok(registry)
}

pub fn check_plugin_registry(path: &Path) -> Result<PluginRegistryCheck, String> {
    let registry = load_plugin_registry(path)?;
    let base_dir = path.parent().unwrap_or_else(|| Path::new("."));
    let plugins = registry
        .plugins
        .iter()
        .map(|plugin| check_plugin(base_dir, plugin))
        .collect::<Vec<_>>();
    // Disabled entries are manifest/readiness records only.
    // They may still surface path issues for visibility, but they do not
    // make the registry unhealthy until they are enabled.
    let ok = plugins
        .iter()
        .all(|plugin| !plugin.enabled || plugin.issues.is_empty());
    Ok(PluginRegistryCheck {
        ok,
        registry_path: path.display().to_string(),
        plugin_count: registry.plugins.len(),
        plugins,
    })
}

pub fn summarize_plugin_registry(path: &Path) -> PluginRegistrySummary {
    if !path.exists() {
        return PluginRegistrySummary {
            registry_path: path.display().to_string(),
            available: false,
            ok: false,
            plugin_count: 0,
            enabled_count: 0,
            issue_count: 0,
        };
    }

    match check_plugin_registry(path) {
        Ok(check) => PluginRegistrySummary {
            registry_path: check.registry_path,
            available: true,
            ok: check.ok,
            plugin_count: check.plugin_count,
            enabled_count: check.plugins.iter().filter(|plugin| plugin.enabled).count(),
            issue_count: check
                .plugins
                .iter()
                .filter(|plugin| plugin.enabled)
                .map(|plugin| plugin.issues.len())
                .sum(),
        },
        Err(_) => PluginRegistrySummary {
            registry_path: path.display().to_string(),
            available: true,
            ok: false,
            plugin_count: 0,
            enabled_count: 0,
            issue_count: 1,
        },
    }
}

fn check_plugin(base_dir: &Path, plugin: &PluginManifest) -> PluginCheckItem {
    let mut issues = Vec::new();
    if plugin.id.trim().is_empty() {
        issues.push("id_empty".to_string());
    }
    if plugin.display_name.trim().is_empty() {
        issues.push("display_name_empty".to_string());
    }
    let command_state =
        check_optional_path(base_dir, plugin.command.as_deref(), "command", &mut issues);
    let config_state = check_optional_path(
        base_dir,
        plugin.config_path.as_deref(),
        "config_path",
        &mut issues,
    );
    PluginCheckItem {
        id: plugin.id.clone(),
        kind: plugin.kind.clone(),
        enabled: plugin.enabled,
        command_state,
        config_state,
        issues,
    }
}

fn check_optional_path(
    base_dir: &Path,
    raw: Option<&str>,
    field: &str,
    issues: &mut Vec<String>,
) -> String {
    let Some(raw) = raw else {
        return "none".to_string();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        issues.push(format!("{field}_empty"));
        return "empty".to_string();
    }
    let path = resolve_path(base_dir, trimmed);
    if path.exists() {
        "exists".to_string()
    } else {
        issues.push(format!("{field}_missing:{}", path.display()));
        "missing".to_string()
    }
}

fn resolve_path(base_dir: &Path, raw: &str) -> PathBuf {
    let path = PathBuf::from(raw);
    if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    }
}

fn validate_unique_ids(registry: &PluginRegistry) -> Result<(), String> {
    let mut ids = std::collections::BTreeSet::new();
    for plugin in &registry.plugins {
        if !ids.insert(plugin.id.clone()) {
            return Err(format!("plugin_registry_duplicate_id: {}", plugin.id));
        }
    }
    Ok(())
}
