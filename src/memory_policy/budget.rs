use crate::common::{AgentId, AllocationId, TaskId, Timestamp};
use crate::memory_policy::ReservationToken;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetMode {
    HardLimit,
    SoftLimitWithEviction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BudgetConfig {
    pub total_budget_bytes: u64,
    pub reserved_system_bytes: u64,
    pub reservation_ttl_ms: u64,
    pub mode: BudgetMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionRequest {
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub requested_bytes: u64,
    pub priority: u8,
    pub requested_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveAllocation {
    pub allocation_id: AllocationId,
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub allocated_bytes: u64,
    pub priority: u8,
    pub started_at: Timestamp,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DenyReason {
    BudgetExceeded,
    ReservationExpired,
    ConcurrentModification,
    CandidateNotEvictable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionDecision {
    Grant {
        granted_bytes: u64,
    },
    Degrade {
        granted_bytes: u64,
        evict: Vec<AllocationId>,
    },
    Deny(DenyReason),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryAdmissionPolicy {
    pub config: BudgetConfig,
    pub active_allocations: Vec<ActiveAllocation>,
    pub reservations: Vec<ReservationToken>,
    pub next_allocation_seq: u64,
}
