use std::path::PathBuf;

use chuang_agent::self_experiment::{
    ExperimentCompleteRequest, ExperimentOutcome, ExperimentRequest, SelfExperimentPlanner,
};

use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_skill::{run_skill_solidify, skill_propose_request_for_experiment};

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
    let outcome = request.outcome.clone();
    let experiment_id = request.experiment_id.clone();
    let summary = request.summary.clone();
    let receipt = planner.complete(&ExperimentCompleteRequest {
        experiment_id: experiment_id.clone(),
        outcome: outcome.clone(),
        summary: summary.clone(),
        next_step: request.next_step,
    })?;

    // Self-experiment closure: when a benchmark-gated experiment succeeds,
    // solidify the learned skill through the same gate `skill solidify`
    // enforces (strict score improvement required; nothing is written on
    // refusal). Kept separate from the experiment report itself.
    let solidify_output = if request.benchmark_gate.is_some() {
        if outcome != ExperimentOutcome::Success {
            return Err(
                "experiment_complete_solidify_refused: benchmark gate only allowed on success"
                    .to_string(),
            );
        }
        let after_score = request.benchmark_after_score.ok_or_else(|| {
            "experiment_complete_solidify_refused: --benchmark-after-score required with --benchmark-gate"
                .to_string()
        })?;
        let skills_root = request
            .skills_root
            .clone()
            .unwrap_or_else(|| PathBuf::from("data/skills"));
        let proposal = skill_propose_request_for_experiment(
            &experiment_id,
            &summary,
            request.agent_id.as_deref().unwrap_or("chuang"),
        );
        Some(run_skill_solidify(
            &proposal,
            &skills_root,
            "benchmark-gated-experiment",
            None,
            Some("solidified after benchmark-gated experiment success"),
            request.approval_threshold,
            request.benchmark_gate.as_deref(),
            Some(after_score),
            request.benchmark_root.as_deref(),
        )?)
    } else {
        None
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("experiment_completed: {}", receipt.experiment_id);
            println!("experiment_report_path: {}", receipt.report_path);
            println!("experiment_outcome: {}", receipt.outcome);
            if let Some(output) = &solidify_output {
                let (gate, passed, best, required, writes, skills_root) =
                    output.benchmark_gate_summary();
                println!(
                    "experiment_solidified: gate={} passed={} best={} required={} writes={} skills_root={}",
                    gate.unwrap_or("none"),
                    passed,
                    best.map(|score| score.to_string()).unwrap_or_else(|| "none".to_string()),
                    required.map(|score| score.to_string()).unwrap_or_else(|| "none".to_string()),
                    writes,
                    skills_root,
                );
            }
        }
        ControlOutputFormat::Json => {
            let mut value = serde_json::to_value(&receipt).map_err(|e| e.to_string())?;
            if let Some(output) = &solidify_output {
                value["solidified_skill"] =
                    serde_json::to_value(output).map_err(|e| e.to_string())?;
            }
            print_json(&value)?;
        }
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
    benchmark_gate: Option<String>,
    benchmark_after_score: Option<u16>,
    benchmark_root: Option<PathBuf>,
    skills_root: Option<PathBuf>,
    agent_id: Option<String>,
    approval_threshold: u16,
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
    let mut benchmark_gate: Option<String> = None;
    let mut benchmark_after_score: Option<u16> = None;
    let mut benchmark_root: Option<PathBuf> = None;
    let mut skills_root: Option<PathBuf> = None;
    let mut agent_id: Option<String> = None;
    let mut approval_threshold = 80u16;

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
            "--benchmark-gate" => {
                let value = take_value(args, &mut index, "--benchmark-gate")?;
                if value.trim().is_empty() {
                    return Err(
                        "experiment complete --benchmark-gate must not be empty".to_string()
                    );
                }
                benchmark_gate = Some(value);
            }
            "--benchmark-after-score" => {
                let value = take_value(args, &mut index, "--benchmark-after-score")?;
                benchmark_after_score =
                    Some(value.parse::<u16>().map_err(|_| {
                        "--benchmark-after-score requires numeric value".to_string()
                    })?);
            }
            "--benchmark-root" => {
                let value = take_value(args, &mut index, "--benchmark-root")?;
                if value.trim().is_empty() {
                    return Err(
                        "experiment complete --benchmark-root must not be empty".to_string()
                    );
                }
                benchmark_root = Some(PathBuf::from(value));
            }
            "--skills-root" => {
                let value = take_value(args, &mut index, "--skills-root")?;
                if value.trim().is_empty() {
                    return Err("experiment complete --skills-root must not be empty".to_string());
                }
                skills_root = Some(PathBuf::from(value));
            }
            "--agent-id" => {
                let value = take_value(args, &mut index, "--agent-id")?;
                if value.trim().is_empty() {
                    return Err("experiment complete --agent-id must not be empty".to_string());
                }
                agent_id = Some(value);
            }
            "--approval-threshold" => {
                let value = take_value(args, &mut index, "--approval-threshold")?;
                approval_threshold = value
                    .parse::<u16>()
                    .map_err(|_| "--approval-threshold requires numeric value".to_string())?;
                if approval_threshold > 100 {
                    return Err("--approval-threshold must be <= 100".to_string());
                }
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
        benchmark_gate,
        benchmark_after_score,
        benchmark_root,
        skills_root,
        agent_id,
        approval_threshold,
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
