//! `cli_config` 模块。内部实现模块（无公开顶层项）。

use std::fs;

use chuang_agent::kernel_status::build_chuang_mvp_status;

use crate::cli_args::{
    effective_config_source, parse_config_init, parse_status_cli_options, parse_status_output,
};
use crate::cli_output::{print_config_summary, print_json, usage, ControlOutputFormat};
use crate::cli_runtime::kernel_config_from_runtime;
use crate::cli_types::{ConfigCheckCliOutput, ConfigInitCliOutput};

const DEFAULT_CONFIG_TEMPLATE: &str = include_str!("../config.example.toml");

pub(crate) fn config_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("check") => config_check_command(&args[1..]),
        Some("show") => config_show_command(&args[1..]),
        Some("init") => config_init_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn config_check_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_status_cli_options(args)?;
    let kernel = kernel_config_from_runtime(&options.runtime)?;
    build_chuang_mvp_status(&options.runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let result = ConfigCheckCliOutput {
        ok: true,
        source: effective_config_source(args)?.unwrap_or_else(|| "defaults".to_string()),
        summary: options.runtime.summary(),
    };

    match output {
        ControlOutputFormat::Text => {
            println!(
                "config_ok source={} provider={} model={} subagent={} queue_root={}",
                result.source,
                result.summary.provider_kind,
                result.summary.model_name,
                result.summary.subagent_kind,
                result.summary.subagent_queue_root
            );
        }
        ControlOutputFormat::Json => print_json(&result)?,
    }

    Ok(())
}

fn config_show_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_status_cli_options(args)?;
    options
        .runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let result = ConfigCheckCliOutput {
        ok: true,
        source: effective_config_source(args)?.unwrap_or_else(|| "defaults".to_string()),
        summary: options.runtime.summary(),
    };

    match output {
        ControlOutputFormat::Text => {
            print_config_summary(result.ok, &result.source, &result.summary)
        }
        ControlOutputFormat::Json => print_json(&result)?,
    }

    Ok(())
}

fn config_init_command(args: &[String]) -> Result<(), String> {
    let request = parse_config_init(args)?;
    if request.path.exists() {
        return Err(format!(
            "config_init_refused: path already exists: {}",
            request.path.display()
        ));
    }
    if let Some(parent) = request
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent).map_err(|e| {
            format!(
                "config_init_parent_create_failed path={} error={e}",
                parent.display()
            )
        })?;
    }
    fs::write(&request.path, DEFAULT_CONFIG_TEMPLATE).map_err(|e| {
        format!(
            "config_init_write_failed path={} error={e}",
            request.path.display()
        )
    })?;

    let output = ConfigInitCliOutput {
        written: true,
        path: request.path.display().to_string(),
    };
    match request.output {
        ControlOutputFormat::Text => {
            println!("config_initialized path={}", output.path);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}
