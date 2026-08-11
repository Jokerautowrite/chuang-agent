//! `tool_loop_meta` 模块。公开接口：struct ToolLoopMeta；fn from_extra, typed_from_extra, parse_json_field, parse_json_vec, parse_json_value, parse_json_vec_value, derive_tool_protocol_correction_context, derive_tool_protocol_typed_failure。

use std::collections::{BTreeMap, BTreeSet};

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolLoopMeta<C = Value, E = Value, V = Value> {
    pub tool_call_count: usize,
    pub tool_protocol_error_count: usize,
    pub tool_trace: String,
    pub tool_report: Option<Value>,
    pub tool_calls: Vec<C>,
    pub tool_protocol_errors: Vec<E>,
    pub tool_events: Vec<V>,
}

impl ToolLoopMeta<Value, Value, Value> {
    pub fn from_extra(extra: &BTreeMap<String, String>) -> Result<Self, String> {
        Self::typed_from_extra(extra)
    }
}

impl<C, E, V> ToolLoopMeta<C, E, V>
where
    C: DeserializeOwned,
    E: DeserializeOwned,
    V: DeserializeOwned,
{
    pub fn typed_from_extra(extra: &BTreeMap<String, String>) -> Result<Self, String> {
        Ok(Self {
            tool_call_count: extra
                .get("tool_call_count")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0),
            tool_protocol_error_count: extra
                .get("tool_protocol_error_count")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0),
            tool_trace: extra.get("tool_trace").cloned().unwrap_or_default(),
            tool_report: extra
                .get("tool_report_json")
                .map(|value| serde_json::from_str(value))
                .transpose()
                .map_err(|e| format!("tool_report_json_parse_failed: {e}"))?,
            tool_calls: parse_json_vec::<C>(extra, "tool_calls_json")?,
            tool_protocol_errors: parse_json_vec::<E>(extra, "tool_protocol_errors_json")?,
            tool_events: parse_json_vec::<V>(extra, "tool_events_json")?,
        })
    }
}

pub fn parse_json_field<T>(extra: &BTreeMap<String, String>, key: &str) -> Result<Option<T>, String>
where
    T: DeserializeOwned,
{
    match extra.get(key) {
        Some(value) => serde_json::from_str(value)
            .map(Some)
            .map_err(|e| format!("{key}_parse_failed: {e}")),
        None => Ok(None),
    }
}

pub fn parse_json_vec<T>(extra: &BTreeMap<String, String>, key: &str) -> Result<Vec<T>, String>
where
    T: DeserializeOwned,
{
    Ok(parse_json_field::<Vec<T>>(extra, key)?.unwrap_or_default())
}

pub fn parse_json_value(
    extra: &BTreeMap<String, String>,
    key: &str,
) -> Result<Option<Value>, String> {
    parse_json_field::<Value>(extra, key)
}

pub fn parse_json_vec_value(
    extra: &BTreeMap<String, String>,
    key: &str,
) -> Result<Vec<Value>, String> {
    parse_json_vec::<Value>(extra, key)
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ToolProtocolErrorLite {
    code: String,
    message: String,
}

pub fn derive_tool_protocol_correction_context(extra: &BTreeMap<String, String>) -> Option<String> {
    let errors =
        parse_json_vec::<ToolProtocolErrorLite>(extra, "tool_protocol_errors_json").ok()?;
    let last = errors.last()?;
    let hint = match last.code.as_str() {
        "invalid_action_json" => {
            if last.message.contains("EOF") || last.message.contains("eof") {
                "上轮 ACTION JSON 在传输中被截断（EOF），工具未执行。下一轮请重新输出完整的 ACTION JSON；\
                 命令太长/转义太多时请拆成多条短 ACTION 分步执行，避免单条超长 JSON 被截断。"
            } else if last.message.contains("trailing text") {
                "上轮输出含 ACTION 后 trailing 文本。下一轮只输出一个结构：ACTION JSON 或 FINAL。"
            } else {
                "上轮 ACTION JSON 非法。下一轮只输出合法 ACTION JSON，或直接 FINAL。"
            }
        }
        "plain_text_response" => "上轮是普通文本。下一轮只能输出 ACTION JSON 或 FINAL。",
        "missing_action_prefix" => "上轮缺少 ACTION 前缀。下一轮请输出 ACTION: {...} 或 FINAL。",
        "invalid_legacy_tool_call_json" => {
            "上轮 TOOL_CALL JSON 非法。请改为 ACTION JSON，或直接 FINAL。"
        }
        "unsupported_action_schema_version" => {
            "上轮 ACTION schema_version 不支持。请用当前 schema_version。"
        }
        _ => "上轮工具协议不合规。下一轮只输出一个结构：ACTION JSON 或 FINAL。",
    };
    Some(hint.to_string())
}

pub fn derive_tool_protocol_typed_failure(
    extra: &BTreeMap<String, String>,
) -> Option<(String, String)> {
    let status = extra.get("tool_loop_status").map(|value| value.as_str())?;
    match status {
        "implicit_final_plain_text" | "tool_loop_exhausted" | "exhausted" => Some((
            "missing_final".to_string(),
            "tool loop exhausted without valid FINAL; only fallback plain text available"
                .to_string(),
        )),
        _ => None,
    }
}

pub fn collect_unified_execution_failure_classes(
    extra: &BTreeMap<String, String>,
) -> BTreeSet<String> {
    let mut classes = BTreeSet::new();
    if let Ok(Some(report)) = parse_json_value(extra, "tool_report_json") {
        append_unified_failure_classes_from_calls(
            &mut classes,
            report
                .get("calls")
                .and_then(|value| value.as_array())
                .map(Vec::as_slice)
                .unwrap_or(&[]),
        );
    }
    if let Ok(calls) = parse_json_vec_value(extra, "tool_calls_json") {
        append_unified_failure_classes_from_calls(&mut classes, &calls);
    }
    if let Ok(events) = parse_json_vec_value(extra, "tool_events_json") {
        append_unified_failure_classes_from_calls(&mut classes, &events);
    }
    classes
}

fn append_unified_failure_classes_from_calls(classes: &mut BTreeSet<String>, calls: &[Value]) {
    for call in calls {
        if call.get("ok").and_then(|value| value.as_bool()) == Some(false) {
            if let Some(class) = call.get("failure_class").and_then(|value| value.as_str()) {
                if is_unified_execution_failure_class(class) {
                    classes.insert(class.to_string());
                }
            }
        }
    }
}

fn is_unified_execution_failure_class(class: &str) -> bool {
    matches!(
        class,
        "adapter_unavailable" | "permission_denied" | "timeout" | "invalid_output"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        collect_unified_execution_failure_classes, derive_tool_protocol_typed_failure, ToolLoopMeta,
    };
    use std::collections::BTreeMap;

    #[test]
    fn parses_tool_loop_meta_from_runtime_extra() {
        let mut extra = BTreeMap::new();
        extra.insert("tool_call_count".to_string(), "2".to_string());
        extra.insert("tool_protocol_error_count".to_string(), "1".to_string());
        extra.insert("tool_trace".to_string(), "call trace".to_string());
        extra.insert(
            "tool_report_json".to_string(),
            r#"{"schema_version":6}"#.to_string(),
        );
        extra.insert(
            "tool_calls_json".to_string(),
            r#"[{"tool":"write_file"}]"#.to_string(),
        );
        extra.insert(
            "tool_protocol_errors_json".to_string(),
            r#"[{"code":"plain_text_response"}]"#.to_string(),
        );
        extra.insert(
            "tool_events_json".to_string(),
            r#"[{"kind":"tool_call"},{"kind":"protocol_error"}]"#.to_string(),
        );

        let meta = ToolLoopMeta::from_extra(&extra).expect("meta should parse");

        assert_eq!(meta.tool_call_count, 2);
        assert_eq!(meta.tool_protocol_error_count, 1);
        assert_eq!(meta.tool_trace, "call trace");
        assert!(meta.tool_report.is_some());
        assert_eq!(meta.tool_calls.len(), 1);
        assert_eq!(meta.tool_protocol_errors.len(), 1);
        assert_eq!(meta.tool_events.len(), 2);
    }

    #[test]
    fn completed_after_tool_limit_is_not_classified_as_missing_final() {
        let extra = BTreeMap::from([(
            "tool_loop_status".to_string(),
            "completed_after_tool_limit".to_string(),
        )]);

        assert_eq!(derive_tool_protocol_typed_failure(&extra), None);
    }

    #[test]
    fn collects_unified_execution_failures_from_runtime_extra() {
        let mut extra = BTreeMap::new();
        extra.insert(
            "tool_calls_json".to_string(),
            r#"[
                {"ok":false,"failure_class":"timeout"},
                {"ok":false,"failure_class":"nonzero_exit"}
            ]"#
            .to_string(),
        );
        extra.insert(
            "tool_events_json".to_string(),
            r#"[
                {"kind":"tool_call","ok":false,"failure_class":"permission_denied"},
                {"kind":"tool_call","ok":true,"failure_class":null}
            ]"#
            .to_string(),
        );

        let classes = collect_unified_execution_failure_classes(&extra);
        assert_eq!(
            classes.into_iter().collect::<Vec<_>>(),
            vec!["permission_denied".to_string(), "timeout".to_string()]
        );
    }
}
