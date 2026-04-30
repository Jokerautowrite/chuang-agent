use crate::common::{AgentId, TaskId, Timestamp};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditRecord {
    pub operation: String,
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub delta_bytes: i64,
    pub reason: String,
    pub timestamp: Timestamp,
}

pub trait Auditable {
    fn audit_log(&self) -> AuditRecord;
}

pub trait IdempotentKey {
    fn idempotency_key(&self) -> String;
}
