//! `memory_policy::eviction` 模块。公开接口：struct EvictionPlan；fn candidate_count。

use crate::common::AllocationId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvictionPlan {
    pub candidates: Vec<AllocationId>,
    pub expected_freed_bytes: u64,
}

impl EvictionPlan {
    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }
}
