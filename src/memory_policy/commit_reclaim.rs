use crate::common::{AgentId, AllocationId, TaskId, Timestamp};
use crate::memory_policy::{
    ActiveAllocation, AdmissionDecision, AdmissionRequest, BudgetConfig, DenyReason,
    MemoryAdmissionPolicy, ReservationToken,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FreedBytes(pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitError {
    ReservationExpired,
    ConcurrentModification,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReclaimError {
    AllocationNotFound { task_id: TaskId, agent_id: AgentId },
}

pub trait BudgetManager: Send + Sync {
    fn try_reserve(&mut self, request: &AdmissionRequest) -> Result<ReservationToken, DenyReason>;
    fn commit(&mut self, token: ReservationToken) -> Result<AllocationId, CommitError>;
    fn release_reservation(&mut self, token: ReservationToken);
    fn reclaim(&mut self, task_id: &TaskId, agent_id: &AgentId)
        -> Result<FreedBytes, ReclaimError>;
    fn admit_with_eviction(
        &mut self,
        request: &AdmissionRequest,
        evict_candidates: &[AllocationId],
    ) -> Result<AdmissionDecision, DenyReason>;
    fn active_allocations(&self) -> Vec<ActiveAllocation>;
    fn budget_config(&self) -> BudgetConfig;
}

impl MemoryAdmissionPolicy {
    fn available_bytes(&self) -> u64 {
        let active_sum: u64 = self
            .active_allocations
            .iter()
            .map(|allocation| allocation.allocated_bytes)
            .sum();
        let reserved_sum: u64 = self
            .reservations
            .iter()
            .map(|reservation| reservation.granted_bytes)
            .sum();

        self.config
            .total_budget_bytes
            .saturating_sub(self.config.reserved_system_bytes)
            .saturating_sub(active_sum)
            .saturating_sub(reserved_sum)
    }

    pub fn expire_reservations_at(&mut self, now: &str) -> FreedBytes {
        let mut released = 0;
        self.reservations.retain(|reservation| {
            let expired = reservation.is_expired_at(now);
            if expired {
                released += reservation.granted_bytes;
            }
            !expired
        });
        FreedBytes(released)
    }

    fn evict_allocations(&mut self, evict_candidates: &[AllocationId]) -> Result<u64, DenyReason> {
        let mut reclaimed = 0;

        for candidate in evict_candidates {
            let Some(index) = self
                .active_allocations
                .iter()
                .position(|allocation| &allocation.allocation_id == candidate)
            else {
                return Err(DenyReason::CandidateNotEvictable);
            };
            reclaimed += self.active_allocations[index].allocated_bytes;
        }

        self.active_allocations
            .retain(|allocation| !evict_candidates.contains(&allocation.allocation_id));

        Ok(reclaimed)
    }
}

impl BudgetManager for MemoryAdmissionPolicy {
    fn try_reserve(&mut self, request: &AdmissionRequest) -> Result<ReservationToken, DenyReason> {
        if request.requested_bytes > self.available_bytes() {
            return Err(DenyReason::BudgetExceeded);
        }

        let token = ReservationToken {
            id: format!("reserve-{}", self.reservations.len() + 1),
            task_id: request.task_id.clone(),
            agent_id: request.agent_id.clone(),
            granted_bytes: request.requested_bytes,
            priority: request.priority,
            requested_at: request.requested_at.clone(),
            expires_at: Timestamp(format!("{}+ttl", request.requested_at.0)),
        };
        self.reservations.push(token.clone());
        Ok(token)
    }

    fn commit(&mut self, token: ReservationToken) -> Result<AllocationId, CommitError> {
        let Some(index) = self
            .reservations
            .iter()
            .position(|reservation| reservation.id == token.id)
        else {
            return Err(CommitError::ReservationExpired);
        };

        let token = self.reservations.remove(index);
        let allocation_id = AllocationId(format!("alloc-{}", self.next_allocation_seq));
        self.next_allocation_seq += 1;
        self.active_allocations.push(ActiveAllocation {
            allocation_id: allocation_id.clone(),
            task_id: token.task_id,
            agent_id: token.agent_id,
            allocated_bytes: token.granted_bytes,
            priority: token.priority,
            started_at: token.requested_at,
        });

        Ok(allocation_id)
    }

    fn release_reservation(&mut self, token: ReservationToken) {
        if let Some(index) = self
            .reservations
            .iter()
            .position(|reservation| reservation.id == token.id)
        {
            self.reservations.remove(index);
        }
    }

    fn reclaim(
        &mut self,
        task_id: &TaskId,
        agent_id: &AgentId,
    ) -> Result<FreedBytes, ReclaimError> {
        let Some(index) = self.active_allocations.iter().position(|allocation| {
            &allocation.task_id == task_id && &allocation.agent_id == agent_id
        }) else {
            return Err(ReclaimError::AllocationNotFound {
                task_id: task_id.clone(),
                agent_id: agent_id.clone(),
            });
        };

        let allocation = self.active_allocations.remove(index);
        Ok(FreedBytes(allocation.allocated_bytes))
    }

    fn admit_with_eviction(
        &mut self,
        request: &AdmissionRequest,
        evict_candidates: &[AllocationId],
    ) -> Result<AdmissionDecision, DenyReason> {
        let reclaimed = self.evict_allocations(evict_candidates)?;
        if request.requested_bytes > self.available_bytes().saturating_add(reclaimed) {
            return Err(DenyReason::BudgetExceeded);
        }

        let allocation_id = AllocationId(format!("alloc-{}", self.next_allocation_seq));
        self.next_allocation_seq += 1;
        self.active_allocations.push(ActiveAllocation {
            allocation_id,
            task_id: request.task_id.clone(),
            agent_id: request.agent_id.clone(),
            allocated_bytes: request.requested_bytes,
            priority: request.priority,
            started_at: request.requested_at.clone(),
        });

        Ok(AdmissionDecision::Grant {
            granted_bytes: request.requested_bytes,
        })
    }

    fn active_allocations(&self) -> Vec<ActiveAllocation> {
        self.active_allocations.clone()
    }

    fn budget_config(&self) -> BudgetConfig {
        self.config.clone()
    }
}
