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
    Contact { urgency: f64 },
    /// 找事做/自我调节（逃避或宣泄）
    FindActivity { urgency: f64 },
    /// 注意到沉默但还没想动（内心念头）
    Observation { urgency: f64 },
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
        self.last_tick_at = Some("now".to_string());
        // Fake：connection 随时间缓慢增长（确定性），阈值触发 observation。
        let growth = (minutes_elapsed * 0.0007).min(1.0);
        self.axes.connection = clamp(self.axes.connection + growth, 0.0, 1.0);
        let mut triggers = Vec::new();
        if self.axes.connection >= 0.5 {
            triggers.push(EmotionTrigger::Contact {
                urgency: (self.axes.connection - 0.5) / 0.5,
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
}
