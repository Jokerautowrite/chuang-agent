use std::collections::BTreeMap;

use serde::de::DeserializeOwned;
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

#[cfg(test)]
mod tests {
    use super::ToolLoopMeta;
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
}
