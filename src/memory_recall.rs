//! `memory_recall` 模块。公开接口：struct RecallRequest, RecallHit, RecallResult, MemoryRecallPipeline；enum MemoryRecallError；fn new, store_mut, recall。

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};

use crate::context_engine::{ContextSegment, SegmentSource};
use crate::memory_store::{
    MemoryLayerBoundary, MemoryMatchMode, MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallRequest {
    pub query_text: String,
    pub metadata: BTreeMap<String, String>,
    pub limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallHit {
    pub record: MemoryRecord,
    pub score: u32,
    pub rank: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecallResult {
    pub hits: Vec<RecallHit>,
    pub segments: Vec<ContextSegment>,
    pub summary: String,
    pub agent_input: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryRecallError {
    InvalidRequest(&'static str),
    Store(MemoryStoreError),
}

pub struct MemoryRecallPipeline<S> {
    store: S,
}

impl<S> MemoryRecallPipeline<S> {
    pub fn new(store: S) -> Self {
        Self { store }
    }

    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
}

impl<S: MemoryStore> MemoryRecallPipeline<S> {
    pub fn recall(&self, request: &RecallRequest) -> Result<RecallResult, MemoryRecallError> {
        if request.limit == 0 {
            return Err(MemoryRecallError::InvalidRequest("limit_must_be_positive"));
        }

        let hits = self
            .store
            .search(&MemoryQuery {
                text: Some(request.query_text.clone()),
                metadata: request.metadata.clone(),
                limit: request.limit,
                match_mode: MemoryMatchMode::Token,
            })
            .map_err(MemoryRecallError::Store)?;

        let ranked_hits: Vec<RecallHit> = hits
            .into_iter()
            .enumerate()
            .map(|(index, hit)| RecallHit {
                record: hit.record,
                score: hit.score,
                rank: index + 1,
            })
            .collect();

        let segments = ranked_hits
            .iter()
            .map(|hit| recall_hit_to_segment(hit))
            .collect::<Vec<_>>();

        let summary = ranked_hits
            .iter()
            .map(|hit| hit.record.content.clone())
            .collect::<Vec<_>>()
            .join("\n");

        let mut agent_input_lines = vec![
            "[memory-recall]".to_string(),
            format!("query={}", request.query_text),
            format!("hits={}", ranked_hits.len()),
            "boundary=recall_only archive_read_only=true maintenance_writeback=false decay_review=false".to_string(),
        ];

        for hit in &ranked_hits {
            let boundary = MemoryLayerBoundary::for_record(&hit.record);
            agent_input_lines.push(format!(
                "{}. [{}] layer={} writeback_target={} archive_read_only={} decay_review_only={} {}",
                hit.rank,
                hit.record.id,
                boundary.layer.as_str(),
                boundary.writeback_target,
                boundary.archive_read_only,
                boundary.decay_review_only,
                hit.record.content
            ));
        }

        let agent_input = agent_input_lines.join("\n");

        Ok(RecallResult {
            hits: ranked_hits,
            segments,
            summary,
            agent_input,
        })
    }
}

fn recall_hit_to_segment(hit: &RecallHit) -> ContextSegment {
    let boundary = MemoryLayerBoundary::for_record(&hit.record);
    let mut metadata = hit.record.metadata.clone();
    metadata
        .entry("memory_layer".to_string())
        .or_insert_with(|| boundary.layer.as_str().to_string());
    metadata.insert(
        "memory_boundary".to_string(),
        "recall_only_no_writeback".to_string(),
    );
    metadata.insert(
        "archive_read_only".to_string(),
        boundary.archive_read_only.to_string(),
    );
    metadata.insert(
        "maintenance_writeback_allowed".to_string(),
        boundary.maintenance_writeback_allowed.to_string(),
    );
    metadata.insert(
        "decay_review_only".to_string(),
        boundary.decay_review_only.to_string(),
    );
    metadata.insert(
        "writeback_target".to_string(),
        boundary.writeback_target.to_string(),
    );

    ContextSegment {
        id: hit.record.id.clone(),
        source: SegmentSource::Memory,
        content: hit.record.content.clone(),
        tokens: Some(estimate_tokens(&hit.record.content)),
        priority: map_score_to_priority(hit.score),
        created_at: parse_timestamp(&hit.record.created_at),
        last_accessed: parse_timestamp(&hit.record.created_at),
        metadata: metadata.into_iter().collect(),
    }
}

fn map_score_to_priority(score: u32) -> u8 {
    match score {
        0..=4 => 100,
        5..=7 => 150,
        _ => 200,
    }
}

fn estimate_tokens(content: &str) -> u32 {
    content.chars().count().min(u32::MAX as usize) as u32
}

fn parse_timestamp(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("memory record created_at should be valid RFC3339")
        .with_timezone(&Utc)
}
