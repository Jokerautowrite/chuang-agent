//! `common::audit` 模块。公开接口：trait Auditable, IdempotentKey；struct AuditRecord。

use crate::common::{AgentId, TaskId, Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
