//! `context_engine::summary_compression` 模块。公开接口：struct SummaryCompressionContextEngine；fn new。

use super::{
    ContextBudget, ContextEngine, ContextPackError, ContextPacker, ContextSegment, PackedContext,
    SegmentSource,
};

const SUMMARY_COMPRESSION_PREVIEW_CHARS: usize = 80;
pub const DEFAULT_CONTEXT_RECENT_TURNS: usize = 10;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryCompressionContextEngine {
    budget: ContextBudget,
    recent_turns: usize,
}

impl SummaryCompressionContextEngine {
    pub fn new(budget: ContextBudget) -> Self {
        Self::with_recent_turns(budget, DEFAULT_CONTEXT_RECENT_TURNS)
    }

    pub fn with_recent_turns(budget: ContextBudget, recent_turns: usize) -> Self {
        Self {
            budget,
            recent_turns: recent_turns.max(1),
        }
    }
}

impl ContextEngine for SummaryCompressionContextEngine {
    fn kind(&self) -> &'static str {
        "summary_compression"
    }

    fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError> {
        ContextPacker::new(self.budget.clone()).pack(compress_segments(segments, self.recent_turns))
    }
}

fn compress_segments(
    mut segments: Vec<ContextSegment>,
    recent_turns: usize,
) -> Vec<ContextSegment> {
    let mut history = segments
        .iter_mut()
        .filter(|segment| {
            segment.metadata.get("kind").map(String::as_str) == Some("recent_conversation_turn")
        })
        .collect::<Vec<_>>();
    let first_recent = history.len().saturating_sub(recent_turns * 2);
    for (index, segment) in history.iter_mut().enumerate() {
        if index >= first_recent {
            segment
                .metadata
                .insert("recent_turn_protected".to_string(), "true".to_string());
        } else {
            let original_chars = segment.content.chars().count();
            if original_chars > SUMMARY_COMPRESSION_PREVIEW_CHARS {
                let compressed =
                    truncate_chars(&segment.content, SUMMARY_COMPRESSION_PREVIEW_CHARS);
                segment.content = format!("{compressed}...");
                segment.tokens = Some(segment.content.chars().count() as u32);
                segment
                    .metadata
                    .insert("summary_compressed".to_string(), "true".to_string());
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

        let original_chars = segment.content.chars().count();
        if original_chars <= SUMMARY_COMPRESSION_PREVIEW_CHARS {
            continue;
        }

        let compressed_content =
            truncate_chars(&segment.content, SUMMARY_COMPRESSION_PREVIEW_CHARS);
        segment.content = format!("{compressed_content}...");
        segment.tokens = Some(segment.content.chars().count().min(u32::MAX as usize) as u32);
        segment
            .metadata
            .insert("summary_compressed".to_string(), "true".to_string());
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
