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
