use std::cmp::Reverse;
use std::collections::HashMap;

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
    pub tokens: Option<u16>,
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
    pub max_tokens: u16,
    pub reserve_system_tokens: u16,
    pub min_working_tokens: u16,
    pub max_tool_results: usize,
    pub max_memory_segments: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackedContext {
    pub segments: Vec<ContextSegment>,
    pub total_tokens: u16,
    pub dropped_ids: Vec<String>,
    pub drop_reasons: Vec<DropReason>,
    pub budget_exceeded: bool,
    pub budget_exceeded_reasons: Vec<BudgetExceededReason>,
    pub working_reservation: Option<WorkingReservation>,
    pub trace: Vec<ContextPackTraceStep>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextPackTraceStep {
    pub name: &'static str,
    pub input_count: usize,
    pub output_count: usize,
    pub dropped_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkingReservation {
    pub reserved_segment_id: String,
    pub reserved_tokens: u16,
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
}

impl DropReasonKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::BudgetLimit => "budget_limit",
            Self::ToolResultTrim => "tool_result_trim",
            Self::MemoryTrim => "memory_trim",
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
        let normalized = self.normalize_segments(segments);
        trace.push(ContextPackTraceStep {
            name: "normalize_tokens",
            input_count: original_count,
            output_count: normalized.len(),
            dropped_count: 0,
        });
        let before_trim_dropped = dropped_ids.len();
        let trimmed = self.trim_segments(normalized, &mut dropped_ids, &mut drop_reasons);
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
            .sum::<u16>();

        if system_tokens > self.budget.reserve_system_tokens
            || system_tokens > self.budget.max_tokens
        {
            return Err(ContextPackError::BudgetExceeded {
                required_system_tokens: system_tokens,
                max_tokens: self.budget.max_tokens,
            });
        }

        let mut sorted = trimmed.clone();
        sorted.sort_by_key(|segment| {
            (
                Reverse(segment.priority),
                Reverse(segment.last_accessed),
                Reverse(segment.created_at),
            )
        });
        trace.push(ContextPackTraceStep {
            name: "rank",
            input_count: trimmed.len(),
            output_count: sorted.len(),
            dropped_count: 0,
        });

        let before_reservation_dropped = dropped_ids.len();
        let mut budget_exceeded_reasons = Vec::new();
        let working_reservation = self.reserve_minimum_working_segments(
            &sorted,
            &mut dropped_ids,
            &mut drop_reasons,
            &mut budget_exceeded_reasons,
        );
        trace.push(ContextPackTraceStep {
            name: "reserve_working",
            input_count: sorted.len(),
            output_count: sorted.len(),
            dropped_count: dropped_ids.len().saturating_sub(before_reservation_dropped),
        });
        let reserved_working_ids = working_reservation
            .as_ref()
            .map(|reservation| vec![reservation.reserved_segment_id.clone()])
            .unwrap_or_default();

        let mut packed = Vec::new();
        let mut total_tokens = 0u16;
        let reserved_tokens = sorted
            .iter()
            .filter(|segment| reserved_working_ids.contains(&segment.id))
            .map(|segment| segment.tokens.unwrap_or(0))
            .sum::<u16>();

        let merge_input_count = sorted.len();
        let before_merge_dropped = dropped_ids.len();
        for segment in sorted {
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

        Ok(PackedContext {
            segments: packed,
            total_tokens,
            dropped_ids,
            drop_reasons,
            budget_exceeded,
            budget_exceeded_reasons,
            working_reservation,
            trace,
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

    fn reserve_minimum_working_segments(
        &self,
        sorted: &[ContextSegment],
        dropped_ids: &mut Vec<String>,
        drop_reasons: &mut Vec<DropReason>,
        budget_exceeded_reasons: &mut Vec<BudgetExceededReason>,
    ) -> Option<WorkingReservation> {
        if self.budget.min_working_tokens == 0 {
            return None;
        }

        let system_tokens = sorted
            .iter()
            .filter(|segment| matches!(segment.source, SegmentSource::System))
            .map(|segment| segment.tokens.unwrap_or(0))
            .sum::<u16>();

        let candidate = sorted
            .iter()
            .filter(|segment| matches!(segment.source, SegmentSource::Working))
            .filter(|segment| segment.tokens.unwrap_or(0) >= self.budget.min_working_tokens)
            .max_by_key(|segment| (segment.priority, segment.last_accessed, segment.created_at));

        let Some(candidate) = candidate else {
            return None;
        };

        let candidate_tokens = candidate.tokens.unwrap_or(0);
        if system_tokens.saturating_add(candidate_tokens) > self.budget.max_tokens {
            budget_exceeded_reasons.push(BudgetExceededReason::MinWorkingTokensUnmet);
            return None;
        }

        let mut reservation_drops = Vec::new();
        let budget_after_reservation = self.budget.max_tokens - candidate_tokens;
        for segment in sorted.iter().filter(|segment| segment.id != candidate.id) {
            let tokens = segment.tokens.unwrap_or(0);
            if matches!(segment.source, SegmentSource::System) {
                continue;
            }
            if tokens > budget_after_reservation.saturating_sub(system_tokens) {
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

    fn estimate_tokens(&self, content: &str) -> u16 {
        content.chars().count().min(u16::MAX as usize) as u16
    }
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextPackError {
    BudgetExceeded {
        required_system_tokens: u16,
        max_tokens: u16,
    },
}
