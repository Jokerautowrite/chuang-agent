//! EmotionSlot：可拔插的情感槽位。
//!
//! 设计来源：jiwen（积温）五轴连续状态模型（MIT）。
//! - connection：连接需求（0..=1，多久没听到主人）
//! - pride：骄傲（-1..=1）
//! - valence：愉悦度（-1..=1，Russell 环状模型）
//! - arousal：唤醒度（-1..=1，Russell 环状模型）
//! - immersion：沉浸度（0..=1）
//!
//! 铁律：接口先行，实现第二；每个 slot 先 Fake 后真实；不依赖具体存储/模型。

use serde::{Deserialize, Serialize};

/// 当前时间 RFC3339（情感 tick/持久化统一使用，可排序可解析）。
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// 五轴情感状态。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EmotionAxes {
    pub connection: f64,
    pub pride: f64,
    pub valence: f64,
    pub arousal: f64,
    pub immersion: f64,
}

impl Default for EmotionAxes {
    fn default() -> Self {
        Self {
            connection: 0.0,
            pride: 0.0,
            valence: 0.0,
            arousal: 0.0,
            immersion: 0.0,
        }
    }
}

/// 情绪 delta：对话分析（轻量模型或规则）对五轴的修正量。
/// None 表示该轴不变。
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct EmotionDelta {
    pub connection: Option<f64>,
    pub pride: Option<f64>,
    pub valence: Option<f64>,
    pub arousal: Option<f64>,
    pub immersion: Option<f64>,
}

/// 阈值触发的动作（jiwen 语义）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum EmotionTrigger {
    /// 主动开口（想联系主人）
    Contact {
        urgency: f64,
        #[serde(default, skip_serializing_if = "is_false")]
        forced: bool,
    },
    /// 找事做/自我调节（逃避或宣泄）
    FindActivity {
        urgency: f64,
        reason: String,
    },
    /// 注意到沉默但还没想动（内心念头）
    Observation { urgency: f64 },
}

fn is_false(value: &bool) -> bool {
    !*value
}

/// 状态快照：供 context 注入、日志、跨会话持久化。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmotionStateSnapshot {
    pub axes: EmotionAxes,
    pub prompt_context: String,
    pub style_guidance: String,
    pub last_tick_at: Option<String>,
}

/// 槽位错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmotionSlotError {
    pub message: String,
}

impl EmotionSlotError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// EmotionSlot trait：可拔插的情感槽位。
pub trait EmotionSlot {
    /// 时间流逝（心跳）：返回阈值触发的动作。
    fn tick(&mut self, minutes_elapsed: f64) -> Result<Vec<EmotionTrigger>, EmotionSlotError>;
    /// 对话情绪修正：每轮对话后注入 delta。
    fn observe_delta(&mut self, delta: &EmotionDelta) -> Result<(), EmotionSlotError>;
    /// 当前状态快照（含人话描述，供 prompt 注入）。
    fn snapshot(&self) -> Result<EmotionStateSnapshot, EmotionSlotError>;
    /// 记录当前活动（沉浸度缓冲）。
    fn set_activity(&mut self, activity: &str, label: Option<&str>) -> Result<(), EmotionSlotError>;
    /// 重置连接需求（如刚回复完主人）。
    fn reset_connection(&mut self) -> Result<(), EmotionSlotError>;
}

fn clamp(value: f64, min: f64, max: f64) -> f64 {
    value.clamp(min, max)
}

/// Fake 实现：确定性的契约测试实现 + 真实实现接入前的占位。
#[derive(Debug, Clone)]
pub struct FakeEmotionSlot {
    axes: EmotionAxes,
    last_activity: Option<String>,
    last_tick_at: Option<String>,
    ticks: u64,
}

impl Default for FakeEmotionSlot {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeEmotionSlot {
    pub fn new() -> Self {
        Self {
            axes: EmotionAxes::default(),
            last_activity: None,
            last_tick_at: None,
            ticks: 0,
        }
    }

    pub fn with_axes(axes: EmotionAxes) -> Self {
        Self {
            axes,
            last_activity: None,
            last_tick_at: None,
            ticks: 0,
        }
    }

    /// 从持久化状态恢复（跨轮恢复情绪记忆）。
    pub fn from_persisted(axes: EmotionAxes, last_tick_at: Option<String>) -> Self {
        Self {
            axes,
            last_activity: None,
            last_tick_at,
            ticks: 0,
        }
    }

    fn describe_axes(&self) -> String {
        let mut parts = Vec::new();
        if self.axes.valence > 0.3 {
            parts.push("心情不错".to_string());
        } else if self.axes.valence < -0.3 {
            parts.push("心情低落".to_string());
        } else {
            parts.push("心情平稳".to_string());
        }
        if self.axes.arousal > 0.3 {
            parts.push("有些焦躁".to_string());
        } else if self.axes.arousal < -0.3 {
            parts.push("比较平静".to_string());
        }
        if self.axes.connection > 0.5 {
            parts.push("很想联系主人".to_string());
        } else if self.axes.connection > 0.2 {
            parts.push("有点想主人了".to_string());
        }
        parts.join("，")
    }
}

impl EmotionSlot for FakeEmotionSlot {
    fn tick(&mut self, minutes_elapsed: f64) -> Result<Vec<EmotionTrigger>, EmotionSlotError> {
        if minutes_elapsed < 0.0 {
            return Err(EmotionSlotError::new("tick minutes_elapsed must be >= 0"));
        }
        self.ticks += 1;
        self.last_tick_at = Some(now_rfc3339());
        // Fake：connection 随时间缓慢增长（确定性），阈值触发 observation。
        let growth = (minutes_elapsed * 0.0007).min(1.0);
        self.axes.connection = clamp(self.axes.connection + growth, 0.0, 1.0);
        let mut triggers = Vec::new();
        if self.axes.connection >= 0.5 {
            triggers.push(EmotionTrigger::Contact {
                urgency: (self.axes.connection - 0.5) / 0.5,
                forced: true,
            });
        } else if self.axes.connection >= 0.2 {
            triggers.push(EmotionTrigger::Observation {
                urgency: (self.axes.connection - 0.2) / 0.3,
            });
        }
        Ok(triggers)
    }

    fn observe_delta(&mut self, delta: &EmotionDelta) -> Result<(), EmotionSlotError> {
        if let Some(v) = delta.connection {
            self.axes.connection = clamp(self.axes.connection + v, 0.0, 1.0);
        }
        if let Some(v) = delta.pride {
            self.axes.pride = clamp(self.axes.pride + v, -1.0, 1.0);
        }
        if let Some(v) = delta.valence {
            self.axes.valence = clamp(self.axes.valence + v, -1.0, 1.0);
        }
        if let Some(v) = delta.arousal {
            self.axes.arousal = clamp(self.axes.arousal + v, -1.0, 1.0);
        }
        if let Some(v) = delta.immersion {
            self.axes.immersion = clamp(self.axes.immersion + v, 0.0, 1.0);
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<EmotionStateSnapshot, EmotionSlotError> {
        Ok(EmotionStateSnapshot {
            axes: self.axes,
            prompt_context: format!(
                "当前情绪状态：{}。连接需求={:.2}，骄傲={:.2}，愉悦度={:.2}，唤醒度={:.2}，沉浸度={:.2}",
                self.describe_axes(),
                self.axes.connection,
                self.axes.pride,
                self.axes.valence,
                self.axes.arousal,
                self.axes.immersion
            ),
            style_guidance: if self.axes.valence < -0.3 && self.axes.connection > 0.3 {
                "主人可能正需要陪伴：回应要温和、有耐心，不要过于活泼。".to_string()
            } else {
                "保持自然、真实的陪伴语气。".to_string()
            },
            last_tick_at: self.last_tick_at.clone(),
        })
    }

    fn set_activity(&mut self, activity: &str, _label: Option<&str>) -> Result<(), EmotionSlotError> {
        if activity.trim().is_empty() {
            return Err(EmotionSlotError::new("activity must not be empty"));
        }
        self.last_activity = Some(activity.to_string());
        self.axes.immersion = clamp(self.axes.immersion + 0.5, 0.0, 1.0);
        Ok(())
    }

    fn reset_connection(&mut self) -> Result<(), EmotionSlotError> {
        self.axes.connection = 0.0;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_fake() -> FakeEmotionSlot {
        FakeEmotionSlot::new()
    }

    #[test]
    fn fake_tick_returns_observation_then_contact_as_connection_grows() {
        let mut slot = default_fake();
        assert_eq!(slot.tick(1.0).expect("tick should work"), vec![]);
        // 0.0007/min * ~290 min = 0.203 -> observation
        let early = slot.tick(289.0).expect("tick should work");
        assert_eq!(early.len(), 1);
        assert!(matches!(early[0], EmotionTrigger::Observation { .. }));
        // 继续增长到 >= 0.5 -> contact
        let later = slot.tick(600.0).expect("tick should work");
        assert!(matches!(later[0], EmotionTrigger::Contact { .. }));
    }

    #[test]
    fn fake_observe_delta_moves_axes_and_clamps() {
        let mut slot = default_fake();
        slot.observe_delta(&EmotionDelta {
            valence: Some(0.8),
            ..Default::default()
        })
        .expect("observe should work");
        let snap = slot.snapshot().expect("snapshot should work");
        assert!((snap.axes.valence - 0.8).abs() < 1e-9);
        // 超大 delta 被 clamp 到 1.0
        slot.observe_delta(&EmotionDelta {
            valence: Some(10.0),
            ..Default::default()
        })
        .expect("observe should work");
        assert_eq!(slot.snapshot().unwrap().axes.valence, 1.0);
    }

    #[test]
    fn fake_snapshot_serializes_and_is_human_readable() {
        let mut slot = default_fake();
        slot.observe_delta(&EmotionDelta {
            valence: Some(-0.6),
            connection: Some(0.4),
            ..Default::default()
        })
        .expect("observe should work");
        let snap = slot.snapshot().expect("snapshot should work");
        let json = serde_json::to_string(&snap).expect("snapshot should serialize");
        assert!(json.contains("valence"));
        assert!(snap.prompt_context.contains("心情低落"));
        assert!(snap.prompt_context.contains("有点想主人了"));
    }

    #[test]
    fn fake_tick_rejects_negative_minutes() {
        let mut slot = default_fake();
        let err = slot.tick(-1.0).expect_err("negative minutes should fail");
        assert!(err.message.contains(">= 0"));
    }

    #[test]
    fn fake_reset_connection_clears_connection() {
        let mut slot = default_fake();
        slot.observe_delta(&EmotionDelta {
            connection: Some(0.9),
            ..Default::default()
        })
        .expect("observe should work");
        slot.reset_connection().expect("reset should work");
        assert_eq!(slot.snapshot().unwrap().axes.connection, 0.0);
    }

    #[test]
    fn fake_set_activity_raises_immersion() {
        let mut slot = default_fake();
        slot.set_activity("写代码", Some("deep-work")).expect("set activity");
        assert!(slot.snapshot().unwrap().axes.immersion > 0.0);
    }

    // ---- JiwenEmotionSlot（真实实现）----

    fn jiwen() -> JiwenEmotionSlot {
        JiwenEmotionSlot::default()
    }

    #[test]
    fn jiwen_connection_grows_over_time_and_triggers_observation_then_contact() {
        let mut slot = jiwen();
        // tick caps at 60 min per call (jiwen semantics); 0.0007/min
        // ~286 min -> 0.20 (observation), ~500 min -> 0.35 (contact)
        let mut got_observation = false;
        let mut got_contact = false;
        for _ in 0..12 {
            let triggers = slot.tick(60.0).expect("tick");
            for t in triggers {
                match t {
                    EmotionTrigger::Observation { .. } => got_observation = true,
                    EmotionTrigger::Contact { .. } => got_contact = true,
                    _ => {}
                }
            }
        }
        assert!(got_observation, "connection should pass observation threshold");
        assert!(got_contact, "connection should pass contact threshold");
    }

    #[test]
    fn jiwen_forced_contact_when_connection_past_force_threshold() {
        let mut slot = JiwenEmotionSlot::with_axes(
            JiwenEmotionConfig::default(),
            EmotionAxes {
                connection: 0.6,
                ..Default::default()
            },
        );
        let triggers = slot.tick(1.0).expect("tick");
        assert!(matches!(
            &triggers[0],
            EmotionTrigger::Contact { forced: true, .. }
        ));
    }

    #[test]
    fn jiwen_pride_blocks_contact_into_find_activity() {
        let mut slot = JiwenEmotionSlot::with_axes(
            JiwenEmotionConfig::default(),
            EmotionAxes {
                connection: 0.4,
                pride: 0.6,
                ..Default::default()
            },
        );
        let triggers = slot.tick(1.0).expect("tick");
        assert!(matches!(
            &triggers[0],
            EmotionTrigger::FindActivity { reason, .. } if reason == "pride_block"
        ));
    }

    #[test]
    fn jiwen_low_valence_triggers_find_activity_self_regulation() {
        let mut slot = JiwenEmotionSlot::with_axes(
            JiwenEmotionConfig {
                thresholds: JiwenThresholds {
                    valence_activity: -0.2,
                    ..Default::default()
                },
                ..Default::default()
            },
            EmotionAxes {
                valence: -0.5,
                ..Default::default()
            },
        );
        let triggers = slot.tick(1.0).expect("tick");
        assert!(matches!(
            &triggers[0],
            EmotionTrigger::FindActivity { reason, .. } if reason == "low_valence"
        ));
    }

    #[test]
    fn jiwen_valence_regresses_to_setpoint_after_delta() {
        let mut slot = jiwen();
        slot.observe_delta(&EmotionDelta {
            valence: Some(0.8),
            ..Default::default()
        })
        .expect("observe");
        assert!((slot.snapshot().unwrap().axes.valence - 0.8).abs() < 1e-9);
        // 长时间 tick：0.005/min 回归到 0（单次 tick 上限 60 分钟）
        for _ in 0..30 {
            slot.tick(60.0).expect("tick");
        }
        assert!(slot.snapshot().unwrap().axes.valence.abs() < 1e-9);
    }

    #[test]
    fn jiwen_arousal_rises_while_waiting_when_enabled() {
        let mut slot = JiwenEmotionSlot::new(JiwenEmotionConfig {
            rates: JiwenRates {
                arousal_connection_rise_threshold: 0.3,
                arousal_connection_rise_rate: 0.002,
                ..Default::default()
            },
            ..Default::default()
        });
        slot.observe_delta(&EmotionDelta {
            connection: Some(0.5),
            ..Default::default()
        })
        .expect("observe");
        slot.tick(60.0).expect("tick");
        let arousal = slot.snapshot().unwrap().axes.arousal;
        assert!(arousal > 0.0, "waiting should raise arousal, got {arousal}");
    }

    #[test]
    fn jiwen_set_activity_raises_immersion_and_decays() {
        let mut slot = jiwen();
        slot.set_activity("reading", Some("看书")).expect("activity");
        let snap = slot.snapshot().unwrap();
        assert!((snap.axes.immersion - 0.6).abs() < 1e-9);
        slot.tick(60.0).expect("tick");
        let after = slot.snapshot().unwrap();
        assert!(after.axes.immersion < 0.6 - 0.5);
    }

    #[test]
    fn jiwen_prompt_context_is_human_readable_after_delta() {
        let mut slot = jiwen();
        slot.observe_delta(&EmotionDelta {
            valence: Some(-0.6),
            connection: Some(0.6),
            ..Default::default()
        })
        .expect("observe");
        let snap = slot.snapshot().unwrap();
        assert!(snap.prompt_context.contains("心情低落"));
        assert!(snap.prompt_context.contains("很想联系主人"));
        assert!(snap.style_guidance.contains("温和"));
    }
}

// ---------------------------------------------------------------------------
// JiwenEmotionSlot：真实实现（移植 jiwen 五轴连续状态数学，MIT）
// 默认参数与上游一致：所有耦合参数默认关闭（向后兼容），按需开启。
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;

/// 漂移/衰减速率（每分钟），默认与 jiwen 一致。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JiwenRates {
    pub immersion_decay: f64,
    pub pride_regress: f64,
    pub accel_delay_minutes: f64,
    pub connection_accel: f64,
    pub valence_regress: f64,
    pub valence_setpoint: f64,
    pub valence_lock_threshold: f64,
    pub valence_lock_factor: f64,
    pub valence_connect_boost: f64,
    pub valence_connect_boost_threshold: f64,
    pub valence_connect_dampen: f64,
    pub valence_connect_dampen_threshold: f64,
    pub arousal_setpoint: f64,
    pub arousal_regress: f64,
    pub arousal_connection_rise_threshold: f64,
    pub arousal_connection_rise_rate: f64,
    pub pride_defend_threshold: f64,
    pub pride_defend_target: f64,
    pub pride_defend_rate: f64,
    pub pride_arousal_conflict_rate: f64,
    pub pride_erosion_rate: f64,
    pub activity_connection_relief: f64,
    pub immersion_dampen_connection: f64,
    pub valence_connection_drift_threshold: f64,
    pub valence_connection_drift_rate: f64,
}

impl Default for JiwenRates {
    fn default() -> Self {
        Self {
            immersion_decay: 0.010,
            pride_regress: 0.003,
            accel_delay_minutes: 0.0,
            connection_accel: 0.0,
            valence_regress: 0.005,
            valence_setpoint: 0.0,
            valence_lock_threshold: 1.0,
            valence_lock_factor: 1.0,
            valence_connect_boost: 0.0,
            valence_connect_boost_threshold: -0.2,
            valence_connect_dampen: 0.0,
            valence_connect_dampen_threshold: -0.4,
            arousal_setpoint: 0.0,
            arousal_regress: 0.005,
            arousal_connection_rise_threshold: 1.0,
            arousal_connection_rise_rate: 0.002,
            pride_defend_threshold: 1.0,
            pride_defend_target: 0.5,
            pride_defend_rate: 0.003,
            pride_arousal_conflict_rate: 0.0,
            pride_erosion_rate: 0.0,
            activity_connection_relief: 0.0,
            immersion_dampen_connection: 1.0,
            valence_connection_drift_threshold: 0.0,
            valence_connection_drift_rate: 0.0,
        }
    }
}

/// 阈值（默认与 jiwen 一致）。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct JiwenThresholds {
    pub observation: f64,
    pub consider_contact: f64,
    pub force_contact: f64,
    pub pride_block: f64,
    pub valence_activity: f64,
    pub arousal_agitation: f64,
}

impl Default for JiwenThresholds {
    fn default() -> Self {
        Self {
            observation: 0.20,
            consider_contact: 0.35,
            force_contact: 0.50,
            pride_block: 0.50,
            valence_activity: -1.0, // -1.0 = never
            arousal_agitation: 0.7,
        }
    }
}

/// 当前活动（沉浸度缓冲）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JiwenActivity {
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// JiwenEmotionSlot 配置。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JiwenEmotionConfig {
    pub rates: JiwenRates,
    pub thresholds: JiwenThresholds,
    /// 连接需求基础增长率（/min）。jiwen 默认 0.0007（无消息源时）。
    pub connection_rate_per_min: f64,
    /// 活动类型 → 初始沉浸度。
    pub immersion_map: BTreeMap<String, f64>,
}

impl Default for JiwenEmotionConfig {
    fn default() -> Self {
        let mut immersion_map = BTreeMap::new();
        immersion_map.insert("reading".to_string(), 0.6);
        immersion_map.insert("search".to_string(), 0.4);
        immersion_map.insert("browse".to_string(), 0.35);
        immersion_map.insert("observe".to_string(), 0.15);
        Self {
            rates: JiwenRates::default(),
            thresholds: JiwenThresholds::default(),
            connection_rate_per_min: 0.0007,
            immersion_map,
        }
    }
}

/// jiwen 数学的真实 EmotionSlot 实现（纯内存；持久化由上层接入）。
#[derive(Debug, Clone)]
pub struct JiwenEmotionSlot {
    axes: EmotionAxes,
    config: JiwenEmotionConfig,
    last_activity: Option<JiwenActivity>,
    last_tick_at: Option<String>,
}

impl Default for JiwenEmotionSlot {
    fn default() -> Self {
        Self::new(JiwenEmotionConfig::default())
    }
}

impl JiwenEmotionSlot {
    pub fn new(config: JiwenEmotionConfig) -> Self {
        Self {
            axes: EmotionAxes::default(),
            config,
            last_activity: None,
            last_tick_at: None,
        }
    }

    pub fn with_axes(config: JiwenEmotionConfig, axes: EmotionAxes) -> Self {
        Self {
            axes,
            config,
            last_activity: None,
            last_tick_at: None,
        }
    }

    /// 从持久化状态恢复（跨轮恢复情绪记忆 + 上次心跳时间）。
    pub fn from_persisted(
        config: JiwenEmotionConfig,
        axes: EmotionAxes,
        last_tick_at: Option<String>,
    ) -> Self {
        Self {
            axes,
            config,
            last_activity: None,
            last_tick_at,
        }
    }

    fn check_thresholds(&self) -> Vec<EmotionTrigger> {
        let t = &self.config.thresholds;
        let c = self.axes.connection;
        let p = self.axes.pride;
        let i = self.axes.immersion;
        let v = self.axes.valence;
        let a = self.axes.arousal;
        let mut triggers = Vec::new();

        if c >= t.observation && c < t.consider_contact {
            triggers.push(EmotionTrigger::Observation {
                urgency: (c - t.observation) / (t.consider_contact - t.observation),
            });
        }

        if c >= t.consider_contact && c < t.force_contact {
            if p >= t.pride_block {
                if i < 0.2 {
                    triggers.push(EmotionTrigger::FindActivity {
                        urgency: c - 0.30,
                        reason: "pride_block".to_string(),
                    });
                }
            } else {
                triggers.push(EmotionTrigger::Contact {
                    urgency: c - 0.30,
                    forced: false,
                });
            }
        }

        if c >= t.force_contact {
            triggers.push(EmotionTrigger::Contact {
                urgency: (c - 0.40).min(1.0),
                forced: true,
            });
        }

        if v <= t.valence_activity || a >= t.arousal_agitation {
            let already_finding = triggers
                .iter()
                .any(|tr| matches!(tr, EmotionTrigger::FindActivity { .. }));
            if !already_finding && i < 0.3 {
                let reason = if v <= t.valence_activity {
                    "low_valence"
                } else {
                    "high_arousal"
                };
                let mag = if v <= t.valence_activity { v.abs() } else { a };
                triggers.push(EmotionTrigger::FindActivity {
                    urgency: mag.min(1.0),
                    reason: reason.to_string(),
                });
            }
        }

        triggers
    }
}

impl EmotionSlot for JiwenEmotionSlot {
    fn tick(&mut self, minutes_elapsed: f64) -> Result<Vec<EmotionTrigger>, EmotionSlotError> {
        if minutes_elapsed < 0.0 {
            return Err(EmotionSlotError::new(
                "tick minutes_elapsed must be >= 0",
            ));
        }
        let mins = minutes_elapsed.min(60.0);
        let r = &self.config.rates;
        let t = &self.config.thresholds;

        // 1) connection：基础速率 × 加速 × valence 耦合 × 沉浸阻尼
        let accel_factor = if r.connection_accel > 0.0 {
            (1.0 + self.axes.connection).powf(r.connection_accel)
        } else {
            1.0
        };
        let mut valence_multiplier = 1.0;
        if r.valence_connect_dampen > 0.0
            && self.axes.valence < r.valence_connect_dampen_threshold
        {
            valence_multiplier = r.valence_connect_dampen;
        } else if r.valence_connect_boost > 0.0
            && self.axes.valence < r.valence_connect_boost_threshold
        {
            valence_multiplier = r.valence_connect_boost;
        }
        let immersion_factor = if r.immersion_dampen_connection > 0.0 {
            (1.0 - self.axes.immersion * r.immersion_dampen_connection).max(0.0)
        } else {
            1.0
        };
        let effective_rate = self.config.connection_rate_per_min
            * accel_factor
            * valence_multiplier
            * immersion_factor;
        self.axes.connection =
            clamp(self.axes.connection + effective_rate * mins, 0.0, 1.0);

        // 2) immersion 衰减（简化：按 tick 时长衰减，近似 jiwen 的 sinceActivity）
        if self.last_activity.is_some() {
            self.axes.immersion =
                clamp(self.axes.immersion - r.immersion_decay * mins, 0.0, 1.0);
            if self.axes.immersion <= 0.01 {
                self.last_activity = None;
                self.axes.immersion = 0.0;
            }
        }

        // 3) pride：防御（被冷落）或回归
        if self.axes.connection >= r.pride_defend_threshold {
            let target = clamp(r.pride_defend_target, -1.0, 1.0);
            if self.axes.pride < target {
                self.axes.pride =
                    target.min(self.axes.pride + r.pride_defend_rate * mins);
            } else if self.axes.pride > target {
                self.axes.pride =
                    target.max(self.axes.pride - r.pride_defend_rate * mins);
            }
        } else {
            if self.axes.pride > 0.0 {
                self.axes.pride =
                    (self.axes.pride - r.pride_regress * mins).max(0.0);
            } else if self.axes.pride < 0.0 {
                self.axes.pride =
                    (self.axes.pride + r.pride_regress * mins).min(0.0);
            }
        }
        // 4) pride 侵蚀（想念太重撑不住）
        if r.pride_erosion_rate > 0.0
            && self.axes.connection >= t.force_contact
            && self.axes.pride > 0.0
        {
            self.axes.pride = (self.axes.pride - r.pride_erosion_rate * mins).max(0.0);
        }

        // 5) valence 回归（connection 高时锁定 → 回归变慢）
        let valence_regress_rate = if self.axes.connection >= r.valence_lock_threshold {
            r.valence_regress * r.valence_lock_factor
        } else {
            r.valence_regress
        };
        let setpoint = clamp(r.valence_setpoint, -1.0, 1.0);
        if self.axes.valence > setpoint {
            self.axes.valence = (self.axes.valence - valence_regress_rate * mins).max(setpoint);
        } else if self.axes.valence < setpoint {
            self.axes.valence = (self.axes.valence + valence_regress_rate * mins).min(setpoint);
        }
        // 6) connection 驱动的 valence 下沉
        if r.valence_connection_drift_rate > 0.0
            && self.axes.connection >= r.valence_connection_drift_threshold
        {
            let drift = r.valence_connection_drift_rate * mins * self.axes.connection;
            self.axes.valence = clamp(self.axes.valence - drift, -1.0, 1.0);
        }

        // 7) arousal：回归力 + 上升力（等待焦躁）+ pride 冲突，两力竞争
        let mut arousal_regress_force = 0.0;
        if self.axes.arousal > r.arousal_setpoint {
            arousal_regress_force = -r.arousal_regress * mins;
        } else if self.axes.arousal < r.arousal_setpoint {
            arousal_regress_force = r.arousal_regress * mins;
        }
        let mut arousal_rise_force = 0.0;
        if self.axes.connection >= r.arousal_connection_rise_threshold {
            arousal_rise_force = r.arousal_connection_rise_rate * mins;
        }
        if r.pride_arousal_conflict_rate > 0.0
            && self.axes.connection >= t.consider_contact
            && self.axes.pride >= t.pride_block
        {
            arousal_rise_force += r.pride_arousal_conflict_rate * mins;
        }
        let net_arousal = self.axes.arousal + arousal_regress_force + arousal_rise_force;
        if arousal_regress_force < 0.0
            && net_arousal < r.arousal_setpoint
            && arousal_rise_force == 0.0
        {
            self.axes.arousal = r.arousal_setpoint;
        } else if arousal_regress_force > 0.0
            && net_arousal > r.arousal_setpoint
            && arousal_rise_force == 0.0
        {
            self.axes.arousal = r.arousal_setpoint;
        } else {
            self.axes.arousal = clamp(net_arousal, -1.0, 1.0);
        }

        self.last_tick_at = Some(now_rfc3339());
        Ok(self.check_thresholds())
    }

    fn observe_delta(&mut self, delta: &EmotionDelta) -> Result<(), EmotionSlotError> {
        if let Some(v) = delta.connection {
            self.axes.connection = clamp(self.axes.connection + v, 0.0, 1.0);
        }
        if let Some(v) = delta.pride {
            self.axes.pride = clamp(self.axes.pride + v, -1.0, 1.0);
        }
        if let Some(v) = delta.valence {
            self.axes.valence = clamp(self.axes.valence + v, -1.0, 1.0);
        }
        if let Some(v) = delta.arousal {
            self.axes.arousal = clamp(self.axes.arousal + v, -1.0, 1.0);
        }
        if let Some(v) = delta.immersion {
            self.axes.immersion = clamp(self.axes.immersion + v, 0.0, 1.0);
        }
        Ok(())
    }

    fn snapshot(&self) -> Result<EmotionStateSnapshot, EmotionSlotError> {
        let axes = self.axes;
        let mut parts = Vec::new();
        if axes.valence > 0.3 {
            parts.push("心情不错".to_string());
        } else if axes.valence < -0.3 {
            parts.push("心情低落".to_string());
        } else {
            parts.push("心情平稳".to_string());
        }
        if axes.arousal > 0.3 {
            parts.push("有些焦躁".to_string());
        } else if axes.arousal < -0.3 {
            parts.push("比较平静".to_string());
        }
        if axes.connection > 0.5 {
            parts.push("很想联系主人".to_string());
        } else if axes.connection > 0.2 {
            parts.push("有点想主人了".to_string());
        }
        if axes.immersion > 0.4 {
            parts.push("正沉浸在某件事里".to_string());
        }
        Ok(EmotionStateSnapshot {
            axes,
            prompt_context: format!(
                "当前情绪状态：{}。连接需求={:.2}，骄傲={:.2}，愉悦度={:.2}，唤醒度={:.2}，沉浸度={:.2}",
                parts.join("，"),
                axes.connection,
                axes.pride,
                axes.valence,
                axes.arousal,
                axes.immersion
            ),
            style_guidance: if axes.valence < -0.3 && axes.connection > 0.3 {
                "主人可能正需要陪伴：回应要温和、有耐心，不要过于活泼。".to_string()
            } else if axes.valence > 0.3 && axes.arousal > 0.3 {
                "主人情绪高涨：可以一起开心，节奏轻快一些。".to_string()
            } else {
                "保持自然、真实的陪伴语气。".to_string()
            },
            last_tick_at: self.last_tick_at.clone(),
        })
    }

    fn set_activity(&mut self, activity: &str, label: Option<&str>) -> Result<(), EmotionSlotError> {
        if activity.trim().is_empty() {
            return Err(EmotionSlotError::new("activity must not be empty"));
        }
        let same_type = self
            .last_activity
            .as_ref()
            .map(|a| a.kind == activity)
            .unwrap_or(false);
        self.last_activity = Some(JiwenActivity {
            kind: activity.to_string(),
            label: label.map(String::from),
        });
        self.axes.immersion = self
            .config
            .immersion_map
            .get(activity)
            .copied()
            .unwrap_or(0.2);
        if self.config.rates.activity_connection_relief > 0.0 && !same_type {
            self.axes.connection = (self.axes.connection
                - self.config.rates.activity_connection_relief)
                .max(0.01);
        }
        Ok(())
    }

    fn reset_connection(&mut self) -> Result<(), EmotionSlotError> {
        self.axes.connection = 0.0;
        Ok(())
    }
}
