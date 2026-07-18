use std::collections::BTreeMap;
use std::env;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::cli_types::{CliOptions, ConversationHistoryItem};
use crate::{spawn_repl_turn, spawn_repl_turn_task, RunningTurn};

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

    fn from_parts(
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
            }),
            ReplTransportKind::AppServerSocket => Ok(Self {
                kind: ReplTransportKind::AppServerSocket,
                socket: Some(socket),
                workspace_root,
                thread_id: None,
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
                spawn_repl_turn_task(user_input.clone(), false, move |_, _| {
                    app_server_turn(&socket, &workspace_root, thread_id.as_deref(), &user_input)
                })
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
        }
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
) -> Result<chuang_agent::agent_runtime::RuntimeResult, String> {
    let response = app_server_rpc_request(
        socket,
        json!({
            "id": 1,
            "method": "turn/start",
            "params": {
                "threadId": thread_id.unwrap_or(""),
                "workspaceRoot": workspace_root,
                "text": user_input,
            }
        }),
    )?;
    runtime_result_from_app_server_response(user_input, &response)
}

fn app_server_rpc_request(socket: &Path, request: Value) -> Result<Value, String> {
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
    fn socket_transport_reuses_one_thread_and_maps_turn_metadata() {
        let socket = temp_socket("thread-reuse");
        let listener = UnixListener::bind(&socket).expect("test socket should bind");
        let server = thread::spawn(move || {
            let mut requests = Vec::new();
            for index in 1..=2 {
                let (stream, _) = listener.accept().expect("socket client should connect");
                let mut reader =
                    BufReader::new(stream.try_clone().expect("stream clone should work"));
                let mut line = String::new();
                reader.read_line(&mut line).expect("request should read");
                let request: Value =
                    serde_json::from_str(line.trim()).expect("request should be JSON");
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
        assert_eq!(requests[0]["params"]["threadId"], "");
        assert_eq!(requests[1]["params"]["threadId"], "socket-thread-1");
        assert_eq!(
            requests[0]["params"]["workspaceRoot"],
            workspace.display().to_string()
        );
        assert_eq!(
            requests[1]["params"]["workspaceRoot"],
            workspace.display().to_string()
        );
    }

    #[test]
    fn socket_failure_returns_explicit_error_without_local_fallback() {
        let socket = temp_socket("unavailable");
        let workspace = env::current_dir().expect("workspace should resolve");
        let mut transport =
            ReplTurnTransport::from_parts(Some("socket"), false, socket.clone(), workspace)
                .expect("socket transport should construct");
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
}
