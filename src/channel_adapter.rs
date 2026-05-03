use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelInboundMessage {
    pub channel: String,
    pub message_id: String,
    pub sender_id: String,
    pub workspace_root: String,
    pub text: String,
    pub thread_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChannelOutboundMessage {
    pub channel: String,
    pub message_id: String,
    pub thread_id: Option<String>,
    pub text: String,
}

impl ChannelInboundMessage {
    pub fn validate(&self) -> Result<(), String> {
        require_non_empty("channel", &self.channel)?;
        require_non_empty("message_id", &self.message_id)?;
        require_non_empty("sender_id", &self.sender_id)?;
        require_non_empty("workspace_root", &self.workspace_root)?;
        require_non_empty("text", &self.text)
    }
}

pub fn app_server_turn_start_request(
    request_id: impl Into<Value>,
    inbound: &ChannelInboundMessage,
) -> Result<Value, String> {
    inbound.validate()?;
    let thread_id = inbound.thread_id.as_deref().unwrap_or("");
    Ok(json!({
        "id": request_id.into(),
        "method": "turn/start",
        "params": {
            "threadId": thread_id,
            "workspaceRoot": inbound.workspace_root,
            "text": inbound.text,
            "channel": inbound.channel,
            "channelMessageId": inbound.message_id,
            "senderId": inbound.sender_id,
        }
    }))
}

pub fn outbound_from_app_server_event(
    inbound: &ChannelInboundMessage,
    event: &Value,
) -> Option<ChannelOutboundMessage> {
    let method = event.get("method")?.as_str()?;
    if method != "item/agentMessage/delta" && method != "item/completed" {
        return None;
    }

    let params = event.get("params")?;
    let text = params
        .get("delta")
        .or_else(|| params.get("item").and_then(|item| item.get("text")))?
        .as_str()?
        .trim()
        .to_string();
    if text.is_empty() {
        return None;
    }

    Some(ChannelOutboundMessage {
        channel: inbound.channel.clone(),
        message_id: inbound.message_id.clone(),
        thread_id: params
            .get("threadId")
            .and_then(|value| value.as_str())
            .map(str::to_string)
            .or_else(|| inbound.thread_id.clone()),
        text,
    })
}

pub fn outbounds_from_app_server_events(
    inbound: &ChannelInboundMessage,
    events: &[Value],
) -> Vec<ChannelOutboundMessage> {
    let completed = events
        .iter()
        .filter(|event| {
            event.get("method").and_then(|value| value.as_str()) == Some("item/completed")
        })
        .filter_map(|event| outbound_from_app_server_event(inbound, event))
        .collect::<Vec<_>>();
    if !completed.is_empty() {
        return completed;
    }

    events
        .iter()
        .filter_map(|event| outbound_from_app_server_event(inbound, event))
        .collect()
}

fn require_non_empty(field: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err(format!("channel message requires {field}"));
    }
    Ok(())
}
