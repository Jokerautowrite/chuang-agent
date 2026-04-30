use std::collections::BTreeMap;
use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use chuang_agent::agent_runtime::{AgentRuntime, RuntimeRequest};
use chuang_agent::memory_store::MemoryStore;
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::responder::ProviderTransport;
use chuang_agent::runtime_config::{OpenAICompatibleConfig, ProviderConfig, RuntimeConfig};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CliOptions {
    runtime: RuntimeConfig,
}

fn main() {
    if let Err(message) = run_cli() {
        eprintln!("{message}");
        std::process::exit(1);
    }
}

fn run_cli() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("run") => run_command(&args[2..]),
        Some("repl") => repl_command(&args[2..]),
        _ => Err(usage()),
    }
}

fn run_command(args: &[String]) -> Result<(), String> {
    let (options, user_input) = parse_db_and_input(args)?;
    let result = run_with_options(&options, user_input)?;
    print_runtime_result(&result);
    Ok(())
}

fn repl_command(args: &[String]) -> Result<(), String> {
    let options = parse_cli_options(args)?;

    println!("chuang-agent repl ready (输入 exit 退出)");
    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line in stdin.lock().lines() {
        let line = line.map_err(|e| format!("stdin_read_failed: {e}"))?;
        let input = line.trim();
        if input.eq_ignore_ascii_case("exit") || input.eq_ignore_ascii_case("quit") {
            break;
        }
        if input.is_empty() {
            continue;
        }

        let result = run_with_options(&options, input.to_string())?;
        print_runtime_result(&result);
        writeln!(stdout, "---").map_err(|e| format!("stdout_write_failed: {e}"))?;
        stdout
            .flush()
            .map_err(|e| format!("stdout_flush_failed: {e}"))?;
    }

    Ok(())
}

fn run_with_options(
    options: &CliOptions,
    user_input: String,
) -> Result<chuang_agent::agent_runtime::RuntimeResult, String> {
    options
        .runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    match options.runtime.provider.build_openai_compatible() {
        Ok(Some(provider)) => {
            let mut store = SqliteMemoryStore::open(&options.runtime.db_path)
                .map_err(|e| format!("failed_to_open_db: {e:?}"))?;
            seed_default_memory_if_empty(&mut store)?;
            let runtime = AgentRuntime::with_responder(store, provider);
            runtime
                .run(&RuntimeRequest {
                    user_input,
                    recall_limit: options.runtime.recall_limit,
                    metadata: options.runtime.metadata.clone(),
                    context_budget: Some(options.runtime.context_budget.clone()),
                })
                .map_err(|e| format!("runtime_failed: {e:?}"))
        }
        Ok(None) => {
            let runtime = build_runtime(&options.runtime.db_path)?;
            runtime
                .run(&RuntimeRequest {
                    user_input,
                    recall_limit: options.runtime.recall_limit,
                    metadata: options.runtime.metadata.clone(),
                    context_budget: Some(options.runtime.context_budget.clone()),
                })
                .map_err(|e| format!("runtime_failed: {e:?}"))
        }
        Err(err) => Err(format!("config_invalid: {}: {}", err.field, err.message)),
    }
}

fn build_runtime(db_path: &PathBuf) -> Result<AgentRuntime<SqliteMemoryStore>, String> {
    let mut store =
        SqliteMemoryStore::open(db_path).map_err(|e| format!("failed_to_open_db: {e:?}"))?;
    seed_default_memory_if_empty(&mut store)?;
    Ok(AgentRuntime::new(store))
}

fn parse_db_and_input(args: &[String]) -> Result<(CliOptions, String), String> {
    let options = parse_cli_options(args)?;
    let mut user_input: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => index += 2,
            "--input" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                user_input = Some(value.clone());
                index += 2;
            }
            "--provider-base-url"
            | "--provider-api-key"
            | "--provider-model"
            | "--provider-id"
            | "--provider-transport" => index += 2,
            _ => return Err(usage()),
        }
    }

    Ok((options, user_input.ok_or_else(usage)?))
}

fn parse_cli_options(args: &[String]) -> Result<CliOptions, String> {
    let mut db_path: Option<PathBuf> = None;
    let mut provider_id: Option<String> = None;
    let mut provider_base_url: Option<String> = None;
    let mut provider_api_key: Option<String> = None;
    let mut provider_model: Option<String> = None;
    let mut provider_transport: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--db" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                db_path = Some(PathBuf::from(value));
                index += 2;
            }
            "--input" => index += 2,
            "--provider-base-url" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_base_url = Some(value.clone());
                index += 2;
            }
            "--provider-api-key" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_api_key = Some(value.clone());
                index += 2;
            }
            "--provider-model" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_model = Some(value.clone());
                index += 2;
            }
            "--provider-transport" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_transport = Some(value.clone());
                index += 2;
            }
            "--provider-id" => {
                let value = args.get(index + 1).ok_or_else(usage)?;
                provider_id = Some(value.clone());
                index += 2;
            }
            _ => return Err(usage()),
        }
    }

    let mut runtime = RuntimeConfig::new(db_path.unwrap_or_else(default_db_path));
    runtime.provider = match (provider_base_url, provider_api_key, provider_model) {
        (None, None, None) => ProviderConfig::Fake {
            provider_id: "fake-runtime".to_string(),
            model_name: "stub-responder".to_string(),
        },
        (Some(base_url), Some(api_key), Some(model_name)) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                provider_id: provider_id.unwrap_or_else(|| "openai-compatible-cli".to_string()),
                base_url,
                api_key,
                model_name,
                transport: parse_provider_transport(provider_transport.as_deref())?,
            })
        }
        _ => {
            return Err(
                "provider config requires base_url + api_key + model (optional: provider_id)"
                    .to_string(),
            )
        }
    };

    Ok(CliOptions { runtime })
}

fn print_runtime_result(result: &chuang_agent::agent_runtime::RuntimeResult) {
    println!("model_name: {}", result.response.model_name);
    println!("body: {}", result.response.body);
    println!("trace: {}", result.response.trace);
    println!(
        "provider: {}",
        result
            .response
            .meta
            .provider
            .as_deref()
            .unwrap_or("unknown")
    );
    println!("recall_hits: {}", result.recall_hit_count);
    println!("recall_summary: {}", result.recall_summary);
    println!(
        "context_drop_reasons: {}",
        format_drop_reasons(&result.context_debug.drop_reasons)
    );
    println!(
        "context_working_reservation: {}",
        format_working_reservation(&result.context_debug)
    );
    println!(
        "context_budget_exceeded: {}",
        result.context_debug.budget_exceeded
    );
    println!(
        "context_budget_exceeded_reasons: {}",
        format_budget_exceeded_reasons(&result.context_debug.budget_exceeded_reasons)
    );

    for (key, value) in &result.response.meta.extra {
        println!("{key}: {value}");
    }
}

fn format_drop_reasons(reasons: &[chuang_agent::context_engine::DropReason]) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(|reason| format!("{}:{}", reason.segment_id, reason.reason.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

fn format_budget_exceeded_reasons(
    reasons: &[chuang_agent::context_engine::BudgetExceededReason],
) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(chuang_agent::context_engine::BudgetExceededReason::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn format_working_reservation(debug: &chuang_agent::agent_runtime::ContextDebugInfo) -> String {
    debug
        .working_reservation
        .as_ref()
        .map(|reservation| {
            format!(
                "reserved={}@{} reason={} dropped={}",
                reservation.reserved_segment_id,
                reservation.reserved_tokens,
                reservation.reason.as_str(),
                if reservation.dropped_segment_ids.is_empty() {
                    "none".to_string()
                } else {
                    reservation.dropped_segment_ids.join(",")
                }
            )
        })
        .unwrap_or_else(|| "none".to_string())
}

fn parse_provider_transport(raw: Option<&str>) -> Result<ProviderTransport, String> {
    raw.unwrap_or("stub").parse()
}

fn seed_default_memory_if_empty(store: &mut SqliteMemoryStore) -> Result<(), String> {
    let existing = store
        .search(&chuang_agent::memory_store::MemoryQuery {
            text: None,
            metadata: BTreeMap::new(),
            limit: 1,
        })
        .map_err(|e| format!("seed_search_failed: {e:?}"))?;

    if !existing.is_empty() {
        return Ok(());
    }

    store
        .put(chuang_agent::memory_store::MemoryRecord {
            id: "boot-seed-1".to_string(),
            content: "创项目先跑起来，先闭环再优化。".to_string(),
            metadata: BTreeMap::from([("kind".to_string(), "goal".to_string())]),
            created_at: "2026-04-30T00:00:00Z".to_string(),
            expires_at: None,
        })
        .map_err(|e| format!("seed_put_failed: {e:?}"))?;

    Ok(())
}

fn default_db_path() -> PathBuf {
    PathBuf::from("./data/chuang-agent.db")
}

fn usage() -> String {
    "usage: cargo run -- <run|repl> [--db PATH] [--input TEXT] [--provider-base-url URL --provider-api-key KEY --provider-model MODEL [--provider-id ID]]".to_string()
}
