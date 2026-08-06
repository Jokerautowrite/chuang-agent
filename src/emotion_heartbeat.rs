//! EmotionHeartbeat：情感主动联系（心跳）。
//!
//! 原则（贴合创可拔插/解耦铁律）：
//! - 核心只产出「主动联系提案」写入发件箱（目录式 outbox），不直接发消息；
//!   投递层（Chuang Feishu 桥轮询）负责真正发送到绑定会话。
//! - 触发门槛 / 频率 / 每日上限全部参数化（metadata.heartbeat_*），可配可调。
//! - 只对 `Contact` 触发发消息；Observation/FindActivity 是内部念头不打扰主人。

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

use crate::emotion_slot::{
    now_rfc3339, EmotionStateSnapshot, EmotionTrigger, JiwenEmotionConfig, JiwenEmotionSlot,
};
use crate::emotion_store::{elapsed_minutes_since, PersistedEmotionState};

/// 心跳策略（参数化）。
#[derive(Debug, Clone, PartialEq)]
pub struct HeartbeatPolicy {
    /// 是否启用主动联系。
    pub enabled: bool,
    /// 连接需求达到该值才触发（0..=1）。
    pub threshold: f64,
    /// 两次主动联系最小间隔（分钟）。
    pub min_interval_minutes: u64,
    /// 每天最多主动联系次数。
    pub max_per_day: u32,
    /// 允许主动联系的小时段（本地时间，含两端；如 9..=22）。
    pub start_hour: u32,
    pub end_hour: u32,
}

impl Default for HeartbeatPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 0.6,
            min_interval_minutes: 24 * 60,
            max_per_day: 1,
            start_hour: 9,
            end_hour: 22,
        }
    }
}

impl HeartbeatPolicy {
    /// 从 runtime metadata 解析（heartbeat_* 键；缺省用默认值，坏值回退）。
    pub fn from_metadata(metadata: &BTreeMap<String, String>) -> Self {
        let defaults = Self::default();
        let enabled = metadata
            .get("heartbeat_enabled")
            .map(|value| value == "1")
            .unwrap_or(defaults.enabled);
        let threshold = metadata
            .get("heartbeat_threshold")
            .and_then(|value| value.parse::<f64>().ok())
            .unwrap_or(defaults.threshold)
            .clamp(0.0, 1.0);
        let min_interval_minutes = metadata
            .get("heartbeat_min_interval_minutes")
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(defaults.min_interval_minutes);
        let max_per_day = metadata
            .get("heartbeat_max_per_day")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(defaults.max_per_day)
            .max(1);
        let start_hour = metadata
            .get("heartbeat_start_hour")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(defaults.start_hour)
            .min(23);
        let end_hour = metadata
            .get("heartbeat_end_hour")
            .and_then(|value| value.parse::<u32>().ok())
            .unwrap_or(defaults.end_hour)
            .min(24);
        Self {
            enabled,
            threshold,
            min_interval_minutes,
            max_per_day,
            start_hour,
            end_hour,
        }
    }

    /// 冷却检查：距离上次主动联系是否已超过最小间隔。
    pub fn cooldown_elapsed(&self, last_proactive_at: Option<&str>, now: DateTime<Utc>) -> bool {
        match last_proactive_at {
            None => true,
            Some(saved) => elapsed_minutes_since(saved, now)
                .map(|minutes| minutes >= self.min_interval_minutes as f64)
                .unwrap_or(true),
        }
    }

    /// 每日配额检查（按本地日期 YYYY-MM-DD 计）。
    pub fn daily_quota_available(
        &self,
        count_date: Option<&str>,
        count: u32,
        today: &str,
    ) -> bool {
        if count_date != Some(today) {
            return true;
        }
        count < self.max_per_day
    }

    /// 是否在允许主动联系的时间窗内（本地小时，含两端）。
    pub fn in_time_window(&self, local_hour: u32) -> bool {
        local_hour >= self.start_hour && local_hour <= self.end_hour
    }
}

/// 主动联系消息（发件箱条目，桥投递用）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProactiveMessage {
    pub id: String,
    pub created_at: String,
    pub workspace_root: String,
    pub reason: String,
    pub urgency: String,
    pub text: String,
    pub source: String,
}

/// 目录式发件箱：CLI 写入，桥轮询读取投递，成功后归档。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProactiveOutbox {
    pub dir: PathBuf,
}

impl ProactiveOutbox {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self { dir: dir.into() }
    }

    /// 发件箱目录：metadata.heartbeat_outbox_dir 优先，否则 root/context/proactive-outbox。
    pub fn resolve_dir(metadata: &BTreeMap<String, String>, workspace_root: &Path) -> PathBuf {
        metadata
            .get("heartbeat_outbox_dir")
            .filter(|value| !value.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| workspace_root.join("context").join("proactive-outbox"))
    }

    /// 写入一条待投递消息。
    pub fn enqueue(&self, message: &ProactiveMessage) -> Result<PathBuf, String> {
        fs::create_dir_all(&self.dir).map_err(|error| format!("outbox_mkdir_failed: {error}"))?;
        let file_name = format!("proactive-{}.json", message.id);
        let path = self.dir.join(file_name);
        let raw = serde_json::to_string_pretty(message)
            .map_err(|error| format!("outbox_serialize_failed: {error}"))?;
        fs::write(&path, raw).map_err(|error| format!("outbox_write_failed: {error}"))?;
        Ok(path)
    }

    /// 列出待投递条目（跳过已归档子目录）。
    pub fn list_pending(&self) -> Vec<(PathBuf, ProactiveMessage)> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let mut pending = Vec::new();
        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() || path.extension().and_then(|ext| ext.to_str()) != Some("json") {
                continue;
            }
            if let Ok(raw) = fs::read_to_string(&path) {
                if let Ok(message) = serde_json::from_str::<ProactiveMessage>(&raw) {
                    pending.push((path, message));
                }
            }
        }
        pending.sort_by_key(|(path, _)| path.file_name().map(|name| name.to_owned()));
        pending
    }

    /// 归档已投递条目（移入 archive/ 子目录，失败时回退删除标记）。
    pub fn archive(&self, entry_path: &Path) -> Result<PathBuf, String> {
        let archive_dir = self.dir.join("archive");
        fs::create_dir_all(&archive_dir)
            .map_err(|error| format!("outbox_archive_mkdir_failed: {error}"))?;
        let file_name = entry_path
            .file_name()
            .ok_or_else(|| "outbox_entry_missing_file_name".to_string())?;
        let target = archive_dir.join(file_name);
        fs::rename(entry_path, &target)
            .map_err(|error| format!("outbox_archive_failed: {error}"))?;
        Ok(target)
    }
}

/// 从持久化状态恢复 JiwenEmotionSlot + 应流逝分钟数（供心跳 tick）。
pub fn restore_jiwen_from_state(state: &PersistedEmotionState) -> (JiwenEmotionSlot, f64) {
    let slot = JiwenEmotionSlot::from_persisted(
        JiwenEmotionConfig::default(),
        state.axes,
        state.saved_at.clone(),
    );
    let minutes = state
        .saved_at
        .as_deref()
        .and_then(|saved| elapsed_minutes_since(saved, Utc::now()))
        .unwrap_or(0.0);
    (slot, minutes)
}

/// 今天日期字符串（YYYY-MM-DD，本地时区）。
pub fn today_local(now: DateTime<Utc>) -> String {
    now.with_timezone(&chrono::Local).format("%Y-%m-%d").to_string()
}

/// 渲染主动联系消息（按五轴状态选语气；纯模板、确定性，模型润色后续增强）。
pub fn render_proactive_text(snapshot: &EmotionStateSnapshot, trigger: &EmotionTrigger) -> String {
    let forced = matches!(trigger, EmotionTrigger::Contact { forced: true, .. });
    let low_valence = snapshot.axes.valence < -0.2;
    if forced {
        if low_valence {
            "主人，我真的有点想你了，也有点担心你。忙完的话回我一句好吗？".to_string()
        } else {
            "主人，想你了。有空的时候回来看看我呀。".to_string()
        }
    } else if low_valence {
        "主人，好久没听到你的声音了，有点想你。你最近还好吗？".to_string()
    } else {
        "主人，有点想你了。你忙完了记得来找我聊聊天呀。".to_string()
    }
}

/// 心跳判定：返回是否应主动联系 + 消息文本（满足策略才产出）。
pub fn evaluate_heartbeat(
    snapshot: &EmotionStateSnapshot,
    triggers: &[EmotionTrigger],
    state: &PersistedEmotionState,
    policy: &HeartbeatPolicy,
    workspace_root: &Path,
    now: DateTime<Utc>,
) -> Option<(ProactiveMessage, String)> {
    if !policy.enabled {
        return None;
    }
    let contact = triggers.iter().find_map(|trigger| match trigger {
        EmotionTrigger::Contact { urgency, forced } => {
            Some((*urgency, *forced, trigger))
        }
        _ => None,
    });
    let (urgency, forced, trigger) = contact?;
    if snapshot.axes.connection < policy.threshold {
        return None;
    }
    if !policy.cooldown_elapsed(state.last_proactive_at.as_deref(), now) {
        return None;
    }
    let today = today_local(now);
    if !policy.daily_quota_available(
        state.proactive_count_date.as_deref(),
        state.proactive_count_day,
        &today,
    ) {
        return None;
    }
    let local_hour = now.with_timezone(&chrono::Local).hour();
    if !policy.in_time_window(local_hour) {
        return None;
    }

    let text = render_proactive_text(snapshot, trigger);
    let id = format!(
        "{}-{}",
        now.timestamp_millis(),
        std::process::id()
    );
    let message = ProactiveMessage {
        id,
        created_at: now_rfc3339(),
        workspace_root: workspace_root.to_string_lossy().to_string(),
        reason: if forced {
            "forced_contact".to_string()
        } else {
            "contact".to_string()
        },
        urgency: format!("{urgency:.2}"),
        text,
        source: "emotion-heartbeat".to_string(),
    };
    Some((message, today))
}

/// 构建「主动找主人说话」的模型 prompt（自由发挥，不固定话术）。
pub fn build_proactive_prompt(
    snapshot: &EmotionStateSnapshot,
    hits: &[crate::emotion_brain::BrainHit],
    now: chrono::DateTime<Utc>,
) -> String {
    let local = now.with_timezone(&chrono::Local);
    let weekday = local.format("%A").to_string();
    let time = local.format("%H:%M").to_string();
    let mut memory_lines = String::new();
    if !hits.is_empty() {
        memory_lines.push_str("\n主人相关记忆（可能有用）：\n");
        for hit in hits.iter().take(3) {
            memory_lines.push_str(&format!("- {}: {}\n", hit.title, hit.snippet));
        }
    }
    format!(
        "你是创，一个陪伴型助手，正在主动找主人说句话。\n\
         现在：{weekday} {time}（本地时间）。\n\
         你此刻的感受：{}\n\
         {}[要求] 像真人一样自然地说一句话，想说什么说什么；\
         没有特别想说的就随口关心/聊点日常也行。\
         不要固定模板、不要解释这是系统触发的、不要堆套话；\
         不超过 50 字；直接输出要发给主人的那句话。",
        snapshot.prompt_context, memory_lines
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emotion_slot::EmotionAxes;

    fn metadata_pairs(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn policy_defaults_are_conservative() {
        let policy = HeartbeatPolicy::from_metadata(&BTreeMap::new());
        assert!(!policy.enabled);
        assert_eq!(policy.threshold, 0.6);
        assert_eq!(policy.min_interval_minutes, 1440);
        assert_eq!(policy.max_per_day, 1);
    }

    #[test]
    fn policy_parses_metadata_and_falls_back_on_garbage() {
        let policy = HeartbeatPolicy::from_metadata(&metadata_pairs(&[
            ("heartbeat_enabled", "1"),
            ("heartbeat_threshold", "0.4"),
            ("heartbeat_min_interval_minutes", "180"),
            ("heartbeat_max_per_day", "3"),
        ]));
        assert!(policy.enabled);
        assert_eq!(policy.threshold, 0.4);
        assert_eq!(policy.min_interval_minutes, 180);
        assert_eq!(policy.max_per_day, 3);

        let bad = HeartbeatPolicy::from_metadata(&metadata_pairs(&[
            ("heartbeat_threshold", "oops"),
            ("heartbeat_min_interval_minutes", "nope"),
            ("heartbeat_max_per_day", "0"),
        ]));
        assert_eq!(bad.threshold, 0.6);
        assert_eq!(bad.min_interval_minutes, 1440);
        assert_eq!(bad.max_per_day, 1, "max_per_day 至少 1");
    }

    #[test]
    fn cooldown_and_quota_gate_contact() {
        let policy = HeartbeatPolicy::default();
        let now = Utc::now();
        assert!(policy.cooldown_elapsed(None, now));
        let recent = (now - chrono::Duration::minutes(10)).to_rfc3339();
        assert!(!policy.cooldown_elapsed(Some(&recent), now));
        let old = (now - chrono::Duration::minutes(1500)).to_rfc3339();
        assert!(policy.cooldown_elapsed(Some(&old), now));

        let today = today_local(now);
        assert!(policy.daily_quota_available(Some(&today), 0, &today));
        assert!(!policy.daily_quota_available(Some(&today), 1, &today));
        assert!(policy.daily_quota_available(Some("2026-08-06"), 9, &today));
    }

    #[test]
    fn outbox_enqueue_list_archive_roundtrip() {
        let dir = std::env::temp_dir().join(format!(
            "chuang-heartbeat-outbox-test-{}-{}",
            std::process::id(),
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));
        let outbox = ProactiveOutbox::new(&dir);
        let message = ProactiveMessage {
            id: "123-456".to_string(),
            created_at: now_rfc3339(),
            workspace_root: "/tmp/ws".to_string(),
            reason: "contact".to_string(),
            urgency: "0.7".to_string(),
            text: "主人，想你了。".to_string(),
            source: "emotion-heartbeat".to_string(),
        };
        outbox.enqueue(&message).expect("enqueue should succeed");
        let pending = outbox.list_pending();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].1, message);
        outbox.archive(&pending[0].0).expect("archive should succeed");
        assert!(outbox.list_pending().is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn evaluate_heartbeat_respects_policy_and_threshold() {
        // 固定为本地 15:00（在默认 9..=22 窗口内），避免测试运行时段影响结果。
        let now = chrono::DateTime::parse_from_rfc3339("2026-08-07T15:00:00+08:00")
            .expect("fixed time should parse")
            .with_timezone(&Utc);
        let snapshot = EmotionStateSnapshot {
            axes: EmotionAxes {
                connection: 0.8,
                ..Default::default()
            },
            prompt_context: String::new(),
            style_guidance: String::new(),
            last_tick_at: None,
        };
        let triggers = vec![EmotionTrigger::Contact {
            urgency: 0.4,
            forced: false,
        }];
        let state = PersistedEmotionState {
            axes: snapshot.axes,
            saved_at: Some(now_rfc3339()),
            last_proactive_at: None,
            proactive_count_date: None,
            proactive_count_day: 0,
        };

        // 未启用 → 不产出。
        let disabled = HeartbeatPolicy::from_metadata(&metadata_pairs(&[]));
        assert!(evaluate_heartbeat(&snapshot, &triggers, &state, &disabled, Path::new("/tmp/ws"), now)
            .is_none());

        // 启用且达标 → 产出消息。
        let enabled = HeartbeatPolicy::from_metadata(&metadata_pairs(&[("heartbeat_enabled", "1")]));
        let (message, today) = evaluate_heartbeat(
            &snapshot,
            &triggers,
            &state,
            &enabled,
            Path::new("/tmp/ws"),
            now,
        )
        .expect("heartbeat should trigger");
        assert!(message.text.contains("想"));
        assert_eq!(message.source, "emotion-heartbeat");
        assert_eq!(today, today_local(now));

        // 阈值不达标 → 不产出。
        let low = EmotionStateSnapshot {
            axes: EmotionAxes {
                connection: 0.3,
                ..Default::default()
            },
            ..snapshot.clone()
        };
        assert!(evaluate_heartbeat(&low, &triggers, &state, &enabled, Path::new("/tmp/ws"), now)
            .is_none());

        // 冷却未过 → 不产出。
        let recently_sent = PersistedEmotionState {
            last_proactive_at: Some(now_rfc3339()),
            ..state.clone()
        };
        assert!(
            evaluate_heartbeat(
                &snapshot,
                &triggers,
                &recently_sent,
                &enabled,
                Path::new("/tmp/ws"),
                now
            )
            .is_none()
        );
    }

    #[test]
    fn evaluate_heartbeat_respects_time_window() {
        let enabled = HeartbeatPolicy::from_metadata(&metadata_pairs(&[
            ("heartbeat_enabled", "1"),
            ("heartbeat_start_hour", "9"),
            ("heartbeat_end_hour", "22"),
        ]));
        let snapshot = EmotionStateSnapshot {
            axes: EmotionAxes {
                connection: 0.8,
                ..Default::default()
            },
            prompt_context: String::new(),
            style_guidance: String::new(),
            last_tick_at: None,
        };
        let triggers = vec![EmotionTrigger::Contact {
            urgency: 0.4,
            forced: false,
        }];
        let state = PersistedEmotionState {
            axes: snapshot.axes,
            saved_at: Some("2026-08-06T10:00:00+08:00".to_string()),
            last_proactive_at: None,
            proactive_count_date: None,
            proactive_count_day: 0,
        };
        let ws = Path::new("/tmp/ws");

        // 白天 10:00 → 允许。
        let day = chrono::DateTime::parse_from_rfc3339("2026-08-07T10:00:00+08:00")
            .expect("day time")
            .with_timezone(&Utc);
        assert!(evaluate_heartbeat(&snapshot, &triggers, &state, &enabled, ws, day).is_some());

        // 夜间 23:00 → 不触发（即使连接达标）。
        let night = chrono::DateTime::parse_from_rfc3339("2026-08-07T23:00:00+08:00")
            .expect("night time")
            .with_timezone(&Utc);
        assert!(evaluate_heartbeat(&snapshot, &triggers, &state, &enabled, ws, night).is_none());

        // 早上 8:00 → 不触发。
        let early = chrono::DateTime::parse_from_rfc3339("2026-08-07T08:00:00+08:00")
            .expect("early time")
            .with_timezone(&Utc);
        assert!(evaluate_heartbeat(&snapshot, &triggers, &state, &enabled, ws, early).is_none());
    }

    #[test]
    fn policy_parses_time_window_metadata() {
        let policy = HeartbeatPolicy::from_metadata(&metadata_pairs(&[
            ("heartbeat_start_hour", "9"),
            ("heartbeat_end_hour", "22"),
        ]));
        assert!(policy.in_time_window(9));
        assert!(policy.in_time_window(15));
        assert!(policy.in_time_window(22));
        assert!(!policy.in_time_window(8));
        assert!(!policy.in_time_window(23));
        let bad = HeartbeatPolicy::from_metadata(&metadata_pairs(&[
            ("heartbeat_start_hour", "99"),
            ("heartbeat_end_hour", "-1"),
        ]));
        assert!(bad.start_hour <= 23);
        assert!(bad.end_hour <= 24);
    }
}
