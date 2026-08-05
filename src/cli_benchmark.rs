use std::fs;
use std::path::PathBuf;

use chuang_agent::benchmark::{
    BenchmarkDef, BenchmarkRunRequest, BenchmarkStore, CaseScore,
};
use chuang_agent::benchmark_evaluator::{BenchmarkEvaluator, CaseAnswer, EvaluateRequest};
use chuang_agent::runtime_config::{OpenAICompatibleConfig, RuntimeConfig};
use chuang_agent::runtime_config_file::load_runtime_config_file;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

const DEFAULT_BENCHMARK_ROOT: &str = "benchmarks";

pub(crate) fn benchmark_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("list") => benchmark_list_command(&args[1..]),
        Some("init") => benchmark_init_command(&args[1..]),
        Some("verify") => benchmark_verify_command(&args[1..]),
        Some("run") => benchmark_run_command(&args[1..]),
        Some("show") => benchmark_show_command(&args[1..]),
        Some("evaluate") => benchmark_evaluate_command(&args[1..]),
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

fn benchmark_evaluate_command(args: &[String]) -> Result<(), String> {
    let mut benchmark_id: Option<String> = None;
    let mut answers_path: Option<PathBuf> = None;
    let mut root = PathBuf::from(DEFAULT_BENCHMARK_ROOT);
    let mut config_path = PathBuf::from("config.toml");
    let mut dry_run = false;
    let mut record = false;
    let mut output = ControlOutputFormat::Text;
    let mut provider_override: Option<(String, String, String, String)> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--id" => {
                benchmark_id = Some(take_value(args, &mut index, "--id")?);
            }
            "--answers" => {
                answers_path = Some(PathBuf::from(take_value(args, &mut index, "--answers")?));
            }
            "--root" => {
                root = PathBuf::from(take_value(args, &mut index, "--root")?);
            }
            "--config" => {
                config_path = PathBuf::from(take_value(args, &mut index, "--config")?);
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--record" => {
                record = true;
                index += 1;
            }
            "--provider-base-url" => {
                provider_override.get_or_insert_with(|| (String::new(), String::new(), String::new(), String::new())).0 =
                    take_value(args, &mut index, "--provider-base-url")?;
            }
            "--provider-api-key" => {
                provider_override.get_or_insert_with(|| (String::new(), String::new(), String::new(), String::new())).1 =
                    take_value(args, &mut index, "--provider-api-key")?;
            }
            "--provider-model" => {
                provider_override.get_or_insert_with(|| (String::new(), String::new(), String::new(), String::new())).2 =
                    take_value(args, &mut index, "--provider-model")?;
            }
            "--provider-id" => {
                provider_override.get_or_insert_with(|| (String::new(), String::new(), String::new(), String::new())).3 =
                    take_value(args, &mut index, "--provider-id")?;
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let benchmark_id =
        benchmark_id.ok_or_else(|| "benchmark evaluate requires --id <benchmark-id>".to_string())?;
    let answers_path = answers_path
        .ok_or_else(|| "benchmark evaluate requires --answers <answers.json>".to_string())?;
    let raw = fs::read_to_string(&answers_path)
        .map_err(|e| format!("cannot read answers file {}: {e}", answers_path.display()))?;
    let answers: Vec<CaseAnswer> = serde_json::from_str(&raw)
        .map_err(|e| format!("invalid answers file: {e}"))?;

    let runtime: RuntimeConfig = load_runtime_config_file(&config_path)
        .map_err(|e| format!("cannot load config {}: {e:?}", config_path.display()))?;
    let provider = match provider_override {
        Some((base_url, api_key, model_name, provider_id)) => {
            let base = if base_url.is_empty() {
                provider_base_url(&runtime)
            } else {
                base_url
            };
            let key = if api_key.is_empty() {
                provider_api_key(&runtime)
            } else {
                api_key
            };
            let model = if model_name.is_empty() {
                provider_model(&runtime)
            } else {
                model_name
            };
            let pid = if provider_id.is_empty() {
                provider_id_name(&runtime)
            } else {
                provider_id
            };
            OpenAICompatibleConfig {
                provider_id: pid,
                base_url: base,
                api_key: key,
                model_name: model,
                transport: provider_transport(&runtime),
                reasoning_effort: None,
                request_timeout_ms: None,
                tls_ca_cert_path: None,
            }
        }
        None => provider_from_runtime(&runtime)?,
    };

    let store = BenchmarkStore::new(&root);
    let evaluator = BenchmarkEvaluator::new(store.clone(), provider);
    let receipt = evaluator.evaluate(&EvaluateRequest {
        benchmark_id: benchmark_id.clone(),
        answers,
        dry_run,
    })?;

    let recorded = if record && !receipt.dry_run {
        let run_receipt = store.record_run(&BenchmarkRunRequest {
            benchmark_id: benchmark_id.clone(),
            case_scores: receipt.case_scores.clone(),
        })?;
        Some(run_receipt)
    } else {
        None
    };

    match output {
        ControlOutputFormat::Text => {
            println!("benchmark_evaluate: {}", receipt.benchmark_id);
            println!("provider: {} model: {}", receipt.provider_id, receipt.model_name);
            println!("dry_run: {}", receipt.dry_run);
            println!("evaluated_case_count: {}", receipt.evaluated_case_count);
            for score in &receipt.case_scores {
                println!(
                    "  [{}] {}/{} reason={}",
                    score.case_id, score.score, score.max_score, score.reason
                );
            }
            if let Some(run_receipt) = &recorded {
                println!("recorded: run_id={} accepted_as_best={}", run_receipt.run_id, run_receipt.accepted_as_best);
                println!("scoreboard: {}", run_receipt.scoreboard_path.display());
            } else if record && dry_run {
                println!("recorded: skipped (dry-run)");
            }
        }
        ControlOutputFormat::Json => {
            let mut value = serde_json::to_value(&receipt).map_err(|e| e.to_string())?;
            if let Some(run_receipt) = &recorded {
                value["recorded"] = serde_json::to_value(run_receipt).map_err(|e| e.to_string())?;
            }
            print_json(&value)?;
        }
    }
    Ok(())
}

fn provider_from_runtime(runtime: &RuntimeConfig) -> Result<OpenAICompatibleConfig, String> {
    use chuang_agent::runtime_config::ProviderConfig;
    match &runtime.provider {
        ProviderConfig::OpenAICompatible(config) => Ok(config.clone()),
        ProviderConfig::Fake { provider_id, .. } => Err(format!(
            "provider={provider_id} is fake; benchmark evaluate needs a real openai_compatible provider (config.toml)"
        )),
        ProviderConfig::Fallback { primary, .. } => match primary.as_ref() {
            ProviderConfig::OpenAICompatible(config) => Ok(config.clone()),
            _ => Err("fallback primary provider is not openai_compatible".to_string()),
        },
    }
}

fn provider_base_url(runtime: &RuntimeConfig) -> String {
    provider_from_runtime(runtime).map(|c| c.base_url).unwrap_or_default()
}
fn provider_api_key(runtime: &RuntimeConfig) -> String {
    provider_from_runtime(runtime).map(|c| c.api_key).unwrap_or_default()
}
fn provider_model(runtime: &RuntimeConfig) -> String {
    provider_from_runtime(runtime).map(|c| c.model_name).unwrap_or_default()
}
fn provider_id_name(runtime: &RuntimeConfig) -> String {
    provider_from_runtime(runtime).map(|c| c.provider_id).unwrap_or_default()
}
fn provider_transport(runtime: &RuntimeConfig) -> chuang_agent::provider_openai_compatible::ProviderTransport {
    provider_from_runtime(runtime)
        .map(|c| c.transport)
        .unwrap_or(chuang_agent::provider_openai_compatible::ProviderTransport::Native)
}
