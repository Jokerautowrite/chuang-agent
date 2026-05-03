use chuang_agent::channel_adapter::{
    app_server_turn_start_request, outbound_from_app_server_event,
    outbounds_from_app_server_events, ChannelInboundMessage,
};
use serde_json::json;

fn inbound() -> ChannelInboundMessage {
    ChannelInboundMessage {
        channel: "feishu-dedicated-chuang".to_string(),
        message_id: "msg-1".to_string(),
        sender_id: "user-1".to_string(),
        workspace_root: "/home/user/projects/chuang-agent".to_string(),
        text: "还在吗？".to_string(),
        thread_id: Some("chuang-thread-1".to_string()),
    }
}

#[test]
fn channel_adapter_builds_app_server_turn_start_request() {
    let request = app_server_turn_start_request(7, &inbound()).expect("request should build");

    assert_eq!(request["id"], 7);
    assert_eq!(request["method"], "turn/start");
    assert_eq!(request["params"]["threadId"], "chuang-thread-1");
    assert_eq!(
        request["params"]["workspaceRoot"],
        "/home/user/projects/chuang-agent"
    );
    assert_eq!(request["params"]["text"], "还在吗？");
    assert_eq!(request["params"]["channel"], "feishu-dedicated-chuang");
    assert_eq!(request["params"]["channelMessageId"], "msg-1");
    assert_eq!(request["params"]["senderId"], "user-1");
}

#[test]
fn channel_adapter_rejects_empty_text_before_app_server() {
    let mut message = inbound();
    message.text = " ".to_string();

    let err = app_server_turn_start_request(1, &message).expect_err("empty text should fail");

    assert_eq!(err, "channel message requires text");
}

#[test]
fn channel_adapter_extracts_outbound_delta_event() {
    let event = json!({
        "method": "item/agentMessage/delta",
        "params": {
            "threadId": "chuang-thread-1",
            "turnId": "chuang-turn-1",
            "delta": "我在。"
        }
    });

    let outbound =
        outbound_from_app_server_event(&inbound(), &event).expect("delta should become outbound");

    assert_eq!(outbound.channel, "feishu-dedicated-chuang");
    assert_eq!(outbound.message_id, "msg-1");
    assert_eq!(outbound.thread_id.as_deref(), Some("chuang-thread-1"));
    assert_eq!(outbound.text, "我在。");
}

#[test]
fn channel_adapter_ignores_non_message_events() {
    let event = json!({
        "method": "turn/completed",
        "params": {
            "threadId": "chuang-thread-1"
        }
    });

    assert!(outbound_from_app_server_event(&inbound(), &event).is_none());
}

#[test]
fn channel_adapter_prefers_completed_event_over_delta_batch() {
    let events = vec![
        json!({
            "method": "item/agentMessage/delta",
            "params": {
                "threadId": "chuang-thread-1",
                "turnId": "chuang-turn-1",
                "delta": "我"
            }
        }),
        json!({
            "method": "item/completed",
            "params": {
                "threadId": "chuang-thread-1",
                "turnId": "chuang-turn-1",
                "item": {
                    "type": "agentMessage",
                    "text": "我在。"
                }
            }
        }),
    ];

    let outbound = outbounds_from_app_server_events(&inbound(), &events);

    assert_eq!(outbound.len(), 1);
    assert_eq!(outbound[0].text, "我在。");
}
