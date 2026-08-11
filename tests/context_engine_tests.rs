use std::collections::HashMap;

use chrono::{DateTime, Utc};
use chuang_agent::context_engine::{
    ContextBudget, ContextCompactionEventKind, ContextEngine, ContextPackError, ContextPacker,
    ContextSegment, CompactionStrategy, DeterministicContextEngine, SegmentSource,
    SummaryCompressionContextEngine, WorkingReservationReason, strip_image_payloads,
};

fn ts(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value)
        .expect("timestamp should parse")
        .with_timezone(&Utc)
}

fn segment(
    id: &str,
    source: SegmentSource,
    content: &str,
    tokens: Option<u32>,
    priority: u8,
    created_at: &str,
    last_accessed: &str,
) -> ContextSegment {
    ContextSegment {
        id: id.to_string(),
        source,
        content: content.to_string(),
        tokens,
        priority,
        created_at: ts(created_at),
        last_accessed: ts(last_accessed),
        metadata: HashMap::new(),
    }
}

fn budget(max_tokens: u32, reserve_system_tokens: u32, min_working_tokens: u32) -> ContextBudget {
    ContextBudget {
        max_tokens,
        reserve_system_tokens,
        min_working_tokens,
        max_tool_results: 5,
        max_memory_segments: 20,
    }
}

#[test]
fn deterministic_context_engine_wraps_budget_packer_behavior() {
    let engine = DeterministicContextEngine::new(budget(30, 10, 0));

    let packed = engine
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                "memory",
                Some(8),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("deterministic context engine should pack");

    assert_eq!(engine.kind(), "deterministic_budget");
    assert_eq!(packed.total_tokens, 18);
    assert_eq!(packed.dropped_ids, Vec::<String>::new());
}

#[test]
fn summary_compression_context_engine_is_selectable_and_compresses_long_memory_segments() {
    let engine = SummaryCompressionContextEngine::new(budget(100, 10, 0));

    let packed = engine
        .pack(vec![
            segment(
                "working-1",
                SegmentSource::Working,
                "working",
                Some(8),
                200,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                &"memory-".repeat(20),
                Some(140),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("summary compression engine should pack");

    assert_eq!(engine.kind(), "summary_compression");
    assert_eq!(packed.total_tokens, 91);
    assert_eq!(packed.dropped_ids, Vec::<String>::new());
    let memory = packed
        .segments
        .iter()
        .find(|segment| segment.id == "memory-1")
        .expect("memory segment should exist");
    assert!(memory.content.ends_with("..."));
    assert!(memory
        .metadata
        .get("summary_compressed")
        .is_some_and(|value| value == "true"));
}

#[test]
fn summary_compression_preserves_configured_recent_turns() {
    let engine = SummaryCompressionContextEngine::with_recent_turns(budget(5000, 10, 0), 2);
    let mut segments = Vec::new();
    for index in 0..6 {
        let mut item = segment(
            &format!("turn-{index}"),
            SegmentSource::Working,
            &format!("turn-{index}-{}", "detail".repeat(40)),
            None,
            100,
            "2026-04-30T18:00:00Z",
            "2026-04-30T18:00:00Z",
        );
        item.metadata
            .insert("kind".to_string(), "recent_conversation_turn".to_string());
        segments.push(item);
    }
    let packed = engine.pack(segments).unwrap();
    for index in 0..2 {
        let item = packed
            .segments
            .iter()
            .find(|item| item.id == format!("turn-{index}"))
            .unwrap();
        assert!(
            item.content.ends_with("..."),
            "old turn {index} should compress"
        );
    }
    for index in 2..6 {
        let item = packed
            .segments
            .iter()
            .find(|item| item.id == format!("turn-{index}"))
            .unwrap();
        assert!(
            !item.content.ends_with("..."),
            "recent turn {index} should remain raw"
        );
        assert_eq!(
            item.metadata.get("recent_turn_protected"),
            Some(&"true".to_string())
        );
    }
}

#[test]
fn pack_rejects_when_system_budget_cannot_be_reserved() {
    let packer = ContextPacker::new(budget(20, 30, 0));
    let segments = vec![segment(
        "system-1",
        SegmentSource::System,
        "system instruction",
        Some(30),
        255,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    )];

    let error = packer.pack(segments).expect_err("pack should fail");

    assert_eq!(
        error,
        ContextPackError::BudgetExceeded {
            required_system_tokens: 30,
            max_tokens: 20,
        }
    );
}

#[test]
fn pack_normalizes_missing_tokens_before_budget_merge() {
    let packer = ContextPacker::new(budget(21, 10, 0));
    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system stable",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                "12345678901",
                None,
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
        ])
        .expect("pack should succeed");

    let memory = packed
        .segments
        .iter()
        .find(|segment| segment.id == "memory-1")
        .expect("memory segment should remain");
    assert_eq!(memory.tokens, Some(11));
    assert_eq!(packed.total_tokens, 21);
    assert!(packed
        .trace
        .iter()
        .any(|step| step.name == "normalize_tokens" && step.input_count == 2));
}

#[test]
fn pack_deduplicates_exact_content_before_budget_merge() {
    let packer = ContextPacker::new(budget(20, 10, 0));

    let packed = packer
        .pack(vec![
            segment(
                "memory-old",
                SegmentSource::Memory,
                "same fact",
                Some(9),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-new",
                SegmentSource::Memory,
                "same   fact",
                Some(9),
                120,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "work",
                Some(4),
                220,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
        ])
        .expect("pack should succeed");

    assert!(packed
        .segments
        .iter()
        .any(|segment| segment.id == "memory-new"));
    assert!(!packed
        .segments
        .iter()
        .any(|segment| segment.id == "memory-old"));
    assert!(packed.dropped_ids.iter().any(|id| id == "memory-old"));
    assert!(packed.drop_reasons.iter().any(|reason| {
        reason.segment_id == "memory-old" && reason.reason.as_str() == "duplicate_content"
    }));
    assert!(packed
        .trace
        .iter()
        .any(|step| step.name == "dedupe" && step.dropped_count == 1));
}

#[test]
fn pack_keeps_system_segments_even_when_other_segments_are_dropped() {
    let packer = ContextPacker::new(budget(40, 10, 0));
    let result = packer.pack(vec![
        segment(
            "memory-1",
            SegmentSource::Memory,
            "old memory",
            Some(20),
            50,
            "2026-04-30T18:00:00Z",
            "2026-04-30T18:00:00Z",
        ),
        segment(
            "system-1",
            SegmentSource::System,
            "system instruction",
            Some(10),
            255,
            "2026-04-30T18:00:01Z",
            "2026-04-30T18:00:01Z",
        ),
        segment(
            "working-1",
            SegmentSource::Working,
            "current user task",
            Some(25),
            180,
            "2026-04-30T18:00:02Z",
            "2026-04-30T18:00:02Z",
        ),
    ]);

    let packed = result.expect("pack should succeed");

    assert!(packed
        .segments
        .iter()
        .any(|segment| segment.id == "system-1"));
    assert!(packed.dropped_ids.iter().any(|id| id == "memory-1"));
}

#[test]
fn pack_keeps_reserved_tool_session_and_history_segments_under_budget_pressure() {
    let packer = ContextPacker::new(budget(240, 16, 0));
    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "system-capabilities",
                SegmentSource::Identity,
                "capability primer",
                Some(120),
                254,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "session-context",
                SegmentSource::Identity,
                "session and workspace context",
                Some(20),
                253,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "tool-instructions",
                SegmentSource::Identity,
                "tool instructions",
                Some(30),
                252,
                "2026-04-30T18:00:03Z",
                "2026-04-30T18:00:03Z",
            ),
            segment(
                "recent-conversation-history",
                SegmentSource::Working,
                "recent chat history",
                Some(40),
                241,
                "2026-04-30T18:00:04Z",
                "2026-04-30T18:00:04Z",
            ),
            segment(
                "memory-pressure",
                SegmentSource::Memory,
                "very long memory",
                Some(80),
                100,
                "2026-04-30T18:00:05Z",
                "2026-04-30T18:00:05Z",
            ),
        ])
        .expect("pack should succeed");

    let ids: Vec<String> = packed
        .segments
        .iter()
        .map(|segment| segment.id.clone())
        .collect();
    for id in [
        "system-1",
        "system-capabilities",
        "session-context",
        "tool-instructions",
        "recent-conversation-history",
    ] {
        assert!(ids.iter().any(|kept| kept == id), "{id} should be kept");
    }
    assert!(packed.dropped_ids.iter().any(|id| id == "memory-pressure"));
    assert!(!packed
        .dropped_ids
        .iter()
        .any(|id| id == "system-capabilities"));
    assert!(!packed.dropped_ids.iter().any(|id| id == "session-context"));
    assert!(!packed
        .dropped_ids
        .iter()
        .any(|id| id == "tool-instructions"));
    assert!(!packed
        .dropped_ids
        .iter()
        .any(|id| id == "recent-conversation-history"));
}

#[test]
fn pack_trims_tool_results_to_latest_n_before_rank() {
    let mut segments = Vec::new();
    for index in 0..7 {
        segments.push(segment(
            &format!("tool-{index}"),
            SegmentSource::ToolResult,
            "tool output",
            Some(5),
            90,
            &format!("2026-04-30T18:00:0{index}Z"),
            &format!("2026-04-30T18:00:0{index}Z"),
        ));
    }

    let packer = ContextPacker::new(budget(200, 0, 0));
    let packed = packer.pack(segments).expect("pack should succeed");

    let tool_ids: Vec<String> = packed
        .segments
        .iter()
        .filter(|segment| matches!(segment.source, SegmentSource::ToolResult))
        .map(|segment| segment.id.clone())
        .collect();

    assert_eq!(tool_ids.len(), 5);
    assert!(!tool_ids.iter().any(|id| id == "tool-0"));
    assert!(!tool_ids.iter().any(|id| id == "tool-1"));
}

#[test]
fn pack_orders_segments_by_priority_then_last_accessed_then_created_at() {
    let packer = ContextPacker::new(budget(200, 0, 0));
    let packed = packer
        .pack(vec![
            segment(
                "memory-1",
                SegmentSource::Memory,
                "priority low",
                Some(5),
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:05Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "priority high older",
                Some(5),
                200,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:03Z",
            ),
            segment(
                "working-2",
                SegmentSource::Working,
                "priority high newer",
                Some(5),
                200,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:04Z",
            ),
        ])
        .expect("pack should succeed");

    let ids: Vec<String> = packed
        .segments
        .iter()
        .map(|segment| segment.id.clone())
        .collect();
    assert_eq!(ids, vec!["working-2", "working-1", "memory-1"]);
}

#[test]
fn pack_restores_highest_priority_working_segment_when_budget_allows() {
    let packer = ContextPacker::new(budget(35, 10, 5));
    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                "memory first",
                Some(20),
                220,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "must keep one working",
                Some(5),
                180,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
        ])
        .expect("pack should succeed");

    assert!(packed
        .segments
        .iter()
        .any(|segment| segment.id == "working-1"));
    let reservation = packed
        .working_reservation
        .expect("working reservation should exist");
    assert_eq!(reservation.reserved_segment_id, "working-1");
    assert_eq!(reservation.reserved_tokens, 5);
    assert_eq!(
        reservation.reason,
        WorkingReservationReason::MinimumWorkingTokens
    );
}

#[test]
fn pack_keeps_only_most_recent_memory_segments_before_budget_ranking() {
    let packer = ContextPacker::new(ContextBudget {
        max_tokens: 200,
        reserve_system_tokens: 0,
        min_working_tokens: 0,
        max_tool_results: 5,
        max_memory_segments: 2,
    });

    let packed = packer
        .pack(vec![
            segment(
                "memory-old",
                SegmentSource::Memory,
                "old memory",
                Some(5),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "memory-mid",
                SegmentSource::Memory,
                "mid memory",
                Some(5),
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "memory-new",
                SegmentSource::Memory,
                "new memory",
                Some(5),
                100,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:03Z",
            ),
        ])
        .expect("pack should succeed");

    let memory_ids: Vec<String> = packed
        .segments
        .iter()
        .filter(|segment| matches!(segment.source, SegmentSource::Memory))
        .map(|segment| segment.id.clone())
        .collect();

    assert_eq!(memory_ids, vec!["memory-new", "memory-mid"]);
    assert!(packed.dropped_ids.iter().any(|id| id == "memory-old"));
}

#[test]
fn pack_marks_budget_exceeded_when_minimum_working_tokens_cannot_fit() {
    let packer = ContextPacker::new(budget(20, 10, 6));
    let _packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                "memory first",
                Some(10),
                180,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "working segment cannot fit",
                Some(6),
                170,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
        ])
        .expect("pack should succeed");
}

#[test]
fn pack_reserves_minimum_working_tokens_before_lower_priority_segments() {
    let packer = ContextPacker::new(budget(26, 10, 8));
    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                "memory first",
                Some(12),
                170,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "working segment should be reserved",
                Some(8),
                160,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
        ])
        .expect("pack should succeed");

    let ids: Vec<String> = packed
        .segments
        .iter()
        .map(|segment| segment.id.clone())
        .collect();
    assert!(ids.iter().any(|id| id == "working-1"));
    assert!(!ids.iter().any(|id| id == "memory-1"));
    assert!(packed.dropped_ids.iter().any(|id| id == "memory-1"));
    assert!(!packed.budget_exceeded);
}

#[test]
fn pack_records_first_version_pipeline_trace_and_rendered_prompt() {
    let packer = ContextPacker::new(ContextBudget {
        max_tokens: 24,
        reserve_system_tokens: 10,
        min_working_tokens: 5,
        max_tool_results: 1,
        max_memory_segments: 1,
    });

    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "tool-old",
                SegmentSource::ToolResult,
                "old tool",
                Some(3),
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "tool-new",
                SegmentSource::ToolResult,
                "new tool",
                Some(3),
                100,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "memory-old",
                SegmentSource::Memory,
                "old memory",
                Some(3),
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "memory-new",
                SegmentSource::Memory,
                "new memory",
                Some(3),
                100,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "work",
                Some(5),
                220,
                "2026-04-30T18:00:03Z",
                "2026-04-30T18:00:03Z",
            ),
        ])
        .expect("pack should succeed");

    let trace_names: Vec<&'static str> = packed.trace.iter().map(|step| step.name).collect();
    assert_eq!(
        trace_names,
        vec![
            "normalize_tokens",
            "dedupe",
            "trim",
            "rank",
            "reserve_working",
            "merge_under_budget"
        ]
    );
    assert!(packed
        .trace
        .iter()
        .any(|step| step.name == "trim" && step.dropped_count == 2));

    let rendered = packed.render_prompt();
    assert!(rendered.contains("pack_trace=normalize_tokens:6->6(-0),dedupe:6->6(-0),trim:6->4(-2)"));
    assert!(rendered.contains("drop_reasons=tool-old:tool_result_trim,memory-old:memory_trim"));
    assert!(rendered.contains("context_compaction_started"));
    assert!(rendered.contains("context_segment_dropped:tool-old:tool_result_trim:@trim"));
    assert!(rendered.contains("context_compaction_completed:packed:@merge_under_budget"));
    assert!(rendered.contains("- Working/p220 [working-1] work"));
}

#[test]
fn pack_records_structured_compaction_events_without_segment_content() {
    let packer = ContextPacker::new(ContextBudget {
        max_tokens: 22,
        reserve_system_tokens: 10,
        min_working_tokens: 5,
        max_tool_results: 1,
        max_memory_segments: 1,
    });

    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-secret-old",
                SegmentSource::Memory,
                "Authorization: Bearer should-not-appear",
                Some(3),
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "memory-new",
                SegmentSource::Memory,
                "new memory",
                Some(3),
                100,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "tool-old",
                SegmentSource::ToolResult,
                "tool secret=should-not-appear",
                Some(3),
                90,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "tool-new",
                SegmentSource::ToolResult,
                "new tool",
                Some(3),
                90,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "working segment",
                Some(5),
                220,
                "2026-04-30T18:00:03Z",
                "2026-04-30T18:00:03Z",
            ),
        ])
        .expect("pack should succeed");

    let event_kinds: Vec<&'static str> = packed
        .compaction_events
        .iter()
        .map(|event| event.kind.as_str())
        .collect();
    assert_eq!(event_kinds.first(), Some(&"context_compaction_started"));
    assert_eq!(event_kinds.last(), Some(&"context_compaction_completed"));
    assert!(packed.compaction_events.iter().any(|event| {
        event.kind == ContextCompactionEventKind::SegmentDropped
            && event.segment_id.as_deref() == Some("memory-secret-old")
            && event.reason.as_deref() == Some("memory_trim")
            && event.trace_step == Some("trim")
    }));
    assert!(packed.compaction_events.iter().any(|event| {
        event.kind == ContextCompactionEventKind::SegmentDropped
            && event.segment_id.as_deref() == Some("tool-old")
            && event.reason.as_deref() == Some("tool_result_trim")
            && event.trace_step == Some("trim")
    }));

    let rendered = packed.render_prompt();
    assert!(rendered.contains("compaction_events="));
    assert!(rendered.contains("context_segment_dropped:memory-secret-old:memory_trim:@trim"));
    assert!(!rendered.contains("should-not-appear"));
}

#[test]
fn compaction_summary_is_queryable_without_segment_content() {
    let packer = ContextPacker::new(ContextBudget {
        max_tokens: 22,
        reserve_system_tokens: 10,
        min_working_tokens: 5,
        max_tool_results: 1,
        max_memory_segments: 1,
    });

    let packed = packer
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(10),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-secret-old",
                SegmentSource::Memory,
                "Authorization: Bearer should-not-appear",
                Some(3),
                100,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "memory-new",
                SegmentSource::Memory,
                "new memory",
                Some(3),
                100,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "tool-old",
                SegmentSource::ToolResult,
                "tool secret=should-not-appear",
                Some(3),
                90,
                "2026-04-30T18:00:01Z",
                "2026-04-30T18:00:01Z",
            ),
            segment(
                "tool-new",
                SegmentSource::ToolResult,
                "new tool",
                Some(3),
                90,
                "2026-04-30T18:00:02Z",
                "2026-04-30T18:00:02Z",
            ),
            segment(
                "working-1",
                SegmentSource::Working,
                "working segment",
                Some(5),
                220,
                "2026-04-30T18:00:03Z",
                "2026-04-30T18:00:03Z",
            ),
        ])
        .expect("pack should succeed");

    let summary = packed.compaction_summary();
    assert_eq!(summary.started_count, 1);
    assert_eq!(summary.completed_count, 1);
    assert!(summary.dropped_count >= 2);
    assert!(summary
        .dropped_segment_ids
        .iter()
        .any(|segment_id| segment_id == "memory-secret-old"));
    assert!(summary
        .dropped_segment_ids
        .iter()
        .any(|segment_id| segment_id == "tool-old"));
    assert_eq!(summary.drop_reason_counts.get("memory_trim"), Some(&1));
    assert_eq!(summary.drop_reason_counts.get("tool_result_trim"), Some(&1));
    assert!(summary.trace_steps.contains(&"trim".to_string()));
    assert!(summary
        .trace_steps
        .contains(&"merge_under_budget".to_string()));

    let rendered = format!("{summary:?}");
    assert!(!rendered.contains("should-not-appear"));
    assert!(!rendered.contains("Authorization"));
    assert!(!rendered.contains("secret="));
}

#[test]
fn compaction_strategy_cascade_enum_orders_and_degrades() {
    use CompactionStrategy::*;
    assert_eq!(
        CompactionStrategy::CASCADE_ORDER,
        [Snip, Micro, Collapse, Auto, SessionMemory]
    );
    assert_eq!(Snip.level(), 1);
    assert_eq!(Micro.level(), 2);
    assert_eq!(Collapse.level(), 3);
    assert_eq!(Auto.level(), 4);
    assert_eq!(SessionMemory.level(), 5);

    assert_eq!(Auto.degrade(), Some(Collapse));
    assert_eq!(Collapse.degrade(), Some(Micro));
    assert_eq!(Micro.degrade(), Some(Snip));
    assert_eq!(Snip.degrade(), None);
    assert_eq!(SessionMemory.degrade(), Some(Auto));

    for strategy in CompactionStrategy::CASCADE_ORDER {
        assert_eq!(
            CompactionStrategy::from_str(strategy.as_str()),
            Some(strategy),
            "as_str/from_str should round-trip for {strategy:?}"
        );
    }
    assert_eq!(CompactionStrategy::from_str("bogus"), None);
}

#[test]
fn summary_compression_breaker_opens_after_three_consecutive_failures() {
    // 系统段超出 reserve 预算 → pack 失败，连续 3 次后熔断打开。
    let engine = SummaryCompressionContextEngine::new(budget(20, 30, 0))
        .with_circuit_breaker(3, 60);
    let failing = vec![segment(
        "system-1",
        SegmentSource::System,
        "system instruction",
        Some(30),
        255,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    )];

    for _ in 0..2 {
        let error = engine
            .pack(failing.clone())
            .expect_err("oversized system should fail pack");
        assert_eq!(
            error,
            ContextPackError::BudgetExceeded {
                required_system_tokens: 30,
                max_tokens: 20,
            }
        );
    }
    let status = engine.circuit_breaker_status();
    assert!(!status.open, "breaker should stay closed after 2 failures");
    assert_eq!(status.consecutive_failures, 2);

    let _ = engine
        .pack(failing)
        .expect_err("third failure should still fail pack");
    let status = engine.circuit_breaker_status();
    assert!(status.open, "breaker should open after 3 consecutive failures");
    assert_eq!(status.consecutive_failures, 3);
    assert_eq!(status.threshold, 3);
    assert!(status.opened_at.is_some());
    assert!(status.last_failure_at.is_some());
}

#[test]
fn summary_compression_breaker_skips_compression_while_open() {
    // 阈值 1：一次失败即熔断；随后可成功的 pack 跳过自动压缩（不截断、不标记压缩）。
    let engine =
        SummaryCompressionContextEngine::new(budget(1000, 10, 0))
            .with_circuit_breaker(1, 60);
    let failing = vec![segment(
        "system-1",
        SegmentSource::System,
        "system instruction",
        Some(30),
        255,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    )];
    let _ = engine.pack(failing).expect_err("failure opens breaker");
    assert!(engine.circuit_breaker_status().open);

    let packed = engine
        .pack(vec![
            segment(
                "system-ok",
                SegmentSource::System,
                "system",
                Some(5),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                &"memory-".repeat(20),
                Some(140),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("pack should succeed without compression");

    let memory = packed
        .segments
        .iter()
        .find(|segment| segment.id == "memory-1")
        .expect("memory segment should exist");
    assert!(
        !memory.content.ends_with("..."),
        "breaker open should skip compression so long memory stays raw"
    );
    assert!(memory
        .metadata
        .get("summary_compressed")
        .is_none(),);
    assert!(packed
        .compaction_events
        .iter()
        .any(|event| event.kind == ContextCompactionEventKind::CompressionSkipped
            && event.reason.as_deref() == Some("circuit_breaker_open")));

    let summary = packed.compaction_summary();
    assert_eq!(summary.compression_skipped_count, 1);
    assert_eq!(
        summary.compression_skipped_reasons,
        vec!["circuit_breaker_open".to_string()]
    );
    assert!(engine.circuit_breaker_status().skipped_compactions >= 1);
}

#[test]
fn summary_compression_breaker_success_resets_consecutive_failures() {
    let engine =
        SummaryCompressionContextEngine::new(budget(1000, 10, 0))
            .with_circuit_breaker(3, 60);
    let failing = vec![segment(
        "system-1",
        SegmentSource::System,
        "system instruction",
        Some(30),
        255,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    )];
    for _ in 0..2 {
        let _ = engine.pack(failing.clone()).expect_err("fail pack");
    }
    assert_eq!(engine.circuit_breaker_status().consecutive_failures, 2);

    let packed = engine
        .pack(vec![
            segment(
                "system-ok",
                SegmentSource::System,
                "system",
                Some(5),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                &"memory-".repeat(20),
                Some(140),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("pack should succeed");

    let memory = packed
        .segments
        .iter()
        .find(|segment| segment.id == "memory-1")
        .expect("memory segment should exist");
    assert!(
        memory.content.ends_with("..."),
        "success should reset breaker so compression runs again"
    );
    let status = engine.circuit_breaker_status();
    assert!(!status.open);
    assert_eq!(status.consecutive_failures, 0);
}

#[test]
fn summary_compression_breaker_manual_reset_and_cooldown_auto_reset() {
    // 手动重置：熔断打开后 reset 关闭并清零。
    let engine =
        SummaryCompressionContextEngine::new(budget(1000, 10, 0))
            .with_circuit_breaker(1, 60);
    let failing = vec![segment(
        "system-1",
        SegmentSource::System,
        "system instruction",
        Some(30),
        255,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    )];
    let _ = engine.pack(failing).expect_err("fail pack");
    assert!(engine.circuit_breaker_status().open);
    engine.reset_circuit_breaker();
    let status = engine.circuit_breaker_status();
    assert!(!status.open);
    assert_eq!(status.consecutive_failures, 0);
    assert!(status.opened_at.is_none());

    // 按配置冷却：cooldown=0 时熔断自动复位，下一次 pack 恢复压缩。
    let engine =
        SummaryCompressionContextEngine::new(budget(1000, 10, 0))
            .with_circuit_breaker(1, 0);
    let failing = vec![segment(
        "system-1",
        SegmentSource::System,
        "system instruction",
        Some(30),
        255,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    )];
    let _ = engine.pack(failing).expect_err("fail pack");
    assert!(engine.circuit_breaker_status().open);

    let packed = engine
        .pack(vec![
            segment(
                "system-ok",
                SegmentSource::System,
                "system",
                Some(5),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                &"memory-".repeat(20),
                Some(140),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("pack should succeed");
    let memory = packed
        .segments
        .iter()
        .find(|segment| segment.id == "memory-1")
        .expect("memory segment should exist");
    assert!(
        memory.content.ends_with("..."),
        "cooldown elapsed should auto-reset breaker and resume compression"
    );
    assert!(packed
        .compaction_events
        .iter()
        .all(|event| event.kind != ContextCompactionEventKind::CompressionSkipped));
    let status = engine.circuit_breaker_status();
    assert!(!status.open);
    assert_eq!(status.consecutive_failures, 0);
}

#[test]
fn summary_compression_strips_image_payloads_before_truncation() {
    let engine = SummaryCompressionContextEngine::new(budget(1000, 10, 0));
    let image = format!("data:image/png;base64,{}", "A".repeat(400));
    let content = format!("说明开头 {image} 说明结尾");
    let packed = engine
        .pack(vec![segment(
            "memory-1",
            SegmentSource::Memory,
            &content,
            None,
            100,
            "2026-04-30T18:00:00Z",
            "2026-04-30T18:00:00Z",
        )])
        .expect("pack should succeed");

    let memory = packed
        .segments
        .iter()
        .find(|segment| segment.id == "memory-1")
        .expect("memory segment should exist");
    assert!(
        memory.content.contains("[image]"),
        "base64 payload should become a text placeholder, got: {}",
        memory.content
    );
    assert!(
        !memory.content.contains("base64"),
        "no base64 payload should survive compression"
    );
    assert_eq!(
        memory.metadata.get("image_stripped").map(String::as_str),
        Some("true")
    );
    assert_eq!(packed.compaction_summary().image_stripped_count, 1);

    // markdown 图片与 image_url JSON 字段同样被 strip。
    let markdown = format!("![alt](data:image/png;base64,{})", "B".repeat(200));
    assert_eq!(strip_image_payloads(&markdown), "![alt]([image])");
    let json = format!("{{\"image_url\": \"data:image/png;base64,{}\"}}", "C".repeat(300));
    let stripped_json = strip_image_payloads(&json);
    assert!(stripped_json.contains("[image]"));
    assert!(!stripped_json.contains("base64"));
}

#[test]
fn summary_compression_recursion_guard_skips_already_summarized_segments() {
    let engine = SummaryCompressionContextEngine::new(budget(5000, 10, 0));
    let long_content = &"already-summarized-".repeat(30);

    let mut turn_summary = segment(
        "mem-turn-summary",
        SegmentSource::Memory,
        &format!("{long_content}-turn-summary"),
        None,
        100,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    );
    turn_summary
        .metadata
        .insert("kind".to_string(), "turn_summary".to_string());

    let mut compaction_source = segment(
        "mem-compaction-source",
        SegmentSource::Memory,
        &format!("{long_content}-compaction-source"),
        None,
        100,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    );
    compaction_source
        .metadata
        .insert("compaction_source".to_string(), "true".to_string());

    let mut compacted = segment(
        "mem-compacted",
        SegmentSource::Memory,
        &format!("{long_content}-compacted"),
        None,
        100,
        "2026-04-30T18:00:00Z",
        "2026-04-30T18:00:00Z",
    );
    compacted
        .metadata
        .insert("compacted".to_string(), "true".to_string());

    let packed = engine
        .pack(vec![turn_summary, compaction_source, compacted])
        .expect("pack should succeed");
    for id in [
        "mem-turn-summary",
        "mem-compaction-source",
        "mem-compacted",
    ] {
        let item = packed
            .segments
            .iter()
            .find(|segment| segment.id == id)
            .unwrap_or_else(|| panic!("{id} should be kept"));
        let expected = match id {
            "mem-turn-summary" => format!("{long_content}-turn-summary"),
            "mem-compaction-source" => format!("{long_content}-compaction-source"),
            "mem-compacted" => format!("{long_content}-compacted"),
            other => panic!("unexpected id {other}"),
        };
        assert_eq!(item.content.as_str(), expected.as_str(), "{id} must not be re-compressed");
        assert!(
            item.metadata.get("summary_compressed").is_none(),
            "{id} must not be marked as re-compressed"
        );
    }
}

#[test]
fn summary_compression_keeps_toolset_segments_untouched() {
    // 压缩 trigger 保持工具集不变：工具说明/系统段内容原样保留（保 prefix cache）。
    let engine = SummaryCompressionContextEngine::new(budget(1000, 10, 0));
    let tool_protocol = "tool instructions: exec_command runs bash, output is truncated by rtk";
    let packed = engine
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(5),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "tool-instructions",
                SegmentSource::Identity,
                tool_protocol,
                Some(80),
                252,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                &"memory-".repeat(20),
                Some(140),
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("pack should succeed");

    let tool = packed
        .segments
        .iter()
        .find(|segment| segment.id == "tool-instructions")
        .expect("tool instructions should be kept");
    assert_eq!(tool.content, tool_protocol, "toolset content must not change");
    assert!(tool.metadata.get("summary_compressed").is_none());
    let system = packed
        .segments
        .iter()
        .find(|segment| segment.id == "system-1")
        .expect("system segment should be kept");
    assert_eq!(system.content, "system instruction");
}

#[test]
fn compaction_summary_reports_strategy_and_strip_metadata() {
    let engine = SummaryCompressionContextEngine::new(budget(1000, 10, 0));
    let image = format!("data:image/jpeg;base64,{}", "D".repeat(300));
    let packed = engine
        .pack(vec![
            segment(
                "system-1",
                SegmentSource::System,
                "system instruction",
                Some(5),
                255,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
            segment(
                "memory-1",
                SegmentSource::Memory,
                &format!("before {image} after"),
                None,
                100,
                "2026-04-30T18:00:00Z",
                "2026-04-30T18:00:00Z",
            ),
        ])
        .expect("pack should succeed");

    let summary = packed.compaction_summary();
    assert_eq!(summary.strategy, "auto");
    assert_eq!(summary.image_stripped_count, 1);
    assert_eq!(summary.compression_skipped_count, 0);
    assert!(summary.compression_skipped_reasons.is_empty());

    // 确定性引擎无压缩 → strategy=none。
    let deterministic = DeterministicContextEngine::new(budget(1000, 10, 0));
    let packed = deterministic
        .pack(vec![segment(
            "memory-1",
            SegmentSource::Memory,
            "short memory",
            Some(12),
            100,
            "2026-04-30T18:00:00Z",
            "2026-04-30T18:00:00Z",
        )])
        .expect("pack should succeed");
    assert_eq!(packed.compaction_summary().strategy, "none");
}

#[test]
fn summary_compression_engine_exposes_strategy_and_breaker_config() {
    let engine = SummaryCompressionContextEngine::with_recent_turns(budget(1000, 10, 0), 3)
        .with_compaction_strategy(CompactionStrategy::Micro)
        .with_circuit_breaker(5, 120);
    assert_eq!(engine.compaction_strategy(), CompactionStrategy::Micro);
    let status = engine.circuit_breaker_status();
    assert_eq!(status.threshold, 5);
    assert_eq!(status.cooldown_secs, 120);
    assert!(!status.open);
}
