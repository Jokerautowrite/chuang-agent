use std::path::PathBuf;

use chuang_agent::self_experiment::{
    ExperimentCompleteRequest, ExperimentOutcome, ExperimentRequest, SelfExperimentPlanner,
};

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn experiment_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("plan") => experiment_plan_command(&args[1..]),
        Some("complete") => experiment_complete_command(&args[1..]),
        Some("list") => experiment_list_command(&args[1..]),
        Some("show") => experiment_show_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn experiment_plan_command(args: &[String]) -> Result<(), String> {
    let request = parse_experiment_plan(args)?;
    let planner = SelfExperimentPlanner::new(&request.root);
    let receipt = planner.create_plan(&ExperimentRequest {
        goal: request.goal,
        success_criteria: request.success_criteria,
        time_budget_minutes: request.time_budget_minutes,
    })?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("experiment_planned: {}", receipt.experiment_id);
            println!("experiment_plan_path: {}", receipt.plan_path);
            println!(
                "experiment_time_budget_minutes: {}",
                receipt.time_budget_minutes
            );
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }

    Ok(())
}

fn experiment_complete_command(args: &[String]) -> Result<(), String> {
    let request = parse_experiment_complete(args)?;
    let planner = SelfExperimentPlanner::new(&request.root);
    let receipt = planner.complete(&ExperimentCompleteRequest {
        experiment_id: request.experiment_id,
        outcome: request.outcome,
        summary: request.summary,
        next_step: request.next_step,
    })?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("experiment_completed: {}", receipt.experiment_id);
            println!("experiment_report_path: {}", receipt.report_path);
            println!("experiment_outcome: {}", receipt.outcome);
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }

    Ok(())
}

fn experiment_list_command(args: &[String]) -> Result<(), String> {
    let request = parse_experiment_list(args)?;
    let planner = SelfExperimentPlanner::new(&request.root);
    let output = planner.list()?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("experiment_root: {}", output.root);
            println!("experiment_count: {}", output.count);
            for item in output.items {
                println!(
                    "experiment id={} status={} has_plan={} has_report={} plan={} report={}",
                    item.experiment_id,
                    item.status,
                    item.has_plan,
                    item.has_report,
                    item.plan_path,
                    item.report_path
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn experiment_show_command(args: &[String]) -> Result<(), String> {
    let request = parse_experiment_show(args)?;
    let planner = SelfExperimentPlanner::new(&request.root);
    let output = planner.show(&request.experiment_id)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("experiment_id: {}", output.experiment_id);
            println!("experiment_status: {}", output.status);
            println!("experiment_plan_path: {}", output.plan_path);
            println!("experiment_report_path: {}", output.report_path);
            if let Some(plan) = output.plan_markdown {
                println!("experiment_plan_markdown:\n{plan}");
            }
            if let Some(report) = output.report_markdown {
                println!("experiment_report_markdown:\n{report}");
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

struct ExperimentPlanCliRequest {
    goal: String,
    success_criteria: String,
    time_budget_minutes: u16,
    root: PathBuf,
    output: ControlOutputFormat,
}

struct ExperimentListCliRequest {
    root: PathBuf,
    output: ControlOutputFormat,
}

struct ExperimentShowCliRequest {
    experiment_id: String,
    root: PathBuf,
    output: ControlOutputFormat,
}

struct ExperimentCompleteCliRequest {
    experiment_id: String,
    outcome: ExperimentOutcome,
    summary: String,
    next_step: String,
    root: PathBuf,
    output: ControlOutputFormat,
}

fn parse_experiment_plan(args: &[String]) -> Result<ExperimentPlanCliRequest, String> {
    let mut goal: Option<String> = None;
    let mut success_criteria: Option<String> = None;
    let mut time_budget_minutes = 30u16;
    let mut root = PathBuf::from("./experiments");
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal" => {
                goal = Some(take_value(args, &mut index, "--goal")?);
            }
            "--success" => {
                success_criteria = Some(take_value(args, &mut index, "--success")?);
            }
            "--time-budget-minutes" => {
                let value = take_value(args, &mut index, "--time-budget-minutes")?;
                time_budget_minutes = value
                    .parse::<u16>()
                    .map_err(|_| format!("invalid --time-budget-minutes: {value}"))?;
            }
            "--root" => {
                root = PathBuf::from(take_value(args, &mut index, "--root")?);
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(ExperimentPlanCliRequest {
        goal: goal.ok_or_else(|| "experiment plan requires --goal".to_string())?,
        success_criteria: success_criteria
            .ok_or_else(|| "experiment plan requires --success".to_string())?,
        time_budget_minutes,
        root,
        output,
    })
}

fn parse_experiment_complete(args: &[String]) -> Result<ExperimentCompleteCliRequest, String> {
    let mut experiment_id: Option<String> = None;
    let mut outcome: Option<ExperimentOutcome> = None;
    let mut summary: Option<String> = None;
    let mut next_step: Option<String> = None;
    let mut root = PathBuf::from("./experiments");
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--experiment-id" => {
                experiment_id = Some(take_value(args, &mut index, "--experiment-id")?);
            }
            "--outcome" => {
                let value = take_value(args, &mut index, "--outcome")?;
                outcome = Some(ExperimentOutcome::parse(&value)?);
            }
            "--summary" => {
                summary = Some(take_value(args, &mut index, "--summary")?);
            }
            "--next" => {
                next_step = Some(take_value(args, &mut index, "--next")?);
            }
            "--root" => {
                root = PathBuf::from(take_value(args, &mut index, "--root")?);
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(ExperimentCompleteCliRequest {
        experiment_id: experiment_id
            .ok_or_else(|| "experiment complete requires --experiment-id".to_string())?,
        outcome: outcome.ok_or_else(|| "experiment complete requires --outcome".to_string())?,
        summary: summary.ok_or_else(|| "experiment complete requires --summary".to_string())?,
        next_step: next_step.ok_or_else(|| "experiment complete requires --next".to_string())?,
        root,
        output,
    })
}

fn parse_experiment_list(args: &[String]) -> Result<ExperimentListCliRequest, String> {
    let mut root = PathBuf::from("./experiments");
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => {
                root = PathBuf::from(take_value(args, &mut index, "--root")?);
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(ExperimentListCliRequest { root, output })
}

fn parse_experiment_show(args: &[String]) -> Result<ExperimentShowCliRequest, String> {
    let mut experiment_id: Option<String> = None;
    let mut root = PathBuf::from("./experiments");
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--experiment-id" => {
                experiment_id = Some(take_value(args, &mut index, "--experiment-id")?);
            }
            "--root" => {
                root = PathBuf::from(take_value(args, &mut index, "--root")?);
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(ExperimentShowCliRequest {
        experiment_id: experiment_id
            .ok_or_else(|| "experiment show requires --experiment-id".to_string())?,
        root,
        output,
    })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("experiment plan requires value after {flag}"))?
        .clone();
    *index += 2;
    Ok(value)
}
