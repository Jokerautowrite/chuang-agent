//! `common::mod` 模块。公开接口：use audit, id, timestamp。

mod audit;
mod id;
mod timestamp;

pub use audit::{AuditRecord, Auditable, IdempotentKey};
pub use id::{AgentId, AllocationId, ReportId, TaskId};
pub use timestamp::Timestamp;
