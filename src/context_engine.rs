use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap};

use chrono::{DateTime, Utc};

mod deterministic;
mod summary_compression;

pub use deterministic::DeterministicContextEngine;
pub use summary_compression::SummaryCompressionContextEngine;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextSegment {
    pub id: String,
    pub source: SegmentSource,
    pub content: String,
    pub tokens: Option<u32>,
    pub priority: u8,
    pub created_at: DateTime<Utc>,
    pub last_accessed: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentSource {
    Identity,
    Memory,
    Working,
    ToolResult,
    Goal,
    System,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextBudget {
    pub max_tokens: u32,
    pub reserve_system_tokens: u32,
    pub min_working_tokens: u32,
    pub max_tool_results: usize,
    pub max_memory_segments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedContext {
    pub segments: Vec<ContextSegment>,
    pub total_tokens: u32,
    pub dropped_ids: Vec<String>,
    pub drop_reasons: Vec<DropReason>,
    pub budget_exceeded: bool,
    pub budget_exceeded_reasons: Vec<BudgetExceededReason>,
    pub working_reservation: Option<WorkingReservation>,
    pub trace: Vec<ContextPackTraceStep>,
    pub compaction_events: Vec<ContextCompactionEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPackTraceStep {
    pub name: &'static str,
    pub input_count: usize,
    pub output_count: usize,
    pub dropped_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCompactionEvent {
    pub kind: ContextCompactionEventKind,
    pub segment_id: Option<String>,
    pub reason: Option<String>,
    pub trace_step: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextCompactionSummary {
    pub event_count: usize,
    pub started_count: usize,
    pub completed_count: usize,
    pub dropped_count: usize,
    pub dropped_segment_ids: Vec<String>,
    pub drop_reason_counts: BTreeMap<String, usize>,
    pub trace_steps: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextCompactionEventKind {
    Started,
    SegmentDropped,
    Completed,
}

impl ContextCompactionEventKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Started => "context_compaction_started",
            Self::SegmentDropped => "context_segment_dropped",
            Self::Completed => "context_compaction_completed",
        }
    }
}

impl PackedContext {
    pub fn compaction_summary(&self) -> ContextCompactionSummary {
        let mut started_count = 0usize;
        let mut completed_count = 0usize;
        let mut dropped_segment_ids = Vec::new();
        let mut drop_reason_counts = BTreeMap::new();
        let mut trace_steps = Vec::new();

        for event in &self.compaction_events {
            match event.kind {
                ContextCompactionEventKind::Started => started_count += 1,
                ContextCompactionEventKind::Completed => completed_count += 1,
                ContextCompactionEventKind::SegmentDropped => {
                    if let Some(segment_id) = &event.segment_id {
                        dropped_segment_ids.push(segment_id.clone());
                    }
                    if let Some(reason) = &event.reason {
                        *drop_reason_counts.entry(reason.clone()).or_insert(0) += 1;
                    }
                }
            }
            if let Some(trace_step) = event.trace_step {
                let step = trace_step.to_string();
                if !trace_steps.iter().any(|existing| existing == &step) {
                    trace_steps.push(step);
                }
            }
        }

        ContextCompactionSummary {
            event_count: self.compaction_events.len(),
            started_count,
            completed_count,
            dropped_count: dropped_segment_ids.len(),
            dropped_segment_ids,
            drop_reason_counts,
            trace_steps,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingReservation {
    pub reserved_segment_id: String,
    pub reserved_tokens: u32,
    pub dropped_segment_ids: Vec<String>,
    pub reason: WorkingReservationReason,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingReservationReason {
    MinimumWorkingTokens,
}

impl WorkingReservationReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MinimumWorkingTokens => "minimum_working_tokens",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DropReason {
    pub segment_id: String,
    pub reason: DropReasonKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DropReasonKind {
    BudgetLimit,
    ToolResultTrim,
    MemoryTrim,
    DuplicateContent,
}

impl DropReasonKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BudgetLimit => "budget_limit",
            Self::ToolResultTrim => "tool_result_trim",
            Self::MemoryTrim => "memory_trim",
            Self::DuplicateContent => "duplicate_content",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetExceededReason {
    MinWorkingTokensUnmet,
}

impl BudgetExceededReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MinWorkingTokensUnmet => "min_working_tokens_unmet",
        }
    }
}

pub struct ContextPacker {
    budget: ContextBudget,
}

pub trait ContextEngine {
    fn kind(&self) -> &'static str;
    fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextEngineKind {
    DeterministicBudget,
    SummaryCompression,
}

impl Default for ContextEngineKind {
    fn default() -> Self {
        Self::DeterministicBudget
    }
}

impl ContextEngineKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::DeterministicBudget => "deterministic_budget",
            Self::SummaryCompression => "summary_compression",
        }
    }

    pub fn pack(
        &self,
        budget: ContextBudget,
        segments: Vec<ContextSegment>,
    ) -> Result<PackedContext, ContextPackError> {
        match self {
            Self::DeterministicBudget => DeterministicContextEngine::new(budget).pack(segments),
            Self::SummaryCompression => SummaryCompressionContextEngine::new(budget).pack(segments),
        }
    }
}

impl ContextPacker {
    pub fn new(budget: ContextBudget) -> Self {
        Self { budget }
    }

    pub fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError> {
        let original_count = segments.len();
        let mut trace = Vec::new();
        let mut dropped_ids = Vec::new();
        let mut drop_reasons = Vec::new();
        let mut compaction_events = Vec::new();
        compaction_events.push(ContextCompactionEvent {
            kind: ContextCompactionEventKind::Started,
            segment_id: None,
            reason: None,
            trace_step: None,
        });
        let normalized = self.normalize_segments(segments);
        trace.push(ContextPackTraceStep {
            name: "normalize_tokens",
            input_count: original_count,
            output_count: normalized.len(),
            dropped_count: 0,
        });
        let before_dedupe_dropped = dropped_ids.len();
        let deduped = self.deduplicate_segments(normalized, &mut dropped_ids, &mut drop_reasons);
        record_compaction_events(
            &mut compaction_events,
            &drop_reasons,
            before_dedupe_dropped,
            "dedupe",
        );
        trace.push(ContextPackTraceStep {
            name: "dedupe",
            input_count: original_count,
            output_count: deduped.len(),
            dropped_count: dropped_ids.len().saturating_sub(before_dedupe_dropped),
        });
        let before_trim_dropped = dropped_ids.len();
        let trimmed = self.trim_segments(deduped, &mut dropped_ids, &mut drop_reasons);
        record_compaction_events(
            &mut compaction_events,
            &drop_reasons,
            before_trim_dropped,
            "trim",
        );
        trace.push(ContextPackTraceStep {
            name: "trim",
            input_count: original_count,
            output_count: trimmed.len(),
            dropped_count: dropped_ids.len().saturating_sub(before_trim_dropped),
        });

        let system_tokens = trimmed
            .iter()
            .filter(|segment| matches!(segment.source, SegmentSource::System))
            .map(|segment| segment.tokens.unwrap_or(0))
            .sum::<u32>();

        if system_tokens > self.budget.reserve_system_tokens
            || system_tokens > self.budget.max_tokens
        {
            return Err(ContextPackError::BudgetExceeded {
                required_system_tokens: system_tokens,
                max_tokens: self.budget.max_tokens,
            });
        }

        let mut protected = Vec::new();
        let mut regular = Vec::new();
        for segment in trimmed.clone() {
            if is_reserved_segment(&segment) {
                protected.push(segment);
            } else {
                regular.push(segment);
            }
        }

        protected.sort_by_key(|segment| {
            (
                Reverse(segment.priority),
                Reverse(segment.last_accessed),
                Reverse(segment.created_at),
            )
        });
        regular.sort_by_key(|segment| {
            (
                Reverse(segment.priority),
                Reverse(segment.last_accessed),
                Reverse(segment.created_at),
            )
        });
        trace.push(ContextPackTraceStep {
            name: "rank",
            input_count: original_count,
            output_count: protected.len() + regular.len(),
            dropped_count: 0,
        });

        let protected_tokens = protected
            .iter()
            .map(|segment| segment.tokens.unwrap_or(0))
            .sum::<u32>();
        if protected_tokens > self.budget.max_tokens {
            return Err(ContextPackError::BudgetExceeded {
                required_system_tokens: protected_tokens,
                max_tokens: self.budget.max_tokens,
            });
        }

        let mut packed = Vec::new();
        let mut total_tokens = 0u32;
        for segment in protected {
            total_tokens = total_tokens.saturating_add(segment.tokens.unwrap_or(0));
            packed.push(segment);
        }

        let before_reservation_dropped = dropped_ids.len();
        let mut budget_exceeded_reasons = Vec::new();
        let working_reservation = self.reserve_minimum_working_segments(
            &regular,
            total_tokens,
            &mut dropped_ids,
            &mut drop_reasons,
            &mut budget_exceeded_reasons,
        );
        record_compaction_events(
            &mut compaction_events,
            &drop_reasons,
            before_reservation_dropped,
            "reserve_working",
        );
        trace.push(ContextPackTraceStep {
            name: "reserve_working",
            input_count: regular.len(),
            output_count: regular.len(),
            dropped_count: dropped_ids.len().saturating_sub(before_reservation_dropped),
        });
        let reserved_working_ids = working_reservation
            .as_ref()
            .map(|reservation| vec![reservation.reserved_segment_id.clone()])
            .unwrap_or_default();

        let reserved_tokens = regular
            .iter()
            .filter(|segment| reserved_working_ids.contains(&segment.id))
            .map(|segment| segment.tokens.unwrap_or(0))
            .sum::<u32>();

        let merge_input_count = regular.len();
        let before_merge_dropped = dropped_ids.len();
        for segment in regular {
            let tokens = segment.tokens.unwrap_or(0);
            if reserved_working_ids.contains(&segment.id) {
                total_tokens = total_tokens.saturating_add(tokens);
                packed.push(segment);
                continue;
            }

            if total_tokens.saturating_add(tokens)
                <= self.budget.max_tokens.saturating_sub(reserved_tokens)
            {
                total_tokens = total_tokens.saturating_add(tokens);
                packed.push(segment);
            } else {
                dropped_ids.push(segment.id.clone());
                drop_reasons.push(DropReason {
                    segment_id: segment.id,
                    reason: DropReasonKind::BudgetLimit,
                });
            }
        }

        let has_working = packed
            .iter()
            .any(|segment| matches!(segment.source, SegmentSource::Working));
        let budget_exceeded = if !reserved_working_ids.is_empty() {
            false
        } else {
            self.budget.min_working_tokens > 0
                && !has_working
                && trimmed.iter().any(|segment| {
                    matches!(segment.source, SegmentSource::Working)
                        && segment.tokens.unwrap_or(0) >= self.budget.min_working_tokens
                })
        };

        if budget_exceeded
            && !budget_exceeded_reasons.contains(&BudgetExceededReason::MinWorkingTokensUnmet)
        {
            budget_exceeded_reasons.push(BudgetExceededReason::MinWorkingTokensUnmet);
        }
        trace.push(ContextPackTraceStep {
            name: "merge_under_budget",
            input_count: merge_input_count,
            output_count: packed.len(),
            dropped_count: dropped_ids.len().saturating_sub(before_merge_dropped),
        });
        record_compaction_events(
            &mut compaction_events,
            &drop_reasons,
            before_merge_dropped,
            "merge_under_budget",
        );
        compaction_events.push(ContextCompactionEvent {
            kind: ContextCompactionEventKind::Completed,
            segment_id: None,
            reason: Some(
                if budget_exceeded {
                    "budget_exceeded"
                } else {
                    "packed"
                }
                .to_string(),
            ),
            trace_step: Some("merge_under_budget"),
        });

        Ok(PackedContext {
            segments: packed,
            total_tokens,
            dropped_ids,
            drop_reasons,
            budget_exceeded,
            budget_exceeded_reasons,
            working_reservation,
            trace,
            compaction_events,
        })
    }

    fn trim_segments(
        &self,
        segments: Vec<ContextSegment>,
        dropped_ids: &mut Vec<String>,
        drop_reasons: &mut Vec<DropReason>,
    ) -> Vec<ContextSegment> {
        let mut protected = Vec::new();
        let mut tool_results = Vec::new();
        let mut memories = Vec::new();
        let mut passthrough = Vec::new();

        for segment in segments {
            if matches!(segment.source, SegmentSource::System) || segment.priority >= 240 {
                protected.push(segment);
            } else {
                match segment.source {
                    SegmentSource::ToolResult => tool_results.push(segment),
                    SegmentSource::Memory => memories.push(segment),
                    _ => passthrough.push(segment),
                }
            }
        }

        tool_results.sort_by_key(|segment| Reverse(segment.created_at));
        let kept_tool_count = self.budget.max_tool_results.min(tool_results.len());
        for segment in tool_results.iter().skip(kept_tool_count) {
            dropped_ids.push(segment.id.clone());
            drop_reasons.push(DropReason {
                segment_id: segment.id.clone(),
                reason: DropReasonKind::ToolResultTrim,
            });
        }
        tool_results.truncate(kept_tool_count);

        memories.sort_by_key(|segment| Reverse(segment.last_accessed));
        let kept_memory_count = self.budget.max_memory_segments.min(memories.len());
        for segment in memories.iter().skip(kept_memory_count) {
            dropped_ids.push(segment.id.clone());
            drop_reasons.push(DropReason {
                segment_id: segment.id.clone(),
                reason: DropReasonKind::MemoryTrim,
            });
        }
        memories.truncate(kept_memory_count);

        let mut result = Vec::new();
        result.extend(protected);
        result.extend(tool_results);
        result.extend(memories);
        result.extend(passthrough);
        result
    }

    fn normalize_segments(&self, segments: Vec<ContextSegment>) -> Vec<ContextSegment> {
        segments
            .into_iter()
            .map(|mut segment| {
                if segment.tokens.is_none() {
                    segment.tokens = Some(self.estimate_tokens(&segment.content));
                }
                segment
            })
            .collect()
    }

    fn deduplicate_segments(
        &self,
        segments: Vec<ContextSegment>,
        dropped_ids: &mut Vec<String>,
        drop_reasons: &mut Vec<DropReason>,
    ) -> Vec<ContextSegment> {
        let mut kept = Vec::<ContextSegment>::new();
        let mut seen = HashMap::<String, usize>::new();

        for segment in segments {
            if !dedupe_candidate(&segment) {
                kept.push(segment);
                continue;
            }

            let key = normalize_dedupe_key(&segment.content);
            if key.is_empty() {
                kept.push(segment);
                continue;
            }

            let Some(existing_index) = seen.get(&key).copied() else {
                seen.insert(key, kept.len());
                kept.push(segment);
                continue;
            };

            if segment_rank_key(&segment) > segment_rank_key(&kept[existing_index]) {
                let dropped = std::mem::replace(&mut kept[existing_index], segment);
                dropped_ids.push(dropped.id.clone());
                drop_reasons.push(DropReason {
                    segment_id: dropped.id,
                    reason: DropReasonKind::DuplicateContent,
                });
            } else {
                dropped_ids.push(segment.id.clone());
                drop_reasons.push(DropReason {
                    segment_id: segment.id,
                    reason: DropReasonKind::DuplicateContent,
                });
            }
        }

        kept
    }

    fn reserve_minimum_working_segments(
        &self,
        sorted: &[ContextSegment],
        base_tokens: u32,
        dropped_ids: &mut Vec<String>,
        drop_reasons: &mut Vec<DropReason>,
        budget_exceeded_reasons: &mut Vec<BudgetExceededReason>,
    ) -> Option<WorkingReservation> {
        if self.budget.min_working_tokens == 0 {
            return None;
        }

        let candidate = sorted
            .iter()
            .filter(|segment| matches!(segment.source, SegmentSource::Working))
            .filter(|segment| segment.tokens.unwrap_or(0) >= self.budget.min_working_tokens)
            .max_by_key(|segment| (segment.priority, segment.last_accessed, segment.created_at));

        let Some(candidate) = candidate else {
            return None;
        };

        let candidate_tokens = candidate.tokens.unwrap_or(0);
        if base_tokens.saturating_add(candidate_tokens) > self.budget.max_tokens {
            budget_exceeded_reasons.push(BudgetExceededReason::MinWorkingTokensUnmet);
            return None;
        }

        let mut reservation_drops = Vec::new();
        let budget_after_reservation = self
            .budget
            .max_tokens
            .saturating_sub(base_tokens)
            .saturating_sub(candidate_tokens);
        for segment in sorted.iter().filter(|segment| segment.id != candidate.id) {
            let tokens = segment.tokens.unwrap_or(0);
            if matches!(segment.source, SegmentSource::System) {
                continue;
            }
            if tokens > budget_after_reservation {
                dropped_ids.push(segment.id.clone());
                reservation_drops.push(segment.id.clone());
                drop_reasons.push(DropReason {
                    segment_id: segment.id.clone(),
                    reason: DropReasonKind::BudgetLimit,
                });
            }
        }

        Some(WorkingReservation {
            reserved_segment_id: candidate.id.clone(),
            reserved_tokens: candidate_tokens,
            dropped_segment_ids: reservation_drops,
            reason: WorkingReservationReason::MinimumWorkingTokens,
        })
    }

    fn estimate_tokens(&self, content: &str) -> u32 {
        content.chars().count().min(u32::MAX as usize) as u32
    }
}

fn dedupe_candidate(segment: &ContextSegment) -> bool {
    matches!(
        segment.source,
        SegmentSource::Identity | SegmentSource::Memory | SegmentSource::Goal
    )
}

fn is_reserved_segment(segment: &ContextSegment) -> bool {
    if matches!(segment.source, SegmentSource::System) {
        return true;
    }

    match segment.id.as_str() {
        "system-capabilities"
        | "tool-instructions"
        | "session-context"
        | "recent-conversation-history" => true,
        _ => matches!(
            segment.metadata.get("kind").map(String::as_str),
            Some(
                "capability_primer"
                    | "tool_protocol"
                    | "session_context"
                    | "recent_conversation_history"
            )
        ),
    }
}

fn normalize_dedupe_key(content: &str) -> String {
    content.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn segment_rank_key(segment: &ContextSegment) -> (u8, DateTime<Utc>, DateTime<Utc>) {
    (segment.priority, segment.last_accessed, segment.created_at)
}

impl PackedContext {
    pub fn render_prompt(&self) -> String {
        let mut lines = vec![
            "[packed-context]".to_string(),
            format!("segments={}", self.segments.len()),
            format!("total_tokens={}", self.total_tokens),
            format!("dropped={}", self.dropped_ids.join(",")),
            format!("drop_reasons={}", render_drop_reasons(&self.drop_reasons)),
            format!("budget_exceeded={}", self.budget_exceeded),
            format!(
                "budget_exceeded_reasons={}",
                render_budget_exceeded_reasons(&self.budget_exceeded_reasons)
            ),
            format!("pack_trace={}", render_pack_trace(&self.trace)),
            format!(
                "compaction_events={}",
                render_compaction_events(&self.compaction_events)
            ),
        ];

        for segment in &self.segments {
            lines.push(format!(
                "- {:?}/p{} [{}] {}",
                segment.source, segment.priority, segment.id, segment.content
            ));
        }

        lines.join("\n")
    }
}

fn render_pack_trace(trace: &[ContextPackTraceStep]) -> String {
    if trace.is_empty() {
        return "none".to_string();
    }
    trace
        .iter()
        .map(|step| {
            format!(
                "{}:{}->{}(-{})",
                step.name, step.input_count, step.output_count, step.dropped_count
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn render_drop_reasons(reasons: &[DropReason]) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(|reason| format!("{}:{}", reason.segment_id, reason.reason.as_str()))
        .collect::<Vec<_>>()
        .join(",")
}

fn render_budget_exceeded_reasons(reasons: &[BudgetExceededReason]) -> String {
    if reasons.is_empty() {
        return "none".to_string();
    }

    reasons
        .iter()
        .map(BudgetExceededReason::as_str)
        .collect::<Vec<_>>()
        .join(",")
}

fn render_compaction_events(events: &[ContextCompactionEvent]) -> String {
    if events.is_empty() {
        return "none".to_string();
    }

    events
        .iter()
        .map(|event| {
            let mut parts = vec![event.kind.as_str().to_string()];
            if let Some(segment_id) = &event.segment_id {
                parts.push(segment_id.clone());
            }
            if let Some(reason) = &event.reason {
                parts.push(reason.clone());
            }
            if let Some(trace_step) = event.trace_step {
                parts.push(format!("@{trace_step}"));
            }
            parts.join(":")
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn record_compaction_events(
    events: &mut Vec<ContextCompactionEvent>,
    reasons: &[DropReason],
    start_index: usize,
    trace_step: &'static str,
) {
    for reason in reasons.iter().skip(start_index) {
        events.push(ContextCompactionEvent {
            kind: ContextCompactionEventKind::SegmentDropped,
            segment_id: Some(reason.segment_id.clone()),
            reason: Some(reason.reason.as_str().to_string()),
            trace_step: Some(trace_step),
        });
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextPackError {
    BudgetExceeded {
        required_system_tokens: u32,
        max_tokens: u32,
    },
}
