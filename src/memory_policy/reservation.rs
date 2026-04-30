use crate::common::Timestamp;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReservationToken {
    pub id: String,
    pub task_id: crate::common::TaskId,
    pub agent_id: crate::common::AgentId,
    pub granted_bytes: u64,
    pub priority: u8,
    pub requested_at: Timestamp,
    pub expires_at: Timestamp,
}

impl ReservationToken {
    pub fn is_expired_at(&self, now: &str) -> bool {
        self.expires_at.0.as_str() <= now
    }
}
