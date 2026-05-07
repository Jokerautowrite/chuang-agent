use std::path::PathBuf;

use chuang_agent::plugin_registry::{check_plugin_registry, load_plugin_registry};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn plugin_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => plugin_list_command(&args[1..]),
        Some("check") => plugin_check_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn plugin_list_command(args: &[String]) -> Result<(), String> {
    let request = parse_plugin_request(args)?;
    let registry = load_plugin_registry(&request.registry)?;
    let output = PluginListOutput {
        registry_path: request.registry.display().to_string(),
        plugin_count: registry.plugins.len(),
        plugins: registry.plugins,
    };
    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "plugin_registry path={} plugin_count={}",
                output.registry_path, output.plugin_count
            );
            for plugin in &output.plugins {
                println!(
                    "plugin id={} kind={:?} enabled={} command={} config={}",
                    plugin.id,
                    plugin.kind,
                    plugin.enabled,
                    plugin.command.as_deref().unwrap_or("none"),
                    plugin.config_path.as_deref().unwrap_or("none")
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }
    Ok(())
}

fn plugin_check_command(args: &[String]) -> Result<(), String> {
    let request = parse_plugin_request(args)?;
    let output = check_plugin_registry(&request.registry)?;
    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "plugin_registry_check ok={} path={} plugin_count={}",
                output.ok, output.registry_path, output.plugin_count
            );
            for plugin in &output.plugins {
                println!(
                    "plugin_check id={} kind={:?} enabled={} readiness={} reason={} capabilities={} dry_run_default={} command={} config={} executes_plugin={} reads_secret={} boundary_check_only={} issues={}",
                    plugin.id,
                    plugin.kind,
                    plugin.enabled,
                    plugin.readiness.state,
                    plugin.readiness.reason,
                    if plugin.capabilities.is_empty() {
                        "none".to_string()
                    } else {
                        plugin.capabilities.join(",")
                    },
                    plugin.dry_run_default,
                    plugin.command_state,
                    plugin.config_state,
                    plugin.executes_plugin,
                    plugin.reads_secret,
                    plugin.boundary.check_only,
                    if plugin.issues.is_empty() {
                        "none".to_string()
                    } else {
                        plugin.issues.join(",")
                    }
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }
    Ok(())
}

fn parse_plugin_request(args: &[String]) -> Result<PluginCliRequest, String> {
    let mut registry = PathBuf::from("plugins/registry.example.json");
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--registry" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "plugin command requires value after --registry".to_string())?;
                registry = PathBuf::from(value);
                index += 2;
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }
    Ok(PluginCliRequest { registry, output })
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PluginCliRequest {
    registry: PathBuf,
    output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PluginListOutput {
    registry_path: String,
    plugin_count: usize,
    plugins: Vec<chuang_agent::plugin_registry::PluginManifest>,
}
