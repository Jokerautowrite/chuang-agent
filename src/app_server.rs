//! `app_server` 模块。内部实现模块（无公开顶层项）。

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Write};
use std::net::Shutdown;
use std::os::unix::fs::{DirBuilderExt, FileTypeExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use bytes::Bytes;
use fs2::FileExt;
use http_body_util::{BodyExt, Full};
use hyper::{Method, Request};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::client::legacy::Client;
use hyper_util::rt::TokioExecutor;
use rusqlite::{params, Connection, OptionalExtension, TransactionBehavior};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::runtime::Builder as TokioRuntimeBuilder;
use tokio::time::timeout;

use crate::cli_runtime::kernel_config_from_runtime;
use crate::cli_runtime::run_with_options;
use crate::cli_types::{CliOptions, ConversationHistoryItem, RunCliRequest};
use chuang_agent::goal_mode::GoalSpec;
use chuang_agent::kernel_status::build_chuang_mvp_status;
use chuang_agent::path_utils::normalize_path_lexically;
use chuang_agent::runtime_config::{
    ConfigSummary, IdentityBootstrapConfig, IdentityMemoryConfig, OpenAICompatibleConfig,
    ProviderConfig, RulesConfig, RuntimeConfig, SubagentQueueConfig, DEFAULT_WORKSPACE_ROOT,
};
use chuang_agent::runtime_config_file::{
    load_runtime_config_file, load_runtime_config_file_with_options, RuntimeConfigFileError,
    RuntimeConfigFileOptions,
};
use chuang_agent::runtime_report::runtime_observability_meta;
use chuang_agent::tool_loop_meta::{parse_json_value, ToolLoopMeta};
use chuang_agent::tool_runtime::{ToolExecutionRecord, ToolProtocolError};

static APP_SERVER_TURN_SEQUENCE: AtomicU64 = AtomicU64::new(1);
const APP_SERVER_SNAPSHOT_SCHEMA_VERSION: i64 = 1;

#[derive(Debug, Default)]
struct AppServerState {
    next_thread_seq: u64,
    next_turn_seq: u64,
    threads: BTreeMap<String, ThreadState>,
    active_turns: BTreeMap<String, ActiveTurn>,
    snapshot_store: Option<AppServerSnapshotStore>,
    db_lock: Option<AppServerDbLock>,
}

type SharedAppServerState = Arc<Mutex<AppServerState>>;

#[derive(Debug)]
struct ActiveTurn {
    thread_id: String,
    turn_id: String,
    guidance_path: PathBuf,
    guidance_writer: Arc<Mutex<File>>,
}

#[derive(Debug, Clone)]
struct ThreadState {
    id: String,
    workspace_root: String,
    display_name: String,
    created_at: u64,
    updated_at: u64,
    turns: Vec<TurnState>,
}

#[derive(Debug, Clone)]
struct TurnState {
    id: String,
    user_text: String,
    assistant_text: String,
    model_name: String,
    status: String,
    provider_meta: BTreeMap<String, String>,
    tool_trace: String,
    tool_surface: Option<Value>,
    updated_at: u64,
}

#[derive(Debug, Clone)]
struct AppServerSnapshotStore {
    db_path: PathBuf,
}

#[derive(Debug)]
struct AppServerDbLock {
    _file: File,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppServerSnapshot {
    schema_version: i64,
    next_thread_seq: u64,
    next_turn_seq: u64,
    threads: BTreeMap<String, AppServerSnapshotThread>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppServerSnapshotThread {
    id: String,
    workspace_root: String,
    display_name: String,
    created_at: u64,
    updated_at: u64,
    turns: Vec<AppServerSnapshotTurn>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AppServerSnapshotTurn {
    id: String,
    user_text: String,
    assistant_text: String,
    model_name: String,
    status: String,
    provider_meta: BTreeMap<String, String>,
    updated_at: u64,
}

pub(crate) fn app_server_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        None => app_server_stdio_command(),
        Some("health") => app_server_health_command(&args[1..]),
        Some("daemon") => app_server_daemon_command(&args[1..]),
        Some("probe") => app_server_probe_command(&args[1..]),
        Some("ask") => app_server_ask_command(&args[1..]),
        Some(_) => Err(
            "usage: chuang-agent app-server [health|daemon --socket PATH|probe --socket PATH [--json]|ask --socket PATH --workspace-root PATH --text TEXT [--thread-id ID] [--json]]"
                .to_string(),
        ),
    }
}

fn app_server_stdio_command() -> Result<(), String> {
    let stdin = io::stdin();
    let mut stdout = io::stdout();
    let mut state = AppServerState::default();
    serve_json_lines(stdin.lock(), &mut stdout, &mut state)
}

fn serve_json_lines<R: BufRead, W: Write>(
    reader: R,
    writer: &mut W,
    state: &mut AppServerState,
) -> Result<(), String> {
    for line in reader.lines() {
        let line = line.map_err(|e| format!("app_server_read_failed: {e}"))?;
        let raw = line.trim();
        if raw.is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) => {
                let _ = write_json_line(
                    writer,
                    &json!({
                        "error": {
                            "message": format!("invalid_json: {error}"),
                        }
                    }),
                );
                continue;
            }
        };

        let Some(method) = parsed.get("method").and_then(|value| value.as_str()) else {
            continue;
        };
        let id = parsed.get("id").cloned();
        let params = parsed.get("params").cloned().unwrap_or(Value::Null);

        if method == "initialized" {
            continue;
        }

        let result = match method {
            "initialize" => Ok(handle_initialize()),
            "model/list" => handle_model_list(&params),
            "thread/start" => handle_thread_start(&mut *state, &params),
            "thread/resume" => handle_thread_resume(&state, &params),
            "thread/list" => Ok(handle_thread_list(&state)),
            "turn/start" => handle_turn_start(state, &params, writer),
            "turn/interrupt" => Err(
                "turn_interrupt_unsupported: synchronous app-server turns cannot be interrupted"
                    .to_string(),
            ),
            _ => Err(format!("unsupported_method: {method}")),
        };

        if let Some(id) = id {
            match result {
                Ok(result) => {
                    write_json_line(writer, &json!({ "id": id, "result": result }))?;
                }
                Err(message) => {
                    write_json_line(
                        writer,
                        &json!({ "id": id, "error": { "message": message } }),
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn app_server_daemon_command(args: &[String]) -> Result<(), String> {
    let socket = parse_socket_only_args(args, "daemon")?;
    let state = Arc::new(Mutex::new(load_daemon_app_server_state()?));
    let listener = bind_app_server_socket(&socket)?;

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let state = Arc::clone(&state);
                thread::spawn(move || {
                    if let Err(error) = serve_unix_client(stream, state) {
                        eprintln!("app_server_client_failed: {error}");
                    }
                });
            }
            Err(error) => return Err(format!("app_server_accept_failed: {error}")),
        }
    }

    Ok(())
}

fn serve_unix_client(stream: UnixStream, state: SharedAppServerState) -> Result<(), String> {
    let mut writer = stream
        .try_clone()
        .map_err(|e| format!("app_server_client_clone_failed: {e}"))?;
    let reader = BufReader::new(stream);
    serve_daemon_json_lines(reader, &mut writer, state)
}

fn serve_daemon_json_lines<R: BufRead, W: Write>(
    reader: R,
    writer: &mut W,
    state: SharedAppServerState,
) -> Result<(), String> {
    for line in reader.lines() {
        let line = line.map_err(|e| format!("app_server_read_failed: {e}"))?;
        let raw = line.trim();
        if raw.is_empty() {
            continue;
        }

        let parsed: Value = match serde_json::from_str(raw) {
            Ok(value) => value,
            Err(error) => {
                let _ = write_json_line(
                    writer,
                    &json!({
                        "error": {
                            "message": format!("invalid_json: {error}"),
                        }
                    }),
                );
                continue;
            }
        };

        let Some(method) = parsed.get("method").and_then(|value| value.as_str()) else {
            continue;
        };
        let id = parsed.get("id").cloned();
        let params = parsed.get("params").cloned().unwrap_or(Value::Null);

        if method == "initialized" {
            continue;
        }

        let result = match method {
            "initialize" => Ok(handle_initialize()),
            "server/status" => with_app_server_state(&state, |state| handle_server_status(state)),
            "model/list" => handle_model_list(&params),
            "thread/start" => with_app_server_state(&state, |state| {
                let result = handle_thread_start(state, &params)?;
                persist_app_server_state(state)?;
                Ok(result)
            }),
            "thread/resume" => {
                with_app_server_state(&state, |state| handle_thread_resume(state, &params))
            }
            "thread/list" => with_app_server_state(&state, |state| Ok(handle_thread_list(state))),
            "turn/start" => handle_live_turn_start(Arc::clone(&state), &params, writer),
            "turn/guidance" => handle_turn_guidance(&state, &params),
            "turn/interrupt" => handle_turn_interrupt(&state, &params),
            _ => Err(format!("unsupported_method: {method}")),
        };

        if let Some(id) = id {
            match result {
                Ok(result) => {
                    write_json_line(writer, &json!({ "id": id, "result": result }))?;
                }
                Err(message) => {
                    write_json_line(
                        writer,
                        &json!({ "id": id, "error": { "message": message } }),
                    )?;
                }
            }
        }
    }

    Ok(())
}

fn with_app_server_state<T>(
    state: &SharedAppServerState,
    action: impl FnOnce(&mut AppServerState) -> Result<T, String>,
) -> Result<T, String> {
    let mut state = state
        .lock()
        .map_err(|_| "app_server_state_lock_poisoned".to_string())?;
    action(&mut state)
}

impl AppServerSnapshotStore {
    fn open(db_path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = db_path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "app_server_snapshot_parent_create_failed: path={} error={error}",
                    parent.display()
                )
            })?;
        }

        let store = Self { db_path };
        let conn = store.connection()?;
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS app_server_snapshots (
                snapshot_id INTEGER PRIMARY KEY CHECK (snapshot_id = 1),
                schema_version INTEGER NOT NULL,
                snapshot_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            ",
        )
        .map_err(|error| format!("app_server_snapshot_schema_failed: {error}"))?;
        Ok(store)
    }

    fn connection(&self) -> Result<Connection, String> {
        Connection::open(&self.db_path).map_err(|error| {
            format!(
                "app_server_snapshot_open_failed: path={} error={error}",
                self.db_path.display()
            )
        })
    }

    fn load(&self) -> Result<Option<AppServerSnapshot>, String> {
        let conn = self.connection()?;
        let row = conn
            .query_row(
                "SELECT schema_version, snapshot_json
                 FROM app_server_snapshots
                 WHERE snapshot_id = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()
            .map_err(|error| format!("app_server_snapshot_load_failed: {error}"))?;
        let Some((schema_version, snapshot_json)) = row else {
            return Ok(None);
        };
        if schema_version != APP_SERVER_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "app_server_snapshot_schema_unsupported: found={schema_version} supported={APP_SERVER_SNAPSHOT_SCHEMA_VERSION}"
            ));
        }
        let snapshot: AppServerSnapshot = serde_json::from_str(&snapshot_json)
            .map_err(|error| format!("app_server_snapshot_decode_failed: {error}"))?;
        if snapshot.schema_version != APP_SERVER_SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "app_server_snapshot_payload_schema_unsupported: found={} supported={APP_SERVER_SNAPSHOT_SCHEMA_VERSION}",
                snapshot.schema_version
            ));
        }
        Ok(Some(snapshot))
    }

    fn save(&self, snapshot: &AppServerSnapshot) -> Result<(), String> {
        let snapshot_json = serde_json::to_string(snapshot)
            .map_err(|error| format!("app_server_snapshot_encode_failed: {error}"))?;
        let mut conn = self.connection()?;
        let transaction = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|error| format!("app_server_snapshot_begin_failed: {error}"))?;
        transaction
            .execute(
                "
                INSERT INTO app_server_snapshots (
                    snapshot_id,
                    schema_version,
                    snapshot_json,
                    updated_at
                )
                VALUES (1, ?1, ?2, ?3)
                ON CONFLICT(snapshot_id) DO UPDATE SET
                    schema_version = excluded.schema_version,
                    snapshot_json = excluded.snapshot_json,
                    updated_at = excluded.updated_at
                ",
                params![
                    APP_SERVER_SNAPSHOT_SCHEMA_VERSION,
                    snapshot_json,
                    now_millis()
                ],
            )
            .map_err(|error| format!("app_server_snapshot_write_failed: {error}"))?;
        transaction
            .commit()
            .map_err(|error| format!("app_server_snapshot_commit_failed: {error}"))?;
        Ok(())
    }

    fn snapshot_updated_at(&self) -> Result<Option<u64>, String> {
        let conn = self.connection()?;
        conn.query_row(
            "SELECT updated_at
             FROM app_server_snapshots
             WHERE snapshot_id = 1",
            [],
            |row| row.get::<_, i64>(0),
        )
        .optional()
        .map(|value| value.map(|value| value.max(0) as u64))
        .map_err(|error| format!("app_server_snapshot_status_failed: {error}"))
    }
}

impl AppServerSnapshot {
    fn from_state(state: &AppServerState) -> Self {
        Self {
            schema_version: APP_SERVER_SNAPSHOT_SCHEMA_VERSION,
            next_thread_seq: state.next_thread_seq,
            next_turn_seq: state.next_turn_seq,
            threads: state
                .threads
                .iter()
                .map(|(thread_id, thread)| {
                    (
                        thread_id.clone(),
                        AppServerSnapshotThread {
                            id: thread.id.clone(),
                            workspace_root: thread.workspace_root.clone(),
                            display_name: thread.display_name.clone(),
                            created_at: thread.created_at,
                            updated_at: thread.updated_at,
                            turns: thread
                                .turns
                                .iter()
                                .map(|turn| AppServerSnapshotTurn {
                                    id: turn.id.clone(),
                                    user_text: turn.user_text.clone(),
                                    assistant_text: turn.assistant_text.clone(),
                                    model_name: turn.model_name.clone(),
                                    status: turn.status.clone(),
                                    provider_meta: app_server_snapshot_provider_meta(
                                        &turn.provider_meta,
                                    ),
                                    updated_at: turn.updated_at,
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
        }
    }

    fn into_state(self, snapshot_store: AppServerSnapshotStore) -> AppServerState {
        let mut state = AppServerState {
            next_thread_seq: self.next_thread_seq,
            next_turn_seq: self.next_turn_seq,
            threads: self
                .threads
                .into_iter()
                .map(|(thread_id, thread)| {
                    (
                        thread_id,
                        ThreadState {
                            id: thread.id,
                            workspace_root: thread.workspace_root,
                            display_name: thread.display_name,
                            created_at: thread.created_at,
                            updated_at: thread.updated_at,
                            turns: thread
                                .turns
                                .into_iter()
                                .map(|turn| TurnState {
                                    id: turn.id,
                                    user_text: turn.user_text,
                                    assistant_text: turn.assistant_text,
                                    model_name: turn.model_name,
                                    status: turn.status,
                                    provider_meta: turn.provider_meta,
                                    tool_trace: String::new(),
                                    tool_surface: None,
                                    updated_at: turn.updated_at,
                                })
                                .collect(),
                        },
                    )
                })
                .collect(),
            active_turns: BTreeMap::new(),
            snapshot_store: Some(snapshot_store),
            db_lock: None,
        };
        restore_app_server_sequence_floors(&mut state);
        state
    }
}

fn load_daemon_app_server_state() -> Result<AppServerState, String> {
    let workspace_root = app_server_config_workspace_root("");
    let runtime = build_runtime_for_workspace_with_options(
        &workspace_root,
        RuntimeConfigFileOptions::allow_missing_env(),
    )?;
    let db_path = normalize_app_server_db_path(runtime.db_path)?;
    let db_lock = AppServerDbLock::acquire(&db_path)?;
    let mut state = load_app_server_state_from_db(db_path)?;
    state.db_lock = Some(db_lock);
    Ok(state)
}

fn load_app_server_state_from_db(db_path: PathBuf) -> Result<AppServerState, String> {
    let snapshot_store = AppServerSnapshotStore::open(db_path)?;
    let mut state = match snapshot_store.load()? {
        Some(snapshot) => snapshot.into_state(snapshot_store),
        None => AppServerState {
            snapshot_store: Some(snapshot_store),
            ..AppServerState::default()
        },
    };
    if recover_interrupted_app_server_turns(&mut state) {
        persist_app_server_state(&state)?;
    }
    Ok(state)
}

impl AppServerDbLock {
    fn acquire(db_path: &Path) -> Result<Self, String> {
        let path = app_server_db_lock_path(db_path)?;
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "app_server_db_lock_parent_create_failed: path={} error={error}",
                    parent.display()
                )
            })?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .map_err(|error| {
                format!(
                    "app_server_db_lock_open_failed: db_path={} lock_path={} error={error}",
                    db_path.display(),
                    path.display()
                )
            })?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).map_err(|error| {
            format!(
                "app_server_db_lock_permissions_failed: lock_path={} error={error}",
                path.display()
            )
        })?;
        file.try_lock_exclusive().map_err(|error| {
            format!(
                "app_server_db_locked: db_path={} lock_path={} error={error}",
                db_path.display(),
                path.display()
            )
        })?;
        Ok(Self { _file: file })
    }
}

fn app_server_db_lock_path(db_path: &Path) -> Result<PathBuf, String> {
    if db_path.as_os_str().is_empty() {
        return Err("app_server_db_lock_path_empty".to_string());
    }
    let mut path = db_path.as_os_str().to_os_string();
    path.push(".lock");
    Ok(PathBuf::from(path))
}

fn normalize_app_server_db_path(db_path: PathBuf) -> Result<PathBuf, String> {
    let path = if db_path.is_absolute() {
        db_path
    } else {
        std::env::current_dir()
            .map_err(|error| format!("app_server_db_path_current_dir_failed: {error}"))?
            .join(db_path)
    };
    Ok(normalize_path_lexically(&path))
}

fn persist_app_server_state(state: &AppServerState) -> Result<(), String> {
    let Some(snapshot_store) = &state.snapshot_store else {
        return Ok(());
    };
    snapshot_store.save(&AppServerSnapshot::from_state(state))
}

fn recover_interrupted_app_server_turns(state: &mut AppServerState) -> bool {
    let now = now_millis();
    let mut recovered = false;
    for thread in state.threads.values_mut() {
        for turn in &mut thread.turns {
            if turn.status == "active" {
                turn.status = "interrupted".to_string();
                turn.provider_meta.insert(
                    "app_server_interruption_reason".to_string(),
                    "daemon_restarted_before_turn_completion".to_string(),
                );
                turn.updated_at = now;
                thread.updated_at = now;
                recovered = true;
            }
        }
    }
    recovered
}

fn handle_server_status(state: &AppServerState) -> Result<Value, String> {
    let thread_count = state.threads.len();
    let turn_count = state
        .threads
        .values()
        .map(|thread| thread.turns.len())
        .sum::<usize>();
    let active_count = state
        .threads
        .values()
        .flat_map(|thread| &thread.turns)
        .filter(|turn| turn.status == "active")
        .count();
    let interrupted_count = state
        .threads
        .values()
        .flat_map(|thread| &thread.turns)
        .filter(|turn| turn.status == "interrupted")
        .count();
    let snapshot_updated_at = state
        .snapshot_store
        .as_ref()
        .map(AppServerSnapshotStore::snapshot_updated_at)
        .transpose()?
        .flatten();

    Ok(json!({
        "persistence": {
            "enabled": state.snapshot_store.is_some(),
            "schema": state.snapshot_store.as_ref().map(|_| APP_SERVER_SNAPSHOT_SCHEMA_VERSION),
            "lock_held": state.db_lock.is_some(),
            "thread_count": thread_count,
            "turn_count": turn_count,
            "active_count": active_count,
            "interrupted_count": interrupted_count,
            "snapshot_updated_at": snapshot_updated_at,
        }
    }))
}

fn restore_app_server_sequence_floors(state: &mut AppServerState) {
    for thread in state.threads.values() {
        state.next_thread_seq = state
            .next_thread_seq
            .max(app_server_sequence_from_id(&thread.id, "chuang-thread-"));
        for turn in &thread.turns {
            state.next_turn_seq = state
                .next_turn_seq
                .max(app_server_sequence_from_id(&turn.id, "chuang-turn-"));
        }
    }
}

fn app_server_sequence_from_id(id: &str, prefix: &str) -> u64 {
    id.strip_prefix(prefix)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0)
}

fn app_server_snapshot_provider_meta(
    provider_meta: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    provider_meta
        .iter()
        .filter(|(key, _)| {
            matches!(
                key.as_str(),
                "pending_approval_id" | "pending_approval_path" | "app_server_interruption_reason"
            )
        })
        .map(|(key, value)| (key.clone(), value.clone()))
        .collect()
}

fn parse_socket_only_args(args: &[String], command: &str) -> Result<PathBuf, String> {
    let mut socket = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--socket" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    format!("app-server {command} requires a value after --socket")
                })?;
                socket = Some(PathBuf::from(value));
                index += 2;
            }
            _ => {
                return Err(format!(
                    "usage: chuang-agent app-server {command} --socket PATH"
                ))
            }
        }
    }
    let socket = socket.ok_or_else(|| format!("app-server {command} requires --socket PATH"))?;
    if socket.as_os_str().is_empty() {
        return Err(format!(
            "app-server {command} requires a non-empty --socket PATH"
        ));
    }
    Ok(socket)
}

fn bind_app_server_socket(socket: &Path) -> Result<UnixListener, String> {
    match fs::symlink_metadata(socket) {
        Ok(metadata) => {
            if !metadata.file_type().is_socket() {
                return Err(format!(
                    "app_server_socket_path_not_socket: {}",
                    socket.display()
                ));
            }
            match UnixStream::connect(socket) {
                Ok(stream) => {
                    let _ = stream.shutdown(Shutdown::Both);
                    return Err(format!(
                        "app_server_already_running: socket={}",
                        socket.display()
                    ));
                }
                Err(error) if stale_socket_connect_error(&error) => {
                    let stale_path = next_stale_socket_path(socket)?;
                    fs::rename(socket, &stale_path).map_err(|rename_error| {
                        format!(
                            "app_server_stale_socket_rename_failed: socket={} stale={} error={rename_error}",
                            socket.display(),
                            stale_path.display()
                        )
                    })?;
                }
                Err(error) => {
                    return Err(format!(
                        "app_server_socket_probe_failed: socket={} error={error}",
                        socket.display()
                    ))
                }
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(format!(
                "app_server_socket_metadata_failed: socket={} error={error}",
                socket.display()
            ))
        }
    }

    let listener = UnixListener::bind(socket).map_err(|e| {
        format!(
            "app_server_socket_bind_failed: socket={} error={e}",
            socket.display()
        )
    })?;
    fs::set_permissions(socket, fs::Permissions::from_mode(0o600)).map_err(|e| {
        format!(
            "app_server_socket_permissions_failed: socket={} error={e}",
            socket.display()
        )
    })?;
    Ok(listener)
}

fn stale_socket_connect_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionRefused
            | io::ErrorKind::NotFound
            | io::ErrorKind::AddrNotAvailable
    )
}

fn next_stale_socket_path(socket: &Path) -> Result<PathBuf, String> {
    let parent = socket.parent().unwrap_or_else(|| Path::new("."));
    let name = socket
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| format!("app_server_socket_invalid_name: {}", socket.display()))?;
    let timestamp = now_millis();
    let pid = process::id();
    for sequence in 0..1024 {
        let stale = parent.join(format!("{name}.stale-{timestamp}-{pid}-{sequence}"));
        if !stale.exists() {
            return Ok(stale);
        }
    }
    Err(format!(
        "app_server_stale_socket_name_exhausted: {}",
        socket.display()
    ))
}

fn app_server_probe_command(args: &[String]) -> Result<(), String> {
    let (socket, output_json) = parse_probe_args(args)?;
    let result = rpc_request(
        &socket,
        json!({"id": 1, "method": "initialize", "params": {}}),
    )?;
    let output = json!({
        "ok": true,
        "socket": socket,
        "server": result,
    });
    if output_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| format!("json_render_failed: {e}"))?
        );
    } else {
        println!("app_server_available: true");
        println!("socket: {}", socket.display());
        println!(
            "server: {}",
            output["server"]["serverInfo"]["name"]
                .as_str()
                .unwrap_or("unknown")
        );
    }
    Ok(())
}

fn parse_probe_args(args: &[String]) -> Result<(PathBuf, bool), String> {
    let mut socket = None;
    let mut output_json = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--socket" => {
                let value = args
                    .get(index + 1)
                    .ok_or_else(|| "app-server probe requires value after --socket".to_string())?;
                socket = Some(PathBuf::from(value));
                index += 2;
            }
            "--json" => {
                output_json = true;
                index += 1;
            }
            _ => {
                return Err(
                    "usage: chuang-agent app-server probe --socket PATH [--json]".to_string(),
                )
            }
        }
    }
    Ok((
        socket.ok_or_else(|| "app-server probe requires --socket PATH".to_string())?,
        output_json,
    ))
}

fn app_server_ask_command(args: &[String]) -> Result<(), String> {
    let ask = parse_ask_args(args)?;
    let request = json!({
        "id": 1,
        "method": "turn/start",
        "params": {
            "threadId": ask.thread_id,
            "workspaceRoot": ask.workspace_root,
            "text": ask.text,
        }
    });
    let result = rpc_request(&ask.socket, request)?;
    let assistant_text = result["thread"]["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .and_then(|turn| {
            turn["items"]
                .as_array()
                .and_then(|items| items.iter().find(|item| item["type"] == "agentMessage"))
                .and_then(|item| item["text"].as_str())
        })
        .map(str::to_string)
        .unwrap_or_default();
    let output = json!({
        "assistant_text": assistant_text,
        "thread_id": result["thread"]["id"],
        "turn": result["turn"],
    });

    if ask.output_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|e| format!("json_render_failed: {e}"))?
        );
    } else {
        println!("{}", output["assistant_text"].as_str().unwrap_or(""));
        println!(
            "thread_id: {}",
            output["thread_id"].as_str().unwrap_or("unknown")
        );
        println!(
            "turn_id: {}",
            output["turn"]["id"].as_str().unwrap_or("unknown")
        );
        println!(
            "turn_status: {}",
            output["turn"]["status"].as_str().unwrap_or("unknown")
        );
    }
    Ok(())
}

struct AppServerAskArgs {
    socket: PathBuf,
    workspace_root: String,
    text: String,
    thread_id: Option<String>,
    output_json: bool,
}

fn parse_ask_args(args: &[String]) -> Result<AppServerAskArgs, String> {
    let mut socket = None;
    let mut workspace_root = None;
    let mut text = None;
    let mut thread_id = None;
    let mut output_json = false;
    let mut index = 0;
    while index < args.len() {
        let value = |flag: &str| {
            args.get(index + 1)
                .cloned()
                .ok_or_else(|| format!("app-server ask requires value after {flag}"))
        };
        match args[index].as_str() {
            "--socket" => {
                socket = Some(PathBuf::from(value("--socket")?));
                index += 2;
            }
            "--workspace-root" => {
                workspace_root = Some(value("--workspace-root")?);
                index += 2;
            }
            "--text" => {
                text = Some(value("--text")?);
                index += 2;
            }
            "--thread-id" => {
                thread_id = Some(value("--thread-id")?);
                index += 2;
            }
            "--json" => {
                output_json = true;
                index += 1;
            }
            _ => {
                return Err(
                    "usage: chuang-agent app-server ask --socket PATH --workspace-root PATH --text TEXT [--thread-id ID] [--json]"
                        .to_string(),
                )
            }
        }
    }
    let text = normalize_text(text.as_deref());
    if text.is_empty() {
        return Err("app-server ask requires non-empty --text TEXT".to_string());
    }
    Ok(AppServerAskArgs {
        socket: socket.ok_or_else(|| "app-server ask requires --socket PATH".to_string())?,
        workspace_root: workspace_root
            .ok_or_else(|| "app-server ask requires --workspace-root PATH".to_string())?,
        text,
        thread_id: thread_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        output_json,
    })
}

fn rpc_request(socket: &Path, request: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket).map_err(|e| {
        format!(
            "app_server_unavailable: socket={} error={e}",
            socket.display()
        )
    })?;
    write_json_line(&mut stream, &request)?;
    let request_id = request
        .get("id")
        .cloned()
        .ok_or_else(|| "app_server_client_request_missing_id".to_string())?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        let read = reader
            .read_line(&mut line)
            .map_err(|e| format!("app_server_client_read_failed: {e}"))?;
        if read == 0 {
            return Err(format!(
                "app_server_unavailable: socket={} error=connection_closed_before_response",
                socket.display()
            ));
        }
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|e| format!("app_server_client_invalid_json: {e}"))?;
        if value.get("id") != Some(&request_id) {
            continue;
        }
        if let Some(message) = value["error"]["message"].as_str() {
            return Err(format!("app_server_rpc_failed: {message}"));
        }
        return value
            .get("result")
            .cloned()
            .ok_or_else(|| "app_server_client_response_missing_result".to_string());
    }
}

fn app_server_health_command(args: &[String]) -> Result<(), String> {
    let mut workspace_root = String::new();
    let mut output_json = false;
    let mut diagnostic = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--workspace-root" => {
                workspace_root = args
                    .get(index + 1)
                    .ok_or_else(|| {
                        "app-server health requires value after --workspace-root".to_string()
                    })?
                    .clone();
                index += 2;
            }
            "--json" => {
                output_json = true;
                index += 1;
            }
            "--diagnostic" => {
                diagnostic = true;
                index += 1;
            }
            _ => {
                return Err(
                    "usage: cargo run -- app-server health [--workspace-root PATH] [--diagnostic] [--json]"
                        .to_string(),
                )
            }
        }
    }

    let normalized_workspace_root = normalize_workspace_root(&workspace_root);
    let workspace_status = workspace_status_for_root(&normalized_workspace_root);
    let runtime = if diagnostic {
        build_runtime_for_workspace_with_options(
            &normalized_workspace_root,
            RuntimeConfigFileOptions::allow_missing_env(),
        )?
    } else {
        build_runtime_for_workspace(&normalized_workspace_root)?
    };
    runtime
        .validate()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let kernel = kernel_config_from_runtime(&runtime)?;
    let status = build_chuang_mvp_status(&runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let config_summary = runtime.summary();
    let diagnostic_status = app_server_health_diagnostic_status(&config_summary);
    let diagnostic_summary = app_server_health_diagnostic_summary(&config_summary, diagnostic);
    let next_actions = app_server_health_next_actions(&config_summary);
    let identity_memory_root = runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?
        .root
        .display()
        .to_string();
    let result = json!({
        "ok": true,
        "server": "chuang-agent-app-server",
        "version": env!("CARGO_PKG_VERSION"),
        "workspace_root": normalized_workspace_root,
        "workspace": workspace_status,
        "model": provider_summary_model_name(&runtime),
        "diagnostic_mode": diagnostic,
        "diagnostic_status": diagnostic_status,
        "diagnostic_summary": diagnostic_summary.clone(),
        "next_actions": next_actions.clone(),
        "api_key_state": config_summary.api_key_state,
        "placeholder_warnings": config_summary.placeholder_warnings,
        "subagent_live_worker": config_summary.subagent_live_worker,
        "runtime_capability_primer": status.runtime_capability_primer.clone(),
        "goal_mode": status.goal_mode,
        "goal_run": status.goal_run,
        "runtime_report_surface": status.runtime_report_surface,
        "policy_tool_status": status.policy_tool_status,
        "provider_readiness": status.provider_readiness,
        "atomic_tools": status.atomic_tools.clone(),
        "project_readiness": status.project_readiness,
        "local_contract_readiness": status.local_contract_readiness,
        "release_readiness": status.release_readiness,
        "third_test_candidate": status.third_test_candidate,
        "channel_readiness": status.channel_readiness,
        "subagent_readiness": status.subagent_readiness,
        "live_adapter_gates": status.live_adapter_gates,
        "effective_live_adapter_gates": status.effective_live_adapter_gates,
        "app_server_service": status.app_server_service,
        "live_readiness": status.live_readiness,
        "external_ai_readiness": status.external_ai_readiness,
        "db_path": runtime.db_path.display().to_string(),
        "identity_memory_root": identity_memory_root,
        "identity_soul_path": runtime.identity_bootstrap.soul_path.display().to_string(),
        "rules_core_path": runtime.rules.core_path.display().to_string(),
        "subagent_queue_root": runtime.subagent_queue.root.display().to_string(),
    });

    if output_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result)
                .map_err(|e| format!("json_render_failed: {e}"))?
        );
    } else {
        println!("app_server_ok: true");
        println!("diagnostic_mode: {diagnostic}");
        println!("diagnostic_status: {}", diagnostic_status);
        println!("diagnostic_summary: {}", diagnostic_summary);
        println!(
            "workspace_root: {}",
            result["workspace_root"].as_str().unwrap_or("")
        );
        println!(
            "workspace_config_root: {}",
            result["workspace"]["config_root"].as_str().unwrap_or("")
        );
        println!(
            "workspace_app_server_child_root: {}",
            result["workspace"]["app_server_child_root"]
                .as_str()
                .unwrap_or("")
        );
        println!(
            "workspace_matches_config: {}",
            result["workspace"]["matches_config"]
                .as_bool()
                .unwrap_or(false)
        );
        println!("model: {}", result["model"].as_str().unwrap_or(""));
        println!(
            "api_key_state: {}",
            result["api_key_state"].as_str().unwrap_or("none")
        );
        if config_summary.placeholder_warnings.is_empty() {
            println!("placeholder_warnings: none");
        } else {
            println!(
                "placeholder_warnings: {}",
                config_summary.placeholder_warnings.join(";")
            );
        }
        println!(
            "subagent_live_worker: enabled={} adapter_kind={} status={} starts_worker={} available={} reason={}",
            config_summary.subagent_live_worker.enabled,
            config_summary.subagent_live_worker.adapter_kind,
            config_summary.subagent_live_worker.status,
            config_summary.subagent_live_worker.starts_worker,
            config_summary.subagent_live_worker.available,
            config_summary.subagent_live_worker.reason
        );
        println!(
            "runtime_capability_primer: {}",
            status.runtime_capability_primer
        );
        if next_actions.is_empty() {
            println!("next_actions: none");
        } else {
            println!("next_actions: {}", next_actions.join(";"));
        }
        println!(
            "provider_readiness: ok={} state={} kind={} transport={} fallback_configured={} timeout_ms={} api_key_state={} placeholder_warnings={}",
            status.provider_readiness.ok,
            status.provider_readiness.overall_state,
            status.provider_readiness.provider_kind,
            status.provider_readiness.transport,
            status.provider_readiness.fallback_configured,
            status
                .provider_readiness
                .request_timeout_ms
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            status
                .provider_readiness
                .api_key_state
                .as_deref()
                .unwrap_or("none"),
            status.provider_readiness.placeholder_warning_count
        );
        println!(
            "provider_readiness_current: {}",
            status.provider_readiness.current
        );
        println!(
            "provider_readiness_next_action: {}",
            status.provider_readiness.next_action
        );
        println!(
            "atomic_tools: source={} ok={} total={} mapped={} interface_only={} manifest_schema_version={} action_schema_version={} report_schema_version={}",
            status.atomic_tools.source,
            status.atomic_tools.ok,
            status.atomic_tools.total_count,
            status.atomic_tools.mapped_count,
            status.atomic_tools.interface_only_count,
            status.atomic_tools.manifest_schema_version,
            status.atomic_tools.tool_action_schema_version,
            status.atomic_tools.tool_report_schema_version
        );
        println!(
            "atomic_tools_executable: {}",
            format_text_list(&status.atomic_tools.governed_executable_atomic_tool_names)
        );
        println!(
            "atomic_tools_interface_only: {}",
            format_text_list(&status.atomic_tools.interface_only_atomic_tool_names)
        );
        println!(
            "atomic_tools_desktop_browser_interface_only: {} reason={}",
            format_text_list(
                &status
                    .atomic_tools
                    .desktop_browser_interface_only_atomic_tool_names
            ),
            status.atomic_tools.interface_only_reason
        );
        println!(
            "atomic_tools_desktop_browser_live_gated: {} required=adapter,live_gate,allowlist,audit_receipt",
            format_text_list(
                &status
                    .atomic_tools
                    .desktop_browser_live_gated_atomic_tool_names
            )
        );
        println!(
            "atomic_tools_self_check_entrypoints: {}",
            format_text_list(&status.atomic_tools.local_cli_self_check_entrypoints)
        );
        println!(
            "policy_tool_status: active_profile={} normal_local_action_default={} high_risk_boundary={} ga_tool_descriptors={}/{} missing={}",
            status.policy_tool_status.active_permission_profile,
            status.policy_tool_status.local_ga_normal_local_action_default,
            status.policy_tool_status.local_ga_high_risk_boundary_summary,
            status.policy_tool_status.ga_tool_descriptor_mapped_count,
            status.policy_tool_status.tool_descriptor_count,
            status.policy_tool_status.ga_tool_descriptor_missing.len()
        );
        println!(
            "goal_mode: ok={} kind={} cli_entrypoint={} context_source={} default_goal_id={} allowed_slots={} checkpoint_policy=progress_log:{} handoff:{} commit:{} final_report_policy=validation:{} next_steps:{} bypasses_governance={} adds_core_slot={}",
            status.goal_mode.ok,
            status.goal_mode.kind,
            status.goal_mode.cli_entrypoint,
            status.goal_mode.context_source,
            status.goal_mode.default_goal_id,
            status.goal_mode.default_allowed_slots.join(","),
            status.goal_mode.checkpoint_policy.update_progress_log,
            status.goal_mode.checkpoint_policy.update_handoff,
            status.goal_mode.checkpoint_policy.commit_checkpoint,
            status.goal_mode.final_report_policy.include_validation,
            status.goal_mode.final_report_policy.include_next_steps,
            status.goal_mode.bypasses_governance,
            status.goal_mode.adds_core_slot
        );
        println!(
            "goal_run: ok={} plan_exists={} goal_id={} checkpoints={} workers={} validation_commands={} path={}",
            status.goal_run.ok,
            status.goal_run.plan_exists,
            status.goal_run.goal_id,
            status.goal_run.checkpoint_count,
            status.goal_run.worker_count,
            status.goal_run.validation_command_count,
            status.goal_run.path
        );
        println!(
            "runtime_report_surface: ok={} artifacts={} observability_fields={} artifact_locators={} observability={}",
            status.runtime_report_surface.ok,
            status.runtime_report_surface.artifact_count,
            status.runtime_report_surface.observability_field_count,
            format_text_list(&status.runtime_report_surface.artifact_locators),
            format_text_list(&status.runtime_report_surface.observability_fields)
        );
        println!(
            "goal_run_readiness: ok={} plan_exists={} goal_id={} checkpoints={} workers={} validation_commands={} checkpoint_log_complete={} last_checkpoint={} last_summary={} last_created_at={} last_completed_worker_ids={} last_validation_notes={} incomplete_reasons={}",
            status.goal_run.ok,
            status.goal_run.plan_exists,
            status.goal_run.goal_id,
            status.goal_run.checkpoint_count,
            status.goal_run.worker_count,
            status.goal_run.validation_command_count,
            status.goal_run.checkpoint_log_complete,
            status
                .goal_run
                .last_checkpoint_id
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_summary
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_created_at
                .as_deref()
                .unwrap_or("none"),
            status
                .goal_run
                .last_checkpoint_completed_worker_ids
                .as_ref()
                .map(|values| values.join(","))
                .unwrap_or_else(|| "none".to_string()),
            status
                .goal_run
                .last_checkpoint_validation_notes
                .as_ref()
                .map(|values| values.join(" | "))
                .unwrap_or_else(|| "none".to_string()),
            if status.goal_run.incomplete_reasons.is_empty() {
                "none".to_string()
            } else {
                status.goal_run.incomplete_reasons.join(";")
            }
        );
        println!(
            "goal_run_checkpoint_log_complete: {}",
            status.goal_run.checkpoint_log_complete
        );
        println!(
            "goal_run_last_checkpoint: {}",
            status
                .goal_run
                .last_checkpoint_id
                .as_deref()
                .unwrap_or("none")
        );
        println!(
            "goal_run_last_checkpoint_summary: {}",
            status
                .goal_run
                .last_checkpoint_summary
                .as_deref()
                .unwrap_or("none")
        );
        println!(
            "goal_run_last_checkpoint_created_at: {}",
            status
                .goal_run
                .last_checkpoint_created_at
                .as_deref()
                .unwrap_or("none")
        );
        println!(
            "goal_run_last_checkpoint_completed_worker_ids: {}",
            status
                .goal_run
                .last_checkpoint_completed_worker_ids
                .as_ref()
                .map(|values| values.join(","))
                .unwrap_or_else(|| "none".to_string())
        );
        println!(
            "goal_run_last_checkpoint_validation_notes: {}",
            status
                .goal_run
                .last_checkpoint_validation_notes
                .as_ref()
                .map(|values| values.join(" | "))
                .unwrap_or_else(|| "none".to_string())
        );
        if let Some(read_error) = &status.goal_run.read_error {
            println!("goal_run_read_error: {read_error}");
        }
        println!(
            "goal_run_incomplete_reasons: {}",
            if status.goal_run.incomplete_reasons.is_empty() {
                "none".to_string()
            } else {
                status.goal_run.incomplete_reasons.join(";")
            }
        );
        println!(
            "local_contract_readiness: ok={} state={} contracts={} connects_real_external_services={} writes_core_memory={} executes_plugins={}",
            status.local_contract_readiness.ok,
            status.local_contract_readiness.overall_state,
            status.local_contract_readiness.contract_count,
            status.local_contract_readiness.connects_real_external_services,
            status.local_contract_readiness.writes_core_memory,
            status.local_contract_readiness.executes_plugins
        );
        println!(
            "subagent_readiness: ok={} state={} mode={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} layers={} ready={} partial={} deferred={} blocked={} live_worker_available={} worker_runtime_state={} worker_runtime_blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={}",
            status.subagent_readiness.ok,
            status.subagent_readiness.overall_state,
            status.subagent_readiness.mode,
            status.subagent_readiness.local_contract_ready,
            status.subagent_readiness.local_contract_state,
            status.subagent_readiness.live_adapter_ready,
            status.subagent_readiness.live_adapter_state,
            status.subagent_readiness.layer_count,
            status.subagent_readiness.ready_count,
            status.subagent_readiness.partial_count,
            status.subagent_readiness.deferred_count,
            status.subagent_readiness.blocked_count,
            status.subagent_readiness.live_worker_available,
            status.subagent_readiness.worker_runtime_state,
            status.subagent_readiness.worker_runtime_blocked_reason,
            status.subagent_readiness.capability_route_state,
            status.subagent_readiness.capability_mismatch_blocks_live,
            status.subagent_readiness.capability_mismatch_reason
        );
        println!(
            "subagent_worker_runtime_reason: {}",
            status.subagent_readiness.worker_runtime_reason
        );
        println!(
            "subagent_model_tool_worker: available={} state={} reason={}",
            status.subagent_readiness.model_tool_worker_available,
            status.subagent_readiness.model_tool_worker_state,
            status.subagent_readiness.model_tool_worker_reason
        );
        println!(
            "subagent_readiness_local_contract_reason: {}",
            status.subagent_readiness.local_contract_reason
        );
        println!(
            "subagent_readiness_live_adapter_reason: {}",
            status.subagent_readiness.live_adapter_reason
        );
        for layer in &status.subagent_readiness.layers {
            println!(
                "subagent_layer name={} state={} local_contract_ready={} local_contract_state={} live_adapter_ready={} live_adapter_state={} live_worker_available={} worker_runtime_state={} blocked_reason={} capability_route_state={} capability_mismatch_blocks_live={} capability_mismatch_reason={} boundary={} local_contract_reason={} live_adapter_reason={} next={}",
                layer.name,
                layer.state,
                layer.local_contract_ready,
                layer.local_contract_state,
                layer.live_adapter_ready,
                layer.live_adapter_state,
                layer.live_worker_available,
                layer.worker_runtime_state,
                layer.blocked_reason,
                layer.capability_route_state,
                layer.capability_mismatch_blocks_live,
                layer.capability_mismatch_reason,
                layer.boundary,
                layer.local_contract_reason,
                layer.live_adapter_reason,
                layer.next_action
            );
        }
        println!(
            "live_adapter_gates: ok={} state={} gates={} enabled={} disabled={}",
            status.live_adapter_gates.ok,
            status.live_adapter_gates.overall_state,
            status.live_adapter_gates.gate_count,
            status.live_adapter_gates.enabled_count,
            status.live_adapter_gates.disabled_count
        );
        for gate in &status.live_adapter_gates.gates {
            println!(
                "live_adapter_gate name={} state={} enabled={} default_enabled={} env_value_state={} required_env={} audit_label={} preflight={} must_reject={} reason={} next={}",
                gate.name,
                gate.state,
                gate.enabled,
                gate.default_enabled,
                gate.env_value_state,
                gate.required_env,
                gate.audit_label,
                format_text_list(&gate.preflight_checks),
                format_text_list(&gate.must_reject_capabilities),
                gate.reason,
                gate.next_action
            );
        }
        let service = &status.app_server_service;
        println!(
            "app_server_service: name={} observation_state={} loaded={} active={} substate={} enabled={} main_pid={} restart_count={} fragment_path={} binary_summary={} caller_environment={} service_environment={} effective_environment={} persistence_state={} persistence_enabled={} schema={} lock_held={} threads={} turns={} active={} interrupted={} snapshot_updated_at={}",
            service.service_name,
            service.observation_state,
            service.loaded.as_deref().unwrap_or("none"),
            service.active.as_deref().unwrap_or("none"),
            service.substate.as_deref().unwrap_or("none"),
            service.enabled.as_deref().unwrap_or("none"),
            service
                .main_pid
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            service
                .restart_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "none".to_string()),
            service.fragment_path.as_deref().unwrap_or("none"),
            service.binary_summary.as_deref().unwrap_or("none"),
            service.caller_environment.source,
            service
                .service_environment
                .as_ref()
                .map(|environment| environment.source.as_str())
                .unwrap_or("unavailable"),
            service.effective_environment,
            service.persistence.observation_state,
            service
                .persistence
                .persistence_enabled
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .schema
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .lock_held
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .thread_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .turn_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .active_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .interrupted_count
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string()),
            service
                .persistence
                .snapshot_updated_at
                .map(|value| value.to_string())
                .unwrap_or_else(|| "unavailable".to_string())
        );
        println!(
            "effective_live_adapter_gates: source={} ok={} state={} gates={} enabled={} disabled={}",
            service.effective_environment,
            status.effective_live_adapter_gates.ok,
            status.effective_live_adapter_gates.overall_state,
            status.effective_live_adapter_gates.gate_count,
            status.effective_live_adapter_gates.enabled_count,
            status.effective_live_adapter_gates.disabled_count
        );
        for gate in &status.effective_live_adapter_gates.gates {
            println!(
                "effective_live_adapter_gate name={} state={} enabled={} default_enabled={} env_value_state={} required_env={} audit_label={} preflight={} must_reject={} reason={} next={}",
                gate.name,
                gate.state,
                gate.enabled,
                gate.default_enabled,
                gate.env_value_state,
                gate.required_env,
                gate.audit_label,
                format_text_list(&gate.preflight_checks),
                format_text_list(&gate.must_reject_capabilities),
                gate.reason,
                gate.next_action
            );
        }
        let live_readiness = &status.live_readiness;
        println!(
            "live_readiness: ok={} state={} local_ready_scope={} ga_local_mapped_only={} desktop_browser_live_gated={} browser_worker_frozen={} live_worker_available={} real_external_acceptance_pending={} provider_live_request_verified_by_status={} mapped_does_not_mean_live={} gated_does_not_mean_ready={} frozen_does_not_mean_ready={} ready_does_not_mean_live={}",
            live_readiness.ok,
            live_readiness.overall_state,
            live_readiness.local_ready_scope,
            live_readiness.ga_local_mapped_only,
            live_readiness.desktop_browser_live_gated,
            live_readiness.browser_worker_frozen,
            live_readiness.live_worker_available,
            live_readiness.real_external_acceptance_pending,
            live_readiness.provider_live_request_verified_by_status,
            live_readiness.mapped_does_not_mean_live,
            live_readiness.gated_does_not_mean_ready,
            live_readiness.frozen_does_not_mean_ready,
            live_readiness.ready_does_not_mean_live
        );
        println!(
            "release_readiness: ok={} name={} state={}",
            status.release_readiness.ok,
            status.release_readiness.release_name,
            status.release_readiness.overall_state
        );
        println!(
            "release_acceptance: count={} connects_real_external_services={} verifies_real_external_services={} uses_stub_or_local_fixtures={}",
            status.release_readiness.acceptance_count,
            status.release_readiness.connects_real_external_services,
            status.release_readiness.verifies_real_external_services,
            status.release_readiness.uses_stub_or_local_fixtures
        );
        println!(
            "third_test_candidate: ok={} state={} local_gate_ready={} smoke_script={} marker={} requires_manual_live_check={} connects_real_external_services={} operator_env_blocks_100_percent={} real_live_ready={}",
            status.third_test_candidate.ok,
            status.third_test_candidate.overall_state,
            status.third_test_candidate.local_gate_ready,
            status.third_test_candidate.smoke_script,
            status.third_test_candidate.marker,
            status.third_test_candidate.requires_manual_live_check,
            status.third_test_candidate.connects_real_external_services,
            status.third_test_candidate.operator_env_blocks_100_percent,
            status.third_test_candidate.real_live_ready
        );
    }

    Ok(())
}

fn handle_initialize() -> Value {
    json!({
        "serverInfo": {
            "name": "chuang-agent-app-server",
            "version": env!("CARGO_PKG_VERSION"),
        },
        "capabilities": {
            "threads": true,
            "models": true,
            "turns": true,
        }
    })
}

fn handle_model_list(params: &Value) -> Result<Value, String> {
    let workspace_root = normalize_workspace_root(
        params
            .get("workspaceRoot")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    let runtime = build_runtime_for_workspace(&app_server_config_workspace_root(&workspace_root))?;
    let model_name = provider_primary_model_name(&runtime);
    // [metadata] model_list=model1,model2,... 可暴露多个可用模型；
    // 未配置时保持向后兼容（只返回主模型）。
    let extra_models = runtime
        .metadata
        .get("model_list")
        .map(|value| {
            value
                .split(',')
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty() && item.as_str() != model_name)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let mut models = vec![{
        json!({
            "id": model_name,
            "model": model_name,
            "displayName": model_name,
            "isDefault": true,
            "supportedReasoningEfforts": ["low", "medium", "high", "xhigh", "max"],
            "defaultReasoningEffort": "medium",
        })
    }];
    for extra in extra_models {
        models.push(json!({
            "id": extra,
            "model": extra,
            "displayName": extra,
            "isDefault": false,
            "supportedReasoningEfforts": ["low", "medium", "high", "xhigh", "max"],
            "defaultReasoningEffort": "medium",
        }));
    }
    Ok(json!({
        "data": models,
    }))
}

fn handle_thread_start(state: &mut AppServerState, params: &Value) -> Result<Value, String> {
    let workspace_root = normalize_workspace_root(
        params
            .get("cwd")
            .and_then(|value| value.as_str())
            .unwrap_or(""),
    );
    let display_name = params
        .get("displayName")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace thread".to_string());
    let thread = create_thread(state, workspace_root, display_name);
    Ok(json!({
        "thread": thread_to_json(&thread),
    }))
}

fn handle_thread_resume(state: &AppServerState, params: &Value) -> Result<Value, String> {
    let thread_id = params
        .get("threadId")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();
    let Some(thread) = state.threads.get(thread_id) else {
        return Err(format!("unknown_thread: {thread_id}"));
    };

    Ok(json!({
        "thread": thread_to_resume_json(thread),
    }))
}

fn handle_thread_list(state: &AppServerState) -> Value {
    let mut threads = state
        .threads
        .values()
        .map(thread_to_list_json)
        .collect::<Vec<_>>();
    threads.sort_by(|left, right| right["updatedAt"].as_u64().cmp(&left["updatedAt"].as_u64()));
    json!({
        "data": threads,
        "nextCursor": "",
    })
}

#[derive(Debug, Clone)]
struct PreparedLiveTurn {
    thread_id: String,
    turn_id: String,
    workspace_root: String,
    input_text: String,
    image_paths: Vec<String>,
    conversation_history: Vec<ConversationHistoryItem>,
    goal_spec: Option<GoalSpec>,
    guidance_path: PathBuf,
    progress_path: PathBuf,
}

#[derive(Debug)]
struct LiveTurnStorage {
    guidance_path: PathBuf,
    progress_path: PathBuf,
    guidance_writer: Arc<Mutex<File>>,
}

struct LiveTurnResult {
    tool_run: ToolLoopResult,
    live_readiness: Value,
    context_max_tokens: u32,
    elapsed_ms: u64,
}

fn handle_live_turn_start(
    state: SharedAppServerState,
    params: &Value,
    writer: &mut dyn Write,
) -> Result<Value, String> {
    let prepared = prepare_live_turn(&state, params)?;
    if let Err(error) = write_json_line(
        writer,
        &json!({
            "method": "turn/started",
            "params": {
                "threadId": prepared.thread_id,
                "turn": { "id": prepared.turn_id },
            }
        }),
    ) {
        if record_live_turn_failure(&state, &prepared, "cancelled").is_err() {
            unregister_active_turn(&state, &prepared.thread_id, &prepared.turn_id);
        }
        return Err(error);
    }

    let task_params = params.clone();
    let task = prepared.clone();
    let (sender, receiver) = mpsc::channel();
    let runtime_worker = thread::spawn(move || {
        let _ = sender.send(run_live_turn(task, &task_params));
    });

    let mut progress_cursor = 0usize;
    let mut stream_error = None;
    let runtime_result = loop {
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(result) => {
                if stream_error.is_none() {
                    if let Err(error) =
                        emit_live_progress(writer, &prepared, &mut progress_cursor, true)
                    {
                        stream_error = Some(error);
                    }
                }
                break result;
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stream_error.is_none() {
                    if let Err(error) =
                        emit_live_progress(writer, &prepared, &mut progress_cursor, false)
                    {
                        stream_error = Some(error);
                    }
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                break Err("app_server_runtime_worker_disconnected".to_string());
            }
        }
    };
    let _ = runtime_worker.join();

    match runtime_result {
        Ok(live_turn) => {
            let thread = record_live_turn_success(&state, &prepared, &live_turn)?;
            if let Some(error) = stream_error {
                return Err(error);
            }
            emit_live_turn_success(writer, &prepared, &thread, live_turn)
        }
        Err(error) => {
            let status = if error.contains("turn_cancelled_at_safe_point:") {
                "cancelled"
            } else {
                "failed"
            };
            record_live_turn_failure(&state, &prepared, status)?;
            if let Some(stream_error) = stream_error {
                return Err(stream_error);
            }
            write_json_line(
                writer,
                &json!({
                    "method": "turn/completed",
                    "params": {
                        "threadId": prepared.thread_id,
                        "turn": {
                            "id": prepared.turn_id,
                            "status": status,
                            "error": error,
                        }
                    }
                }),
            )?;
            Err(error)
        }
    }
}

fn prepare_live_turn(
    state: &SharedAppServerState,
    params: &Value,
) -> Result<PreparedLiveTurn, String> {
    let requested_thread_id =
        normalize_text(params.get("threadId").and_then(|value| value.as_str()));
    let requested_workspace_root = params
        .get("workspaceRoot")
        .and_then(|value| value.as_str())
        .map(normalize_workspace_root);
    let (input_text, image_paths) = extract_turn_input(params);
    if input_text.is_empty() {
        if image_paths.is_empty() {
            return Err("turn/start requires non-empty input".to_string());
        }
    }
    let goal_spec = extract_turn_goal(params)?;

    let (thread_id, turn_id, workspace_root, conversation_history) =
        with_app_server_state(state, |state| {
            let thread_id = if requested_thread_id.is_empty() {
                let workspace_root =
                    requested_workspace_root.unwrap_or_else(|| normalize_workspace_root(""));
                create_thread(
                    state,
                    workspace_root.clone(),
                    thread_display_name(&workspace_root),
                )
                .id
            } else if let Some(thread) = state.threads.get(&requested_thread_id) {
                if let Some(workspace_root) = requested_workspace_root.as_deref() {
                    ensure_thread_workspace_matches(thread, workspace_root)?;
                }
                requested_thread_id.clone()
            } else {
                return Err(format!("unknown_thread: {requested_thread_id}"));
            };

            if state.active_turns.contains_key(&thread_id) {
                return Err(format!("thread_busy: threadId={thread_id}"));
            }

            let workspace_root = state
                .threads
                .get(&thread_id)
                .map(|thread| thread.workspace_root.clone())
                .ok_or_else(|| format!("unknown_thread: {thread_id}"))?;
            let turn_id = next_turn_id(state);
            let conversation_history = recent_thread_history(state, &thread_id, 6);
            persist_app_server_state(state)?;

            Ok((thread_id, turn_id, workspace_root, conversation_history))
        })?;

    let storage = create_live_turn_storage()?;
    with_app_server_state(state, |state| {
        if state.active_turns.contains_key(&thread_id) {
            return Err(format!("thread_busy: threadId={thread_id}"));
        }
        state.active_turns.insert(
            thread_id.clone(),
            ActiveTurn {
                thread_id: thread_id.clone(),
                turn_id: turn_id.clone(),
                guidance_path: storage.guidance_path.clone(),
                guidance_writer: Arc::clone(&storage.guidance_writer),
            },
        );
        let now = now_millis();
        let thread = state
            .threads
            .get_mut(&thread_id)
            .ok_or_else(|| format!("unknown_thread: {thread_id}"))?;
        thread.updated_at = now;
        thread.turns.push(TurnState {
            id: turn_id.clone(),
            user_text: input_text.clone(),
            assistant_text: String::new(),
            model_name: String::new(),
            status: "active".to_string(),
            provider_meta: BTreeMap::new(),
            tool_trace: String::new(),
            tool_surface: None,
            updated_at: now,
        });
        if let Err(error) = persist_app_server_state(state) {
            remove_active_turn(state, &thread_id, &turn_id);
            if let Some(thread) = state.threads.get_mut(&thread_id) {
                thread.turns.retain(|turn| turn.id != turn_id);
            }
            return Err(error);
        }

        Ok(PreparedLiveTurn {
            thread_id,
            turn_id,
            workspace_root,
            input_text,
            image_paths,
            conversation_history,
            goal_spec,
            guidance_path: storage.guidance_path,
            progress_path: storage.progress_path,
        })
    })
}

fn create_live_turn_storage() -> Result<LiveTurnStorage, String> {
    let runtime_root = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    if let Some(runtime_root) = runtime_root {
        let base = runtime_root.join("chuang-agent").join("live-turns");
        ensure_private_directory(&runtime_root.join("chuang-agent"))?;
        ensure_private_directory(&base)?;
        return create_live_turn_storage_under(&base);
    }
    create_live_turn_storage_under(&std::env::temp_dir())
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                return Err(format!(
                    "live_turn_storage_not_private_directory: {}",
                    path.display()
                ));
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            match fs::DirBuilder::new().mode(0o700).create(path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    let metadata = fs::symlink_metadata(path).map_err(|error| {
                        format!(
                            "live_turn_storage_directory_metadata_failed: path={} error={error}",
                            path.display()
                        )
                    })?;
                    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
                        return Err(format!(
                            "live_turn_storage_not_private_directory: {}",
                            path.display()
                        ));
                    }
                }
                Err(error) => {
                    return Err(format!(
                        "live_turn_storage_directory_create_failed: path={} error={error}",
                        path.display()
                    ))
                }
            }
        }
        Err(error) => {
            return Err(format!(
                "live_turn_storage_directory_metadata_failed: path={} error={error}",
                path.display()
            ))
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|error| {
        format!(
            "live_turn_storage_directory_permissions_failed: path={} error={error}",
            path.display()
        )
    })
}

fn create_live_turn_storage_under(base: &Path) -> Result<LiveTurnStorage, String> {
    for _ in 0..1024 {
        let nonce = APP_SERVER_TURN_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let turn_dir = base.join(format!("turn-{}-{}-{nonce}", process::id(), now_millis()));
        match fs::DirBuilder::new().mode(0o700).create(&turn_dir) {
            Ok(()) => {
                fs::set_permissions(&turn_dir, fs::Permissions::from_mode(0o700)).map_err(
                    |error| {
                        format!(
                            "live_turn_storage_directory_permissions_failed: path={} error={error}",
                            turn_dir.display()
                        )
                    },
                )?;
                return create_live_turn_files(&turn_dir);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "live_turn_storage_directory_create_failed: path={} error={error}",
                    turn_dir.display()
                ))
            }
        }
    }
    Err(format!(
        "live_turn_storage_directory_name_exhausted: {}",
        base.display()
    ))
}

fn create_live_turn_files(turn_dir: &Path) -> Result<LiveTurnStorage, String> {
    let guidance_path = turn_dir.join("guidance.txt");
    let progress_path = turn_dir.join("progress.jsonl");
    let guidance_file = create_private_turn_file(&guidance_path, true)?;
    let _progress_file = create_private_turn_file(&progress_path, false)?;
    Ok(LiveTurnStorage {
        guidance_path,
        progress_path,
        guidance_writer: Arc::new(Mutex::new(guidance_file)),
    })
}

fn create_private_turn_file(path: &Path, append: bool) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options
        .create_new(true)
        .write(true)
        .append(append)
        .mode(0o600);
    let file = options.open(path).map_err(|error| {
        format!(
            "live_turn_storage_file_create_failed: path={} error={error}",
            path.display()
        )
    })?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .map_err(|error| {
            format!(
                "live_turn_storage_file_permissions_failed: path={} error={error}",
                path.display()
            )
        })?;
    Ok(file)
}

fn run_live_turn(mut prepared: PreparedLiveTurn, params: &Value) -> Result<LiveTurnResult, String> {
    let mut runtime =
        build_runtime_for_workspace(&app_server_config_workspace_root(&prepared.workspace_root))?;
    runtime
        .metadata
        .insert("channel".to_string(), "app-server".to_string());
    let runtime = override_runtime_model(runtime, params);
    // 识图兜底：主模型不支持视觉时，先把图片交给视觉模型描述成文字，
    // 描述结果并入本轮输入；历史只保留文本，图片不代入后续会话。
    if !prepared.image_paths.is_empty() {
        let described = describe_images_with_vision(&runtime, &prepared.image_paths);
        match described {
            Ok(text) => {
                if !prepared.input_text.is_empty() {
                    prepared.input_text.push('\n');
                }
                prepared.input_text.push_str("[用户附带的图片内容]\n");
                prepared.input_text.push_str(&text);
            }
            Err(error) => {
                eprintln!("[vision-fallback] describe failed: {error}");
                let paths = prepared.image_paths.join(", ");
                if !prepared.input_text.is_empty() {
                    prepared.input_text.push('\n');
                }
                prepared.input_text.push_str(&format!(
                    "[图片无法通过视觉模型识别（{error}），图片路径：{paths}]"
                ));
            }
        }
    }
    let live_readiness = build_chuang_mvp_status(&runtime, &kernel_config_from_runtime(&runtime)?)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?
        .live_readiness;
    let live_readiness =
        serde_json::to_value(live_readiness).map_err(|e| format!("json_render_failed: {e}"))?;
    let context_max_tokens = runtime.context_budget.max_tokens;
    let started_at = Instant::now();
    let tool_run = run_turn_with_tools(
        &runtime,
        &prepared.thread_id,
        &prepared.workspace_root,
        &prepared.input_text,
        prepared.conversation_history,
        prepared.goal_spec,
        Some(&prepared.guidance_path),
        Some(&prepared.progress_path),
    )?;

    Ok(LiveTurnResult {
        tool_run,
        live_readiness,
        context_max_tokens,
        elapsed_ms: started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
    })
}

fn emit_live_progress(
    writer: &mut dyn Write,
    prepared: &PreparedLiveTurn,
    cursor: &mut usize,
    include_unterminated_tail: bool,
) -> Result<(), String> {
    let content = match fs::read(&prepared.progress_path) {
        Ok(content) => content,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(format!(
                "app_server_progress_read_failed: path={} error={error}",
                prepared.progress_path.display()
            ))
        }
    };
    let start = (*cursor).min(content.len());
    let unread = &content[start..];
    let mut consumed = 0usize;
    for line in unread.split_inclusive(|byte| *byte == b'\n') {
        let terminated = line.last() == Some(&b'\n');
        if !terminated && !include_unterminated_tail {
            break;
        }
        consumed += line.len();
        let line = line
            .strip_suffix(b"\n")
            .unwrap_or(line)
            .strip_suffix(b"\r")
            .unwrap_or(line);
        if line.is_empty() {
            continue;
        }
        let Ok(event) = serde_json::from_slice::<Value>(line) else {
            continue;
        };
        write_json_line(
            writer,
            &json!({
                "method": "turn/progress",
                "params": {
                    "threadId": prepared.thread_id,
                    "turnId": prepared.turn_id,
                    "event": event,
                }
            }),
        )?;
    }
    *cursor = start + consumed;
    Ok(())
}

fn record_live_turn_success(
    state: &SharedAppServerState,
    prepared: &PreparedLiveTurn,
    live_turn: &LiveTurnResult,
) -> Result<ThreadState, String> {
    with_app_server_state(state, |state| {
        let result = &live_turn.tool_run.result;
        let now = now_millis();
        let status = app_server_turn_status(&result.response.meta.extra).to_string();
        let thread = {
            let thread = state
                .threads
                .get_mut(&prepared.thread_id)
                .ok_or_else(|| format!("unknown_thread: {}", prepared.thread_id))?;
            thread.updated_at = now;
            let turn = thread
                .turns
                .iter_mut()
                .find(|turn| turn.id == prepared.turn_id)
                .ok_or_else(|| format!("unknown_turn: {}", prepared.turn_id))?;
            turn.user_text = prepared.input_text.clone();
            turn.assistant_text = result.response.body.clone();
            turn.model_name = result.response.model_name.clone();
            turn.status = status;
            turn.provider_meta = result.response.meta.extra.clone();
            turn.tool_trace = live_turn.tool_run.tool_trace.clone();
            turn.tool_surface = live_turn.tool_run.tool_surface.clone();
            turn.updated_at = now;
            thread.clone()
        };
        remove_active_turn(state, &prepared.thread_id, &prepared.turn_id);
        persist_app_server_state(state)?;
        Ok(thread)
    })
}

fn record_live_turn_failure(
    state: &SharedAppServerState,
    prepared: &PreparedLiveTurn,
    status: &str,
) -> Result<(), String> {
    with_app_server_state(state, |state| {
        let now = now_millis();
        let thread = state
            .threads
            .get_mut(&prepared.thread_id)
            .ok_or_else(|| format!("unknown_thread: {}", prepared.thread_id))?;
        thread.updated_at = now;
        let turn = thread
            .turns
            .iter_mut()
            .find(|turn| turn.id == prepared.turn_id)
            .ok_or_else(|| format!("unknown_turn: {}", prepared.turn_id))?;
        turn.status = status.to_string();
        turn.updated_at = now;
        remove_active_turn(state, &prepared.thread_id, &prepared.turn_id);
        persist_app_server_state(state)?;
        Ok(())
    })
}

fn unregister_active_turn(state: &SharedAppServerState, thread_id: &str, turn_id: &str) {
    let _ = with_app_server_state(state, |state| {
        remove_active_turn(state, thread_id, turn_id);
        Ok(())
    });
}

fn remove_active_turn(state: &mut AppServerState, thread_id: &str, turn_id: &str) {
    if state
        .active_turns
        .get(thread_id)
        .map(|active| active.turn_id == turn_id)
        .unwrap_or(false)
    {
        state.active_turns.remove(thread_id);
    }
}

fn emit_live_turn_success(
    writer: &mut dyn Write,
    prepared: &PreparedLiveTurn,
    thread: &ThreadState,
    live_turn: LiveTurnResult,
) -> Result<Value, String> {
    let tool_run = live_turn.tool_run;
    let result = tool_run.result;
    let status = app_server_turn_status(&result.response.meta.extra).to_string();
    let assistant_text = result.response.body.clone();
    let model_name = result.response.model_name.clone();
    let tool_call_count = tool_run.tool_calls.len();
    let tool_protocol_error_count = tool_run.tool_protocol_errors.len();
    let runtime_observability = runtime_observability_meta(&result);

    write_json_line(
        writer,
        &json!({
            "method": "item/agentMessage/delta",
            "params": {
                "threadId": prepared.thread_id,
                "turnId": prepared.turn_id,
                "delta": assistant_text,
            }
        }),
    )?;
    write_json_line(
        writer,
        &json!({
            "method": "item/completed",
            "params": {
                "threadId": prepared.thread_id,
                "turnId": prepared.turn_id,
                "item": {
                    "type": "agentMessage",
                    "text": assistant_text,
                }
            }
        }),
    )?;
    write_json_line(
        writer,
        &json!({
            "method": "turn/completed",
            "params": {
                "threadId": prepared.thread_id,
                "turn": {
                    "id": prepared.turn_id,
                    "status": status,
                    "runtimeReportId": tool_run.runtime_report_id.clone(),
                    "toolCallCount": tool_call_count,
                    "toolProtocolErrorCount": tool_protocol_error_count,
                    "toolTrace": tool_run.tool_trace.clone(),
                    "toolReport": tool_run.tool_report.clone(),
                    "toolSurface": tool_run.tool_surface.clone(),
                    "toolCalls": tool_run.tool_calls.iter().map(tool_execution_record_to_json).collect::<Vec<_>>(),
                    "toolProtocolErrors": tool_run.tool_protocol_errors.iter().map(tool_protocol_error_to_json).collect::<Vec<_>>(),
                    "toolEvents": tool_run.tool_events,
                    "providerMeta": result.response.meta.extra.clone(),
                    "runtimeObservability": runtime_observability.clone(),
                    "liveReadiness": live_turn.live_readiness.clone(),
                }
            }
        }),
    )?;

    Ok(json!({
        "thread": thread_to_resume_json(thread),
        "turn": {
            "id": prepared.turn_id,
            "status": status,
            "runtimeReportId": tool_run.runtime_report_id,
            "modelName": model_name,
            "finishReason": result.response.meta.finish_reason.clone().unwrap_or_else(|| "completed".to_string()),
            "elapsedMs": live_turn.elapsed_ms,
            "recallHitCount": result.recall_hit_count,
            "packedTokenCount": result.packed_token_count,
            "contextEngineKind": result.context_engine_kind,
            "contextMaxTokens": live_turn.context_max_tokens,
            "providerMeta": result.response.meta.extra,
            "runtimeObservability": runtime_observability,
            "liveReadiness": live_turn.live_readiness,
            "trace": result.response.trace,
            "apiCallCount": 1,
            "toolCallCount": tool_call_count,
            "toolProtocolErrorCount": tool_protocol_error_count,
            "toolTrace": tool_run.tool_trace,
            "toolReport": tool_run.tool_report,
            "toolSurface": tool_run.tool_surface,
            "toolCalls": tool_run.tool_calls.iter().map(tool_execution_record_to_json).collect::<Vec<_>>(),
            "toolProtocolErrors": tool_run.tool_protocol_errors.iter().map(tool_protocol_error_to_json).collect::<Vec<_>>(),
            "toolEvents": tool_run.tool_events,
        }
    }))
}

fn handle_turn_guidance(state: &SharedAppServerState, params: &Value) -> Result<Value, String> {
    let note = normalize_text(params.get("text").and_then(|value| value.as_str()));
    if note.is_empty() {
        return Err("turn/guidance requires non-empty text".to_string());
    }
    with_active_turn_for_control(state, params, |active| append_live_turn_note(active, &note))?;
    Ok(json!({
        "accepted": true,
        "status": "guidance_queued",
    }))
}

fn handle_turn_interrupt(state: &SharedAppServerState, params: &Value) -> Result<Value, String> {
    with_active_turn_for_control(state, params, |active| {
        append_live_turn_note(active, "[chuang-control] stop")
    })?;
    Ok(json!({
        "accepted": true,
        "status": "interrupt_requested",
        "effectiveAt": "next_safe_point",
    }))
}

fn with_active_turn_for_control<T>(
    state: &SharedAppServerState,
    params: &Value,
    action: impl FnOnce(&ActiveTurn) -> Result<T, String>,
) -> Result<T, String> {
    let thread_id = normalize_text(params.get("threadId").and_then(|value| value.as_str()));
    let turn_id = normalize_text(params.get("turnId").and_then(|value| value.as_str()));
    with_app_server_state(state, |state| {
        let active = state
            .active_turns
            .get(&thread_id)
            .filter(|active| active.thread_id == thread_id && active.turn_id == turn_id)
            .ok_or_else(|| turn_not_active_error(&thread_id, &turn_id))?;
        action(active)
    })
}

fn turn_not_active_error(thread_id: &str, turn_id: &str) -> String {
    format!("turn_not_active: threadId={thread_id} turnId={turn_id}")
}

fn append_live_turn_note(active: &ActiveTurn, note: &str) -> Result<(), String> {
    let note = normalize_text(Some(note));
    if note.is_empty() {
        return Err("live_turn_guidance_requires_non_empty_text".to_string());
    }
    let mut writer = active
        .guidance_writer
        .lock()
        .map_err(|_| "live_turn_guidance_lock_poisoned".to_string())?;
    writer
        .write_all(note.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|error| {
            format!(
                "guidance_write_failed: path={} error={error}",
                active.guidance_path.display()
            )
        })
}

fn handle_turn_start(
    state: &mut AppServerState,
    params: &Value,
    writer: &mut dyn Write,
) -> Result<Value, String> {
    let thread_id = normalize_text(params.get("threadId").and_then(|value| value.as_str()));
    let requested_workspace_root = params
        .get("workspaceRoot")
        .and_then(|value| value.as_str())
        .map(normalize_workspace_root);
    let input_text = extract_turn_input_text(params);
    if input_text.is_empty() {
        return Err("turn/start requires non-empty input".to_string());
    }
    let goal_spec = extract_turn_goal(params)?;

    let thread_id = if thread_id.is_empty() {
        let workspace_root =
            requested_workspace_root.unwrap_or_else(|| normalize_workspace_root(""));
        let thread = create_thread(
            state,
            workspace_root.clone(),
            thread_display_name(&workspace_root),
        );
        thread.id
    } else {
        if let Some(thread) = state.threads.get(&thread_id) {
            if let Some(requested_workspace_root) = requested_workspace_root.as_deref() {
                ensure_thread_workspace_matches(thread, requested_workspace_root)?;
            }
            thread_id
        } else {
            return Err(format!("unknown_thread: {thread_id}"));
        }
    };
    let workspace_root = state
        .threads
        .get(&thread_id)
        .map(|thread| thread.workspace_root.clone())
        .ok_or_else(|| format!("unknown_thread: {thread_id}"))?;

    let mut runtime =
        build_runtime_for_workspace(&app_server_config_workspace_root(&workspace_root))?;
    runtime
        .metadata
        .insert("channel".to_string(), "app-server".to_string());
    let runtime = override_runtime_model(runtime, params);
    let live_readiness = build_chuang_mvp_status(&runtime, &kernel_config_from_runtime(&runtime)?)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?
        .live_readiness;
    let context_max_tokens = runtime.context_budget.max_tokens;
    let started_at = Instant::now();
    let conversation_history = recent_thread_history(state, &thread_id, 6);
    let tool_run = run_turn_with_tools(
        &runtime,
        &thread_id,
        &workspace_root,
        &input_text,
        conversation_history,
        goal_spec,
        None,
        None,
    )?;
    let result = tool_run.result.clone();
    let tool_trace = tool_run.tool_trace.clone();
    let tool_calls = tool_run.tool_calls.clone();
    let tool_report = tool_run.tool_report.clone();
    let tool_surface = tool_run.tool_surface.clone();
    let tool_protocol_errors = tool_run.tool_protocol_errors.clone();
    let tool_events = tool_run.tool_events.clone();
    let tool_call_count = tool_calls.len();
    let tool_protocol_error_count = tool_protocol_errors.len();
    let runtime_observability = runtime_observability_meta(&result);
    let elapsed_ms = started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64;
    let turn_id = next_turn_id(state);
    let assistant_text = result.response.body.clone();
    let model_name = result.response.model_name.clone();
    let status = app_server_turn_status(&result.response.meta.extra).to_string();
    let now = now_millis();
    let thread = state
        .threads
        .get_mut(&thread_id)
        .ok_or_else(|| format!("unknown_thread: {thread_id}"))?;
    thread.updated_at = now;
    thread.turns.push(TurnState {
        id: turn_id.clone(),
        user_text: input_text.clone(),
        assistant_text: assistant_text.clone(),
        model_name: model_name.clone(),
        status: status.clone(),
        provider_meta: result.response.meta.extra.clone(),
        tool_trace: tool_trace.clone(),
        tool_surface: tool_surface.clone(),
        updated_at: now,
    });

    write_json_line(
        writer,
        &json!({
            "method": "turn/started",
            "params": {
                "threadId": thread_id,
                "turn": { "id": turn_id },
            }
        }),
    )?;
    write_json_line(
        writer,
        &json!({
            "method": "item/agentMessage/delta",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "delta": assistant_text,
            }
        }),
    )?;
    write_json_line(
        writer,
        &json!({
            "method": "item/completed",
            "params": {
                "threadId": thread_id,
                "turnId": turn_id,
                "item": {
                    "type": "agentMessage",
                    "text": assistant_text,
                }
            }
        }),
    )?;
    write_json_line(
        writer,
        &json!({
            "method": "turn/completed",
            "params": {
                "threadId": thread_id,
                "turn": {
                    "id": turn_id,
                    "status": status,
                    "runtimeReportId": tool_run.runtime_report_id.clone(),
                    "toolCallCount": tool_call_count,
                    "toolProtocolErrorCount": tool_protocol_error_count,
                    "toolTrace": tool_trace.clone(),
                    "toolReport": tool_report.clone(),
                    "toolSurface": tool_surface.clone(),
                    "toolCalls": tool_calls
                        .iter()
                        .map(tool_execution_record_to_json)
                        .collect::<Vec<_>>(),
                    "toolProtocolErrors": tool_protocol_errors
                        .iter()
                        .map(tool_protocol_error_to_json)
                        .collect::<Vec<_>>(),
                    "toolEvents": tool_events,
                    "providerMeta": result.response.meta.extra.clone(),
                    "runtimeObservability": runtime_observability.clone(),
                    "liveReadiness": live_readiness.clone(),
                }
            }
        }),
    )?;

    Ok(json!({
        "thread": thread_to_resume_json(
            state.threads.get(&thread_id).ok_or_else(|| format!("unknown_thread: {thread_id}"))?
        ),
        "turn": {
            "id": thread_turn_id(state, &thread_id).unwrap_or_default(),
            "status": status,
            "runtimeReportId": tool_run.runtime_report_id,
            "modelName": model_name,
            "finishReason": result
                .response
                .meta
                .finish_reason
                .clone()
                .unwrap_or_else(|| "completed".to_string()),
            "elapsedMs": elapsed_ms,
            "recallHitCount": result.recall_hit_count,
            "packedTokenCount": result.packed_token_count,
            "contextEngineKind": result.context_engine_kind,
            "contextMaxTokens": context_max_tokens,
            "providerMeta": result.response.meta.extra,
            "runtimeObservability": runtime_observability,
            "liveReadiness": live_readiness,
            "trace": result.response.trace,
            "apiCallCount": 1,
            "toolCallCount": tool_call_count,
            "toolProtocolErrorCount": tool_protocol_error_count,
            "toolTrace": tool_trace,
            "toolReport": tool_report,
            "toolSurface": tool_surface,
            "toolCalls": tool_calls
                .iter()
                .map(tool_execution_record_to_json)
                .collect::<Vec<_>>(),
            "toolProtocolErrors": tool_protocol_errors
                .iter()
                .map(tool_protocol_error_to_json)
                .collect::<Vec<_>>(),
            "toolEvents": tool_events,
        }
    }))
}

#[derive(Debug)]
struct ToolLoopResult {
    result: chuang_agent::agent_runtime::RuntimeResult,
    tool_calls: Vec<ToolExecutionRecord>,
    tool_protocol_errors: Vec<ToolProtocolError>,
    tool_events: Vec<Value>,
    tool_trace: String,
    tool_report: Option<Value>,
    tool_surface: Option<Value>,
    runtime_report_id: Option<String>,
}

fn run_turn_with_tools(
    runtime: &RuntimeConfig,
    thread_id: &str,
    workspace_root: &str,
    original_input: &str,
    conversation_history: Vec<ConversationHistoryItem>,
    goal_spec: Option<GoalSpec>,
    live_guidance_path: Option<&Path>,
    progress_path: Option<&Path>,
) -> Result<ToolLoopResult, String> {
    let request = RunCliRequest {
        options: CliOptions {
            runtime: runtime.clone(),
        },
        user_input: original_input.to_string(),
        workspace_root: Some(PathBuf::from(workspace_root)),
        remember: false,
        session_id: Some(thread_id.to_string()),
        remember_session: true,
        conversation_history,
        remember_identity: false,
        remember_experience: false,
        dispatch_subagent: false,
        goal_spec,
        knowledge_context: None,
        live_guidance_path: live_guidance_path.map(Path::to_path_buf),
        progress_path: progress_path.map(Path::to_path_buf),
    };

    let (result, records) = run_with_options(&request)?;
    let tool_meta =
        ToolLoopMeta::<ToolExecutionRecord, ToolProtocolError, Value>::typed_from_extra(
            &result.response.meta.extra,
        )?;
    let tool_surface = parse_json_value(&result.response.meta.extra, "tool_surface_json")?;

    Ok(ToolLoopResult {
        result,
        tool_calls: tool_meta.tool_calls,
        tool_protocol_errors: tool_meta.tool_protocol_errors,
        tool_events: tool_meta.tool_events,
        tool_trace: tool_meta.tool_trace,
        tool_report: tool_meta.tool_report,
        tool_surface,
        runtime_report_id: records.runtime_report_id,
    })
}

fn app_server_turn_status(provider_meta: &BTreeMap<String, String>) -> &'static str {
    if provider_meta
        .get("human_input_required")
        .map(|value| value == "true")
        .unwrap_or(false)
        || provider_meta
            .get("tool_loop_status")
            .map(|value| value == "human_input_required")
            .unwrap_or(false)
        || (provider_meta.contains_key("pending_approval_id")
            && provider_meta.contains_key("pending_approval_path"))
    {
        "human_input_required"
    } else if provider_meta.contains_key("provider_failure_reason_code")
        || provider_meta.contains_key("provider_error_class")
        || provider_meta
            .get("provider_response_ok")
            .map(|value| value == "false")
            .unwrap_or(false)
    {
        "provider_error"
    } else {
        "completed"
    }
}

fn tool_execution_record_to_json(record: &ToolExecutionRecord) -> Value {
    json!({
        "tool": record.tool_name,
        "atomicTool": record.atomic_tool_name,
        "ok": record.ok,
        "summary": record.summary,
        "decision": record.decision,
        "durationMs": record.duration_ms,
        "retryable": record.retryable,
        "targetPath": record.target_path,
        "resolvedPath": record.resolved_path,
        "cwd": record.cwd,
        "command": record.command,
        "entries": record.entries,
        "outputBytes": record.output_bytes,
        "outputLines": record.output_lines,
        "stderrBytes": record.stderr_bytes,
        "stderrLines": record.stderr_lines,
        "output": record.output,
        "stdout": record.stdout,
        "stderr": record.stderr,
        "exitCode": record.exit_code,
        "changedFiles": record.changed_files,
        "writeBeforeBytes": record.write_before_bytes,
        "writeAfterBytes": record.write_after_bytes,
        "writeChanged": record.write_changed,
        "writeOperation": record.write_operation,
        "writeDiffPreview": record.write_diff_preview,
        "writeDiffTruncated": record.write_diff_truncated,
        "failureClass": record.failure_class,
        "outputRedacted": record.output_redacted,
        "stdoutRedacted": record.stdout_redacted,
        "stderrRedacted": record.stderr_redacted,
        "outputTruncated": record.output_truncated,
        "stdoutTruncated": record.stdout_truncated,
        "stderrTruncated": record.stderr_truncated,
        "call": &record.call,
    })
}

fn tool_protocol_error_to_json(error: &ToolProtocolError) -> Value {
    json!({
        "code": error.code,
        "message": error.message,
        "raw": error.raw,
    })
}

fn create_thread(
    state: &mut AppServerState,
    workspace_root: String,
    display_name: String,
) -> ThreadState {
    state.next_thread_seq += 1;
    let thread_id = format!("chuang-thread-{}", state.next_thread_seq);
    let now = now_millis();
    let thread = ThreadState {
        id: thread_id.clone(),
        workspace_root,
        display_name,
        created_at: now,
        updated_at: now,
        turns: Vec::new(),
    };
    state.threads.insert(thread_id, thread.clone());
    thread
}

fn ensure_thread_workspace_matches(
    thread: &ThreadState,
    requested_workspace_root: &str,
) -> Result<(), String> {
    if thread.workspace_root == requested_workspace_root {
        return Ok(());
    }
    Err(format!(
        "workspace_root_mismatch: thread_id={} thread_workspace={} requested_workspace={}",
        thread.id, thread.workspace_root, requested_workspace_root
    ))
}

fn next_turn_id(state: &mut AppServerState) -> String {
    state.next_turn_seq += 1;
    format!("chuang-turn-{}", state.next_turn_seq)
}

fn thread_turn_id(state: &AppServerState, thread_id: &str) -> Option<String> {
    state
        .threads
        .get(thread_id)?
        .turns
        .last()
        .map(|turn| turn.id.clone())
}

fn recent_thread_history(
    state: &AppServerState,
    thread_id: &str,
    max_turns: usize,
) -> Vec<ConversationHistoryItem> {
    let Some(thread) = state.threads.get(thread_id) else {
        return Vec::new();
    };
    thread
        .turns
        .iter()
        .rev()
        .filter(|turn| turn_status_is_admissible_for_history(&turn.status))
        .take(max_turns)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .flat_map(|turn| {
            [
                ConversationHistoryItem {
                    role: "user".to_string(),
                    text: turn.user_text.clone(),
                },
                ConversationHistoryItem {
                    role: "assistant".to_string(),
                    text: turn.assistant_text.clone(),
                },
            ]
        })
        .filter(|item| !item.text.trim().is_empty())
        .collect()
}

fn turn_status_is_admissible_for_history(status: &str) -> bool {
    matches!(status, "completed" | "human_input_required")
}

fn thread_to_json(thread: &ThreadState) -> Value {
    json!({
        "id": thread.id,
        "cwd": thread.workspace_root,
        "name": thread.display_name,
        "preview": thread.turns.last().map(|turn| turn.assistant_text.clone()).unwrap_or_default(),
        "createdAt": thread.created_at,
        "updatedAt": thread.updated_at,
        "sourceKind": "appServer",
        "turns": thread.turns.iter().map(turn_to_json).collect::<Vec<_>>(),
    })
}

fn thread_to_resume_json(thread: &ThreadState) -> Value {
    thread_to_json(thread)
}

fn thread_to_list_json(thread: &ThreadState) -> Value {
    json!({
        "id": thread.id,
        "cwd": thread.workspace_root,
        "name": thread.display_name,
        "preview": thread.turns.last().map(|turn| turn.assistant_text.clone()).unwrap_or_default(),
        "updatedAt": thread.updated_at,
        "sourceKind": "appServer",
    })
}

fn turn_to_json(turn: &TurnState) -> Value {
    json!({
        "id": turn.id,
        "updatedAt": turn.updated_at,
        "status": turn.status,
        "providerMeta": turn.provider_meta,
        "toolTrace": turn.tool_trace,
        "toolSurface": turn.tool_surface,
        "items": [
            {
                "type": "userMessage",
                "content": [
                    {
                        "type": "text",
                        "text": turn.user_text,
                    }
                ],
            },
            {
                "type": "agentMessage",
                "text": turn.assistant_text,
                "model": turn.model_name,
            }
        ]
    })
}

/// 提取 turn 输入：返回 (文本, 图片路径列表)。
/// 图片以 `localImage` item 传入（桥层 attachment 渲染），路径为本地文件。
fn extract_turn_input(params: &Value) -> (String, Vec<String>) {
    let mut image_paths = Vec::new();
    let mut text = String::new();

    if let Some(t) = params.get("text").and_then(|value| value.as_str()) {
        text = normalize_text(Some(t));
    }

    if let Some(input) = params.get("input").and_then(|value| value.as_array()) {
        let mut parts = Vec::new();
        for item in input {
            if let Some(t) = item.get("text").and_then(|value| value.as_str()) {
                let normalized = normalize_text(Some(t));
                if !normalized.is_empty() {
                    parts.push(normalized);
                }
            } else if let Some(path) = item.get("path").and_then(|value| value.as_str()) {
                if item.get("type").and_then(|value| value.as_str()) == Some("localImage") {
                    let normalized = normalize_text(Some(path));
                    if !normalized.is_empty() {
                        image_paths.push(normalized);
                    }
                }
            }
        }
        if !parts.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(&parts.join("\n"));
        }
    }

    (text, image_paths)
}

fn extract_turn_input_text(params: &Value) -> String {
    extract_turn_input(params).0
}

/// 用视觉模型把本地图片描述成文字（识图兜底）。
/// 走与主 provider 相同的 opencodex 路由（base_url/api_key），模型用 runtime.vision_model。
fn describe_images_with_vision(
    runtime: &RuntimeConfig,
    image_paths: &[String],
) -> Result<String, String> {
    let vision_model = runtime
        .vision_model
        .as_deref()
        .ok_or_else(|| "vision_model not configured in config.toml".to_string())?;

    let (base_url, api_key, request_timeout_ms) =
        match first_openai_compatible_provider(&runtime.provider) {
            Some(cfg) => (
                cfg.base_url.clone(),
                cfg.api_key.clone(),
                cfg.request_timeout_ms,
            ),
            None => {
                return Err("vision describe: no openai_compatible provider in chain".to_string())
            }
        };

    let mut content_parts: Vec<Value> = Vec::new();
    // 只给视觉模型固定的描述指令，不传用户原始提问，避免视觉模型
    // 直接回答用户问题而不描述图片内容。
    content_parts.push(json!({
        "type": "text",
        "text": "请详细描述这张图片的内容，包括画面中的主体、文字、布局与关键细节。"
    }));
    for path in image_paths {
        let data =
            fs::read(path).map_err(|e| format!("vision describe: read {} failed: {e}", path))?;
        let mime = guess_image_mime(path);
        let encoded = base64::engine::general_purpose::STANDARD.encode(&data);
        content_parts.push(json!({
            "type": "image_url",
            "image_url": { "url": format!("data:{mime};base64,{encoded}") }
        }));
    }
    let request_timeout_ms = request_timeout_ms.unwrap_or(60_000);

    let body = json!({
        "model": vision_model,
        "messages": [{
            "role": "user",
            "content": content_parts
        }],
        "max_tokens": 2000,
    });
    let url = format!("{}/chat/completions", base_url.trim_end_matches('/'));

    let tokio_runtime = TokioRuntimeBuilder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("vision describe: tokio runtime: {e}"))?;

    tokio_runtime.block_on(async move {
        let connector = HttpsConnectorBuilder::new()
            .with_native_roots()
            .map_err(|e| format!("vision describe: tls roots: {e}"))?
            .https_or_http()
            .enable_http1()
            .build();
        let client: Client<_, Full<Bytes>> = Client::builder(TokioExecutor::new()).build(connector);
        let req = Request::builder()
            .method(Method::POST)
            .uri(&url)
            .header("Authorization", format!("Bearer {api_key}"))
            .header("Content-Type", "application/json")
            .body(Full::new(Bytes::from(body.to_string())))
            .map_err(|e| format!("vision describe: request build: {e}"))?;
        let response = timeout(
            Duration::from_millis(request_timeout_ms),
            client.request(req),
        )
        .await
        .map_err(|_| format!("vision describe: timeout after {request_timeout_ms}ms"))?
        .map_err(|e| format!("vision describe: send: {e}"))?;
        let status_code = response.status().as_u16();
        let response_body_json = timeout(
            Duration::from_millis(request_timeout_ms),
            response.into_body().collect(),
        )
        .await
        .map_err(|_| "vision describe: body timeout".to_string())?
        .map_err(|e| format!("vision describe: body: {e}"))?
        .to_bytes();
        let response_body = String::from_utf8_lossy(&response_body_json).to_string();
        if status_code != 200 {
            return Err(format!(
                "vision describe: upstream status={status_code} body={}",
                truncate_for_error(&response_body, 300)
            ));
        }
        let parsed: Value = serde_json::from_str(&response_body)
            .map_err(|e| format!("vision describe: parse response: {e}"))?;
        let content = &parsed["choices"][0]["message"]["content"];
        let text = content
            .as_str()
            .map(|s| s.to_string())
            .or_else(|| {
                content.as_array().map(|parts| {
                    parts
                        .iter()
                        .filter_map(|part| part["text"].as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                })
            })
            .ok_or_else(|| {
                format!(
                    "vision describe: no content in response: {}",
                    truncate_for_error(&response_body, 300)
                )
            })?;
        Ok(text)
    })
}

/// 沿 provider 链（嵌套 Fallback）找第一个 OpenAICompatible 配置。
fn first_openai_compatible_provider(provider: &ProviderConfig) -> Option<&OpenAICompatibleConfig> {
    match provider {
        ProviderConfig::OpenAICompatible(cfg) => Some(cfg),
        ProviderConfig::Fallback { primary, .. } => first_openai_compatible_provider(primary),
        ProviderConfig::Fake { .. } => None,
    }
}

fn guess_image_mime(path: &str) -> &'static str {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else {
        "image/png"
    }
}

fn truncate_for_error(input: &str, max: usize) -> String {
    if input.chars().count() <= max {
        input.to_string()
    } else {
        input.chars().take(max).collect::<String>() + "…"
    }
}

fn extract_turn_goal(params: &Value) -> Result<Option<GoalSpec>, String> {
    let Some(goal) = params.get("goal").and_then(|value| value.as_str()) else {
        return Ok(None);
    };
    let goal = normalize_text(Some(goal));
    if goal.is_empty() {
        return Err("turn/start goal must not be empty".to_string());
    }
    Ok(Some(GoalSpec::mainline_mvp(goal)))
}

pub(crate) fn build_runtime_for_workspace(workspace_root: &str) -> Result<RuntimeConfig, String> {
    build_runtime_for_workspace_with_options(workspace_root, RuntimeConfigFileOptions::strict())
}

fn build_runtime_for_workspace_with_options(
    workspace_root: &str,
    options: RuntimeConfigFileOptions,
) -> Result<RuntimeConfig, String> {
    let base_dir = workspace_base_dir(workspace_root);
    let config_path = base_dir.join("config.toml");
    let mut runtime = if config_path.exists() {
        if options == RuntimeConfigFileOptions::strict() {
            load_runtime_config_file(&config_path)
                .map_err(|error| runtime_config_file_error(&error))?
        } else {
            load_runtime_config_file_with_options(&config_path, options)
                .map_err(|error| runtime_config_file_error(&error))?
        }
    } else {
        RuntimeConfig::new(base_dir.join("data/chuang-agent.db"))
    };

    normalize_runtime_paths(&mut runtime, &base_dir);
    if !config_path.exists()
        || runtime.permission.workspace_root == PathBuf::from(DEFAULT_WORKSPACE_ROOT)
    {
        runtime.permission.workspace_root = base_dir.clone();
    }
    Ok(runtime)
}

fn override_runtime_model(mut runtime: RuntimeConfig, params: &Value) -> RuntimeConfig {
    let requested_model = params
        .get("model")
        .and_then(|value| value.as_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let Some(requested_model) = requested_model else {
        return runtime;
    };

    runtime.provider = match runtime.provider {
        ProviderConfig::Fake { provider_id, .. } => ProviderConfig::Fake {
            provider_id,
            model_name: requested_model,
        },
        ProviderConfig::OpenAICompatible(config) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                model_name: requested_model,
                ..config
            })
        }
        ProviderConfig::Fallback {
            primary,
            fallback,
            policy,
        } => ProviderConfig::Fallback {
            primary: Box::new(override_provider_model(*primary, requested_model)),
            fallback,
            policy,
        },
    };

    runtime
}

fn normalize_runtime_paths(runtime: &mut RuntimeConfig, base_dir: &Path) {
    runtime.db_path = resolve_path_if_relative(base_dir, runtime.db_path.clone());
    runtime.permission.workspace_root =
        resolve_path_if_relative(base_dir, runtime.permission.workspace_root.clone());
    runtime.identity_memory = match runtime.identity_memory.clone() {
        IdentityMemoryConfig::HermesDualFile {
            root,
            user_max_chars,
            memory_max_chars,
        } => IdentityMemoryConfig::HermesDualFile {
            root: resolve_path_if_relative(base_dir, root),
            user_max_chars,
            memory_max_chars,
        },
    };
    runtime.subagent_queue = SubagentQueueConfig {
        root: resolve_path_if_relative(base_dir, runtime.subagent_queue.root.clone()),
    };
    runtime.identity_bootstrap = IdentityBootstrapConfig {
        root: resolve_path_if_relative(base_dir, runtime.identity_bootstrap.root.clone()),
        soul_path: resolve_path_if_relative(base_dir, runtime.identity_bootstrap.soul_path.clone()),
        story_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.story_path.clone(),
        ),
        first_wake_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.first_wake_path.clone(),
        ),
        agents_registry_path: resolve_path_if_relative(
            base_dir,
            runtime.identity_bootstrap.agents_registry_path.clone(),
        ),
    };
    runtime.rules = RulesConfig {
        root: resolve_path_if_relative(base_dir, runtime.rules.root.clone()),
        core_path: resolve_path_if_relative(base_dir, runtime.rules.core_path.clone()),
    };
    normalize_provider_paths(&mut runtime.provider, base_dir);
}

fn override_provider_model(provider: ProviderConfig, model_name: String) -> ProviderConfig {
    match provider {
        ProviderConfig::Fake { provider_id, .. } => ProviderConfig::Fake {
            provider_id,
            model_name,
        },
        ProviderConfig::OpenAICompatible(config) => {
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
                model_name,
                ..config
            })
        }
        ProviderConfig::Fallback {
            primary,
            fallback,
            policy,
        } => ProviderConfig::Fallback {
            primary: Box::new(override_provider_model(*primary, model_name)),
            fallback,
            policy,
        },
    }
}

fn normalize_provider_paths(provider: &mut ProviderConfig, base_dir: &Path) {
    match provider {
        ProviderConfig::Fake { .. } => {}
        ProviderConfig::OpenAICompatible(config) => {
            if let Some(path) = &config.tls_ca_cert_path {
                config.tls_ca_cert_path = Some(resolve_path_if_relative(base_dir, path.clone()));
            }
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => {
            normalize_provider_paths(primary, base_dir);
            normalize_provider_paths(fallback, base_dir);
        }
    }
}

fn resolve_path_if_relative(base_dir: &Path, path: PathBuf) -> PathBuf {
    let resolved = if path.is_absolute() {
        path
    } else {
        base_dir.join(path)
    };
    if let Ok(canonical) = resolved.canonicalize() {
        canonical
    } else {
        normalize_path_lexically(&resolved)
    }
}

fn workspace_status_for_root(workspace_root: &str) -> Value {
    let workspace_root =
        resolve_path_if_relative(Path::new("."), workspace_base_dir(workspace_root));
    let configured_workspace_root = std::env::var("CHUANG_AGENT_WORKSPACE_ROOT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .map(|path| resolve_path_if_relative(Path::new("."), path));
    let config_source = if configured_workspace_root.is_some() {
        "CHUANG_AGENT_WORKSPACE_ROOT"
    } else {
        "workspace_root_arg"
    };
    let config_root = configured_workspace_root
        .clone()
        .unwrap_or_else(|| workspace_root.clone());
    let app_server_child_root = workspace_root.clone();
    let workspace_root = workspace_root.display().to_string();
    let app_server_child_root = app_server_child_root.display().to_string();
    let config_root = config_root.display().to_string();
    json!({
        "workspace_root": workspace_root,
        "app_server_child_root": app_server_child_root,
        "config_root": config_root,
        "config_source": config_source,
        "config_path": PathBuf::from(&workspace_root).join("config.toml").display().to_string(),
        "matches_config": workspace_root == config_root,
    })
}

fn workspace_base_dir(workspace_root: &str) -> PathBuf {
    let trimmed = workspace_root.trim();
    if trimmed.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(trimmed)
    }
}

fn app_server_config_workspace_root(requested_workspace_root: &str) -> String {
    std::env::var("CHUANG_AGENT_WORKSPACE_ROOT")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| requested_workspace_root.to_string())
}

fn normalize_workspace_root(raw: &str) -> String {
    let trimmed = raw.trim();
    let path = if trimmed.is_empty() {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    } else {
        PathBuf::from(trimmed)
    };
    resolve_path_if_relative(Path::new("."), path)
        .display()
        .to_string()
}

fn thread_display_name(workspace_root: &str) -> String {
    let path = PathBuf::from(workspace_root);
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "workspace thread".to_string())
}

fn runtime_config_file_error(error: &RuntimeConfigFileError) -> String {
    match error {
        RuntimeConfigFileError::ReadFailed { path } => {
            format!("runtime_config_read_failed: {}", path.display())
        }
        RuntimeConfigFileError::InvalidLine { line, content } => {
            format!("runtime_config_invalid_line:{line}:{content}")
        }
        RuntimeConfigFileError::InvalidValue { key, value } => {
            format!("runtime_config_invalid_value:{key}:{value}")
        }
        RuntimeConfigFileError::MissingEnv { name } => {
            format!("runtime_config_missing_env:{name}")
        }
    }
}

fn write_json_line(writer: &mut dyn Write, value: &Value) -> Result<(), String> {
    let rendered = serde_json::to_string(value).map_err(|e| format!("json_render_failed: {e}"))?;
    writer
        .write_all(rendered.as_bytes())
        .and_then(|_| writer.write_all(b"\n"))
        .and_then(|_| writer.flush())
        .map_err(|e| format!("app_server_write_failed: {e}"))
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

fn normalize_text(value: Option<&str>) -> String {
    value.unwrap_or("").trim().to_string()
}

fn provider_summary_model_name(runtime: &RuntimeConfig) -> String {
    match &runtime.provider {
        ProviderConfig::Fake { model_name, .. } => model_name.clone(),
        ProviderConfig::OpenAICompatible(OpenAICompatibleConfig { model_name, .. }) => {
            model_name.clone()
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => format!(
            "{}->{}",
            provider_config_model_name(primary),
            provider_config_model_name(fallback)
        ),
    }
}

/// 对外暴露的主模型名：Fallback 链路只取主模型，避免把
/// "primary->fallback" 内部链路名暴露给 UI/桥（桥按精确模型名匹配）。
fn provider_primary_model_name(runtime: &RuntimeConfig) -> String {
    let mut provider = &runtime.provider;
    loop {
        match provider {
            ProviderConfig::Fake { model_name, .. } => return model_name.clone(),
            ProviderConfig::OpenAICompatible(OpenAICompatibleConfig { model_name, .. }) => {
                return model_name.clone();
            }
            ProviderConfig::Fallback { primary, .. } => {
                provider = primary;
            }
        }
    }
}

pub(crate) fn app_server_health_diagnostic_status(summary: &ConfigSummary) -> &'static str {
    if summary.placeholder_warnings.is_empty() {
        "ready"
    } else {
        "warning"
    }
}

pub(crate) fn app_server_health_diagnostic_summary(
    summary: &ConfigSummary,
    diagnostic_mode: bool,
) -> String {
    if summary.placeholder_warnings.is_empty() {
        if diagnostic_mode {
            "app-server workspace config is ready in diagnostic mode; no live provider request was made."
                .to_string()
        } else {
            "app-server workspace config is ready; no live provider request was made.".to_string()
        }
    } else {
        let mode = if diagnostic_mode {
            "diagnostic mode"
        } else {
            "workspace config"
        };
        format!(
            "app-server {mode} loaded with {} local warning(s).",
            summary.placeholder_warnings.len()
        )
    }
}

pub(crate) fn app_server_health_next_actions(summary: &ConfigSummary) -> Vec<String> {
    let mut actions = Vec::new();

    if let Some(api_key_state) = &summary.api_key_state {
        if let Some(env_name) = api_key_state
            .strip_prefix("<missing:")
            .and_then(|value| value.strip_suffix('>'))
        {
            push_unique_action(
                &mut actions,
                format!(
                    "set {env_name} in the workspace environment before switching app-server out of diagnostic mode"
                ),
            );
        }
    }

    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("provider=fake"))
    {
        push_unique_action(
            &mut actions,
            "configure an openai_compatible provider for real conversation".to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("transport=stub"))
    {
        push_unique_action(
            &mut actions,
            "switch provider transport to native or curl for real calls".to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("actuator=fake"))
    {
        push_unique_action(
            &mut actions,
            "configure command-backed actuator before expecting desktop/browser operation"
                .to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("subagent=fake"))
    {
        push_unique_action(
            &mut actions,
            "configure queued_external subagents before expecting live worker dispatch".to_string(),
        );
    }
    if summary
        .placeholder_warnings
        .iter()
        .any(|warning| warning.contains("control_plane=fake_local"))
    {
        push_unique_action(
            &mut actions,
            "configure command control before expecting real service control".to_string(),
        );
    }

    actions
}

pub(crate) fn push_unique_action(actions: &mut Vec<String>, action: String) {
    if !actions.iter().any(|existing| existing == &action) {
        actions.push(action);
    }
}

fn format_text_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(",")
    }
}

fn provider_config_model_name(provider: &ProviderConfig) -> String {
    match provider {
        ProviderConfig::Fake { model_name, .. } => model_name.clone(),
        ProviderConfig::OpenAICompatible(OpenAICompatibleConfig { model_name, .. }) => {
            model_name.clone()
        }
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => format!(
            "{}->{}",
            provider_config_model_name(primary),
            provider_config_model_name(fallback)
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_snapshot_recovers_active_turn_and_preserves_safe_history() {
        let db_path = std::env::temp_dir().join(format!(
            "chuang-agent-app-server-snapshot-test-{}-{}.db",
            process::id(),
            APP_SERVER_TURN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let snapshot_store =
            AppServerSnapshotStore::open(db_path.clone()).expect("snapshot store should open");
        let mut state = AppServerState {
            snapshot_store: Some(snapshot_store),
            ..AppServerState::default()
        };
        let thread = create_thread(
            &mut state,
            "/tmp/workspace".to_string(),
            "workspace".to_string(),
        );
        let thread_id = thread.id;
        let turn_id = next_turn_id(&mut state);
        state
            .threads
            .get_mut(&thread_id)
            .expect("thread should exist")
            .turns
            .push(TurnState {
                id: turn_id,
                user_text: "remember this".to_string(),
                assistant_text: String::new(),
                model_name: String::new(),
                status: "active".to_string(),
                provider_meta: BTreeMap::from([
                    (
                        "pending_approval_id".to_string(),
                        "approval-safe".to_string(),
                    ),
                    (
                        "pending_approval_path".to_string(),
                        "/tmp/approval-safe".to_string(),
                    ),
                    (
                        "tool_calls_json".to_string(),
                        "sensitive-tool-output".to_string(),
                    ),
                    ("api_key".to_string(), "sensitive-api-key".to_string()),
                ]),
                tool_trace: "sensitive tool trace".to_string(),
                tool_surface: Some(json!({"secret": "sensitive-tool-surface"})),
                updated_at: 1,
            });
        persist_app_server_state(&state).expect("snapshot should save");

        let restored = load_app_server_state_from_db(db_path.clone())
            .expect("snapshot should reload after daemon restart");
        let restored_turn = &restored
            .threads
            .get(&thread_id)
            .expect("thread should restore")
            .turns[0];
        assert_eq!(restored_turn.status, "interrupted");
        assert_eq!(
            restored_turn
                .provider_meta
                .get("app_server_interruption_reason"),
            Some(&"daemon_restarted_before_turn_completion".to_string())
        );
        assert_eq!(
            restored_turn.provider_meta.get("pending_approval_id"),
            Some(&"approval-safe".to_string())
        );
        assert_eq!(restored_turn.tool_trace, "");
        assert!(restored_turn.tool_surface.is_none());

        let restored_again = load_app_server_state_from_db(db_path.clone())
            .expect("recovered snapshot should reload a second time");
        assert_eq!(
            restored_again
                .threads
                .get(&thread_id)
                .expect("thread should survive a second restart")
                .turns[0]
                .provider_meta
                .get("app_server_interruption_reason"),
            Some(&"daemon_restarted_before_turn_completion".to_string())
        );

        let mut restored = restored_again;
        assert_eq!(
            create_thread(
                &mut restored,
                "/tmp/workspace".to_string(),
                "workspace".to_string()
            )
            .id,
            "chuang-thread-2"
        );
        assert_eq!(next_turn_id(&mut restored), "chuang-turn-2");

        let conn = Connection::open(db_path).expect("snapshot database should open");
        let snapshot_json: String = conn
            .query_row(
                "SELECT snapshot_json FROM app_server_snapshots WHERE snapshot_id = 1",
                [],
                |row| row.get(0),
            )
            .expect("snapshot row should exist");
        assert!(!snapshot_json.contains("tool_calls_json"));
        assert!(!snapshot_json.contains("sensitive-api-key"));
        assert!(!snapshot_json.contains("sensitive tool trace"));
        assert!(!snapshot_json.contains("sensitive-tool-surface"));
    }

    #[test]
    fn pending_approval_turn_status_requires_human_input() {
        let provider_meta = BTreeMap::from([
            (
                "pending_approval_id".to_string(),
                "approval-test".to_string(),
            ),
            (
                "pending_approval_path".to_string(),
                "/tmp/approval-test.json".to_string(),
            ),
        ]);

        assert_eq!(
            app_server_turn_status(&provider_meta),
            "human_input_required"
        );
    }

    #[test]
    fn thread_turn_snapshot_retains_provider_metadata() {
        let turn = TurnState {
            id: "turn-test".to_string(),
            user_text: "test".to_string(),
            assistant_text: "approval required".to_string(),
            model_name: "test-model".to_string(),
            status: "human_input_required".to_string(),
            provider_meta: BTreeMap::from([(
                "pending_approval_id".to_string(),
                "approval-test".to_string(),
            )]),
            tool_trace: String::new(),
            tool_surface: None,
            updated_at: 1,
        };

        let snapshot = turn_to_json(&turn);
        assert_eq!(snapshot["status"], "human_input_required");
        assert_eq!(
            snapshot["providerMeta"]["pending_approval_id"],
            "approval-test"
        );
    }

    #[test]
    fn recent_thread_history_only_includes_admissible_turn_statuses() {
        let mut state = AppServerState::default();
        let thread = create_thread(
            &mut state,
            "/tmp/workspace".to_string(),
            "workspace".to_string(),
        );
        let thread_id = thread.id;
        {
            let thread = state
                .threads
                .get_mut(&thread_id)
                .expect("thread should exist");
            for (id, status, user, assistant) in [
                ("turn-1", "completed", "first", "first answer"),
                ("turn-2", "cancelled", "cancelled request", ""),
                ("turn-3", "failed", "failed request", ""),
                (
                    "turn-provider-error",
                    "provider_error",
                    "provider error request",
                    "provider error answer",
                ),
                (
                    "turn-4",
                    "human_input_required",
                    "approval",
                    "need approval",
                ),
            ] {
                thread.turns.push(TurnState {
                    id: id.to_string(),
                    user_text: user.to_string(),
                    assistant_text: assistant.to_string(),
                    model_name: "test-model".to_string(),
                    status: status.to_string(),
                    provider_meta: BTreeMap::new(),
                    tool_trace: String::new(),
                    tool_surface: None,
                    updated_at: 1,
                });
            }
        }

        let history = recent_thread_history(&state, &thread_id, 6);
        assert_eq!(
            history
                .iter()
                .map(|item| (item.role.as_str(), item.text.as_str()))
                .collect::<Vec<_>>(),
            vec![
                ("user", "first"),
                ("assistant", "first answer"),
                ("user", "approval"),
                ("assistant", "need approval"),
            ]
        );
    }

    #[test]
    fn active_turn_control_write_and_removal_share_a_linearization_point() {
        let guidance_path = std::env::temp_dir().join(format!(
            "chuang-agent-app-server-control-test-{}-{}",
            process::id(),
            APP_SERVER_TURN_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let guidance_writer = Arc::new(Mutex::new(
            create_private_turn_file(&guidance_path, true)
                .expect("private guidance file should create"),
        ));
        let state = Arc::new(Mutex::new(AppServerState::default()));
        let thread_id = "thread-control-test".to_string();
        let turn_id = "turn-control-test".to_string();
        state
            .lock()
            .expect("state lock should acquire")
            .active_turns
            .insert(
                thread_id.clone(),
                ActiveTurn {
                    thread_id: thread_id.clone(),
                    turn_id: turn_id.clone(),
                    guidance_path: guidance_path.clone(),
                    guidance_writer,
                },
            );

        let params = json!({
            "threadId": thread_id,
            "turnId": turn_id,
        });
        let (control_entered_tx, control_entered_rx) = std::sync::mpsc::sync_channel(0);
        let (release_control_tx, release_control_rx) = std::sync::mpsc::sync_channel(0);
        let control_state = Arc::clone(&state);
        let control_params = params.clone();
        let control = thread::spawn(move || {
            with_active_turn_for_control(&control_state, &control_params, |active| {
                control_entered_tx
                    .send(())
                    .expect("control entry signal should send");
                release_control_rx
                    .recv()
                    .expect("control write should be released");
                append_live_turn_note(active, "accepted before unregister")
            })
        });
        control_entered_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("control should enter the active-turn linearization point");

        let (unregister_done_tx, unregister_done_rx) = std::sync::mpsc::sync_channel(0);
        let unregister_state = Arc::clone(&state);
        let unregister_thread_id = thread_id.clone();
        let unregister_turn_id = turn_id.clone();
        let unregister = thread::spawn(move || {
            unregister_active_turn(
                &unregister_state,
                &unregister_thread_id,
                &unregister_turn_id,
            );
            unregister_done_tx
                .send(())
                .expect("unregister completion signal should send");
        });
        assert!(
            unregister_done_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "unregister must wait for an accepted control write"
        );

        release_control_tx
            .send(())
            .expect("control write release should send");
        control
            .join()
            .expect("control write should finish")
            .expect("control write should be accepted");
        unregister_done_rx
            .recv_timeout(Duration::from_secs(1))
            .expect("unregister should finish after the control write");
        unregister.join().expect("unregister thread should finish");

        assert_eq!(
            fs::read_to_string(&guidance_path).expect("guidance contents should read"),
            "accepted before unregister\n"
        );
        let error = with_active_turn_for_control(&state, &params, |active| {
            append_live_turn_note(active, "must not be written")
        })
        .expect_err("unregistered turn should reject control writes");
        assert!(error.contains("turn_not_active"));
        assert_eq!(
            fs::read_to_string(&guidance_path).expect("guidance contents should remain readable"),
            "accepted before unregister\n"
        );
    }
}
