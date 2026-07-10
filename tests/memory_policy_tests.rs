use chuang_agent::common::{AgentId, AllocationId, TaskId, Timestamp};
use chuang_agent::memory_policy::{
    ActiveAllocation, AdmissionDecision, AdmissionRequest, BudgetConfig, BudgetManager, BudgetMode,
    CommitError, EvictionPlan, FreedBytes, MemoryAdmissionPolicy, ReservationToken,
};

fn sample_policy() -> MemoryAdmissionPolicy {
    MemoryAdmissionPolicy {
        config: BudgetConfig {
            total_budget_bytes: 1024,
            reserved_system_bytes: 128,
            reservation_ttl_ms: 5_000,
            mode: BudgetMode::HardLimit,
        },
        active_allocations: vec![],
        reservations: vec![],
        next_allocation_seq: 1,
    }
}

fn sample_request(id: &str, bytes: u64, priority: u8) -> AdmissionRequest {
    AdmissionRequest {
        task_id: TaskId(format!("task-{}", id)),
        agent_id: AgentId(format!("agent-{}", id)),
        requested_bytes: bytes,
        priority,
        requested_at: Timestamp("2026-04-30T10:30:00.123Z".to_string()),
    }
}

fn reserve_at(policy: &mut MemoryAdmissionPolicy, request: &AdmissionRequest) -> ReservationToken {
    policy
        .try_reserve_at(request, Timestamp("2026-04-30T10:30:00.000Z".to_string()))
        .expect("reservation should succeed")
}

fn commit_at(policy: &mut MemoryAdmissionPolicy, token: ReservationToken) -> AllocationId {
    policy
        .commit_at(token, "2026-04-30T10:30:01.000Z")
        .expect("commit should succeed")
}

#[test]
fn budget_config_defaults_can_express_hard_limit() {
    let policy = sample_policy();

    assert!(matches!(policy.config.mode, BudgetMode::HardLimit));
    assert_eq!(policy.active_allocations.len(), 0);
}

#[test]
fn admission_decision_can_capture_eviction_plan() {
    let decision = AdmissionDecision::Degrade {
        granted_bytes: 256,
        evict: vec![AllocationId("alloc-1".to_string())],
    };

    assert!(matches!(decision, AdmissionDecision::Degrade { .. }));
}

#[test]
fn request_and_reservation_token_hold_timing_metadata() {
    let request = sample_request("1", 256, 1);
    let token = ReservationToken {
        id: "reserve-1".to_string(),
        task_id: request.task_id.clone(),
        agent_id: request.agent_id.clone(),
        granted_bytes: 256,
        priority: request.priority,
        requested_at: request.requested_at.clone(),
        expires_at: Timestamp("2026-04-30T10:30:05.123Z".to_string()),
    };
    let active = ActiveAllocation {
        allocation_id: AllocationId("alloc-1".to_string()),
        task_id: request.task_id.clone(),
        agent_id: request.agent_id.clone(),
        allocated_bytes: 256,
        priority: request.priority,
        started_at: Timestamp("2026-04-30T10:30:01.123Z".to_string()),
    };

    assert_eq!(token.granted_bytes, active.allocated_bytes);
    assert_eq!(active.task_id.0, "task-1");
}

#[test]
fn try_reserve_adds_pending_reservation_without_active_allocation() {
    let mut policy = sample_policy();
    let request = sample_request("1", 256, 1);

    let token = reserve_at(&mut policy, &request);

    assert_eq!(token.granted_bytes, 256);
    assert_eq!(token.expires_at.0, "2026-04-30T10:30:05.000Z");
    assert_eq!(policy.active_allocations.len(), 0);
    assert_eq!(policy.reservations.len(), 1);
}

#[test]
fn commit_moves_reservation_into_active_allocations() {
    let mut policy = sample_policy();
    let request = sample_request("1", 256, 1);
    let token = reserve_at(&mut policy, &request);

    let allocation_id = commit_at(&mut policy, token);

    assert_eq!(allocation_id.0, "alloc-1");
    assert_eq!(policy.reservations.len(), 0);
    assert_eq!(policy.active_allocations.len(), 1);
    assert_eq!(policy.active_allocations[0].allocated_bytes, 256);
}

#[test]
fn release_reservation_clears_pending_reservation() {
    let mut policy = sample_policy();
    let request = sample_request("1", 256, 1);
    let token = reserve_at(&mut policy, &request);

    policy.release_reservation(token);

    assert!(policy.reservations.is_empty());
    assert!(policy.active_allocations.is_empty());
}

#[test]
fn reclaim_releases_active_allocation_bytes() {
    let mut policy = sample_policy();
    let request = sample_request("1", 256, 1);
    let token = reserve_at(&mut policy, &request);
    let _allocation_id = commit_at(&mut policy, token);

    let freed = policy
        .reclaim(
            &TaskId("task-1".to_string()),
            &AgentId("agent-1".to_string()),
        )
        .expect("reclaim should succeed");

    assert_eq!(freed, FreedBytes(256));
    assert!(policy.active_allocations.is_empty());
}

#[test]
fn commit_rejects_unknown_reservation() {
    let mut policy = sample_policy();
    let token = ReservationToken {
        id: "missing".to_string(),
        task_id: TaskId("task-1".to_string()),
        agent_id: AgentId("agent-1".to_string()),
        granted_bytes: 256,
        priority: 1,
        requested_at: Timestamp("2026-04-30T10:30:00.123Z".to_string()),
        expires_at: Timestamp("2026-04-30T10:30:05.123Z".to_string()),
    };

    let err = policy.commit(token).expect_err("commit should fail");

    assert_eq!(err, CommitError::ReservationExpired);
}

#[test]
fn duplicate_reclaim_is_idempotent() {
    let mut policy = sample_policy();
    let request = sample_request("1", 256, 1);
    let token = reserve_at(&mut policy, &request);
    commit_at(&mut policy, token);

    let first = policy
        .reclaim(
            &TaskId("task-1".to_string()),
            &AgentId("agent-1".to_string()),
        )
        .expect("first reclaim should succeed");
    let duplicate = policy
        .reclaim(
            &TaskId("task-1".to_string()),
            &AgentId("agent-1".to_string()),
        )
        .expect("duplicate reclaim should be idempotent");

    assert_eq!(first, FreedBytes(256));
    assert_eq!(duplicate, FreedBytes(0));
}

#[test]
fn reservation_token_can_report_expiry_state() {
    let token = ReservationToken {
        id: "reserve-1".to_string(),
        task_id: TaskId("task-1".to_string()),
        agent_id: AgentId("agent-1".to_string()),
        granted_bytes: 256,
        priority: 1,
        requested_at: Timestamp("2026-04-30T10:30:00.123Z".to_string()),
        expires_at: Timestamp("2026-04-30T10:30:05.123Z".to_string()),
    };

    assert!(token.is_expired_at("2026-04-30T10:30:06.000Z"));
    assert!(!token.is_expired_at("2026-04-30T10:30:04.000Z"));
}

#[test]
fn eviction_plan_tracks_candidate_totals() {
    let plan = EvictionPlan {
        candidates: vec![
            AllocationId("alloc-1".to_string()),
            AllocationId("alloc-2".to_string()),
        ],
        expected_freed_bytes: 512,
    };

    assert_eq!(plan.candidate_count(), 2);
    assert_eq!(plan.expected_freed_bytes, 512);
}

#[test]
fn admit_with_eviction_replaces_candidates_with_new_allocation() {
    let mut policy = sample_policy();
    let low1 = sample_request("1", 200, 1);
    let low2 = sample_request("2", 200, 1);
    let token1 = reserve_at(&mut policy, &low1);
    let token2 = reserve_at(&mut policy, &low2);
    let alloc1 = commit_at(&mut policy, token1);
    let alloc2 = commit_at(&mut policy, token2);

    let high = sample_request("high", 500, 9);
    let decision = policy
        .admit_with_eviction(&high, &[alloc1.clone(), alloc2.clone()])
        .expect("eviction should succeed");

    assert_eq!(decision, AdmissionDecision::Grant { granted_bytes: 500 });
    assert_eq!(policy.active_allocations.len(), 1);
    assert_eq!(policy.active_allocations[0].task_id.0, "task-high");
    assert_eq!(policy.active_allocations[0].allocated_bytes, 500);
}

#[test]
fn admit_with_eviction_fails_when_candidate_missing() {
    let mut policy = sample_policy();
    let high = sample_request("high", 500, 9);

    let err = policy
        .admit_with_eviction(&high, &[AllocationId("missing".to_string())])
        .expect_err("missing candidate should fail");

    assert_eq!(
        err,
        chuang_agent::memory_policy::DenyReason::CandidateNotEvictable
    );
    assert!(policy.active_allocations.is_empty());
}

#[test]
fn admit_with_eviction_rolls_back_when_reclaimed_budget_is_insufficient() {
    let mut policy = sample_policy();
    let low = sample_request("low", 400, 1);
    let token = reserve_at(&mut policy, &low);
    let allocation_id = commit_at(&mut policy, token);
    let before = policy.clone();

    let too_large = sample_request("large", 900, 9);
    let err = policy
        .admit_with_eviction(&too_large, &[allocation_id])
        .expect_err("insufficient eviction should fail");

    assert_eq!(err, chuang_agent::memory_policy::DenyReason::BudgetExceeded);
    assert_eq!(policy, before);
}

#[test]
fn admit_with_eviction_rolls_back_when_candidate_list_contains_duplicates() {
    let mut policy = sample_policy();
    let low = sample_request("low", 400, 1);
    let token = reserve_at(&mut policy, &low);
    let allocation_id = commit_at(&mut policy, token);
    let before = policy.clone();

    let high = sample_request("high", 500, 9);
    let err = policy
        .admit_with_eviction(&high, &[allocation_id.clone(), allocation_id])
        .expect_err("duplicate candidate should fail validation");

    assert_eq!(
        err,
        chuang_agent::memory_policy::DenyReason::CandidateNotEvictable
    );
    assert_eq!(policy, before);
}

#[test]
fn reservation_ttl_uses_injected_now_and_commit_checks_expiry_boundary() {
    let mut policy = sample_policy();
    let request = sample_request("ttl", 256, 1);
    let token = reserve_at(&mut policy, &request);

    assert_eq!(token.expires_at.0, "2026-04-30T10:30:05.000Z");
    let err = policy
        .commit_at(token.clone(), "2026-04-30T10:30:05.000Z")
        .expect_err("reservation should expire at the boundary");
    assert_eq!(err, CommitError::ReservationExpired);
    assert_eq!(policy.reservations, vec![token]);
    assert!(policy.active_allocations.is_empty());
}

#[test]
fn expire_reservations_releases_only_expired_tokens() {
    let mut policy = sample_policy();
    let expired = ReservationToken {
        id: "reserve-expired".to_string(),
        task_id: TaskId("task-expired".to_string()),
        agent_id: AgentId("agent-expired".to_string()),
        granted_bytes: 128,
        priority: 1,
        requested_at: Timestamp("2026-04-30T10:30:00.000Z".to_string()),
        expires_at: Timestamp("2026-04-30T10:30:01.000Z".to_string()),
    };
    let alive = ReservationToken {
        id: "reserve-alive".to_string(),
        task_id: TaskId("task-alive".to_string()),
        agent_id: AgentId("agent-alive".to_string()),
        granted_bytes: 64,
        priority: 1,
        requested_at: Timestamp("2026-04-30T10:30:00.000Z".to_string()),
        expires_at: Timestamp("2026-04-30T10:30:09.000Z".to_string()),
    };
    policy.reservations = vec![expired, alive.clone()];

    let released = policy.expire_reservations_at("2026-04-30T10:30:05.000Z");

    assert_eq!(released, FreedBytes(128));
    assert_eq!(policy.reservations, vec![alive]);
}
