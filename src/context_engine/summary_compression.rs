//! `context_engine::summary_compression` 模块。
//! 公开接口：struct SummaryCompressionContextEngine, CompactionCircuitBreakerStatus；
//! enum CompactionStrategy；fn new, with_recent_turns, with_compaction_strategy,
//! with_circuit_breaker, reset_circuit_breaker, circuit_breaker_status,
//! compaction_strategy, strip_image_payloads。
//!
//! 升级蓝本（docs/reference-dig-20260810.md §2.3 上下文压缩状态机）：
//! - 策略级联：Snip → Micro → Collapse → Auto → Session Memory。现有实现 = Auto 级；
//!   Snip（丢旧工具输出）由 ContextPacker::trim_segments 承担，Session Memory（与记忆去重）
//!   由 ContextPacker::deduplicate_segments 承担，Collapse（多轮相似工具合并）暂为占位降级点。
//! - 熔断器：连续 3 次自动压缩失败停止重试（防浪费无效调用），可手动重置或按冷却自动复位。
//! - 压缩前 strip images：把 data URL / image_url 替换为文本占位引用，避免压缩请求超长；
//!   压缩完成后不回填（压缩摘要本来就不需要原图）。
//! - 递归保护：压缩/总结路径产物（compaction_source / compacted / summary_compressed /
//!   kind=turn_summary）禁止再次触发压缩。
//! - 压缩 trigger 保持工具集不变：本引擎只重写 segment 内容，从不改动工具定义 → 保 prefix cache。

use std::cell::Cell;
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::{
    ContextBudget, ContextCompactionEvent, ContextCompactionEventKind, ContextEngine,
    ContextPackError, ContextPacker, ContextSegment, PackedContext, SegmentSource,
};

const SUMMARY_COMPRESSION_PREVIEW_CHARS: usize = 80;
pub const DEFAULT_CONTEXT_RECENT_TURNS: usize = 10;
/// 熔断阈值：连续失败 N 次后停止自动压缩（Claude Code 5：连续 3 次 autocompact 失败停止重试）。
pub const DEFAULT_COMPACTION_BREAKER_THRESHOLD: usize = 3;
/// 熔断冷却秒数：熔断打开后经过该时长自动复位（按配置冷却）。
pub const DEFAULT_COMPACTION_BREAKER_COOLDOWN_SECS: u64 = 60;

/// 压缩策略级联（Claude Code 5 策略，从最便宜到最贵）：
/// Snip（丢旧工具输出）→ Micro（单条内去冗余）→ Collapse（多轮相似工具合并）
/// → Auto（90% 阈值全量压缩）→ Session Memory（与记忆去重）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompactionStrategy {
    Snip,
    Micro,
    Collapse,
    Auto,
    SessionMemory,
}

impl CompactionStrategy {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Snip => "snip",
            Self::Micro => "micro",
            Self::Collapse => "collapse",
            Self::Auto => "auto",
            Self::SessionMemory => "session_memory",
        }
    }

    /// 级联顺序（下标即等级，升序 = 越来越激进）。
    pub const CASCADE_ORDER: [CompactionStrategy; 5] = [
        CompactionStrategy::Snip,
        CompactionStrategy::Micro,
        CompactionStrategy::Collapse,
        CompactionStrategy::Auto,
        CompactionStrategy::SessionMemory,
    ];

    pub fn level(&self) -> u8 {
        match self {
            Self::Snip => 1,
            Self::Micro => 2,
            Self::Collapse => 3,
            Self::Auto => 4,
            Self::SessionMemory => 5,
        }
    }

    /// 降级到相邻更便宜的策略；已是 Snip 则返回 None（没有更低级可降）。
    pub fn degrade(&self) -> Option<Self> {
        match self {
            Self::Snip => None,
            Self::Micro => Some(Self::Snip),
            Self::Collapse => Some(Self::Micro),
            Self::Auto => Some(Self::Collapse),
            Self::SessionMemory => Some(Self::Auto),
        }
    }

    pub fn from_str(value: &str) -> Option<Self> {
        Self::CASCADE_ORDER
            .iter()
            .copied()
            .find(|strategy| strategy.as_str() == value)
    }
}

/// 熔断器状态（供 status 面板展示，可序列化）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionCircuitBreakerStatus {
    pub open: bool,
    pub consecutive_failures: usize,
    pub threshold: usize,
    pub cooldown_secs: u64,
    pub last_failure_at: Option<String>,
    pub opened_at: Option<String>,
    pub skipped_compactions: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct BreakerState {
    consecutive_failures: usize,
    open: bool,
    opened_at: Option<DateTime<Utc>>,
    last_failure_at: Option<DateTime<Utc>>,
    skipped_compactions: u64,
}

impl Default for BreakerState {
    fn default() -> Self {
        Self {
            consecutive_failures: 0,
            open: false,
            opened_at: None,
            last_failure_at: None,
            skipped_compactions: 0,
        }
    }
}

#[derive(Debug)]
struct CompactionCircuitBreaker {
    threshold: usize,
    cooldown_secs: u64,
    state: Mutex<BreakerState>,
}

impl CompactionCircuitBreaker {
    fn new(threshold: usize, cooldown_secs: u64) -> Self {
        Self {
            threshold: threshold.max(1),
            cooldown_secs,
            state: Mutex::new(BreakerState::default()),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BreakerState> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// 是否允许发起自动压缩。熔断打开且冷却未到时返回 false（跳过压缩）；
    /// 冷却已到则自动复位（按配置冷却）并允许下一次尝试。
    fn compression_allowed(&self) -> bool {
        let mut state = self.lock();
        if state.open {
            let cooled_down = match state.last_failure_at {
                Some(at) => (Utc::now() - at).num_seconds() >= self.cooldown_secs as i64,
                None => true,
            };
            if cooled_down {
                state.open = false;
                state.consecutive_failures = 0;
                state.opened_at = None;
            } else {
                state.skipped_compactions += 1;
                return false;
            }
        }
        true
    }

    fn record_failure(&self) {
        let mut state = self.lock();
        state.consecutive_failures += 1;
        state.last_failure_at = Some(Utc::now());
        if state.consecutive_failures >= self.threshold {
            state.open = true;
            state.opened_at = state.last_failure_at;
        }
    }

    fn record_success(&self) {
        let mut state = self.lock();
        state.consecutive_failures = 0;
        state.open = false;
        state.opened_at = None;
    }

    fn reset(&self) {
        let mut state = self.lock();
        state.consecutive_failures = 0;
        state.open = false;
        state.opened_at = None;
        state.last_failure_at = None;
    }

    fn status(&self) -> CompactionCircuitBreakerStatus {
        let state = self.lock();
        CompactionCircuitBreakerStatus {
            open: state.open,
            consecutive_failures: state.consecutive_failures,
            threshold: self.threshold,
            cooldown_secs: self.cooldown_secs,
            last_failure_at: state.last_failure_at.map(|at| at.to_rfc3339()),
            opened_at: state.opened_at.map(|at| at.to_rfc3339()),
            skipped_compactions: state.skipped_compactions,
        }
    }
}

#[derive(Debug)]
pub struct SummaryCompressionContextEngine {
    budget: ContextBudget,
    recent_turns: usize,
    strategy: CompactionStrategy,
    breaker: CompactionCircuitBreaker,
}

impl Clone for SummaryCompressionContextEngine {
    fn clone(&self) -> Self {
        Self {
            budget: self.budget.clone(),
            recent_turns: self.recent_turns,
            strategy: self.strategy,
            breaker: CompactionCircuitBreaker {
                threshold: self.breaker.threshold,
                cooldown_secs: self.breaker.cooldown_secs,
                state: Mutex::new(self.breaker.lock().clone()),
            },
        }
    }
}

impl PartialEq for SummaryCompressionContextEngine {
    fn eq(&self, other: &Self) -> bool {
        self.budget == other.budget
            && self.recent_turns == other.recent_turns
            && self.strategy == other.strategy
            && self.breaker.threshold == other.breaker.threshold
            && self.breaker.cooldown_secs == other.breaker.cooldown_secs
            && *self.breaker.lock() == *other.breaker.lock()
    }
}

impl Eq for SummaryCompressionContextEngine {}

impl SummaryCompressionContextEngine {
    pub fn new(budget: ContextBudget) -> Self {
        Self::with_recent_turns(budget, DEFAULT_CONTEXT_RECENT_TURNS)
    }

    pub fn with_recent_turns(budget: ContextBudget, recent_turns: usize) -> Self {
        Self {
            budget,
            recent_turns: recent_turns.max(1),
            strategy: CompactionStrategy::Auto,
            breaker: CompactionCircuitBreaker::new(
                DEFAULT_COMPACTION_BREAKER_THRESHOLD,
                DEFAULT_COMPACTION_BREAKER_COOLDOWN_SECS,
            ),
        }
    }

    /// 指定策略级联等级（当前实现 = Auto 级；Micro/Collapse 复用同一压缩管线，等级用于
    /// 元数据标记与降级顺序）。
    pub fn with_compaction_strategy(mut self, strategy: CompactionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// 配置熔断器（连续失败阈值 + 冷却秒数）。
    pub fn with_circuit_breaker(mut self, threshold: usize, cooldown_secs: u64) -> Self {
        self.breaker = CompactionCircuitBreaker::new(threshold, cooldown_secs);
        self
    }

    /// 手动重置熔断器（失败计数清零、熔断关闭）。
    pub fn reset_circuit_breaker(&self) {
        self.breaker.reset();
    }

    /// 熔断器当前状态（供 status 面板展示）。
    pub fn circuit_breaker_status(&self) -> CompactionCircuitBreakerStatus {
        self.breaker.status()
    }

    pub fn compaction_strategy(&self) -> CompactionStrategy {
        self.strategy
    }
}

impl ContextEngine for SummaryCompressionContextEngine {
    fn kind(&self) -> &'static str {
        "summary_compression"
    }

    fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError> {
        if !self.breaker.compression_allowed() {
            // 熔断打开且冷却未到：跳过自动压缩，走未压缩确定性路径（防浪费无效压缩调用）。
            let mut packed = ContextPacker::new(self.budget.clone()).pack(segments)?;
            packed.compaction_events.push(ContextCompactionEvent {
                kind: ContextCompactionEventKind::CompressionSkipped,
                segment_id: None,
                reason: Some("circuit_breaker_open".to_string()),
                trace_step: Some("summary_compression"),
            });
            return Ok(packed);
        }

        let compressed = compress_segments(segments, self.recent_turns, self.strategy);
        match ContextPacker::new(self.budget.clone()).pack(compressed) {
            Ok(packed) => {
                self.breaker.record_success();
                Ok(packed)
            }
            Err(err) => {
                self.breaker.record_failure();
                Err(err)
            }
        }
    }
}

/// 压缩入口：把图片内容（data URL / image_url / markdown 图片）替换为文本占位引用，
/// 避免压缩请求超长。压缩完成后不回填（压缩摘要不需要原图）。
pub fn strip_image_payloads(content: &str) -> String {
    strip_image_payloads_counted(content).0
}

fn strip_image_payloads_counted(content: &str) -> (String, usize) {
    static MARKDOWN_DATA_IMAGE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static DATA_IMAGE_URL: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();
    static JSON_IMAGE_URL_LONG: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

    let markdown_data_image = MARKDOWN_DATA_IMAGE.get_or_init(|| {
        Regex::new(r"!\[([^\]]*)\]\((data:image/[a-zA-Z0-9.+-]+;base64,[A-Za-z0-9+/=\s]+)\)")
            .expect("markdown data image regex should compile")
    });
    let data_image_url = DATA_IMAGE_URL.get_or_init(|| {
        Regex::new(r"data:image/[a-zA-Z0-9.+-]+;base64,[A-Za-z0-9+/=\s]+")
            .expect("data image url regex should compile")
    });
    let json_image_url_long = JSON_IMAGE_URL_LONG.get_or_init(|| {
        Regex::new(r#"("image_url"\s*:\s*")[^"]{120,}(")"#)
            .expect("json image_url regex should compile")
    });

    let count = Cell::new(0usize);
    let mut result = content.to_string();
    result = markdown_data_image
        .replace_all(&result, |caps: &regex::Captures<'_>| {
            count.set(count.get() + 1);
            format!("![{}]([image])", &caps[1])
        })
        .into_owned();
    result = data_image_url
        .replace_all(&result, |_caps: &regex::Captures<'_>| {
            count.set(count.get() + 1);
            "[image]".to_string()
        })
        .into_owned();
    result = json_image_url_long
        .replace_all(&result, |caps: &regex::Captures<'_>| {
            count.set(count.get() + 1);
            format!("{}[image]{}", &caps[1], &caps[2])
        })
        .into_owned();

    (result, count.get())
}

/// 递归保护：压缩/总结路径产物禁止再次触发压缩。
/// - `compaction_source=true`：由总结/记忆路径显式标记；
/// - `compacted=true`：turn summary 压缩产物（chuang_kernel compact_turn_summary_content）；
/// - `summary_compressed=true`：已被本引擎压缩过一次；
/// - `kind=turn_summary`：已是总结形态的记忆。
fn is_compaction_product(segment: &ContextSegment) -> bool {
    let metadata = &segment.metadata;
    metadata.get("compaction_source").map(String::as_str) == Some("true")
        || metadata.get("compacted").map(String::as_str) == Some("true")
        || metadata.get("summary_compressed").map(String::as_str) == Some("true")
        || metadata.get("kind").map(String::as_str) == Some("turn_summary")
}

fn compress_segments(
    mut segments: Vec<ContextSegment>,
    recent_turns: usize,
    strategy: CompactionStrategy,
) -> Vec<ContextSegment> {
    let strategy_label = strategy.as_str();
    let mut history = segments
        .iter_mut()
        .filter(|segment| {
            segment.metadata.get("kind").map(String::as_str) == Some("recent_conversation_turn")
        })
        .collect::<Vec<_>>();
    let first_recent = history.len().saturating_sub(recent_turns * 2);
    for (index, segment) in history.iter_mut().enumerate() {
        if is_compaction_product(segment) {
            continue;
        }
        if index >= first_recent {
            segment
                .metadata
                .insert("recent_turn_protected".to_string(), "true".to_string());
        } else {
            let original_chars = segment.content.chars().count();
            let (stripped, stripped_count) = strip_image_payloads_counted(&segment.content);
            if stripped_count > 0 {
                segment
                    .metadata
                    .insert("image_stripped".to_string(), "true".to_string());
            }
            if original_chars > SUMMARY_COMPRESSION_PREVIEW_CHARS {
                let compressed = truncate_chars(&stripped, SUMMARY_COMPRESSION_PREVIEW_CHARS);
                segment.content = format!("{compressed}...");
                segment.tokens = Some(segment.content.chars().count() as u32);
                segment
                    .metadata
                    .insert("summary_compressed".to_string(), "true".to_string());
                segment.metadata.insert(
                    "compaction_strategy".to_string(),
                    strategy_label.to_string(),
                );
            }
        }
    }
    for segment in &mut segments {
        if !matches!(
            segment.source,
            SegmentSource::Memory | SegmentSource::ToolResult
        ) {
            continue;
        }
        if is_compaction_product(segment) {
            continue;
        }

        let original_chars = segment.content.chars().count();
        let (stripped, stripped_count) = strip_image_payloads_counted(&segment.content);
        if stripped_count > 0 {
            segment
                .metadata
                .insert("image_stripped".to_string(), "true".to_string());
        }
        if original_chars <= SUMMARY_COMPRESSION_PREVIEW_CHARS {
            continue;
        }

        let compressed_content = truncate_chars(&stripped, SUMMARY_COMPRESSION_PREVIEW_CHARS);
        segment.content = format!("{compressed_content}...");
        segment.tokens = Some(segment.content.chars().count().min(u32::MAX as usize) as u32);
        segment
            .metadata
            .insert("summary_compressed".to_string(), "true".to_string());
        segment.metadata.insert(
            "compaction_strategy".to_string(),
            strategy_label.to_string(),
        );
        segment.metadata.insert(
            "summary_compressed_from_chars".to_string(),
            original_chars.to_string(),
        );
    }

    segments
}

fn truncate_chars(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}
