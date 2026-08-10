//! `plugin_registry` 模块。公开接口：struct PluginRegistry, PluginManifest, PluginRegistryCheck, PluginCheckItem, PluginReadinessEvidence, PluginCheckBoundary, PluginCheckEvidence, PluginPathEvidence；enum PluginKind；fn load_plugin_registry, check_plugin_registry, summarize_plugin_registry。

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
    pub capabilities: Vec<String>,
    pub dry_run_default: bool,
    pub executes_plugin: bool,
    pub reads_secret: bool,
    pub command_state: String,
    pub config_state: String,
    pub readiness: PluginReadinessEvidence,
    pub boundary: PluginCheckBoundary,
    pub evidence: PluginCheckEvidence,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginReadinessEvidence {
    pub state: String,
    pub blocking: bool,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginCheckBoundary {
    pub check_only: bool,
    pub executes_plugin: bool,
    pub reads_secret: bool,
    pub connects_external_service: bool,
    pub writes_files: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginCheckEvidence {
    pub manifest_loaded: bool,
    pub manifest_fields_checked: Vec<String>,
    pub path_checks: Vec<PluginPathEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginPathEvidence {
    pub field: String,
    pub configured: bool,
    pub state: String,
    pub resolved_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PluginRegistrySummary {
    pub registry_path: String,
    pub available: bool,
    pub ok: bool,
    pub plugin_count: usize,
    pub enabled_count: usize,
    pub issue_count: usize,
    pub evidence_available: bool,
    pub check_only: bool,
    pub executes_plugins: bool,
    pub reads_secret: bool,
    pub connects_external_service: bool,
    pub writes_files: bool,
    pub capability_count: usize,
    pub capabilities: Vec<String>,
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
            evidence_available: false,
            check_only: true,
            executes_plugins: false,
            reads_secret: false,
            connects_external_service: false,
            writes_files: false,
            capability_count: 0,
            capabilities: Vec::new(),
        };
    }

    match check_plugin_registry(path) {
        Ok(check) => {
            let capabilities = summarize_capabilities(&check.plugins);
            PluginRegistrySummary {
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
                evidence_available: true,
                check_only: check
                    .plugins
                    .iter()
                    .all(|plugin| plugin.boundary.check_only),
                executes_plugins: check.plugins.iter().any(|plugin| plugin.executes_plugin),
                reads_secret: check.plugins.iter().any(|plugin| plugin.reads_secret),
                connects_external_service: check
                    .plugins
                    .iter()
                    .any(|plugin| plugin.boundary.connects_external_service),
                writes_files: check
                    .plugins
                    .iter()
                    .any(|plugin| plugin.boundary.writes_files),
                capability_count: capabilities.len(),
                capabilities,
            }
        }
        Err(_) => PluginRegistrySummary {
            registry_path: path.display().to_string(),
            available: true,
            ok: false,
            plugin_count: 0,
            enabled_count: 0,
            issue_count: 1,
            evidence_available: false,
            check_only: true,
            executes_plugins: false,
            reads_secret: false,
            connects_external_service: false,
            writes_files: false,
            capability_count: 0,
            capabilities: Vec::new(),
        },
    }
}

fn summarize_capabilities(plugins: &[PluginCheckItem]) -> Vec<String> {
    let mut capabilities = std::collections::BTreeSet::new();
    for plugin in plugins {
        for capability in &plugin.capabilities {
            if !capability.trim().is_empty() {
                capabilities.insert(capability.clone());
            }
        }
    }
    capabilities.into_iter().collect()
}

fn check_plugin(base_dir: &Path, plugin: &PluginManifest) -> PluginCheckItem {
    let mut issues = Vec::new();
    let mut path_checks = Vec::new();
    if plugin.id.trim().is_empty() {
        issues.push("id_empty".to_string());
    }
    if plugin.display_name.trim().is_empty() {
        issues.push("display_name_empty".to_string());
    }
    let command_state = check_optional_path(
        base_dir,
        plugin.command.as_deref(),
        "command",
        &mut issues,
        &mut path_checks,
    );
    let config_state = check_optional_path(
        base_dir,
        plugin.config_path.as_deref(),
        "config_path",
        &mut issues,
        &mut path_checks,
    );
    let readiness = plugin_readiness(plugin.enabled, &issues);
    let boundary = PluginCheckBoundary {
        check_only: true,
        executes_plugin: false,
        reads_secret: false,
        connects_external_service: false,
        writes_files: false,
    };
    let evidence = PluginCheckEvidence {
        manifest_loaded: true,
        manifest_fields_checked: vec![
            "id".to_string(),
            "kind".to_string(),
            "display_name".to_string(),
            "command".to_string(),
            "config_path".to_string(),
            "capabilities".to_string(),
            "enabled".to_string(),
            "dry_run_default".to_string(),
        ],
        path_checks,
    };
    PluginCheckItem {
        id: plugin.id.clone(),
        kind: plugin.kind.clone(),
        enabled: plugin.enabled,
        capabilities: plugin.capabilities.clone(),
        dry_run_default: plugin.dry_run_default,
        executes_plugin: boundary.executes_plugin,
        reads_secret: boundary.reads_secret,
        command_state,
        config_state,
        readiness,
        boundary,
        evidence,
        issues,
    }
}

fn check_optional_path(
    base_dir: &Path,
    raw: Option<&str>,
    field: &str,
    issues: &mut Vec<String>,
    path_checks: &mut Vec<PluginPathEvidence>,
) -> String {
    let Some(raw) = raw else {
        path_checks.push(PluginPathEvidence {
            field: field.to_string(),
            configured: false,
            state: "none".to_string(),
            resolved_path: None,
        });
        return "none".to_string();
    };
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        issues.push(format!("{field}_empty"));
        path_checks.push(PluginPathEvidence {
            field: field.to_string(),
            configured: true,
            state: "empty".to_string(),
            resolved_path: None,
        });
        return "empty".to_string();
    }
    let path = resolve_path(base_dir, trimmed);
    if path.exists() {
        path_checks.push(PluginPathEvidence {
            field: field.to_string(),
            configured: true,
            state: "exists".to_string(),
            resolved_path: Some(path.display().to_string()),
        });
        "exists".to_string()
    } else {
        issues.push(format!("{field}_missing:{}", path.display()));
        path_checks.push(PluginPathEvidence {
            field: field.to_string(),
            configured: true,
            state: "missing".to_string(),
            resolved_path: Some(path.display().to_string()),
        });
        "missing".to_string()
    }
}

fn plugin_readiness(enabled: bool, issues: &[String]) -> PluginReadinessEvidence {
    if !enabled {
        return PluginReadinessEvidence {
            state: "disabled".to_string(),
            blocking: false,
            reason: "plugin_disabled_manifest_only".to_string(),
        };
    }
    if issues.is_empty() {
        PluginReadinessEvidence {
            state: "ready".to_string(),
            blocking: false,
            reason: "enabled_manifest_paths_checked".to_string(),
        }
    } else {
        PluginReadinessEvidence {
            state: "blocked".to_string(),
            blocking: true,
            reason: "enabled_manifest_has_issues".to_string(),
        }
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
