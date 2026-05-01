use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::chuang_kernel::{
    ChuangKernel, ChuangKernelConfig, DEFAULT_MEMORY_WRITE_MAX_CHARS,
};
use chuang_agent::hermes_memory::{DualFileMemoryStore, FileDualFileMemoryStore, HotMemoryEntry};
use chuang_agent::memory_store::MemoryStore;
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use chuang_agent::runtime_config::RuntimeConfig;
use chuang_agent::slot_registry::build_provider_responder;

use crate::cli_types::{CliOptions, RememberedRecords, RunCliRequest};

pub(crate) fn run_with_options(
    request: &RunCliRequest,
) -> Result<
    (
        chuang_agent::agent_runtime::RuntimeResult,
        RememberedRecords,
    ),
    String,
> {
    request
        .options
        .runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;

    let provider = build_provider_responder(&request.options.runtime.provider)
        .map_err(|err| format!("config_invalid: {}: {}", err.field, err.message))?;
    let mut store = SqliteMemoryStore::open(&request.options.runtime.db_path)
        .map_err(|e| format!("failed_to_open_db: {e:?}"))?;
    seed_default_memory_if_empty(&mut store)?;
    let mut kernel = ChuangKernel::with_responder(
        kernel_config_from_runtime(&request.options.runtime)?,
        store,
        provider,
    );
    kernel
        .run_turn(request.user_input.clone())
        .map_err(|e| format!("runtime_failed: {e:?}"))
        .and_then(|turn| remember_turn_if_requested(&request.options, &mut kernel, turn, request))
}

pub(crate) fn kernel_config_from_runtime(
    runtime: &RuntimeConfig,
) -> Result<ChuangKernelConfig, String> {
    let dual_file_config = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let identity_snapshot = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;

    Ok(ChuangKernelConfig {
        agent_id: "chuang-cli".to_string(),
        parent_agent_id: None,
        recall_limit: runtime.recall_limit,
        metadata: runtime.metadata.clone(),
        context_budget: Some(runtime.context_budget.clone()),
        memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        identity_snapshot: Some(identity_snapshot),
    })
}

pub(crate) fn default_db_path() -> PathBuf {
    PathBuf::from("./data/chuang-agent.db")
}

fn remember_turn_if_requested<S, R>(
    options: &CliOptions,
    kernel: &mut ChuangKernel<S, R>,
    turn: chuang_agent::chuang_kernel::ChuangKernelTurn,
    request: &RunCliRequest,
) -> Result<
    (
        chuang_agent::agent_runtime::RuntimeResult,
        RememberedRecords,
    ),
    String,
>
where
    S: MemoryStore,
    R: chuang_agent::responder::Responder,
{
    let mut records = RememberedRecords::default();

    if request.remember {
        records.sqlite_record_id = Some(
            kernel
                .remember_turn(&turn)
                .map_err(format_kernel_memory_error)?,
        );
    }

    if request.remember_identity {
        records.identity_record_id = Some(remember_identity_turn(options, &turn)?);
    }

    Ok((turn.result, records))
}

fn remember_identity_turn(
    options: &CliOptions,
    turn: &chuang_agent::chuang_kernel::ChuangKernelTurn,
) -> Result<String, String> {
    let dual_file_config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let mut store = FileDualFileMemoryStore::open(dual_file_config)
        .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
    let entry_id = unique_identity_turn_id(&turn.turn_id)?;
    let content = format!(
        "user={}\nresponse={}\nsummary={}",
        turn.user_input, turn.result.response.body, turn.report.summary
    );
    store
        .append_memory(HotMemoryEntry {
            id: entry_id.clone(),
            content,
        })
        .map_err(format_identity_memory_error)?;
    Ok(entry_id)
}

fn unique_identity_turn_id(turn_id: &str) -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    Ok(format!(
        "identity-{}-{}-{}",
        turn_id,
        std::process::id(),
        nanos
    ))
}

fn format_identity_memory_error(err: chuang_agent::hermes_memory::DualFileMemoryError) -> String {
    match err {
        chuang_agent::hermes_memory::DualFileMemoryError::StorageUnavailable { path } => {
            format!("identity_memory_write_failed path={}", path.display())
        }
        chuang_agent::hermes_memory::DualFileMemoryError::DuplicateEntry { id } => {
            format!("identity_memory_duplicate_entry id={id}")
        }
        chuang_agent::hermes_memory::DualFileMemoryError::HardLimitExceeded {
            scope,
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "identity_memory_hard_limit_exceeded scope={scope:?} limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

fn format_kernel_memory_error(err: chuang_agent::chuang_kernel::ChuangKernelMemoryError) -> String {
    match err {
        chuang_agent::chuang_kernel::ChuangKernelMemoryError::Store(store_err) => {
            format!("memory_write_failed: {store_err:?}")
        }
        chuang_agent::chuang_kernel::ChuangKernelMemoryError::HardLimitExceeded {
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "memory_write_hard_limit_exceeded limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| format!("{}:{}chars", entry.id, entry.chars))
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
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
