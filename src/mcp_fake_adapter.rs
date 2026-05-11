use std::collections::BTreeMap;

use crate::permission_profile_slot::ToolDescriptorRisk;
use crate::runtime_event_ledger::{RuntimeEvent, RuntimeEventKind, RuntimeRiskDecision};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolSpec {
    pub name: String,
    pub title: String,
    pub input_schema: Value,
    #[serde(default)]
    pub read_only: Option<bool>,
    #[serde(default)]
    pub destructive: Option<bool>,
    #[serde(default)]
    pub open_world: Option<bool>,
    #[serde(default)]
    pub external_commit: Option<bool>,
    #[serde(default)]
    pub risk_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolCall {
    pub name: String,
    #[serde(default)]
    pub arguments: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolResult {
    pub tool_name: String,
    pub ok: bool,
    pub risk: McpToolRiskView,
    pub arguments: Value,
    pub arguments_redacted: bool,
    pub content: Value,
    pub stderr_preview: Option<String>,
    pub output_redacted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolDescriptor {
    pub spec: McpToolSpec,
    pub risk: McpToolRiskView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRuntimeEventInput {
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub call_id: String,
    pub tool_name: String,
    pub risk: McpToolRiskView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpRuntimeEvents {
    pub approval_required: Option<RuntimeEvent>,
    pub tool_started: RuntimeEvent,
    pub tool_finished: RuntimeEvent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpToolRiskView {
    pub name: String,
    pub read_only: bool,
    pub destructive: bool,
    pub open_world: bool,
    pub external_commit: bool,
    pub requires_approval: bool,
    pub omitted_risk_defaults_tightened: bool,
    pub permission_decision_hint: String,
    pub risk_tags: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum McpAdapterError {
    MalformedJson {
        message: String,
        stderr_preview: Option<String>,
    },
    UnknownTool {
        name: String,
        stderr_preview: Option<String>,
    },
    UnsupportedMethod {
        method: String,
        stderr_preview: Option<String>,
    },
    FakeTimeout {
        tool_name: String,
        timeout_ms: u64,
        stderr_preview: Option<String>,
    },
    ToolFailed {
        tool_name: String,
        message: String,
        stderr_preview: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum McpFakeToolResponse {
    Ok(Value),
    Error(String),
    Timeout { timeout_ms: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct McpFakeTool {
    spec: McpToolSpec,
    response: McpFakeToolResponse,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct McpFakeServer {
    tools: BTreeMap<String, McpFakeTool>,
    stderr_noise: Vec<String>,
}

impl McpToolSpec {
    pub fn new(name: impl Into<String>, title: impl Into<String>, input_schema: Value) -> Self {
        Self {
            name: name.into(),
            title: title.into(),
            input_schema,
            read_only: None,
            destructive: None,
            open_world: None,
            external_commit: None,
            risk_tags: Vec::new(),
        }
    }

    pub fn read_only(mut self, value: bool) -> Self {
        self.read_only = Some(value);
        self
    }

    pub fn destructive(mut self, value: bool) -> Self {
        self.destructive = Some(value);
        self
    }

    pub fn open_world(mut self, value: bool) -> Self {
        self.open_world = Some(value);
        self
    }

    pub fn external_commit(mut self, value: bool) -> Self {
        self.external_commit = Some(value);
        self
    }

    pub fn risk_tags(mut self, tags: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.risk_tags = tags.into_iter().map(Into::into).collect();
        self
    }

    pub fn risk_view(&self) -> McpToolRiskView {
        mcp_tool_risk_view(self)
    }
}

impl McpFakeServer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_stderr_noise(mut self, line: impl Into<String>) -> Self {
        self.stderr_noise.push(line.into());
        self
    }

    pub fn with_tool(mut self, spec: McpToolSpec, content: Value) -> Self {
        self.insert_tool(spec, McpFakeToolResponse::Ok(content));
        self
    }

    pub fn with_error_tool(mut self, spec: McpToolSpec, message: impl Into<String>) -> Self {
        self.insert_tool(spec, McpFakeToolResponse::Error(message.into()));
        self
    }

    pub fn with_timeout_tool(mut self, spec: McpToolSpec, timeout_ms: u64) -> Self {
        self.insert_tool(spec, McpFakeToolResponse::Timeout { timeout_ms });
        self
    }

    pub fn list_tools(&self) -> Vec<McpToolSpec> {
        self.tools.values().map(|tool| tool.spec.clone()).collect()
    }

    pub fn list_tool_descriptors(&self) -> Vec<McpToolDescriptor> {
        self.tools
            .values()
            .map(|tool| McpToolDescriptor {
                spec: tool.spec.clone(),
                risk: tool.spec.risk_view(),
            })
            .collect()
    }

    pub fn call_tool(&self, call: McpToolCall) -> Result<McpToolResult, McpAdapterError> {
        let tool = self
            .tools
            .get(&call.name)
            .ok_or_else(|| McpAdapterError::UnknownTool {
                name: call.name.clone(),
                stderr_preview: self.stderr_preview(),
            })?;
        let (arguments, arguments_redacted) = redact_sensitive_value(&call.arguments);

        match &tool.response {
            McpFakeToolResponse::Ok(content) => {
                let (content, output_redacted) = redact_sensitive_value(content);
                Ok(McpToolResult {
                    tool_name: call.name,
                    ok: true,
                    risk: tool.spec.risk_view(),
                    arguments,
                    arguments_redacted,
                    content,
                    stderr_preview: self.stderr_preview(),
                    output_redacted,
                })
            }
            McpFakeToolResponse::Error(message) => {
                let (message, _) = redact_sensitive_text(message);
                Err(McpAdapterError::ToolFailed {
                    tool_name: call.name,
                    message,
                    stderr_preview: self.stderr_preview(),
                })
            }
            McpFakeToolResponse::Timeout { timeout_ms } => Err(McpAdapterError::FakeTimeout {
                tool_name: call.name,
                timeout_ms: *timeout_ms,
                stderr_preview: self.stderr_preview(),
            }),
        }
    }

    pub fn handle_request(&self, raw_json: &str) -> Result<Value, McpAdapterError> {
        let request: Value =
            serde_json::from_str(raw_json).map_err(|error| McpAdapterError::MalformedJson {
                message: format!("request json is malformed: {error}"),
                stderr_preview: self.stderr_preview(),
            })?;

        let method = request
            .get("method")
            .and_then(Value::as_str)
            .ok_or_else(|| McpAdapterError::MalformedJson {
                message: "request method is missing or not a string".to_string(),
                stderr_preview: self.stderr_preview(),
            })?;

        match method {
            "tools/list" => Ok(json!({
                "method": "tools/list",
                "tools": self.list_tools(),
                "descriptors": self.list_tool_descriptors(),
                "stderr_preview": self.stderr_preview(),
            })),
            "tools/call" => {
                let params = request.get("params").cloned().ok_or_else(|| {
                    McpAdapterError::MalformedJson {
                        message: "tools/call params are missing".to_string(),
                        stderr_preview: self.stderr_preview(),
                    }
                })?;
                let call: McpToolCall = serde_json::from_value(params).map_err(|error| {
                    McpAdapterError::MalformedJson {
                        message: format!("tools/call params are malformed: {error}"),
                        stderr_preview: self.stderr_preview(),
                    }
                })?;
                serde_json::to_value(self.call_tool(call)?).map_err(|error| {
                    McpAdapterError::MalformedJson {
                        message: format!("tool result serialization failed: {error}"),
                        stderr_preview: self.stderr_preview(),
                    }
                })
            }
            other => Err(McpAdapterError::UnsupportedMethod {
                method: other.to_string(),
                stderr_preview: self.stderr_preview(),
            }),
        }
    }

    pub fn stderr_preview(&self) -> Option<String> {
        if self.stderr_noise.is_empty() {
            return None;
        }

        let joined = self.stderr_noise.join("\n");
        let (redacted, _) = redact_sensitive_text(&joined);
        Some(redacted.chars().take(512).collect())
    }

    fn insert_tool(&mut self, spec: McpToolSpec, response: McpFakeToolResponse) {
        self.tools
            .insert(spec.name.clone(), McpFakeTool { spec, response });
    }
}

pub fn mcp_tool_risk_view(spec: &McpToolSpec) -> McpToolRiskView {
    let read_only = spec.read_only.unwrap_or(false);
    let destructive = spec.destructive.unwrap_or(!read_only);
    let open_world = spec.open_world.unwrap_or(!read_only);
    let external_commit = spec.external_commit.unwrap_or(!read_only);
    let omitted_risk_defaults_tightened = !read_only
        && (spec.destructive.is_none()
            || spec.open_world.is_none()
            || spec.external_commit.is_none());
    let requires_approval =
        destructive || open_world || external_commit || has_high_risk_tag(&spec.risk_tags);
    let permission_decision_hint = if requires_approval {
        "require_approval"
    } else if read_only {
        "allow"
    } else {
        "allow_with_audit"
    };

    McpToolRiskView {
        name: spec.name.clone(),
        read_only,
        destructive,
        open_world,
        external_commit,
        requires_approval,
        omitted_risk_defaults_tightened,
        permission_decision_hint: permission_decision_hint.to_string(),
        risk_tags: spec.risk_tags.clone(),
    }
}

pub fn mcp_tool_descriptor_risk<'a>(
    spec: &'a McpToolSpec,
    risk_tags_storage: &'a mut Vec<&'a str>,
) -> ToolDescriptorRisk<'a> {
    let risk = mcp_tool_risk_view(spec);
    risk_tags_storage.clear();
    risk_tags_storage.extend(spec.risk_tags.iter().map(String::as_str));

    if risk.open_world && !risk_tags_storage.contains(&"open_world") {
        risk_tags_storage.push("open_world");
    }
    if risk.external_commit && !risk_tags_storage.contains(&"external_commit") {
        risk_tags_storage.push("external_commit");
    }
    if risk.omitted_risk_defaults_tightened
        && !risk_tags_storage.contains(&"omitted_risk_tightened")
    {
        risk_tags_storage.push("omitted_risk_tightened");
    }

    ToolDescriptorRisk {
        name: &spec.name,
        risk_tags: risk_tags_storage.as_slice(),
        read_only: risk.read_only,
        mutating: !risk.read_only,
        destructive: risk.destructive,
        external_commit: risk.external_commit,
        requires_approval: risk.requires_approval,
    }
}

pub fn mcp_call_runtime_events(input: McpRuntimeEventInput, ok: bool) -> McpRuntimeEvents {
    let decision = if input.risk.requires_approval {
        RuntimeRiskDecision::new(
            "require_approval",
            format!("mcp tool {} requires approval", input.tool_name),
        )
        .with_policy_ref("policy://mcp-fake-adapter/risk-view")
    } else if input.risk.read_only {
        RuntimeRiskDecision::new(
            "allow",
            format!("mcp tool {} is read-only", input.tool_name),
        )
        .with_policy_ref("policy://mcp-fake-adapter/read-only")
    } else {
        RuntimeRiskDecision::new(
            "allow_with_audit",
            format!("mcp tool {} is local mutating", input.tool_name),
        )
        .with_policy_ref("policy://mcp-fake-adapter/local-mutating")
    };
    let base_evidence = format!("mcp://tool/{}/{}", input.tool_name, input.call_id);

    let mut tool_started =
        RuntimeEvent::new(RuntimeEventKind::ToolStarted, input.thread_id.clone())
            .with_call_id(input.call_id.clone())
            .with_risk_decision(decision.clone())
            .with_evidence_ref(format!("{base_evidence}/started"));
    let mut tool_finished =
        RuntimeEvent::new(RuntimeEventKind::ToolFinished, input.thread_id.clone())
            .with_call_id(input.call_id.clone())
            .with_risk_decision(RuntimeRiskDecision::new(
                if ok { "tool_result" } else { "tool_error" },
                format!("mcp tool {} finished ok={ok}", input.tool_name),
            ))
            .with_evidence_ref(format!(
                "{base_evidence}/{}",
                if ok { "result" } else { "error" }
            ));
    if let Some(turn_id) = &input.turn_id {
        tool_started = tool_started.with_turn_id(turn_id.clone());
        tool_finished = tool_finished.with_turn_id(turn_id.clone());
    }

    let approval_required = if input.risk.requires_approval {
        let mut event = RuntimeEvent::new(RuntimeEventKind::ApprovalRequested, input.thread_id)
            .with_call_id(input.call_id)
            .with_risk_decision(decision)
            .with_evidence_ref(format!("{base_evidence}/approval_required"));
        if let Some(turn_id) = input.turn_id {
            event = event.with_turn_id(turn_id);
        }
        Some(event)
    } else {
        None
    };

    McpRuntimeEvents {
        approval_required,
        tool_started,
        tool_finished,
    }
}

fn has_high_risk_tag(tags: &[String]) -> bool {
    tags.iter().any(|tag| {
        let normalized = tag.trim().to_ascii_lowercase().replace(['-', ' '], "_");
        matches!(
            normalized.as_str(),
            "delete"
                | "rm"
                | "cleanup"
                | "reset"
                | "uninstall"
                | "purge"
                | "external_send"
                | "public_post"
                | "payment"
                | "order"
                | "verification_code"
                | "secret_access"
                | "network_change"
                | "service_control"
        )
    })
}

fn redact_sensitive_value(value: &Value) -> (Value, bool) {
    match value {
        Value::String(raw) => {
            let (redacted, changed) = redact_sensitive_text(raw);
            (Value::String(redacted), changed)
        }
        Value::Array(values) => {
            let mut changed = false;
            let redacted = values
                .iter()
                .map(|value| {
                    let (value, value_changed) = redact_sensitive_value(value);
                    changed |= value_changed;
                    value
                })
                .collect();
            (Value::Array(redacted), changed)
        }
        Value::Object(values) => {
            let mut changed = false;
            let redacted = values
                .iter()
                .map(|(key, value)| {
                    let sensitive_key = is_sensitive_key(key);
                    let (value, value_changed) = if sensitive_key {
                        (Value::String("<redacted>".to_string()), true)
                    } else {
                        redact_sensitive_value(value)
                    };
                    changed |= value_changed;
                    (key.clone(), value)
                })
                .collect();
            (Value::Object(redacted), changed)
        }
        _ => (value.clone(), false),
    }
}

fn redact_sensitive_text(input: &str) -> (String, bool) {
    let mut changed = false;
    let redacted = input
        .split_whitespace()
        .map(|token| {
            let (token, token_changed) = redact_sensitive_token(token);
            changed |= token_changed;
            token
        })
        .collect::<Vec<_>>()
        .join(" ");
    (redacted, changed)
}

fn redact_sensitive_token(token: &str) -> (String, bool) {
    let lower = token.to_ascii_lowercase();
    for marker in ["api_key=", "token=", "secret=", "password="] {
        if let Some(start) = lower.find(marker) {
            let value_start = start + marker.len();
            let mut redacted = String::new();
            redacted.push_str(&token[..value_start]);
            redacted.push_str("<redacted>");
            return (redacted, true);
        }
    }

    if lower.starts_with("sk-")
        || lower.starts_with("xoxb-")
        || lower.starts_with("ghp_")
        || token.starts_with("AKIA")
    {
        return ("<redacted>".to_string(), true);
    }

    (token.to_string(), false)
}

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key.trim().to_ascii_lowercase().replace(['-', ' '], "_");
    matches!(
        normalized.as_str(),
        "api_key" | "apikey" | "token" | "secret" | "password" | "credential"
    )
}
