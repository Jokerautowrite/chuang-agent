use serde::{Deserialize, Serialize};

use crate::runtime_event_ledger::{RuntimeEvent, RuntimeEventKind};
use crate::subagent_tree_ledger::{
    AgentRole, ReportAdmissionRef, SpawnEdge, SubagentChildrenSummary, SubagentThreadRecord,
    SubagentTreeStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTreeEventBuilder {
    pub created_at: String,
    pub turn_id: Option<String>,
}

impl SubagentTreeEventBuilder {
    pub fn new(created_at: impl Into<String>) -> Self {
        Self {
            created_at: created_at.into(),
            turn_id: None,
        }
    }

    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        self.turn_id = Some(turn_id.into());
        self
    }

    pub fn spawn_event(
        &self,
        root_thread_id: impl Into<String>,
        edge: &SpawnEdge,
    ) -> SubagentTreeRuntimeEvent {
        let root_thread_id = root_thread_id.into();
        let evidence_ref = spawn_evidence_ref(
            &root_thread_id,
            &edge.parent_thread_id,
            &edge.child_thread_id,
        );
        let runtime_event = self
            .base_event(
                RuntimeEventKind::SubagentSpawned,
                edge.child_thread_id.clone(),
                evidence_ref.clone(),
            )
            .with_call_id(format!("subagent-spawn:{}", edge.child_thread_id));

        SubagentTreeRuntimeEvent {
            schema_version: 1,
            bridge_event_kind: SubagentTreeBridgeEventKind::SubagentSpawned,
            runtime_event,
            root_thread_id,
            parent_thread_id: Some(edge.parent_thread_id.clone()),
            child_thread_id: edge.child_thread_id.clone(),
            agent_role: edge.agent_role.clone(),
            nickname: edge.nickname.clone(),
            status: edge.status.clone(),
            admission_id: None,
            report_id: None,
            admission_status: None,
            admission_reason_code: None,
            evidence_ref,
        }
    }

    pub fn report_event(&self, record: &SubagentThreadRecord) -> SubagentTreeRuntimeEvent {
        let evidence_ref = admission_evidence_ref(record.report_admission_ref.as_ref())
            .unwrap_or_else(|| record_evidence_ref(record, "report"));
        let runtime_event = self
            .base_event(
                RuntimeEventKind::SubagentReported,
                record.relation.thread_id.clone(),
                evidence_ref.clone(),
            )
            .with_call_id(format!("subagent-report:{}", record.relation.thread_id));
        self.event_from_record(
            SubagentTreeBridgeEventKind::SubagentReported,
            runtime_event,
            record,
            evidence_ref,
        )
    }

    pub fn close_event(&self, record: &SubagentThreadRecord) -> SubagentTreeRuntimeEvent {
        let evidence_ref = record_evidence_ref(record, "close");
        let runtime_event = self
            .base_event(
                RuntimeEventKind::ToolFinished,
                record.relation.thread_id.clone(),
                evidence_ref.clone(),
            )
            .with_call_id(format!("subagent-close:{}", record.relation.thread_id));
        self.event_from_record(
            SubagentTreeBridgeEventKind::SubagentClosed,
            runtime_event,
            record,
            evidence_ref,
        )
    }

    pub fn message_sent_event(
        &self,
        record: &SubagentThreadRecord,
        message_id: impl Into<String>,
    ) -> SubagentTreeRuntimeEvent {
        let message_id = sanitize_event_segment(&message_id.into());
        let evidence_ref = record_evidence_ref(record, &format!("message/{message_id}"));
        let runtime_event = self
            .base_event(
                RuntimeEventKind::ToolStarted,
                record.relation.thread_id.clone(),
                evidence_ref.clone(),
            )
            .with_call_id(format!(
                "subagent-message:{}:{message_id}",
                record.relation.thread_id
            ));
        self.event_from_record(
            SubagentTreeBridgeEventKind::SubagentMessageSent,
            runtime_event,
            record,
            evidence_ref,
        )
    }

    pub fn wait_started_event(
        &self,
        record: &SubagentThreadRecord,
        wait_id: impl Into<String>,
    ) -> SubagentTreeRuntimeEvent {
        let wait_id = sanitize_event_segment(&wait_id.into());
        let evidence_ref = record_evidence_ref(record, &format!("wait/{wait_id}"));
        let runtime_event = self
            .base_event(
                RuntimeEventKind::ToolStarted,
                record.relation.thread_id.clone(),
                evidence_ref.clone(),
            )
            .with_call_id(format!(
                "subagent-wait:{}:{wait_id}",
                record.relation.thread_id
            ));
        self.event_from_record(
            SubagentTreeBridgeEventKind::SubagentWaitStarted,
            runtime_event,
            record,
            evidence_ref,
        )
    }

    pub fn list_event(
        &self,
        root_thread_id: impl Into<String>,
        parent_thread_id: impl Into<String>,
        children: &[SubagentThreadRecord],
    ) -> SubagentTreeListRuntimeEvent {
        let root_thread_id = root_thread_id.into();
        let parent_thread_id = parent_thread_id.into();
        let evidence_ref = format!(
            "subagent-tree://{}/children/{}",
            root_thread_id, parent_thread_id
        );
        let runtime_event = self
            .base_event(
                RuntimeEventKind::ToolFinished,
                parent_thread_id.clone(),
                evidence_ref.clone(),
            )
            .with_call_id(format!("subagent-list-children:{parent_thread_id}"));
        let children_summary = SubagentChildrenSummary::from_children(&parent_thread_id, children);

        SubagentTreeListRuntimeEvent {
            schema_version: 1,
            bridge_event_kind: SubagentTreeBridgeEventKind::SubagentChildrenListed,
            runtime_event,
            consistency_warnings: list_consistency_warnings(
                &root_thread_id,
                &parent_thread_id,
                children,
            ),
            root_thread_id,
            parent_thread_id,
            children_summary,
            child_count: children.len(),
            children: children
                .iter()
                .map(SubagentTreeChildSnapshot::from_record)
                .collect(),
            evidence_ref,
        }
    }

    fn base_event(
        &self,
        kind: RuntimeEventKind,
        thread_id: String,
        evidence_ref: String,
    ) -> RuntimeEvent {
        let mut event = RuntimeEvent::at(kind, thread_id, self.created_at.clone())
            .with_evidence_ref(evidence_ref);
        if let Some(turn_id) = &self.turn_id {
            event = event.with_turn_id(turn_id.clone());
        }
        event
    }

    fn event_from_record(
        &self,
        bridge_event_kind: SubagentTreeBridgeEventKind,
        runtime_event: RuntimeEvent,
        record: &SubagentThreadRecord,
        evidence_ref: String,
    ) -> SubagentTreeRuntimeEvent {
        let admission = record.report_admission_ref.as_ref();
        SubagentTreeRuntimeEvent {
            schema_version: 1,
            bridge_event_kind,
            runtime_event,
            root_thread_id: record.relation.root_thread_id.clone(),
            parent_thread_id: record.relation.parent_thread_id.clone(),
            child_thread_id: record.relation.thread_id.clone(),
            agent_role: record.agent_role.clone(),
            nickname: record.nickname.clone(),
            status: record.status.clone(),
            admission_id: admission.map(|value| value.admission_id.clone()),
            report_id: admission.and_then(|value| value.report_id.clone()),
            admission_status: admission.map(|value| value.status.clone()),
            admission_reason_code: admission.map(|value| value.reason_code.clone()),
            evidence_ref,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentTreeBridgeEventKind {
    SubagentSpawned,
    SubagentMessageSent,
    SubagentWaitStarted,
    SubagentReported,
    SubagentClosed,
    SubagentChildrenListed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTreeRuntimeEvent {
    pub schema_version: u16,
    pub bridge_event_kind: SubagentTreeBridgeEventKind,
    pub runtime_event: RuntimeEvent,
    pub root_thread_id: String,
    pub parent_thread_id: Option<String>,
    pub child_thread_id: String,
    pub agent_role: AgentRole,
    pub nickname: String,
    pub status: SubagentTreeStatus,
    pub admission_id: Option<String>,
    pub report_id: Option<String>,
    pub admission_status: Option<String>,
    pub admission_reason_code: Option<String>,
    pub evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTreeListRuntimeEvent {
    pub schema_version: u16,
    pub bridge_event_kind: SubagentTreeBridgeEventKind,
    pub runtime_event: RuntimeEvent,
    pub consistency_warnings: Vec<String>,
    pub root_thread_id: String,
    pub parent_thread_id: String,
    pub children_summary: SubagentChildrenSummary,
    pub child_count: usize,
    pub children: Vec<SubagentTreeChildSnapshot>,
    pub evidence_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubagentTreeChildSnapshot {
    pub child_thread_id: String,
    pub parent_thread_id: Option<String>,
    pub root_thread_id: String,
    pub agent_role: AgentRole,
    pub nickname: String,
    pub status: SubagentTreeStatus,
    pub admission_id: Option<String>,
    pub report_id: Option<String>,
    pub admission_status: Option<String>,
    pub admission_reason_code: Option<String>,
    pub evidence_ref: Option<String>,
}

impl SubagentTreeChildSnapshot {
    fn from_record(record: &SubagentThreadRecord) -> Self {
        let admission = record.report_admission_ref.as_ref();
        Self {
            child_thread_id: record.relation.thread_id.clone(),
            parent_thread_id: record.relation.parent_thread_id.clone(),
            root_thread_id: record.relation.root_thread_id.clone(),
            agent_role: record.agent_role.clone(),
            nickname: record.nickname.clone(),
            status: record.status.clone(),
            admission_id: admission.map(|value| value.admission_id.clone()),
            report_id: admission.and_then(|value| value.report_id.clone()),
            admission_status: admission.map(|value| value.status.clone()),
            admission_reason_code: admission.map(|value| value.reason_code.clone()),
            evidence_ref: admission.and_then(|value| value.evidence_ref.clone()),
        }
    }
}

pub fn subagent_spawned_event(
    created_at: impl Into<String>,
    root_thread_id: impl Into<String>,
    edge: &SpawnEdge,
) -> SubagentTreeRuntimeEvent {
    SubagentTreeEventBuilder::new(created_at).spawn_event(root_thread_id, edge)
}

pub fn subagent_reported_event(
    created_at: impl Into<String>,
    record: &SubagentThreadRecord,
) -> SubagentTreeRuntimeEvent {
    SubagentTreeEventBuilder::new(created_at).report_event(record)
}

pub fn subagent_closed_event(
    created_at: impl Into<String>,
    record: &SubagentThreadRecord,
) -> SubagentTreeRuntimeEvent {
    SubagentTreeEventBuilder::new(created_at).close_event(record)
}

pub fn subagent_message_sent_event(
    created_at: impl Into<String>,
    record: &SubagentThreadRecord,
    message_id: impl Into<String>,
) -> SubagentTreeRuntimeEvent {
    SubagentTreeEventBuilder::new(created_at).message_sent_event(record, message_id)
}

pub fn subagent_wait_started_event(
    created_at: impl Into<String>,
    record: &SubagentThreadRecord,
    wait_id: impl Into<String>,
) -> SubagentTreeRuntimeEvent {
    SubagentTreeEventBuilder::new(created_at).wait_started_event(record, wait_id)
}

fn spawn_evidence_ref(
    root_thread_id: &str,
    parent_thread_id: &str,
    child_thread_id: &str,
) -> String {
    format!("subagent-tree://{root_thread_id}/spawn/{parent_thread_id}/{child_thread_id}")
}

fn record_evidence_ref(record: &SubagentThreadRecord, action: &str) -> String {
    format!(
        "subagent-tree://{}/record/{}/{}",
        record.relation.root_thread_id, action, record.relation.thread_id
    )
}

fn sanitize_event_segment(value: &str) -> String {
    let sanitized: String = value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '_'
            }
        })
        .take(96)
        .collect();
    if sanitized.is_empty() {
        "unknown".to_string()
    } else {
        sanitized
    }
}

fn admission_evidence_ref(admission: Option<&ReportAdmissionRef>) -> Option<String> {
    admission.and_then(|value| value.evidence_ref.clone())
}

fn list_consistency_warnings(
    root_thread_id: &str,
    parent_thread_id: &str,
    children: &[SubagentThreadRecord],
) -> Vec<String> {
    let mut warnings = Vec::new();
    for child in children {
        if child.relation.root_thread_id != root_thread_id {
            warnings.push(format!(
                "child_root_mismatch child_thread_id={} expected={} actual={}",
                child.relation.thread_id, root_thread_id, child.relation.root_thread_id
            ));
        }
        if child.relation.parent_thread_id.as_deref() != Some(parent_thread_id) {
            warnings.push(format!(
                "child_parent_mismatch child_thread_id={} expected={} actual={}",
                child.relation.thread_id,
                parent_thread_id,
                child
                    .relation
                    .parent_thread_id
                    .as_deref()
                    .unwrap_or("<none>")
            ));
        }
    }
    warnings
}
