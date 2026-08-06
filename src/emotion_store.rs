//! EmotionStore：情感状态跨轮持久化（可拔插）。
//!
//! 原则：情感核心不依赖具体存储；这里提供最简 JSON 文件实现。
//! - 位置：与 db_path 同目录的 `emotion-state.json`（主人/身份维度全局，不按 session 分片）。
//! - 保存：每轮 turn 结束后 snapshot() → PersistedEmotionState。
//! - 恢复：启动时读回 axes + 上次心跳时间，用真实流逝分钟数 tick（jiwen 连接增长）。
//! - 失败永远静默（load 失败用默认状态继续，save 失败只记日志，不阻断主流程）。

use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::emotion_slot::EmotionAxes;

/// 持久化的情感状态（只存状态与时间，prompt_context/style_guidance 由 snapshot 重算）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PersistedEmotionState {
    pub axes: EmotionAxes,
    /// RFC3339 上次心跳时间。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_at: Option<String>,
}

/// 情感状态文件存储。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmotionStateFile {
    pub path: PathBuf,
}

impl EmotionStateFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// 读取持久化状态；文件不存在返回 Ok(None)，解析失败返回 Err。
    pub fn load(&self) -> Result<Option<PersistedEmotionState>, String> {
        let raw = match fs::read_to_string(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(format!("emotion_state_read_failed: {error}")),
        };
        serde_json::from_str(&raw)
            .map(Some)
            .map_err(|error| format!("emotion_state_parse_failed: {error}"))
    }

    /// 写回持久化状态；失败返回 Err（由调用方决定是否静默）。
    pub fn save(&self, state: &PersistedEmotionState) -> Result<(), String> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("emotion_state_mkdir_failed: {error}"))?;
        }
        let raw = serde_json::to_string_pretty(state)
            .map_err(|error| format!("emotion_state_serialize_failed: {error}"))?;
        fs::write(&self.path, raw).map_err(|error| format!("emotion_state_write_failed: {error}"))
    }
}

/// 情感状态文件路径：与 db_path 同目录。
pub fn resolve_emotion_state_path(db_path: &Path) -> PathBuf {
    db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("emotion-state.json")
}

/// RFC3339 → 距 now 的分钟数（解析失败返回 None）。
pub fn elapsed_minutes_since(rfc3339: &str, now: DateTime<Utc>) -> Option<f64> {
    let saved = DateTime::parse_from_rfc3339(rfc3339)
        .ok()?
        .with_timezone(&Utc);
    let seconds = (now - saved).num_milliseconds().max(0) as f64 / 1000.0;
    Some(seconds / 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "chuang-emotion-store-{}-{}-{name}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ))
    }

    #[test]
    fn save_load_roundtrip() {
        let path = temp_path("roundtrip");
        let store = EmotionStateFile::new(&path);
        let state = PersistedEmotionState {
            axes: EmotionAxes {
                connection: 0.1,
                pride: 0.2,
                valence: 0.3,
                arousal: 0.4,
                immersion: 0.5,
            },
            saved_at: Some("2026-08-07T10:00:00+08:00".to_string()),
        };
        store.save(&state).expect("save should succeed");
        let loaded = store
            .load()
            .expect("load should succeed")
            .expect("state exists");
        assert_eq!(loaded, state);
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn load_missing_file_returns_none() {
        let store = EmotionStateFile::new(temp_path("missing"));
        assert_eq!(store.load().expect("missing should be ok"), None);
    }

    #[test]
    fn load_malformed_file_returns_error() {
        let path = temp_path("malformed");
        fs::write(&path, "not json").expect("write malformed");
        let store = EmotionStateFile::new(&path);
        assert!(store.load().is_err());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn resolve_path_sits_next_to_db() {
        let path = resolve_emotion_state_path(Path::new("/tmp/x/data/chuang-agent.db"));
        assert_eq!(path, PathBuf::from("/tmp/x/data/emotion-state.json"));
    }

    #[test]
    fn elapsed_minutes_computes_positive_delta() {
        let now = Utc::now();
        let past = (now - chrono::Duration::minutes(90)).to_rfc3339();
        let minutes = elapsed_minutes_since(&past, now).expect("parse should work");
        assert!(minutes > 89.0 && minutes < 91.0);
    }

    #[test]
    fn elapsed_minutes_rejects_garbage() {
        assert_eq!(elapsed_minutes_since("not-a-time", Utc::now()), None);
    }
}
