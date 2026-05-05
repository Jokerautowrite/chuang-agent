use chuang_agent::control_workflow::{build_unit_views, ControlUnitView};
use chuang_agent::kernel_status::{build_chuang_mvp_status, ChuangMvpStatus};
use chuang_agent::plugin_registry::{load_plugin_registry, PluginKind, PluginManifest};
use chuang_agent::slot_registry::build_runtime_slots;
use serde::Serialize;
use std::path::PathBuf;

use crate::cli_args::{effective_config_source, parse_status_cli_options, parse_status_output};
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_runtime::kernel_config_from_runtime;

pub(crate) fn console_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("snapshot") => console_snapshot_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn console_snapshot_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_status_cli_options(args)?;
    let kernel = kernel_config_from_runtime(&options.runtime)?;
    let status = build_chuang_mvp_status(&options.runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let control_units = build_unit_views(
        slots
            .control_plane
            .try_list_units()
            .map_err(|err| format!("console_control_list_failed: {err:?}"))?,
    );
    let plugins = load_console_plugins()?;

    let snapshot = ConsoleSnapshot {
        ok: true,
        source: effective_config_source(args)?.unwrap_or_else(|| "defaults".to_string()),
        status,
        control_units,
        plugins,
    };

    match output {
        ControlOutputFormat::Text => print_console_snapshot(&snapshot),
        ControlOutputFormat::Json => print_json(&snapshot)?,
    }

    Ok(())
}

fn print_console_snapshot(snapshot: &ConsoleSnapshot) {
    println!("console_ok: {}", snapshot.ok);
    println!("config_source: {}", snapshot.source);
    println!("provider: {}", snapshot.status.config.provider_kind);
    println!("model: {}", snapshot.status.config.model_name);
    println!("execution: {}", snapshot.status.slots.execution);
    println!(
        "atomic_tools: ok={} total={} mapped={} interface_only={} action_schema_version={} report_schema_version={}",
        snapshot.status.atomic_tools.ok,
        snapshot.status.atomic_tools.total_count,
        snapshot.status.atomic_tools.mapped_count,
        snapshot.status.atomic_tools.interface_only_count,
        snapshot.status.atomic_tools.tool_action_schema_version,
        snapshot.status.atomic_tools.tool_report_schema_version
    );
    println!(
        "atomic_tools_mapped: {}",
        format_name_list(&snapshot.status.atomic_tools.mapped_atomic_tool_names)
    );
    println!(
        "atomic_tools_interface_only: {}",
        format_name_list(
            &snapshot
                .status
                .atomic_tools
                .interface_only_atomic_tool_names
        )
    );
    println!("subagent: {}", snapshot.status.config.subagent_kind);
    println!(
        "project_readiness: ok={} state={}",
        snapshot.status.project_readiness.ok, snapshot.status.project_readiness.overall_state
    );
    println!(
        "channel_readiness: ok={} state={}",
        snapshot.status.channel_readiness.ok, snapshot.status.channel_readiness.overall_state
    );
    println!(
        "subagent_readiness: ok={} state={}",
        snapshot.status.subagent_readiness.ok, snapshot.status.subagent_readiness.overall_state
    );
    println!(
        "external_ai_readiness: ok={} state={}",
        snapshot.status.external_ai_readiness.ok,
        snapshot.status.external_ai_readiness.overall_state
    );
    println!(
        "release_readiness: ok={} name={} state={}",
        snapshot.status.release_readiness.ok,
        snapshot.status.release_readiness.release_name,
        snapshot.status.release_readiness.overall_state
    );
    println!(
        "release_acceptance: count={} connects_real_external_services={} verifies_real_external_services={} uses_stub_or_local_fixtures={}",
        snapshot.status.release_readiness.acceptance_count,
        snapshot
            .status
            .release_readiness
            .connects_real_external_services,
        snapshot
            .status
            .release_readiness
            .verifies_real_external_services,
        snapshot.status.release_readiness.uses_stub_or_local_fixtures
    );
    println!("control_units: {}", snapshot.control_units.len());
    println!("plugins: {}", snapshot.plugins.len());
    println!(
        "plugin_registry: available={} ok={} plugin_count={} enabled_count={} issue_count={}",
        snapshot.status.plugin_registry.available,
        snapshot.status.plugin_registry.ok,
        snapshot.status.plugin_registry.plugin_count,
        snapshot.status.plugin_registry.enabled_count,
        snapshot.status.plugin_registry.issue_count
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ConsoleSnapshot {
    ok: bool,
    source: String,
    status: ChuangMvpStatus,
    control_units: Vec<ControlUnitView>,
    plugins: Vec<PluginOverview>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PluginOverview {
    id: String,
    kind: PluginKind,
    display_name: String,
    enabled: bool,
    capabilities: Vec<String>,
}

fn format_name_list(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(",")
    }
}

fn load_console_plugins() -> Result<Vec<PluginOverview>, String> {
    let path = PathBuf::from("plugins/registry.example.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let registry = load_plugin_registry(&path)?;
    Ok(registry
        .plugins
        .into_iter()
        .map(|plugin: PluginManifest| PluginOverview {
            id: plugin.id,
            kind: plugin.kind,
            display_name: plugin.display_name,
            enabled: plugin.enabled,
            capabilities: plugin.capabilities,
        })
        .collect())
}
