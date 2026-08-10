//! `cli_repl_transport` 模块。内部实现模块（无公开顶层项）。

use std::collections::BTreeMap;
use std::env;
use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use std::thread;
use std::time::Duration;

use serde_json::{json, Value};

use crate::cli_types::{CliOptions, ConversationHistoryItem};
use crate::{spawn_repl_turn, spawn_repl_turn_task, LiveControlGate, RunningTurn};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplTransportKind {
    Local,
    AppServerSocket,
}

#[derive(Debug, Clone)]
pub(crate) struct ReplTurnTransport {
    kind: ReplTransportKind,
    socket: Option<PathBuf>,
    workspace_root: PathBuf,
    thread_id: Option<String>,
    resume_latest_thread: bool,
}

impl ReplTurnTransport {
    pub(crate) fn from_environment() -> Result<Self, String> {
        let stub = env::var("CHUANG_REPL_STUB")
            .map(|value| value == "1")
            .unwrap_or(false);
        let mode = env::var("CHUANG_APP_SERVER_MODE").ok();
        let socket = env::var_os("CHUANG_APP_SERVER_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(default_app_server_socket);
        let current_dir =
            env::current_dir().map_err(|error| format!("repl_workspace_root_failed: {error}"))?;
        let workspace_root = resolve_repl_workspace_root(
            env::var_os("CHUANG_REPL_WORKSPACE_ROOT").map(PathBuf::from),
            current_dir,
        )?;
        Self::from_parts(mode.as_deref(), stub, socket, workspace_root)
    }

    pub(crate) fn from_parts(
        mode: Option<&str>,
        stub: bool,
        socket: PathBuf,
        workspace_root: PathBuf,
    ) -> Result<Self, String> {
        match select_repl_transport(mode, stub)? {
            ReplTransportKind::Local => Ok(Self {
                kind: ReplTransportKind::Local,
                socket: None,
                workspace_root,
                thread_id: None,
                resume_latest_thread: false,
            }),
            ReplTransportKind::AppServerSocket => Ok(Self {
                kind: ReplTransportKind::AppServerSocket,
                socket: Some(socket),
                workspace_root,
                thread_id: None,
                resume_latest_thread: true,
            }),
        }
    }

    pub(crate) fn spawn_turn(
        &mut self,
        options: CliOptions,
        user_input: String,
        conversation_history: Vec<ConversationHistoryItem>,
    ) -> RunningTurn {
        match self.kind {
            ReplTransportKind::Local => spawn_repl_turn(
                options,
                user_input,
                conversation_history,
                Some(self.workspace_root.clone()),
            ),
            ReplTransportKind::AppServerSocket => {
                let socket = self.socket.clone().expect("socket transport has socket");
                let workspace_root = self.workspace_root.clone();
                let thread_id = self.thread_id.clone();
                let resume_latest_thread = self.resume_latest_thread;
                spawn_repl_turn_task(
                    user_input.clone(),
                    true,
                    move |guidance_path, progress_path, live_control_gate| {
                        let thread_id = match (thread_id.as_deref(), resume_latest_thread) {
                            (Some(thread_id), _) => Some(thread_id.to_string()),
                            (None, true) => resolve_latest_thread_id(&socket, &workspace_root)?,
                            (None, false) => None,
                        };
                        app_server_turn(
                            &socket,
                            &workspace_root,
                            thread_id.as_deref(),
                            &user_input,
                            &guidance_path,
                            &progress_path,
                            &live_control_gate,
                        )
                    },
                )
            }
        }
    }

    pub(crate) fn capture_result(&mut self, result: &chuang_agent::agent_runtime::RuntimeResult) {
        if self.kind != ReplTransportKind::AppServerSocket {
            return;
        }
        if let Some(thread_id) = result
            .response
            .meta
            .extra
            .get("app_server_thread_id")
            .map(String::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            self.thread_id = Some(thread_id.to_string());
            self.resume_latest_thread = true;
        }
    }

    pub(crate) fn start_new_thread(&mut self) {
        self.thread_id = None;
        self.resume_latest_thread = false;
    }

    pub(crate) fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    #[cfg(test)]
    fn thread_id(&self) -> Option<&str> {
        self.thread_id.as_deref()
    }
}

fn select_repl_transport(mode: Option<&str>, stub: bool) -> Result<ReplTransportKind, String> {
    if stub {
        return Ok(ReplTransportKind::Local);
    }
    match mode.map(str::trim).filter(|value| !value.is_empty()) {
        None | Some("socket") => Ok(ReplTransportKind::AppServerSocket),
        Some("local") => Ok(ReplTransportKind::Local),
        Some(value) => Err(format!(
            "invalid_chuang_app_server_mode: {value}; expected socket or local"
        )),
    }
}

fn default_app_server_socket() -> PathBuf {
    let runtime_dir = env::var_os("XDG_RUNTIME_DIR").unwrap_or_else(|| "/tmp".into());
    PathBuf::from(runtime_dir).join("chuang-agent/app-server.sock")
}

fn resolve_repl_workspace_root(
    configured: Option<PathBuf>,
    current_dir: PathBuf,
) -> Result<PathBuf, String> {
    let workspace_root = configured.unwrap_or_else(|| current_dir.clone());
    if workspace_root.as_os_str().is_empty() {
        return Err("repl_workspace_root_empty".to_string());
    }
    if workspace_root.is_absolute() {
        Ok(workspace_root)
    } else {
        Ok(current_dir.join(workspace_root))
    }
}

fn app_server_turn(
    socket: &Path,
    workspace_root: &Path,
    thread_id: Option<&str>,
    user_input: &str,
    guidance_path: &Path,
    progress_path: &Path,
    live_control_gate: &LiveControlGate,
) -> Result<chuang_agent::agent_runtime::RuntimeResult, String> {
    let request = json!({
        "id": 1,
        "method": "turn/start",
        "params": {
            "threadId": thread_id.unwrap_or(""),
            "workspaceRoot": workspace_root,
            "text": user_input,
        }
    });
    let response = app_server_stream_turn(
        socket,
        request,
        guidance_path,
        progress_path,
        live_control_gate,
    )?;
    runtime_result_from_app_server_response(user_input, &response)
}

fn resolve_latest_thread_id(
    socket: &Path,
    workspace_root: &Path,
) -> Result<Option<String>, String> {
    let response = app_server_rpc_request(
        socket,
        json!({
            "id": 1,
            "method": "thread/list",
            "params": {},
        }),
    )?;
    let threads = response
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "app_server_client_thread_list_missing_data".to_string())?;
    let workspace_root = workspace_root.to_string_lossy();
    Ok(threads.iter().find_map(|thread| {
        let thread_workspace = thread
            .get("workspaceRoot")
            .or_else(|| thread.get("cwd"))
            .and_then(Value::as_str)
            .map(str::trim)?;
        if thread_workspace != workspace_root {
            return None;
        }
        thread
            .get("id")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|thread_id| !thread_id.is_empty())
            .map(str::to_string)
    }))
}

#[derive(Debug, Clone)]
struct TurnControlTarget {
    thread_id: String,
    turn_id: String,
}

fn app_server_stream_turn(
    socket: &Path,
    request: Value,
    guidance_path: &Path,
    progress_path: &Path,
    live_control_gate: &LiveControlGate,
) -> Result<Value, String> {
    let request_id = request
        .get("id")
        .cloned()
        .ok_or_else(|| "app_server_client_request_missing_id".to_string())?;
    let controls_done = Arc::new(AtomicBool::new(false));
    let control_target = Arc::new(Mutex::new(None));
    let progress_writer = Arc::new(Mutex::new(()));
    let control_handle = spawn_live_control_forwarder(
        socket.to_path_buf(),
        guidance_path.to_path_buf(),
        progress_path.to_path_buf(),
        Arc::clone(&control_target),
        Arc::clone(&controls_done),
        Arc::clone(&progress_writer),
    );
    let result = (|| -> Result<Value, String> {
        let mut stream = UnixStream::connect(socket).map_err(|error| {
            format!(
                "app_server_unavailable: socket={} error={error}",
                socket.display()
            )
        })?;
        let encoded = serde_json::to_string(&request)
            .map_err(|error| format!("app_server_client_json_encode_failed: {error}"))?;
        writeln!(stream, "{encoded}").map_err(|error| {
            format!(
                "app_server_client_write_failed: socket={} error={error}",
                socket.display()
            )
        })?;
        stream.flush().map_err(|error| {
            format!(
                "app_server_client_flush_failed: socket={} error={error}",
                socket.display()
            )
        })?;
        let mut reader = BufReader::new(stream);
        loop {
            let mut line = String::new();
            let read = reader.read_line(&mut line).map_err(|error| {
                format!(
                    "app_server_client_read_failed: socket={} error={error}",
                    socket.display()
                )
            })?;
            if read == 0 {
                return Err(format!(
                    "app_server_unavailable: socket={} error=connection_closed_before_response",
                    socket.display()
                ));
            }
            let value: Value = serde_json::from_str(line.trim())
                .map_err(|error| format!("app_server_client_invalid_json: {error}"))?;
            match value.get("method").and_then(Value::as_str) {
                Some("turn/started") => {
                    if let Some(target) = turn_control_target_from_notification(&value) {
                        if let Ok(mut current) = control_target.lock() {
                            *current = Some(target);
                        } else {
                            append_control_warning(
                                progress_path,
                                "实时控制不可用：回合控制状态已损坏",
                                &progress_writer,
                            );
                        }
                    }
                    continue;
                }
                Some("turn/progress") => {
                    if let Some(event) = value.get("params").and_then(|params| params.get("event"))
                    {
                        append_progress_event(progress_path, event, &progress_writer)?;
                    }
                    continue;
                }
                _ => {}
            }
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
    })();
    live_control_gate.close();
    controls_done.store(true, Ordering::Release);
    let _ = control_handle.join();
    result
}

fn turn_control_target_from_notification(notification: &Value) -> Option<TurnControlTarget> {
    let params = notification.get("params")?;
    let thread_id = params.get("threadId")?.as_str()?.trim();
    let turn_id = params.get("turn")?.get("id")?.as_str()?.trim();
    if thread_id.is_empty() || turn_id.is_empty() {
        return None;
    }
    Some(TurnControlTarget {
        thread_id: thread_id.to_string(),
        turn_id: turn_id.to_string(),
    })
}

fn spawn_live_control_forwarder(
    socket: PathBuf,
    guidance_path: PathBuf,
    progress_path: PathBuf,
    control_target: Arc<Mutex<Option<TurnControlTarget>>>,
    done: Arc<AtomicBool>,
    progress_writer: Arc<Mutex<()>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut bytes_read = 0usize;
        loop {
            drain_live_controls(
                &socket,
                &guidance_path,
                &progress_path,
                &control_target,
                &progress_writer,
                &mut bytes_read,
                false,
            );
            if done.load(Ordering::Acquire) {
                drain_live_controls(
                    &socket,
                    &guidance_path,
                    &progress_path,
                    &control_target,
                    &progress_writer,
                    &mut bytes_read,
                    true,
                );
                return;
            }
            thread::sleep(Duration::from_millis(20));
        }
    })
}

fn drain_live_controls(
    socket: &Path,
    guidance_path: &Path,
    progress_path: &Path,
    control_target: &Arc<Mutex<Option<TurnControlTarget>>>,
    progress_writer: &Arc<Mutex<()>>,
    bytes_read: &mut usize,
    final_drain: bool,
) {
    let target = match control_target.lock() {
        Ok(current) => current.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    };
    if target.is_none() && !final_drain {
        return;
    }
    let lines = match read_new_guidance_lines(guidance_path, bytes_read) {
        Ok(lines) => lines,
        Err(error) => {
            append_control_warning(
                progress_path,
                &format!("实时控制读取失败：{error}"),
                progress_writer,
            );
            return;
        }
    };
    for line in lines {
        if let Some(target) = target.as_ref() {
            if let Err(error) = forward_live_control(socket, target, line.as_str()) {
                append_control_warning(
                    progress_path,
                    &format!("实时控制请求失败：{error}"),
                    progress_writer,
                );
            }
        } else if final_drain {
            let action = if line == "[chuang-control] stop" {
                "停止请求"
            } else {
                "补充要求"
            };
            append_control_warning(
                progress_path,
                &format!("{action}未送达：回合在服务端提供控制标识前已结束"),
                progress_writer,
            );
        }
    }
}

fn read_new_guidance_lines(path: &Path, bytes_read: &mut usize) -> Result<Vec<String>, String> {
    let content = match fs::read_to_string(path) {
        Ok(content) => content,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("guidance_read_failed: {error}")),
    };
    let start = (*bytes_read).min(content.len());
    let tail = &content[start..];
    let Some(last_newline) = tail.rfind('\n') else {
        return Ok(Vec::new());
    };
    let complete_len = last_newline + 1;
    let lines = tail[..complete_len]
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();
    *bytes_read = start + complete_len;
    Ok(lines)
}

fn forward_live_control(
    socket: &Path,
    target: &TurnControlTarget,
    line: &str,
) -> Result<(), String> {
    let (method, params) = if line == "[chuang-control] stop" {
        (
            "turn/interrupt",
            json!({
                "threadId": target.thread_id.as_str(),
                "turnId": target.turn_id.as_str(),
            }),
        )
    } else {
        (
            "turn/guidance",
            json!({
                "threadId": target.thread_id.as_str(),
                "turnId": target.turn_id.as_str(),
                "text": line,
            }),
        )
    };
    let response = app_server_rpc_request(
        socket,
        json!({
            "id": 1,
            "method": method,
            "params": params,
        }),
    )?;
    let accepted = response
        .get("accepted")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    if accepted {
        Ok(())
    } else {
        let status = response
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        Err(format!("app_server_control_rejected: status={status}"))
    }
}

fn append_progress_event(
    path: &Path,
    event: &Value,
    progress_writer: &Arc<Mutex<()>>,
) -> Result<(), String> {
    let mut encoded = serde_json::to_vec(event)
        .map_err(|error| format!("progress_json_encode_failed: {error}"))?;
    encoded.push(b'\n');
    let _writer_guard = progress_writer
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("progress_dir_create_failed: {error}"))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|error| format!("progress_open_failed: {error}"))?;
    file.write_all(&encoded)
        .map_err(|error| format!("progress_write_failed: {error}"))?;
    file.flush()
        .map_err(|error| format!("progress_flush_failed: {error}"))
}

fn append_control_warning(path: &Path, message: &str, progress_writer: &Arc<Mutex<()>>) {
    let _ = append_progress_event(
        path,
        &json!({
            "kind": "live_control_warning",
            "details": {
                "message": message,
            }
        }),
        progress_writer,
    );
}

fn app_server_rpc_request(socket: &Path, request: Value) -> Result<Value, String> {
    let mut stream = UnixStream::connect(socket).map_err(|error| {
        format!(
            "app_server_unavailable: socket={} error={error}",
            socket.display()
        )
    })?;
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|error| format!("app_server_client_timeout_config_failed: {error}"))?;
    let encoded = serde_json::to_string(&request)
        .map_err(|error| format!("app_server_client_json_encode_failed: {error}"))?;
    writeln!(stream, "{encoded}").map_err(|error| {
        format!(
            "app_server_client_write_failed: socket={} error={error}",
            socket.display()
        )
    })?;
    stream.flush().map_err(|error| {
        format!(
            "app_server_client_flush_failed: socket={} error={error}",
            socket.display()
        )
    })?;

    let request_id = request
        .get("id")
        .cloned()
        .ok_or_else(|| "app_server_client_request_missing_id".to_string())?;
    let mut reader = BufReader::new(stream);
    loop {
        let mut line = String::new();
        let read = reader.read_line(&mut line).map_err(|error| {
            format!(
                "app_server_client_read_failed: socket={} error={error}",
                socket.display()
            )
        })?;
        if read == 0 {
            return Err(format!(
                "app_server_unavailable: socket={} error=connection_closed_before_response",
                socket.display()
            ));
        }
        let value: Value = serde_json::from_str(line.trim())
            .map_err(|error| format!("app_server_client_invalid_json: {error}"))?;
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

fn runtime_result_from_app_server_response(
    user_input: &str,
    response: &Value,
) -> Result<chuang_agent::agent_runtime::RuntimeResult, String> {
    let turn = response
        .get("turn")
        .ok_or_else(|| "app_server_client_response_missing_turn".to_string())?;
    let thread_id = response["thread"]["id"]
        .as_str()
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "app_server_client_response_missing_thread_id".to_string())?;
    let body = response["thread"]["turns"]
        .as_array()
        .and_then(|turns| turns.last())
        .and_then(|thread_turn| thread_turn["items"].as_array())
        .and_then(|items| {
            items
                .iter()
                .find(|item| item["type"] == "agentMessage")
                .and_then(|item| item["text"].as_str())
        })
        .unwrap_or_default()
        .to_string();
    let mut extra = json_string_map(turn.get("providerMeta"));
    extra.insert("app_server_thread_id".to_string(), thread_id.to_string());
    copy_turn_value(
        &mut extra,
        turn,
        "runtimeReportId",
        "app_server_runtime_report_id",
    );
    copy_turn_value(
        &mut extra,
        turn,
        "contextMaxTokens",
        "app_server_context_max_tokens",
    );
    copy_turn_value(
        &mut extra,
        turn,
        "toolCallCount",
        "app_server_tool_call_count",
    );
    copy_turn_value(
        &mut extra,
        turn,
        "toolProtocolErrorCount",
        "app_server_tool_protocol_error_count",
    );
    for (source, target) in [
        ("toolReport", "app_server_tool_report_json"),
        ("toolSurface", "app_server_tool_surface_json"),
        ("toolCalls", "app_server_tool_calls_json"),
        ("toolProtocolErrors", "app_server_tool_protocol_errors_json"),
        ("toolEvents", "app_server_tool_events_json"),
        (
            "runtimeObservability",
            "app_server_runtime_observability_json",
        ),
        ("liveReadiness", "app_server_live_readiness_json"),
    ] {
        if let Some(value) = turn.get(source) {
            extra.insert(target.to_string(), value.to_string());
        }
    }

    let recall_hit_count = turn["recallHitCount"].as_u64().unwrap_or(0) as usize;
    Ok(chuang_agent::agent_runtime::RuntimeResult {
        prompt: user_input.to_string(),
        response: chuang_agent::agent_runtime::RuntimeResponse {
            model_name: turn["modelName"]
                .as_str()
                .unwrap_or("app-server")
                .to_string(),
            body,
            trace: turn["trace"].as_str().unwrap_or_default().to_string(),
            meta: chuang_agent::responder::ResponderMeta {
                provider: Some("app-server".to_string()),
                recall_hit_count: Some(recall_hit_count),
                finish_reason: turn["finishReason"].as_str().map(str::to_string),
                extra,
            },
        },
        recall_summary: String::new(),
        recall_hit_count,
        context_engine_kind: turn["contextEngineKind"]
            .as_str()
            .unwrap_or("app-server")
            .to_string(),
        packed_context_preview: String::new(),
        packed_token_count: turn["packedTokenCount"].as_u64().unwrap_or(0) as u32,
        dropped_segment_ids: Vec::new(),
        context_debug: chuang_agent::agent_runtime::ContextDebugInfo {
            drop_reasons: Vec::new(),
            budget_exceeded: false,
            budget_exceeded_reasons: Vec::new(),
            working_reservation: None,
        },
    })
}

fn json_string_map(value: Option<&Value>) -> BTreeMap<String, String> {
    value
        .and_then(Value::as_object)
        .map(|map| {
            map.iter()
                .map(|(key, value)| {
                    (
                        key.clone(),
                        value
                            .as_str()
                            .map(str::to_string)
                            .unwrap_or_else(|| value.to_string()),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

fn copy_turn_value(extra: &mut BTreeMap<String, String>, turn: &Value, source: &str, target: &str) {
    if let Some(value) = turn.get(source) {
        if let Some(value) = value.as_str() {
            extra.insert(target.to_string(), value.to_string());
        } else if value.is_number() || value.is_boolean() {
            extra.insert(target.to_string(), value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::net::UnixListener;
    use std::sync::mpsc;
    use std::thread;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_socket(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        let root = env::temp_dir().join(format!("chuang-repl-transport-{name}-{nonce}"));
        fs::create_dir_all(&root).expect("socket parent should create");
        root.join("app-server.sock")
    }

    fn test_options() -> CliOptions {
        CliOptions {
            runtime: chuang_agent::runtime_config::RuntimeConfig::new(PathBuf::from("memory.db")),
        }
    }

    fn receive_turn(turn: RunningTurn) -> chuang_agent::agent_runtime::RuntimeResult {
        let result = turn
            .receiver
            .recv()
            .expect("turn result should arrive")
            .expect("socket turn should succeed");
        turn.handle.join().expect("turn thread should join");
        result
    }

    fn final_turn_response(request_id: &Value, answer: &str) -> Value {
        final_turn_response_for_thread(request_id, "socket-thread-1", answer)
    }

    fn final_turn_response_for_thread(request_id: &Value, thread_id: &str, answer: &str) -> Value {
        json!({
            "id": request_id,
            "result": {
                "thread": {
                    "id": thread_id,
                    "turns": [{"items": [{"type": "agentMessage", "text": answer}]}]
                },
                "turn": {
                    "modelName": "socket-model",
                    "finishReason": "completed",
                    "recallHitCount": 0,
                    "packedTokenCount": 12,
                    "contextEngineKind": "deterministic",
                    "contextMaxTokens": 64000,
                    "providerMeta": {}
                }
            }
        })
    }

    #[test]
    fn transport_selection_defaults_to_socket_and_keeps_explicit_local_overrides() {
        assert_eq!(
            select_repl_transport(None, false).expect("default transport should parse"),
            ReplTransportKind::AppServerSocket
        );
        assert_eq!(
            select_repl_transport(Some("local"), false).expect("local should parse"),
            ReplTransportKind::Local
        );
        assert_eq!(
            select_repl_transport(Some("socket"), true).expect("stub should force local"),
            ReplTransportKind::Local
        );
        assert!(select_repl_transport(Some("unknown"), false).is_err());
    }

    #[test]
    fn workspace_selection_preserves_launcher_cwd() {
        let current_dir = PathBuf::from("/tmp/chuang-project-root");
        let launcher_cwd = PathBuf::from("/tmp/chuang-caller-workspace");
        assert_eq!(
            resolve_repl_workspace_root(Some(launcher_cwd.clone()), current_dir.clone())
                .expect("launcher workspace should resolve"),
            launcher_cwd
        );
        assert_eq!(
            resolve_repl_workspace_root(None, current_dir.clone())
                .expect("current directory should be the fallback"),
            current_dir
        );
    }

    #[test]
    fn first_socket_turn_filters_thread_list_by_workspace() {
        let socket = temp_socket("thread-workspace-filter");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let workspace = PathBuf::from("/tmp/chuang-workspace-filter");
        let server_workspace = workspace.clone();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("thread/list should connect");
            let mut reader = BufReader::new(stream.try_clone().expect("stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("thread/list request should read");
            let list_request: Value =
                serde_json::from_str(line.trim()).expect("thread/list request should be JSON");
            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                json!({
                    "id": list_request["id"],
                    "result": {
                        "data": [
                            {
                                "id": "other-workspace-thread",
                                "workspaceRoot": "/tmp/other-workspace",
                                "updatedAt": 200
                            },
                            {
                                "id": "matching-workspace-thread",
                                "workspaceRoot": server_workspace,
                                "updatedAt": 100
                            }
                        ]
                    }
                })
            )
            .expect("thread/list response should write");

            let (stream, _) = listener.accept().expect("turn/start should connect");
            let mut reader = BufReader::new(stream.try_clone().expect("stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
            let request: Value =
                serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
            let thread_id = request["params"]["threadId"]
                .as_str()
                .expect("selected thread id should be present")
                .to_string();
            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                final_turn_response_for_thread(&request["id"], &thread_id, "answer")
            )
            .expect("turn response should write");
            (list_request, request)
        });

        let mut transport =
            ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace.clone())
                .expect("socket transport should construct");
        let result = receive_turn(transport.spawn_turn(test_options(), "first".into(), Vec::new()));
        transport.capture_result(&result);
        let (list_request, turn_request) = server.join().expect("server should join");

        assert_eq!(list_request["method"], "thread/list");
        assert_eq!(
            turn_request["params"]["threadId"],
            "matching-workspace-thread"
        );
        assert_eq!(
            turn_request["params"]["workspaceRoot"],
            workspace.display().to_string()
        );
        assert_eq!(transport.thread_id(), Some("matching-workspace-thread"));
    }

    #[test]
    fn new_thread_skips_auto_resume_and_reuses_the_new_thread_afterward() {
        let socket = temp_socket("new-thread");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let workspace = PathBuf::from("/tmp/chuang-new-thread");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("thread/list should connect");
            let mut reader = BufReader::new(stream.try_clone().expect("stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("list request should read");
            let list_request: Value =
                serde_json::from_str(line.trim()).expect("list request should be JSON");
            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                json!({
                    "id": list_request["id"],
                    "result": {
                        "data": [{
                            "id": "old-thread",
                            "workspaceRoot": "/tmp/chuang-new-thread",
                            "updatedAt": 10
                        }]
                    }
                })
            )
            .expect("list response should write");

            let mut requests = Vec::new();
            for expected_thread_id in ["old-thread", "", "new-thread"] {
                let (stream, _) = listener.accept().expect("turn/start should connect");
                let mut reader =
                    BufReader::new(stream.try_clone().expect("stream clone should work"));
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .expect("turn/start request should read");
                let request: Value =
                    serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
                assert_eq!(request["params"]["threadId"], expected_thread_id);
                let mut writer = stream;
                let response_thread_id = if expected_thread_id.is_empty() {
                    "new-thread"
                } else {
                    expected_thread_id
                };
                writeln!(
                    writer,
                    "{}",
                    final_turn_response_for_thread(&request["id"], response_thread_id, "answer")
                )
                .expect("turn response should write");
                requests.push(request);
            }
            requests
        });

        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        let first = receive_turn(transport.spawn_turn(test_options(), "first".into(), Vec::new()));
        transport.capture_result(&first);
        transport.start_new_thread();
        assert_eq!(transport.thread_id(), None);

        let second =
            receive_turn(transport.spawn_turn(test_options(), "second".into(), Vec::new()));
        transport.capture_result(&second);
        let third = receive_turn(transport.spawn_turn(test_options(), "third".into(), Vec::new()));
        transport.capture_result(&third);
        let requests = server.join().expect("server should join");

        assert_eq!(requests[0]["params"]["threadId"], "old-thread");
        assert_eq!(requests[1]["params"]["threadId"], "");
        assert_eq!(requests[2]["params"]["threadId"], "new-thread");
        assert_eq!(transport.thread_id(), Some("new-thread"));
    }

    #[test]
    fn socket_transport_reuses_one_thread_and_maps_turn_metadata() {
        let socket = temp_socket("thread-reuse");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            let (stream, _) = listener.accept().expect("thread/list should connect");
            let mut reader = BufReader::new(stream.try_clone().expect("stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("thread/list request should read");
            let request: Value =
                serde_json::from_str(line.trim()).expect("thread/list request should be JSON");
            requests.push(request.clone());
            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                json!({
                    "id": request["id"],
                    "result": {
                        "data": [{
                            "id": "socket-thread-1",
                            "workspaceRoot": env::current_dir().expect("workspace should resolve"),
                            "updatedAt": 2
                        }]
                    }
                })
            )
            .expect("thread/list response should write");

            for index in 1..=2 {
                let (stream, _) = listener.accept().expect("turn socket should connect");
                let mut reader =
                    BufReader::new(stream.try_clone().expect("stream clone should work"));
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .expect("turn request should read");
                let request: Value =
                    serde_json::from_str(line.trim()).expect("turn request should be JSON");
                requests.push(request.clone());
                let mut writer = stream;
                writeln!(
                    writer,
                    "{}",
                    json!({
                        "id": request["id"],
                        "result": {
                            "thread": {
                                "id": "socket-thread-1",
                                "turns": [{"items": [{"type": "agentMessage", "text": format!("answer-{index}")}]}]
                            },
                            "turn": {
                                "modelName": "socket-model",
                                "finishReason": "completed",
                                "recallHitCount": 2,
                                "packedTokenCount": 1200,
                                "contextEngineKind": "deterministic",
                                "contextMaxTokens": 64000,
                                "providerMeta": {
                                    "tool_loop_status": "completed",
                                    "pending_approval_id": "approval-1",
                                    "pending_approval_path": "/tmp/approval-1"
                                },
                                "toolCalls": [{"tool": "code_execute"}],
                                "toolProtocolErrors": [],
                                "toolEvents": [{"kind": "tool_finished"}]
                            }
                        }
                    })
                )
                .expect("response should write");
            }
            requests
        });
        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport =
            ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace.clone())
                .expect("socket transport should construct");

        let first = receive_turn(transport.spawn_turn(test_options(), "first".into(), Vec::new()));
        assert_eq!(first.response.body, "answer-1");
        assert_eq!(first.response.model_name, "socket-model");
        assert_eq!(first.packed_token_count, 1200);
        assert_eq!(
            first.response.meta.extra.get("tool_loop_status"),
            Some(&"completed".to_string())
        );
        assert_eq!(
            first.response.meta.extra.get("pending_approval_id"),
            Some(&"approval-1".to_string())
        );
        assert_eq!(
            first.response.meta.extra.get("pending_approval_path"),
            Some(&"/tmp/approval-1".to_string())
        );
        assert!(first
            .response
            .meta
            .extra
            .contains_key("app_server_tool_calls_json"));
        transport.capture_result(&first);
        assert_eq!(transport.thread_id(), Some("socket-thread-1"));

        let second =
            receive_turn(transport.spawn_turn(test_options(), "second".into(), Vec::new()));
        transport.capture_result(&second);
        let requests = server.join().expect("server should join");
        assert_eq!(requests[0]["method"], "thread/list");
        assert_eq!(requests[1]["params"]["threadId"], "socket-thread-1");
        assert_eq!(requests[2]["params"]["threadId"], "socket-thread-1");
        assert_eq!(
            requests[1]["params"]["workspaceRoot"],
            workspace.display().to_string()
        );
        assert_eq!(
            requests[2]["params"]["workspaceRoot"],
            workspace.display().to_string()
        );
    }

    #[test]
    fn socket_turn_forwards_progress_and_live_controls_after_started_notification() {
        let socket = temp_socket("stream-controls");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let (start_ready_tx, start_ready_rx) = mpsc::channel();
        let (release_started_tx, release_started_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("start connection should arrive");
            let mut reader =
                BufReader::new(stream.try_clone().expect("start stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
            let start_request: Value =
                serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
            start_ready_tx
                .send(())
                .expect("test should wait before turn/started");
            release_started_rx
                .recv()
                .expect("test should release turn/started");

            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                json!({
                    "method": "turn/started",
                    "params": {
                        "threadId": "socket-thread-1",
                        "turn": {"id": "socket-turn-1"}
                    }
                })
            )
            .expect("turn/started notification should write");
            let progress = json!({
                "schema_version": 2,
                "event": {
                    "kind": "tool_started",
                    "round": 1,
                    "tool": "code_execute",
                    "summary": null,
                    "activity_title": "检查状态",
                    "activity_detail": null
                }
            });
            writeln!(
                writer,
                "{}",
                json!({
                    "method": "turn/progress",
                    "params": {
                        "threadId": "socket-thread-1",
                        "turnId": "socket-turn-1",
                        "event": progress
                    }
                })
            )
            .expect("turn/progress notification should write");

            let mut controls = Vec::new();
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("control connection should arrive");
                let mut reader = BufReader::new(
                    stream
                        .try_clone()
                        .expect("control stream clone should work"),
                );
                let mut line = String::new();
                reader
                    .read_line(&mut line)
                    .expect("control request should read");
                let request: Value =
                    serde_json::from_str(line.trim()).expect("control request should be JSON");
                writeln!(
                    stream,
                    "{}",
                    json!({
                        "id": request["id"],
                        "result": {
                            "accepted": true,
                            "status": "queued",
                            "effectiveAt": "next_safe_point"
                        }
                    })
                )
                .expect("control response should write");
                controls.push(request);
            }
            writeln!(
                writer,
                "{}",
                final_turn_response(&start_request["id"], "answer")
            )
            .expect("final response should write");
            (start_request, controls, progress)
        });

        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "first".into(), Vec::new());
        assert!(turn.supports_live_control);
        let progress_path = turn.progress_path.clone();
        start_ready_rx
            .recv()
            .expect("server should wait before emitting turn/started");
        assert_eq!(
            turn.enqueue_live_control("focus on the failing test")
                .expect("guidance should queue"),
            crate::LiveControlEnqueueResult::Queued
        );
        assert_eq!(
            turn.enqueue_live_control("[chuang-control] stop")
                .expect("stop should queue"),
            crate::LiveControlEnqueueResult::Queued
        );
        release_started_tx
            .send(())
            .expect("server should emit turn/started");

        let result = receive_turn(turn);
        transport.capture_result(&result);
        let (start_request, controls, expected_progress) =
            server.join().expect("server should join");
        let progress_lines = fs::read_to_string(progress_path)
            .expect("turn/progress event should be written locally");
        let forwarded: Value =
            serde_json::from_str(progress_lines.trim()).expect("progress line should remain JSON");

        assert_eq!(start_request["method"], "turn/start");
        assert_eq!(controls.len(), 2);
        assert_eq!(controls[0]["method"], "turn/guidance");
        assert_eq!(controls[0]["params"]["threadId"], "socket-thread-1");
        assert_eq!(controls[0]["params"]["turnId"], "socket-turn-1");
        assert_eq!(controls[0]["params"]["text"], "focus on the failing test");
        assert_eq!(controls[1]["method"], "turn/interrupt");
        assert_eq!(controls[1]["params"]["threadId"], "socket-thread-1");
        assert_eq!(controls[1]["params"]["turnId"], "socket-turn-1");
        assert_eq!(forwarded, expected_progress);
        assert_eq!(result.response.body, "answer");
        assert_eq!(transport.thread_id(), Some("socket-thread-1"));
    }

    #[test]
    fn socket_control_failure_is_written_as_progress_warning() {
        let socket = temp_socket("control-warning");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("start connection should arrive");
            let mut reader =
                BufReader::new(stream.try_clone().expect("start stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
            let request: Value =
                serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                json!({
                    "method": "turn/started",
                    "params": {
                        "threadId": "socket-thread-1",
                        "turn": {"id": "socket-turn-1"}
                    }
                })
            )
            .expect("turn/started notification should write");

            let (mut stream, _) = listener.accept().expect("control connection should arrive");
            let mut control_reader = BufReader::new(
                stream
                    .try_clone()
                    .expect("control stream clone should work"),
            );
            let mut control_line = String::new();
            control_reader
                .read_line(&mut control_line)
                .expect("control request should read");
            let control_request: Value =
                serde_json::from_str(control_line.trim()).expect("control request should be JSON");
            writeln!(
                stream,
                "{}",
                json!({
                    "id": control_request["id"],
                    "error": {"message": "turn_not_running"}
                })
            )
            .expect("control error should write");
            writeln!(writer, "{}", final_turn_response(&request["id"], "answer"))
                .expect("final response should write");
        });

        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "first".into(), Vec::new());
        let progress_path = turn.progress_path.clone();
        assert_eq!(
            turn.enqueue_live_control("please stop soon")
                .expect("guidance should queue"),
            crate::LiveControlEnqueueResult::Queued
        );
        let result = receive_turn(turn);
        transport.capture_result(&result);
        server.join().expect("server should join");

        let progress = fs::read_to_string(progress_path)
            .expect("control failure should be written to progress");
        let warning = progress
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).expect("progress line should be JSON"))
            .find(|line| line["kind"] == "live_control_warning")
            .expect("control warning should be visible to renderers");
        assert!(
            warning["details"]["message"]
                .as_str()
                .is_some_and(|message| message.contains("turn_not_running")),
            "warning={warning}"
        );
    }

    #[test]
    fn socket_final_response_drains_control_queued_immediately_before_completion() {
        let socket = temp_socket("final-drain");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let (started_tx, started_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("start connection should arrive");
            let mut reader =
                BufReader::new(stream.try_clone().expect("start stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
            let request: Value =
                serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
            let mut writer = stream;
            writeln!(
                writer,
                "{}",
                json!({
                    "method": "turn/started",
                    "params": {
                        "threadId": "socket-thread-1",
                        "turn": {"id": "socket-turn-final"}
                    }
                })
            )
            .expect("turn/started notification should write");
            writer.flush().expect("turn/started should flush");
            started_tx
                .send(())
                .expect("test should queue final control");
            finish_rx
                .recv()
                .expect("test should release final response");
            writeln!(writer, "{}", final_turn_response(&request["id"], "answer"))
                .expect("final response should write");
            writer.flush().expect("final response should flush");

            let (mut control, _) = listener.accept().expect("final drain should connect");
            let mut control_reader = BufReader::new(
                control
                    .try_clone()
                    .expect("control stream clone should work"),
            );
            let mut control_line = String::new();
            control_reader
                .read_line(&mut control_line)
                .expect("control request should read");
            let control_request: Value =
                serde_json::from_str(control_line.trim()).expect("control request should be JSON");
            writeln!(
                control,
                "{}",
                json!({
                    "id": control_request["id"],
                    "result": {"accepted": true, "status": "queued"}
                })
            )
            .expect("control response should write");
            control_request
        });

        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "first".into(), Vec::new());
        started_rx.recv().expect("turn should start");
        assert_eq!(
            turn.enqueue_live_control("last safe-point guidance")
                .expect("guidance should queue before final"),
            crate::LiveControlEnqueueResult::Queued
        );
        finish_tx.send(()).expect("server should finish");
        let result = receive_turn(turn);
        let control = server.join().expect("server should join");

        assert_eq!(control["method"], "turn/guidance");
        assert_eq!(control["params"]["turnId"], "socket-turn-final");
        assert_eq!(control["params"]["text"], "last safe-point guidance");
        assert_eq!(result.response.body, "answer");
    }

    #[test]
    fn socket_final_drain_warns_when_accepted_control_has_no_turn_ids() {
        let socket = temp_socket("final-warning");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let (ready_tx, ready_rx) = mpsc::channel();
        let (finish_tx, finish_rx) = mpsc::channel();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("start connection should arrive");
            let mut reader =
                BufReader::new(stream.try_clone().expect("start stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
            let request: Value =
                serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
            ready_tx.send(()).expect("test should queue guidance");
            finish_rx
                .recv()
                .expect("test should release final response");
            writeln!(stream, "{}", final_turn_response(&request["id"], "answer"))
                .expect("final response should write");
        });

        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "first".into(), Vec::new());
        let progress_path = turn.progress_path.clone();
        ready_rx.recv().expect("start request should arrive");
        assert_eq!(
            turn.enqueue_live_control("late guidance")
                .expect("guidance should queue"),
            crate::LiveControlEnqueueResult::Queued
        );
        finish_tx.send(()).expect("server should finish");
        let _ = receive_turn(turn);
        server.join().expect("server should join");

        let progress =
            fs::read_to_string(progress_path).expect("undeliverable control should warn");
        assert!(progress.lines().any(|line| {
            serde_json::from_str::<Value>(line)
                .ok()
                .is_some_and(|value| value["kind"] == "live_control_warning")
        }));
    }

    #[test]
    fn concurrent_progress_and_warning_writes_are_complete_jsonl_records() {
        let path = temp_socket("progress-writer").with_file_name("progress.jsonl");
        let writer = Arc::new(Mutex::new(()));
        let mut handles = Vec::new();
        for worker in 0..6 {
            let path = path.clone();
            let writer = Arc::clone(&writer);
            handles.push(thread::spawn(move || {
                for index in 0..40 {
                    if worker % 2 == 0 {
                        append_progress_event(
                            &path,
                            &json!({
                                "kind": "tool_started",
                                "details": {
                                    "worker": worker,
                                    "index": index,
                                    "payload": "x".repeat(4096)
                                }
                            }),
                            &writer,
                        )
                        .expect("progress event should write");
                    } else {
                        append_control_warning(
                            &path,
                            &format!("worker={worker} index={index}"),
                            &writer,
                        );
                    }
                }
            }));
        }
        for handle in handles {
            handle.join().expect("writer should join");
        }

        let content = fs::read_to_string(path).expect("progress JSONL should exist");
        let lines = content.lines().collect::<Vec<_>>();
        assert_eq!(lines.len(), 240);
        assert!(content.ends_with('\n'));
        assert!(lines
            .iter()
            .all(|line| serde_json::from_str::<Value>(line).is_ok()));
    }

    #[test]
    fn socket_failure_returns_explicit_error_without_local_fallback() {
        let socket = temp_socket("unavailable");
        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport =
            ReplTurnTransport::from_parts(Some("socket"), false, socket.clone(), workspace)
                .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "do not fall back".into(), Vec::new());
        let error = turn
            .receiver
            .recv()
            .expect("turn result should arrive")
            .expect_err("missing socket must fail");
        turn.handle.join().expect("turn thread should join");

        assert!(error.contains("app_server_unavailable"), "error={error}");
        assert!(
            error.contains(&socket.display().to_string()),
            "error={error}"
        );
        assert_eq!(transport.thread_id(), None);
    }

    #[test]
    fn socket_eof_is_explicit_without_local_fallback() {
        let socket = temp_socket("eof");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().expect("start connection should arrive");
            let mut reader =
                BufReader::new(stream.try_clone().expect("start stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
        });
        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "do not fall back".into(), Vec::new());
        let error = turn
            .receiver
            .recv()
            .expect("turn result should arrive")
            .expect_err("EOF must fail");
        turn.handle.join().expect("turn thread should join");
        server.join().expect("server should join");

        assert!(error.contains("app_server_unavailable"), "error={error}");
        assert!(
            error.contains("connection_closed_before_response"),
            "error={error}"
        );
        assert_eq!(transport.thread_id(), None);
    }

    #[test]
    fn socket_unknown_thread_is_explicit_without_local_fallback() {
        let socket = temp_socket("unknown-thread");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("start connection should arrive");
            let mut reader =
                BufReader::new(stream.try_clone().expect("start stream clone should work"));
            let mut line = String::new();
            reader
                .read_line(&mut line)
                .expect("turn/start request should read");
            let request: Value =
                serde_json::from_str(line.trim()).expect("turn/start request should be JSON");
            writeln!(
                stream,
                "{}",
                json!({
                    "id": request["id"],
                    "error": {"message": "unknown_thread: stale-thread"}
                })
            )
            .expect("unknown thread response should write");
        });
        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport = ReplTurnTransport::from_parts(Some("socket"), false, socket, workspace)
            .expect("socket transport should construct");
        transport.start_new_thread();
        let turn = transport.spawn_turn(test_options(), "do not fall back".into(), Vec::new());
        let error = turn
            .receiver
            .recv()
            .expect("turn result should arrive")
            .expect_err("unknown thread must fail");
        turn.handle.join().expect("turn thread should join");
        server.join().expect("server should join");

        assert!(error.contains("app_server_rpc_failed"), "error={error}");
        assert!(error.contains("unknown_thread"), "error={error}");
        assert_eq!(transport.thread_id(), None);
    }
}
