use chuang_agent::external_ai_dispatch::{
    build_external_ai_dispatch, ExternalAiDispatchError, ExternalAiDispatchRequest,
};

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn external_ai_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("dispatch") => external_ai_dispatch_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn external_ai_dispatch_command(args: &[String]) -> Result<(), String> {
    let request = parse_external_ai_dispatch(args)?;
    let output = build_external_ai_dispatch(request).map_err(format_external_ai_error)?;
    match parse_output(args) {
        ControlOutputFormat::Text => {
            println!(
                "external_ai_dispatch adapter={} dry_run={} platform={} audit_id={}",
                output.adapter, output.dry_run, output.request.platform, output.result.audit_id
            );
            println!(
                "connects_real_service={} writes_memory={} quality={}",
                output.connects_real_service, output.writes_memory, output.result.quality
            );
            println!("{}", output.result.result.summary);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }
    Ok(())
}

fn parse_external_ai_dispatch(args: &[String]) -> Result<ExternalAiDispatchRequest, String> {
    let mut platform = None;
    let mut task = None;
    let mut context = None;
    let mut session_hint = None;
    let mut timeout_ms = 60_000u64;
    let mut audit = true;
    let mut dry_run = false;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--platform" => platform = Some(take_value(args, &mut index, "--platform")?),
            "--task" => task = Some(take_value(args, &mut index, "--task")?),
            "--context" => context = Some(take_value(args, &mut index, "--context")?),
            "--session-hint" => {
                session_hint = Some(take_value(args, &mut index, "--session-hint")?)
            }
            "--timeout-ms" => {
                let value = take_value(args, &mut index, "--timeout-ms")?;
                timeout_ms = value.parse::<u64>().map_err(|_| {
                    "external-ai dispatch requires numeric --timeout-ms".to_string()
                })?;
            }
            "--no-audit" => {
                audit = false;
                index += 1;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--json" => index += 1,
            _ => return Err(usage()),
        }
    }

    let mut request = ExternalAiDispatchRequest::new(
        platform.ok_or_else(|| "external-ai dispatch requires --platform".to_string())?,
        task.ok_or_else(|| "external-ai dispatch requires --task".to_string())?,
        context.ok_or_else(|| "external-ai dispatch requires --context".to_string())?,
    );
    request.session_hint = session_hint;
    request.timeout_ms = timeout_ms;
    request.audit = audit;
    request.dry_run = dry_run;
    Ok(request)
}

fn parse_output(args: &[String]) -> ControlOutputFormat {
    if args.iter().any(|arg| arg == "--json") {
        ControlOutputFormat::Json
    } else {
        ControlOutputFormat::Text
    }
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn format_external_ai_error(error: ExternalAiDispatchError) -> String {
    format!(
        "external_ai_dispatch_invalid: {}: {}",
        error.field, error.message
    )
}
