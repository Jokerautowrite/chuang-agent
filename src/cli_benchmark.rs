use std::fs;
use std::path::PathBuf;

use chuang_agent::benchmark::{
    BenchmarkDef, BenchmarkRunRequest, BenchmarkStore, CaseScore,
};

use crate::cli_output::{print_json, usage, ControlOutputFormat};

const DEFAULT_BENCHMARK_ROOT: &str = "benchmarks";

pub(crate) fn benchmark_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => benchmark_list_command(&args[1..]),
        Some("init") => benchmark_init_command(&args[1..]),
        Some("verify") => benchmark_verify_command(&args[1..]),
        Some("run") => benchmark_run_command(&args[1..]),
        Some("show") => benchmark_show_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn benchmark_root(args: &[String]) -> Result<(PathBuf, ControlOutputFormat), String> {
    let mut root = PathBuf::from(DEFAULT_BENCHMARK_ROOT);
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
    Ok((root, output))
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("benchmark {flag} requires a value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn benchmark_list_command(args: &[String]) -> Result<(), String> {
    let (root, output) = benchmark_root(args)?;
    let store = BenchmarkStore::new(&root);
    let ids = store.list()?;
    match output {
        ControlOutputFormat::Text => {
            println!("benchmark_root: {}", root.display());
            println!("benchmark_count: {}", ids.len());
            for id in ids {
                println!("benchmark id={id}");
            }
        }
        ControlOutputFormat::Json => print_json(&ids)?,
    }
    Ok(())
}

fn benchmark_init_command(args: &[String]) -> Result<(), String> {
    let mut def_path: Option<PathBuf> = None;
    let mut root = PathBuf::from(DEFAULT_BENCHMARK_ROOT);
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--def" => {
                def_path = Some(PathBuf::from(take_value(args, &mut index, "--def")?));
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
    let def_path = def_path.ok_or_else(|| "benchmark init requires --def <definition.json>".to_string())?;
    let raw = fs::read_to_string(&def_path)
        .map_err(|e| format!("cannot read def file {}: {e}", def_path.display()))?;
    let def: BenchmarkDef = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid benchmark definition: {e}"))?;

    // Isolation is a hard gate: refuse to install a definition whose statement
    // leaks the rubric to the Target agent.
    let issues = BenchmarkStore::verify_isolation(&def);
    if !issues.is_empty() {
        return Err(format!("isolation violations:\n{}", issues.join("\n")));
    }

    let store = BenchmarkStore::new(&root);
    let def_path_written = store.write_def(&def)?;
    match output {
        ControlOutputFormat::Text => {
            println!("benchmark_init: {}", def.id);
            println!("benchmark_capability: {}", def.capability);
            println!("benchmark_version: {}", def.version);
            println!("benchmark_case_count: {}", def.cases.len());
            println!("benchmark_def_path: {}", def_path_written.display());
        }
        ControlOutputFormat::Json => print_json(&def)?,
    }
    Ok(())
}

fn benchmark_verify_command(args: &[String]) -> Result<(), String> {
    let mut benchmark_id: Option<String> = None;
    let mut root = PathBuf::from(DEFAULT_BENCHMARK_ROOT);
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--id" => {
                benchmark_id = Some(take_value(args, &mut index, "--id")?);
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
    let benchmark_id =
        benchmark_id.ok_or_else(|| "benchmark verify requires --id <benchmark-id>".to_string())?;
    let store = BenchmarkStore::new(&root);
    let issues = store.verify(&benchmark_id)?;

    match output {
        ControlOutputFormat::Text => {
            println!("benchmark_id: {benchmark_id}");
            println!("benchmark_verify: {}", if issues.is_empty() { "ok" } else { "violations" });
            for issue in &issues {
                println!("  - {issue}");
            }
        }
        ControlOutputFormat::Json => print_json(&issues)?,
    }
    Ok(())
}

fn benchmark_run_command(args: &[String]) -> Result<(), String> {
    let mut benchmark_id: Option<String> = None;
    let mut scores_path: Option<PathBuf> = None;
    let mut root = PathBuf::from(DEFAULT_BENCHMARK_ROOT);
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--id" => {
                benchmark_id = Some(take_value(args, &mut index, "--id")?);
            }
            "--scores" => {
                scores_path = Some(PathBuf::from(take_value(args, &mut index, "--scores")?));
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
    let benchmark_id =
        benchmark_id.ok_or_else(|| "benchmark run requires --id <benchmark-id>".to_string())?;
    let scores_path = scores_path
        .ok_or_else(|| "benchmark run requires --scores <case-scores.json>".to_string())?;

    let raw = fs::read_to_string(&scores_path)
        .map_err(|e| format!("cannot read scores file {}: {e}", scores_path.display()))?;
    let case_scores: Vec<CaseScore> = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid case scores: {e}"))?;

    let store = BenchmarkStore::new(&root);
    let receipt = store.record_run(&BenchmarkRunRequest {
        benchmark_id,
        case_scores,
    })?;

    match output {
        ControlOutputFormat::Text => {
            println!("benchmark_run: {}", receipt.run_id);
            println!("benchmark_id: {}", receipt.benchmark_id);
            println!("benchmark_version: {}", receipt.version);
            println!("benchmark_total: {}/{}", receipt.total_score, receipt.max_score);
            println!("benchmark_accepted_as_best: {}", receipt.accepted_as_best);
            println!("benchmark_scoreboard_path: {}", receipt.scoreboard_path.display());
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

fn benchmark_show_command(args: &[String]) -> Result<(), String> {
    let mut benchmark_id: Option<String> = None;
    let mut root = PathBuf::from(DEFAULT_BENCHMARK_ROOT);
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--id" => {
                benchmark_id = Some(take_value(args, &mut index, "--id")?);
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
    let benchmark_id =
        benchmark_id.ok_or_else(|| "benchmark show requires --id <benchmark-id>".to_string())?;
    let store = BenchmarkStore::new(&root);
    let def = store.load_def(&benchmark_id)?;
    let board = store.load_scoreboard(&benchmark_id)?;

    match output {
        ControlOutputFormat::Text => {
            println!("benchmark_id: {}", def.id);
            println!("benchmark_capability: {}", def.capability);
            println!("benchmark_title: {}", def.title);
            println!("benchmark_version: {}", def.version);
            println!("benchmark_cases:");
            for case in &def.cases {
                println!("  - [{}] {}", case.id, case.title);
            }
            println!("benchmark_scoreboard_version: {}", board.version);
            println!(
                "benchmark_best: {}",
                board
                    .best
                    .as_ref()
                    .map(|b| format!("{}/{}", b.total_score, b.max_score))
                    .unwrap_or_else(|| "none".to_string())
            );
            println!(
                "benchmark_latest: {}",
                board
                    .latest
                    .as_ref()
                    .map(|b| format!("{}/{}", b.total_score, b.max_score))
                    .unwrap_or_else(|| "none".to_string())
            );
            println!("benchmark_history_runs: {}", board.history.len());
        }
        ControlOutputFormat::Json => print_json(&serde_json::json!({
            "def": &def,
            "scoreboard": &board,
        }))?,
    }
    Ok(())
}
