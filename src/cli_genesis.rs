use chuang_agent::genesis_actuator::{
    AutoCliGenesisActuator, GenesisActuator, GenesisAskRequest, GenesisConfig,
};

use crate::cli_args::parse_genesis_ask;
use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn genesis_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("ask") => genesis_ask_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn genesis_ask_command(args: &[String]) -> Result<(), String> {
    let request = parse_genesis_ask(args)?;
    if !request.approve_exec {
        return Err("genesis_ask_requires_approve_exec: pass --approve-exec".to_string());
    }
    let mut config = GenesisConfig::new(request.profile_dir);
    config.program = request.program;
    config.cdp_port = request.cdp_port;
    config.timeout_ms = request.timeout_ms;

    let mut actuator = AutoCliGenesisActuator::new(config);
    let response = actuator
        .ask(GenesisAskRequest {
            prompt: request.prompt,
        })
        .map_err(|error| format!("genesis_ask_failed: {error:?}"))?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("genesis_channel: {}", response.channel.as_str());
            println!("answer: {}", response.answer);
            if let Some(repair) = response.primary_repair {
                println!("primary_repair_required: {}", repair.requires_approval);
                println!("primary_repair_reason: {}", repair.reason);
                println!(
                    "primary_repair_recommended_action: {}",
                    repair.recommended_action
                );
            }
            Ok(())
        }
        ControlOutputFormat::Json => print_json(&response),
    }
}
