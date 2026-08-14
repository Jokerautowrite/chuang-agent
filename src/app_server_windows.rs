//! Windows boundary for the Unix-domain-socket app server.

use std::path::{Path, PathBuf};

use chuang_agent::path_utils::normalize_path_lexically;
use chuang_agent::runtime_config::{
    ConfigSummary, IdentityBootstrapConfig, IdentityMemoryConfig, ProviderConfig, RulesConfig,
    RuntimeConfig, SubagentQueueConfig,
};
use chuang_agent::runtime_config_file::load_runtime_config_file;

pub(crate) fn app_server_command(_args: &[String]) -> Result<(), String> {
    Err(
        "app_server_socket_transport_unsupported_on_windows: use the default local REPL mode"
            .to_string(),
    )
}

pub(crate) fn build_runtime_for_workspace(workspace_root: &str) -> Result<RuntimeConfig, String> {
    let base_dir = if workspace_root.trim().is_empty() {
        std::env::current_dir().map_err(|error| format!("workspace_root_failed: {error}"))?
    } else {
        PathBuf::from(workspace_root)
    };
    let config_path = base_dir.join("config.toml");
    let mut runtime = if config_path.exists() {
        load_runtime_config_file(&config_path)
            .map_err(|error| format!("runtime_config_load_failed: {error:?}"))?
    } else {
        RuntimeConfig::new(base_dir.join("data/chuang-agent.db"))
    };
    normalize_runtime_paths(&mut runtime, &base_dir);
    runtime.permission.workspace_root = base_dir;
    Ok(runtime)
}

fn normalize_runtime_paths(runtime: &mut RuntimeConfig, base_dir: &Path) {
    runtime.db_path = resolve_path_if_relative(base_dir, runtime.db_path.clone());
    runtime.identity_memory = match runtime.identity_memory.clone() {
        IdentityMemoryConfig::HermesDualFile {
            root,
            user_max_chars,
            memory_max_chars,
        } => IdentityMemoryConfig::HermesDualFile {
            root: resolve_path_if_relative(base_dir, root),
            user_max_chars,
            memory_max_chars,
        },
    };
    runtime.subagent_queue = SubagentQueueConfig {
        root: resolve_path_if_relative(base_dir, runtime.subagent_queue.root.clone()),
    };
    runtime.identity_bootstrap = IdentityBootstrapConfig {
        root: resolve_path_if_relative(base_dir, runtime.identity_bootstrap.root.clone()),
        soul_path: resolve_path_if_relative(base_dir, runtime.identity_bootstrap.soul_path.clone()),
        story_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.story_path.clone(),
        ),
        first_wake_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.first_wake_path.clone(),
        ),
        agents_registry_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.agents_registry_path.clone(),
        ),
    };
    runtime.rules = RulesConfig {
        root: resolve_path_if_relative(base_dir, runtime.rules.root.clone()),
        core_path: resolve_path_if_relative(base_dir, runtime.rules.core_path.clone()),
    };
    normalize_provider_paths(&mut runtime.provider, base_dir);
}

fn normalize_provider_paths(provider: &mut ProviderConfig, base_dir: &Path) {
    match provider {
        ProviderConfig::Fake { .. } => {}
        ProviderConfig::OpenAICompatible(config) => {
            if let Some(path) = &config.tls_ca_cert_path {
                config.tls_ca_cert_path = Some(resolve_path_if_relative(base_dir, path.clone()));
            }
        }
        ProviderConfig::AnthropicCompatible(config) => {
            if let Some(path) = &config.tls_ca_cert_path {
                config.tls_ca_cert_path = Some(resolve_path_if_relative(base_dir, path.clone()));
            }
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => {
            normalize_provider_paths(primary, base_dir);
            normalize_provider_paths(fallback, base_dir);
        }
    }
}

fn resolve_path_if_relative(base_dir: &Path, path: PathBuf) -> PathBuf {
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    resolved
        .canonicalize()
        .unwrap_or_else(|_| normalize_path_lexically(&resolved))
}

pub(crate) fn app_server_health_diagnostic_status(summary: &ConfigSummary) -> &'static str {
    if summary.placeholder_warnings.is_empty() {
        "ready"
    } else {
        "warning"
    }
}

pub(crate) fn app_server_health_diagnostic_summary(
    summary: &ConfigSummary,
    diagnostic_mode: bool,
) -> String {
    if summary.placeholder_warnings.is_empty() {
        if diagnostic_mode {
            "Windows local runtime config is ready in diagnostic mode; no live provider request was made."
        } else {
            "Windows local runtime config is ready; no live provider request was made."
        }
        .to_string()
    } else {
        format!(
            "Windows local runtime loaded with {} local warning(s).",
            summary.placeholder_warnings.len()
        )
    }
}

pub(crate) fn app_server_health_next_actions(summary: &ConfigSummary) -> Vec<String> {
    let mut actions = Vec::new();
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("provider=fake"))
    {
        actions.push("configure a real provider before expecting live conversation".to_string());
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("actuator=fake"))
    {
        actions.push(
            "Windows real actuator is disabled; keep fake mode or install an allowlisted Windows adapter"
                .to_string(),
        );
    }
    actions
}
