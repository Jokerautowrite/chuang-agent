use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::goal_run::{
    GoalCheckpoint, GoalIntegrationPolicy, GoalRun, GoalRunDiagnostics, GoalRunStore,
    GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn goal_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("plan") => goal_plan_command(&args[1..]),
        Some("show") => goal_show_command(&args[1..]),
        Some("checkpoint") => goal_checkpoint_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn goal_plan_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_plan(args)?;
    let store = GoalRunStore::new(&request.root);
    let mut goal_spec = GoalSpec::mainline_mvp(request.objective);
    goal_spec.goal_id = request.goal_id;
    let run = GoalRun::new(
        goal_spec,
        request.worker_plan,
        request.write_scopes,
        GoalValidationPlan::new(request.validation_commands),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .map_err(format_goal_run_error)?;
    let receipt = store.create(&run).map_err(format_goal_run_error)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_planned: {}", receipt.goal_id);
            println!("goal_path: {}", receipt.path);
            println!("goal_checkpoint_count: {}", receipt.checkpoint_count);
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

fn goal_show_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_show(args)?;
    let store = GoalRunStore::new(&request.root);
    let run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_id: {}", run.goal_spec.goal_id);
            println!("goal_objective: {}", run.goal_spec.objective);
            println!("goal_workers: {}", run.worker_plan.len());
            println!("goal_write_scopes: {}", run.disjoint_write_scopes.len());
            println!(
                "goal_validation_commands: {}",
                run.validation_plan.commands.len()
            );
            println!("goal_checkpoints: {}", run.checkpoint_log.len());
            let diagnostics = run.diagnostics();
            println!(
                "goal_worker_scope_complete: {}",
                diagnostics.worker_scope_complete
            );
            println!(
                "goal_worker_validation_complete: {}",
                diagnostics.worker_validation_complete
            );
            println!(
                "goal_validation_plan_complete: {}",
                diagnostics.validation_plan_complete
            );
            println!(
                "goal_checkpoint_log_complete: {}",
                diagnostics.checkpoint_log_complete
            );
            println!(
                "goal_executes_automatically: {}",
                diagnostics.executes_automatically
            );
            println!(
                "goal_bypasses_governance: {}",
                diagnostics.bypasses_governance
            );
            if let Some(last) = run.checkpoint_log.last() {
                println!("goal_last_checkpoint: {}", last.checkpoint_id);
                println!("goal_last_summary: {}", last.summary);
            }
        }
        ControlOutputFormat::Json => print_json(&GoalRunShowOutput {
            run: &run,
            goal_run_diagnostics: run.diagnostics(),
        })?,
    }
    Ok(())
}

fn goal_checkpoint_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_checkpoint(args)?;
    let store = GoalRunStore::new(&request.root);
    let checkpoint = GoalCheckpoint::new(
        request.checkpoint_id,
        request.summary,
        request.completed_worker_ids,
        request.validation_notes,
    );
    let receipt = store
        .record_checkpoint(&request.goal_id, checkpoint)
        .map_err(format_goal_run_error)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_checkpoint_recorded: {}", receipt.goal_id);
            println!("goal_path: {}", receipt.path);
            println!("goal_checkpoint_count: {}", receipt.checkpoint_count);
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

struct GoalPlanCliRequest {
    goal_id: String,
    objective: String,
    root: PathBuf,
    write_scopes: Vec<GoalWriteScope>,
    worker_plan: Vec<GoalWorkerPlan>,
    validation_commands: Vec<String>,
    output: ControlOutputFormat,
}

struct GoalShowCliRequest {
    goal_id: String,
    root: PathBuf,
    output: ControlOutputFormat,
}

struct GoalCheckpointCliRequest {
    goal_id: String,
    checkpoint_id: String,
    summary: String,
    completed_worker_ids: Vec<String>,
    validation_notes: Vec<String>,
    root: PathBuf,
    output: ControlOutputFormat,
}

#[derive(Serialize)]
struct GoalRunShowOutput<'a> {
    #[serde(flatten)]
    run: &'a GoalRun,
    goal_run_diagnostics: GoalRunDiagnostics,
}

fn parse_goal_plan(args: &[String]) -> Result<GoalPlanCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut objective: Option<String> = None;
    let mut root = default_goal_root();
    let mut write_paths: Vec<String> = Vec::new();
    let mut write_scopes: Vec<GoalWriteScope> = Vec::new();
    let mut worker_plan: Vec<GoalWorkerPlan> = Vec::new();
    let mut validation_commands: Vec<String> = Vec::new();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--objective" => objective = Some(take_value(args, &mut index, "--objective")?),
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--write-path" => write_paths.push(take_value(args, &mut index, "--write-path")?),
            "--scope" => write_scopes.push(parse_write_scope(&take_value(
                args, &mut index, "--scope",
            )?)?),
            "--worker" => worker_plan.push(parse_worker_plan(&take_value(
                args, &mut index, "--worker",
            )?)?),
            "--validation" => {
                validation_commands.push(take_value(args, &mut index, "--validation")?)
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    if write_paths.is_empty() {
        if write_scopes.is_empty() {
            write_paths.push(".".to_string());
        }
    }
    if !write_paths.is_empty() {
        write_scopes.push(GoalWriteScope::new("mainline", write_paths));
    }
    if validation_commands.is_empty() {
        validation_commands = GoalSpec::mainline_mvp("validation defaults").acceptance_checks;
    }
    if worker_plan.is_empty() {
        let default_worker_scope_ids = if write_scopes.is_empty() {
            vec!["mainline".to_string()]
        } else {
            write_scopes
                .iter()
                .map(|scope| scope.scope_id.clone())
                .collect()
        };
        worker_plan.push(GoalWorkerPlan::new(
            "main-process",
            "continue the goal from checkpoints",
            default_worker_scope_ids,
            validation_commands.clone(),
        ));
    }
    for worker in &mut worker_plan {
        if worker.validation_checks.is_empty() {
            worker.validation_checks = validation_commands.clone();
        }
    }

    Ok(GoalPlanCliRequest {
        goal_id,
        objective: objective.ok_or_else(|| "goal plan requires --objective".to_string())?,
        root,
        write_scopes,
        worker_plan,
        validation_commands,
        output,
    })
}

fn parse_write_scope(raw: &str) -> Result<GoalWriteScope, String> {
    let (scope_id, raw_paths) = raw
        .split_once('=')
        .ok_or_else(|| "--scope expects scope_id=path[,path...]".to_string())?;
    let paths = raw_paths
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Err("--scope requires at least one path".to_string());
    }
    Ok(GoalWriteScope::new(scope_id.trim(), paths))
}

fn parse_worker_plan(raw: &str) -> Result<GoalWorkerPlan, String> {
    let parts = raw.split('|').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err("--worker expects worker_id|scope_id[,scope_id...]|objective".to_string());
    }
    let write_scope_ids = parts[1]
        .split(',')
        .map(str::trim)
        .filter(|scope_id| !scope_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if write_scope_ids.is_empty() {
        return Err("--worker requires at least one scope id".to_string());
    }
    Ok(GoalWorkerPlan::new(
        parts[0],
        parts[2],
        write_scope_ids,
        Vec::new(),
    ))
}

fn parse_goal_show(args: &[String]) -> Result<GoalShowCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(GoalShowCliRequest {
        goal_id,
        root,
        output,
    })
}

fn parse_goal_checkpoint(args: &[String]) -> Result<GoalCheckpointCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut checkpoint_id: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut completed_worker_ids: Vec<String> = Vec::new();
    let mut validation_notes: Vec<String> = Vec::new();
    let mut root = default_goal_root();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--checkpoint-id" => {
                checkpoint_id = Some(take_value(args, &mut index, "--checkpoint-id")?)
            }
            "--summary" => summary = Some(take_value(args, &mut index, "--summary")?),
            "--completed-worker-id" => {
                completed_worker_ids.push(take_value(args, &mut index, "--completed-worker-id")?)
            }
            "--validation-note" => {
                validation_notes.push(take_value(args, &mut index, "--validation-note")?)
            }
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(GoalCheckpointCliRequest {
        goal_id,
        checkpoint_id: checkpoint_id.unwrap_or_else(default_checkpoint_id),
        summary: summary.ok_or_else(|| "goal checkpoint requires --summary".to_string())?,
        completed_worker_ids,
        validation_notes,
        root,
        output,
    })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn default_checkpoint_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("checkpoint-{nanos}")
}

fn default_goal_root() -> PathBuf {
    PathBuf::from("./context/goal-runs")
}

fn format_goal_run_error(error: chuang_agent::goal_run::GoalRunError) -> String {
    format!("goal_run_invalid: {}: {}", error.field, error.message)
}
