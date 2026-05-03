use std::collections::BTreeMap;

use chuang_agent::hermes_memory::{
    DualFileMemoryError, DualFileMemoryStore, FileDualFileMemoryStore, HotMemoryEntry,
};
use chuang_agent::memory_store::{MemoryQuery, MemoryStore};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use serde::Serialize;

use crate::cli_args::parse_cli_options;
use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("identity") => identity_memory_command(&args[1..]),
        Some("session") => session_memory_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn identity_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("show") => identity_memory_show_command(&args[1..]),
        Some("append") => identity_memory_append_command(&args[1..]),
        Some("append-experience") => identity_memory_append_experience_command(&args[1..]),
        Some("write-user") => {
            identity_memory_write_command(IdentityMemoryWriteScope::User, &args[1..])
        }
        Some("write-memory") => {
            identity_memory_write_command(IdentityMemoryWriteScope::Memory, &args[1..])
        }
        _ => Err(usage()),
    }
}

fn session_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("search") => session_memory_search_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn session_memory_search_command(args: &[String]) -> Result<(), String> {
    let request = parse_session_memory_search(args)?;
    let options = parse_cli_options(&request.runtime_args)?;
    let store = SqliteMemoryStore::open(&options.runtime.db_path)
        .map_err(|e| format!("session_memory_open_failed: {e:?}"))?;
    let mut metadata = BTreeMap::from([("kind".to_string(), "turn_summary".to_string())]);
    if let Some(session_id) = &request.session_id {
        metadata.insert("memory_scope".to_string(), "session".to_string());
        metadata.insert("session_id".to_string(), session_id.clone());
    }
    let hits = store
        .search(&MemoryQuery {
            text: Some(request.query.clone()),
            metadata,
            limit: request.limit,
        })
        .map_err(|e| format!("session_memory_search_failed: {e:?}"))?;
    let output = SessionMemorySearchOutput {
        query: request.query,
        session_id: request.session_id,
        limit: request.limit,
        hit_count: hits.len(),
        hits: hits
            .into_iter()
            .map(|hit| SessionMemorySearchHitOutput {
                id: hit.record.id,
                score: hit.score,
                content: hit.record.content,
                metadata: hit.record.metadata,
                created_at: hit.record.created_at,
            })
            .collect(),
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "session_memory_search query={} session_id={} hits={}",
                output.query,
                output.session_id.as_deref().unwrap_or("any"),
                output.hit_count
            );
            for hit in &output.hits {
                println!(
                    "hit id={} score={} created_at={}",
                    hit.id, hit.score, hit.created_at
                );
                println!("{}", hit.content);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_show_command(args: &[String]) -> Result<(), String> {
    let request = parse_identity_memory_show(args)?;
    let store = open_identity_memory_store(&request.runtime_args)?;
    let config = store.config().clone();
    let snapshot = store
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;
    let output = IdentityMemoryShowOutput {
        root: config.root.display().to_string(),
        user_file: config.user_file,
        memory_file: config.memory_file,
        experiences_file: config.experiences_file,
        user_max_chars: config.user_max_chars,
        memory_max_chars: config.memory_max_chars,
        user_chars: snapshot.user.chars().count(),
        memory_chars: snapshot.memory.chars().count(),
        experiences_chars: snapshot.experiences.chars().count(),
        user: snapshot.user,
        memory: snapshot.memory,
        experiences: snapshot.experiences,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("identity_memory_root: {}", output.root);
            println!(
                "identity_memory_limits: user={} memory={}",
                output.user_max_chars, output.memory_max_chars
            );
            println!(
                "identity_memory_chars: user={} memory={}",
                output.user_chars, output.memory_chars
            );
            println!("--- USER.md ---");
            println!("{}", output.user);
            println!("--- MEMORY.md ---");
            println!("{}", output.memory);
            println!("--- experiences.md ---");
            println!("{}", output.experiences);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_append_command(args: &[String]) -> Result<(), String> {
    let request = parse_identity_memory_append(args)?;
    let mut store = open_identity_memory_store(&request.runtime_args)?;
    store
        .append_memory(HotMemoryEntry {
            id: request.id.clone(),
            content: request.content,
        })
        .map_err(format_identity_memory_error)?;
    let output = IdentityMemoryMutationOutput {
        scope: "memory".to_string(),
        id: Some(request.id),
        written: true,
        replaced: false,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "identity_memory_appended scope={} id={}",
                output.scope,
                output.id.as_deref().unwrap_or("none")
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_append_experience_command(args: &[String]) -> Result<(), String> {
    let request = parse_identity_memory_append(args)?;
    let mut store = open_identity_memory_store(&request.runtime_args)?;
    store
        .append_experience(HotMemoryEntry {
            id: request.id.clone(),
            content: request.content,
        })
        .map_err(format_identity_memory_error)?;
    let output = IdentityMemoryMutationOutput {
        scope: "experiences".to_string(),
        id: Some(request.id),
        written: true,
        replaced: false,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "identity_memory_appended scope={} id={}",
                output.scope,
                output.id.as_deref().unwrap_or("none")
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_write_command(
    scope: IdentityMemoryWriteScope,
    args: &[String],
) -> Result<(), String> {
    let request = parse_identity_memory_write(args)?;
    if !request.approve_overwrite {
        return Err("identity_memory_write_requires_approve_overwrite".to_string());
    }
    let mut store = open_identity_memory_store(&request.runtime_args)?;
    match scope {
        IdentityMemoryWriteScope::User => store.write_user(&request.content),
        IdentityMemoryWriteScope::Memory => store.write_memory(&request.content),
    }
    .map_err(format_identity_memory_error)?;

    let output = IdentityMemoryMutationOutput {
        scope: scope.as_str().to_string(),
        id: None,
        written: true,
        replaced: true,
    };
    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "identity_memory_written scope={} replaced={}",
                output.scope, output.replaced
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn open_identity_memory_store(runtime_args: &[String]) -> Result<FileDualFileMemoryStore, String> {
    let options = parse_cli_options(runtime_args)?;
    let config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    FileDualFileMemoryStore::open(config).map_err(|e| format!("identity_memory_open_failed: {e:?}"))
}

fn parse_identity_memory_show(args: &[String]) -> Result<IdentityMemoryShowRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }
    Ok(IdentityMemoryShowRequest {
        runtime_args,
        output,
    })
}

fn parse_session_memory_search(args: &[String]) -> Result<SessionMemorySearchRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut query = None;
    let mut session_id = None;
    let mut limit = 5usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--query" => {
                query = Some(take_local_value(args, &mut index, "--query")?);
            }
            "--session-id" => {
                let value = take_local_value(args, &mut index, "--session-id")?;
                if value.trim().is_empty() {
                    return Err("session memory search requires non-empty --session-id".to_string());
                }
                session_id = Some(value);
            }
            "--limit" => {
                let value = take_local_value(args, &mut index, "--limit")?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| "session memory search requires numeric --limit".to_string())?;
                if limit == 0 {
                    return Err("session memory search requires --limit > 0".to_string());
                }
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let query = query.ok_or_else(|| "session memory search requires --query".to_string())?;
    if query.trim().is_empty() {
        return Err("session memory search requires non-empty --query".to_string());
    }

    Ok(SessionMemorySearchRequest {
        runtime_args,
        output,
        query,
        session_id,
        limit,
    })
}

fn parse_identity_memory_append(args: &[String]) -> Result<IdentityMemoryAppendRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut id = None;
    let mut content = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--id" => {
                id = Some(take_local_value(args, &mut index, "--id")?);
            }
            "--content" => {
                content = Some(take_local_value(args, &mut index, "--content")?);
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let id = id.ok_or_else(|| "identity memory append requires --id".to_string())?;
    if id.trim().is_empty() {
        return Err("identity memory append requires non-empty --id".to_string());
    }
    let content = content.ok_or_else(|| "identity memory append requires --content".to_string())?;
    if content.trim().is_empty() {
        return Err("identity memory append requires non-empty --content".to_string());
    }

    Ok(IdentityMemoryAppendRequest {
        runtime_args,
        output,
        id,
        content,
    })
}

fn parse_identity_memory_write(args: &[String]) -> Result<IdentityMemoryWriteRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut content = None;
    let mut approve_overwrite = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--content" => {
                content = Some(take_local_value(args, &mut index, "--content")?);
            }
            "--approve-overwrite" => {
                approve_overwrite = true;
                index += 1;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let content = content.ok_or_else(|| "identity memory write requires --content".to_string())?;

    Ok(IdentityMemoryWriteRequest {
        runtime_args,
        output,
        content,
        approve_overwrite,
    })
}

fn push_value_arg(
    args: &[String],
    index: &mut usize,
    target: &mut Vec<String>,
) -> Result<(), String> {
    let flag = args
        .get(*index)
        .ok_or_else(|| "missing flag".to_string())?
        .clone();
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires value"))?
        .clone();
    target.push(flag);
    target.push(value);
    *index += 2;
    Ok(())
}

fn take_local_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("memory command requires value after {flag}"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn format_identity_memory_error(err: DualFileMemoryError) -> String {
    match err {
        DualFileMemoryError::StorageUnavailable { path } => {
            format!("identity_memory_write_failed path={}", path.display())
        }
        DualFileMemoryError::DuplicateEntry { id } => {
            format!("identity_memory_duplicate_entry id={id}")
        }
        DualFileMemoryError::HardLimitExceeded {
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
                    .map(|entry| {
                        format!(
                            "{}:{}chars:preview={}",
                            entry.id, entry.chars, entry.content_preview
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityMemoryWriteScope {
    User,
    Memory,
}

impl IdentityMemoryWriteScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Memory => "memory",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityMemoryShowRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityMemoryAppendRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    id: String,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityMemoryWriteRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    content: String,
    approve_overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionMemorySearchRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    query: String,
    session_id: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionMemorySearchOutput {
    query: String,
    session_id: Option<String>,
    limit: usize,
    hit_count: usize,
    hits: Vec<SessionMemorySearchHitOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionMemorySearchHitOutput {
    id: String,
    score: u32,
    content: String,
    metadata: BTreeMap<String, String>,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IdentityMemoryShowOutput {
    root: String,
    user_file: String,
    memory_file: String,
    experiences_file: String,
    user_max_chars: usize,
    memory_max_chars: usize,
    user_chars: usize,
    memory_chars: usize,
    experiences_chars: usize,
    user: String,
    memory: String,
    experiences: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IdentityMemoryMutationOutput {
    scope: String,
    id: Option<String>,
    written: bool,
    replaced: bool,
}
