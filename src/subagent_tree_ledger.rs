//! `subagent_tree_ledger` 模块。公开接口：trait SubagentTreeLedger；struct SubagentTreePolicy, ThreadRelation, SpawnEdge, ReportAdmissionRef, SubagentThreadRecord, SubagentChildrenSummary, SpawnRequest, SpawnValidation；enum AgentRole, SubagentTreeStatus, SubagentTreeLedgerError；fn new, is_open, root_thread_id, policy, get, records, summarize_children, validate_spawn。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTreePolicy {
    pub max_depth: u16,
    pub max_concurrent_children: u16,
}

impl SubagentTreePolicy {
    pub fn new(max_depth: u16, max_concurrent_children: u16) -> Self {
        Self {
            max_depth,
            max_concurrent_children,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRelation {
    pub root_thread_id: String,
    pub parent_thread_id: Option<String>,
    pub thread_id: String,
    pub depth: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnEdge {
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub depth: u16,
    pub agent_role: AgentRole,
    pub nickname: String,
    pub status: SubagentTreeStatus,
    pub report_admission_ref: Option<ReportAdmissionRef>,
    pub sequence: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgentRole {
    Root,
    Analyze,
    Execute,
    Orchestrate,
    Reviewer,
    Custom(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentTreeStatus {
    Root,
    Spawned,
    Running,
    Reported,
    Closed,
    Failed,
}

impl SubagentTreeStatus {
    pub fn is_open(&self) -> bool {
        matches!(self, Self::Spawned | Self::Running | Self::Reported)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReportAdmissionRef {
    pub admission_id: String,
    pub report_id: Option<String>,
    pub status: String,
    pub reason_code: String,
    pub evidence_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentThreadRecord {
    pub relation: ThreadRelation,
    pub agent_role: AgentRole,
    pub nickname: String,
    pub status: SubagentTreeStatus,
    pub report_admission_ref: Option<ReportAdmissionRef>,
    pub spawn_edge: Option<SpawnEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentChildrenSummary {
    pub parent_thread_id: String,
    pub child_count: usize,
    pub open_child_count: usize,
    pub reported_child_count: usize,
    pub closed_child_count: usize,
    pub accepted_report_count: usize,
    pub rejected_report_count: usize,
    pub missing_report_count: usize,
    pub child_thread_ids: Vec<String>,
    #[serde(default)]
    pub report_admission_refs: Vec<ReportAdmissionRef>,
    pub report_reason_codes: BTreeMap<String, usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnRequest {
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub agent_role: AgentRole,
    pub nickname: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpawnValidation {
    pub parent_thread_id: String,
    pub child_thread_id: String,
    pub depth: u16,
    pub open_sibling_count: u16,
    pub max_depth: u16,
    pub max_concurrent_children: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SubagentTreeLedgerError {
    EmptyThreadId {
        field: String,
    },
    EmptyNickname,
    UnknownParent {
        parent_thread_id: String,
    },
    DuplicateThread {
        thread_id: String,
    },
    DepthLimitExceeded {
        parent_thread_id: String,
        child_thread_id: String,
        attempted_depth: u16,
        max_depth: u16,
    },
    ConcurrentLimitExceeded {
        parent_thread_id: String,
        open_child_count: u16,
        max_concurrent_children: u16,
    },
    UnknownThread {
        thread_id: String,
    },
    CannotCloseRoot {
        thread_id: String,
    },
}

pub trait SubagentTreeLedger {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnEdge, SubagentTreeLedgerError>;
    fn register_report(
        &mut self,
        thread_id: &str,
        report_admission_ref: ReportAdmissionRef,
    ) -> Result<SubagentThreadRecord, SubagentTreeLedgerError>;
    fn close(&mut self, thread_id: &str) -> Result<SubagentThreadRecord, SubagentTreeLedgerError>;
    fn list_children(&self, parent_thread_id: &str) -> Vec<SubagentThreadRecord>;
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InMemorySubagentTreeLedger {
    policy: SubagentTreePolicy,
    root_thread_id: String,
    next_sequence: u64,
    records: BTreeMap<String, SubagentThreadRecord>,
}

impl InMemorySubagentTreeLedger {
    pub fn new(
        root_thread_id: impl Into<String>,
        policy: SubagentTreePolicy,
    ) -> Result<Self, SubagentTreeLedgerError> {
        let root_thread_id = normalize_required(root_thread_id.into(), "root_thread_id")?;
        let root = SubagentThreadRecord {
            relation: ThreadRelation {
                root_thread_id: root_thread_id.clone(),
                parent_thread_id: None,
                thread_id: root_thread_id.clone(),
                depth: 0,
            },
            agent_role: AgentRole::Root,
            nickname: "root".to_string(),
            status: SubagentTreeStatus::Root,
            report_admission_ref: None,
            spawn_edge: None,
        };
        let mut records = BTreeMap::new();
        records.insert(root_thread_id.clone(), root);
        Ok(Self {
            policy,
            root_thread_id,
            next_sequence: 1,
            records,
        })
    }

    pub fn root_thread_id(&self) -> &str {
        &self.root_thread_id
    }

    pub fn policy(&self) -> &SubagentTreePolicy {
        &self.policy
    }

    pub fn get(&self, thread_id: &str) -> Option<&SubagentThreadRecord> {
        self.records.get(thread_id)
    }

    pub fn records(&self) -> Vec<SubagentThreadRecord> {
        self.records.values().cloned().collect()
    }

    pub fn summarize_children(&self, parent_thread_id: &str) -> SubagentChildrenSummary {
        let children = self.list_children(parent_thread_id);
        SubagentChildrenSummary::from_children(parent_thread_id, &children)
    }

    pub fn validate_spawn(
        &self,
        request: &SpawnRequest,
    ) -> Result<SpawnValidation, SubagentTreeLedgerError> {
        let parent_thread_id =
            normalize_required(request.parent_thread_id.clone(), "parent_thread_id")?;
        let child_thread_id =
            normalize_required(request.child_thread_id.clone(), "child_thread_id")?;
        if request.nickname.trim().is_empty() {
            return Err(SubagentTreeLedgerError::EmptyNickname);
        }
        let parent = self.records.get(&parent_thread_id).ok_or_else(|| {
            SubagentTreeLedgerError::UnknownParent {
                parent_thread_id: parent_thread_id.clone(),
            }
        })?;
        if self.records.contains_key(&child_thread_id) {
            return Err(SubagentTreeLedgerError::DuplicateThread {
                thread_id: child_thread_id,
            });
        }
        let depth = parent.relation.depth.saturating_add(1);
        if depth > self.policy.max_depth {
            return Err(SubagentTreeLedgerError::DepthLimitExceeded {
                parent_thread_id,
                child_thread_id,
                attempted_depth: depth,
                max_depth: self.policy.max_depth,
            });
        }
        let open_sibling_count = self.open_child_count(&parent_thread_id);
        if open_sibling_count >= self.policy.max_concurrent_children {
            return Err(SubagentTreeLedgerError::ConcurrentLimitExceeded {
                parent_thread_id,
                open_child_count: open_sibling_count,
                max_concurrent_children: self.policy.max_concurrent_children,
            });
        }
        Ok(SpawnValidation {
            parent_thread_id,
            child_thread_id,
            depth,
            open_sibling_count,
            max_depth: self.policy.max_depth,
            max_concurrent_children: self.policy.max_concurrent_children,
        })
    }

    fn open_child_count(&self, parent_thread_id: &str) -> u16 {
        self.records
            .values()
            .filter(|record| {
                record.relation.parent_thread_id.as_deref() == Some(parent_thread_id)
                    && record.status.is_open()
            })
            .count()
            .min(usize::from(u16::MAX)) as u16
    }
}

impl SubagentChildrenSummary {
    pub fn from_children(parent_thread_id: &str, children: &[SubagentThreadRecord]) -> Self {
        let mut open_child_count = 0usize;
        let mut reported_child_count = 0usize;
        let mut closed_child_count = 0usize;
        let mut accepted_report_count = 0usize;
        let mut rejected_report_count = 0usize;
        let mut missing_report_count = 0usize;
        let mut child_thread_ids = Vec::new();
        let mut report_admission_refs = Vec::new();
        let mut report_reason_codes = BTreeMap::new();

        for child in children {
            child_thread_ids.push(child.relation.thread_id.clone());
            if child.status.is_open() {
                open_child_count += 1;
            }
            if child.status == SubagentTreeStatus::Reported {
                reported_child_count += 1;
            }
            if child.status == SubagentTreeStatus::Closed {
                closed_child_count += 1;
            }
            if let Some(admission) = &child.report_admission_ref {
                match admission.status.as_str() {
                    "Accepted" => accepted_report_count += 1,
                    "Rejected" => rejected_report_count += 1,
                    _ => {}
                }
                *report_reason_codes
                    .entry(admission.reason_code.clone())
                    .or_insert(0) += 1;
                report_admission_refs.push(admission.clone());
            } else {
                missing_report_count += 1;
            }
        }

        Self {
            parent_thread_id: parent_thread_id.to_string(),
            child_count: children.len(),
            open_child_count,
            reported_child_count,
            closed_child_count,
            accepted_report_count,
            rejected_report_count,
            missing_report_count,
            child_thread_ids,
            report_admission_refs,
            report_reason_codes,
        }
    }
}

impl SubagentTreeLedger for InMemorySubagentTreeLedger {
    fn spawn(&mut self, request: SpawnRequest) -> Result<SpawnEdge, SubagentTreeLedgerError> {
        let validation = self.validate_spawn(&request)?;
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        let edge = SpawnEdge {
            parent_thread_id: validation.parent_thread_id.clone(),
            child_thread_id: validation.child_thread_id.clone(),
            depth: validation.depth,
            agent_role: request.agent_role.clone(),
            nickname: request.nickname.trim().to_string(),
            status: SubagentTreeStatus::Spawned,
            report_admission_ref: None,
            sequence,
        };
        let record = SubagentThreadRecord {
            relation: ThreadRelation {
                root_thread_id: self.root_thread_id.clone(),
                parent_thread_id: Some(validation.parent_thread_id),
                thread_id: validation.child_thread_id.clone(),
                depth: validation.depth,
            },
            agent_role: request.agent_role,
            nickname: request.nickname.trim().to_string(),
            status: SubagentTreeStatus::Spawned,
            report_admission_ref: None,
            spawn_edge: Some(edge.clone()),
        };
        self.records.insert(validation.child_thread_id, record);
        Ok(edge)
    }

    fn register_report(
        &mut self,
        thread_id: &str,
        report_admission_ref: ReportAdmissionRef,
    ) -> Result<SubagentThreadRecord, SubagentTreeLedgerError> {
        let record = self.records.get_mut(thread_id).ok_or_else(|| {
            SubagentTreeLedgerError::UnknownThread {
                thread_id: thread_id.to_string(),
            }
        })?;
        record.status = SubagentTreeStatus::Reported;
        record.report_admission_ref = Some(report_admission_ref.clone());
        if let Some(edge) = &mut record.spawn_edge {
            edge.status = SubagentTreeStatus::Reported;
            edge.report_admission_ref = Some(report_admission_ref);
        }
        Ok(record.clone())
    }

    fn close(&mut self, thread_id: &str) -> Result<SubagentThreadRecord, SubagentTreeLedgerError> {
        if thread_id == self.root_thread_id {
            return Err(SubagentTreeLedgerError::CannotCloseRoot {
                thread_id: thread_id.to_string(),
            });
        }
        let record = self.records.get_mut(thread_id).ok_or_else(|| {
            SubagentTreeLedgerError::UnknownThread {
                thread_id: thread_id.to_string(),
            }
        })?;
        record.status = SubagentTreeStatus::Closed;
        if let Some(edge) = &mut record.spawn_edge {
            edge.status = SubagentTreeStatus::Closed;
        }
        Ok(record.clone())
    }

    fn list_children(&self, parent_thread_id: &str) -> Vec<SubagentThreadRecord> {
        let mut children = self
            .records
            .values()
            .filter(|record| record.relation.parent_thread_id.as_deref() == Some(parent_thread_id))
            .cloned()
            .collect::<Vec<_>>();
        children.sort_by_key(|record| {
            record
                .spawn_edge
                .as_ref()
                .map(|edge| edge.sequence)
                .unwrap_or(0)
        });
        children
    }
}

fn normalize_required(
    value: String,
    field: &'static str,
) -> Result<String, SubagentTreeLedgerError> {
    let trimmed = value.trim().to_string();
    if trimmed.is_empty() {
        return Err(SubagentTreeLedgerError::EmptyThreadId {
            field: field.to_string(),
        });
    }
    Ok(trimmed)
}
