use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

pub const RUNTIME_EVENT_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeEventKind {
    ThreadStarted,
    TurnStarted,
    ContextPacked,
    ProviderRequested,
    ProviderResponded,
    RiskClassified,
    MemoryProposed,
    MemoryCommitted,
    SkillProposed,
    SkillSolidified,
    ToolStarted,
    ToolFinished,
    ApprovalRequested,
    ApprovalResolved,
    ElicitationRequested,
    SubagentSpawned,
    SubagentReported,
    TurnCompleted,
    TurnFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRiskDecision {
    pub decision: String,
    pub reason: String,
    pub policy_ref: Option<String>,
}

impl RuntimeRiskDecision {
    pub fn new(decision: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            decision: decision.into(),
            reason: reason.into(),
            policy_ref: None,
        }
    }

    pub fn with_policy_ref(mut self, policy_ref: impl Into<String>) -> Self {
        self.policy_ref = Some(policy_ref.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub schema_version: u16,
    pub event_type: RuntimeEventKind,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub call_id: Option<String>,
    pub created_at: String,
    pub risk_decision: Option<RuntimeRiskDecision>,
    pub evidence_ref: Option<String>,
}

impl RuntimeEvent {
    pub fn new(event_type: RuntimeEventKind, thread_id: impl Into<String>) -> Self {
        Self::at(event_type, thread_id, current_timestamp())
    }

    pub fn at(
        event_type: RuntimeEventKind,
        thread_id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            schema_version: RUNTIME_EVENT_SCHEMA_VERSION,
            event_type,
            thread_id: thread_id.into(),
            turn_id: None,
            call_id: None,
            created_at: created_at.into(),
            risk_decision: None,
            evidence_ref: None,
        }
    }

    pub fn with_turn_id(mut self, turn_id: impl Into<String>) -> Self {
        self.turn_id = Some(turn_id.into());
        self
    }

    pub fn with_call_id(mut self, call_id: impl Into<String>) -> Self {
        self.call_id = Some(call_id.into());
        self
    }

    pub fn with_risk_decision(mut self, risk_decision: RuntimeRiskDecision) -> Self {
        self.risk_decision = Some(risk_decision);
        self
    }

    pub fn with_evidence_ref(mut self, evidence_ref: impl Into<String>) -> Self {
        self.evidence_ref = Some(evidence_ref.into());
        self
    }
}

fn current_timestamp() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub trait RuntimeEventLedger {
    fn append(&mut self, event: RuntimeEvent) -> Result<(), RuntimeEventLedgerError>;
    fn list(&self) -> Result<Vec<RuntimeEvent>, RuntimeEventLedgerError>;

    fn query_by_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<Vec<RuntimeEvent>, RuntimeEventLedgerError> {
        let events = self.list()?;
        Ok(events
            .into_iter()
            .filter(|event| {
                event.thread_id == thread_id && event.turn_id.as_deref() == Some(turn_id)
            })
            .collect())
    }

    fn query_by_call(&self, call_id: &str) -> Result<Vec<RuntimeEvent>, RuntimeEventLedgerError> {
        let events = self.list()?;
        Ok(events
            .into_iter()
            .filter(|event| event.call_id.as_deref() == Some(call_id))
            .collect())
    }

    fn summarize_turn(
        &self,
        thread_id: &str,
        turn_id: &str,
    ) -> Result<RuntimeTurnSummary, RuntimeEventLedgerError> {
        let events = self.query_by_turn(thread_id, turn_id)?;
        Ok(RuntimeTurnSummary::from_events(thread_id, turn_id, &events))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeTurnSummary {
    pub thread_id: String,
    pub turn_id: String,
    pub event_count: usize,
    pub tool_started_count: usize,
    pub tool_finished_count: usize,
    pub approval_requested_count: usize,
    pub approval_resolved_count: usize,
    pub elicitation_requested_count: usize,
    pub risk_decision_count: usize,
    pub evidence_ref_count: usize,
    pub call_count: usize,
    pub first_created_at: Option<String>,
    pub last_created_at: Option<String>,
    pub event_types: Vec<RuntimeEventKind>,
}

impl RuntimeTurnSummary {
    pub fn from_events(thread_id: &str, turn_id: &str, events: &[RuntimeEvent]) -> Self {
        let mut event_types = Vec::new();
        let mut tool_started_count = 0usize;
        let mut tool_finished_count = 0usize;
        let mut approval_requested_count = 0usize;
        let mut approval_resolved_count = 0usize;
        let mut elicitation_requested_count = 0usize;
        let mut risk_decision_count = 0usize;
        let mut evidence_ref_count = 0usize;
        let mut call_count = 0usize;
        for event in events {
            event_types.push(event.event_type.clone());
            match event.event_type {
                RuntimeEventKind::ToolStarted => tool_started_count += 1,
                RuntimeEventKind::ToolFinished => tool_finished_count += 1,
                RuntimeEventKind::ApprovalRequested => approval_requested_count += 1,
                RuntimeEventKind::ApprovalResolved => approval_resolved_count += 1,
                RuntimeEventKind::ElicitationRequested => elicitation_requested_count += 1,
                _ => {}
            }
            if event.risk_decision.is_some() {
                risk_decision_count += 1;
            }
            if event.evidence_ref.is_some() {
                evidence_ref_count += 1;
            }
            if event.call_id.is_some() {
                call_count += 1;
            }
        }

        Self {
            thread_id: thread_id.to_string(),
            turn_id: turn_id.to_string(),
            event_count: events.len(),
            tool_started_count,
            tool_finished_count,
            approval_requested_count,
            approval_resolved_count,
            elicitation_requested_count,
            risk_decision_count,
            evidence_ref_count,
            call_count,
            first_created_at: events.first().map(|event| event.created_at.clone()),
            last_created_at: events.last().map(|event| event.created_at.clone()),
            event_types,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct InMemoryRuntimeEventLedger {
    events: Vec<RuntimeEvent>,
}

impl InMemoryRuntimeEventLedger {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn into_events(self) -> Vec<RuntimeEvent> {
        self.events
    }
}

impl RuntimeEventLedger for InMemoryRuntimeEventLedger {
    fn append(&mut self, event: RuntimeEvent) -> Result<(), RuntimeEventLedgerError> {
        self.events.push(event);
        Ok(())
    }

    fn list(&self) -> Result<Vec<RuntimeEvent>, RuntimeEventLedgerError> {
        Ok(self.events.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JsonlRuntimeEventLedger {
    path: PathBuf,
}

impl JsonlRuntimeEventLedger {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl RuntimeEventLedger for JsonlRuntimeEventLedger {
    fn append(&mut self, event: RuntimeEvent) -> Result<(), RuntimeEventLedgerError> {
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(|source| RuntimeEventLedgerError::Io {
                action: "open_append",
                path: self.path.clone(),
                source,
            })?;
        let line =
            serde_json::to_string(&event).map_err(RuntimeEventLedgerError::SerializeEvent)?;
        file.write_all(line.as_bytes())
            .and_then(|_| file.write_all(b"\n"))
            .map_err(|source| RuntimeEventLedgerError::Io {
                action: "write_event",
                path: self.path.clone(),
                source,
            })
    }

    fn list(&self) -> Result<Vec<RuntimeEvent>, RuntimeEventLedgerError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }

        let file = File::open(&self.path).map_err(|source| RuntimeEventLedgerError::Io {
            action: "open_read",
            path: self.path.clone(),
            source,
        })?;
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        for (index, line) in reader.lines().enumerate() {
            let line = line.map_err(|source| RuntimeEventLedgerError::Io {
                action: "read_line",
                path: self.path.clone(),
                source,
            })?;
            if line.trim().is_empty() {
                continue;
            }
            let event = serde_json::from_str::<RuntimeEvent>(&line).map_err(|source| {
                RuntimeEventLedgerError::DeserializeEvent {
                    path: self.path.clone(),
                    line: index + 1,
                    source,
                }
            })?;
            events.push(event);
        }
        Ok(events)
    }
}

#[derive(Debug)]
pub enum RuntimeEventLedgerError {
    Io {
        action: &'static str,
        path: PathBuf,
        source: std::io::Error,
    },
    SerializeEvent(serde_json::Error),
    DeserializeEvent {
        path: PathBuf,
        line: usize,
        source: serde_json::Error,
    },
}

impl fmt::Display for RuntimeEventLedgerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeEventLedgerError::Io {
                action,
                path,
                source,
            } => write!(
                f,
                "runtime_event_ledger_io_failed action={action} path={} error={source}",
                path.display()
            ),
            RuntimeEventLedgerError::SerializeEvent(source) => {
                write!(f, "runtime_event_serialize_failed error={source}")
            }
            RuntimeEventLedgerError::DeserializeEvent { path, line, source } => write!(
                f,
                "runtime_event_deserialize_failed path={} line={line} error={source}",
                path.display()
            ),
        }
    }
}

impl std::error::Error for RuntimeEventLedgerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RuntimeEventLedgerError::Io { source, .. } => Some(source),
            RuntimeEventLedgerError::SerializeEvent(source) => Some(source),
            RuntimeEventLedgerError::DeserializeEvent { source, .. } => Some(source),
        }
    }
}
