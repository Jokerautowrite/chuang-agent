//! `context_engine::summary_compression` 模块。公开接口：struct SummaryCompressionContextEngine；fn new。

use super::{
    ContextBudget, ContextEngine, ContextPackError, ContextPacker, ContextSegment, PackedContext,
    SegmentSource,
};

const SUMMARY_COMPRESSION_PREVIEW_CHARS: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryCompressionContextEngine {
    budget: ContextBudget,
}

impl SummaryCompressionContextEngine {
    pub fn new(budget: ContextBudget) -> Self {
        Self { budget }
    }
}

impl ContextEngine for SummaryCompressionContextEngine {
    fn kind(&self) -> &'static str {
        "summary_compression"
    }

    fn pack(&self, segments: Vec<ContextSegment>) -> Result<PackedContext, ContextPackError> {
        ContextPacker::new(self.budget.clone()).pack(compress_segments(segments))
    }
}

fn compress_segments(mut segments: Vec<ContextSegment>) -> Vec<ContextSegment> {
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
