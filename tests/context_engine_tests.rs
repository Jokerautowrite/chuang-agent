use std::collections::HashMap;

use chrono::{DateTime, Utc};
use chuang_agent::context_engine::{
    ContextBudget, ContextPackError, ContextPacker, ContextSegment, SegmentSource,
    WorkingReservationReason,
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
    tokens: Option<u16>,
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

fn budget(max_tokens: u16, reserve_system_tokens: u16, min_working_tokens: u16) -> ContextBudget {
    ContextBudget {
        max_tokens,
        reserve_system_tokens,
        min_working_tokens,
        max_tool_results: 5,
        max_memory_segments: 20,
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
