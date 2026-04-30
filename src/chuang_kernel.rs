use std::collections::BTreeMap;

use crate::agent_runtime::{AgentRuntime, AgentRuntimeError, RuntimeRequest, RuntimeResult};
use crate::context_engine::ContextBudget;
use crate::memory_admission::{
    preview_chars, MemoryEntryView, TextMemoryAdmission, TextMemoryAdmissionDecision,
};
use crate::memory_store::{MemoryQuery, MemoryRecord, MemoryStore, MemoryStoreError};
use crate::responder::{FakeResponder, Responder};
use crate::runtime_report::build_runtime_report;
use crate::subagent_report::SubagentReport;
use serde::Serialize;

pub use crate::memory_admission::DEFAULT_MEMORY_WRITE_MAX_CHARS;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelConfig {
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: Option<ContextBudget>,
    pub memory_write_max_chars: Option<usize>,
}

impl ChuangKernelConfig {
    pub fn mvp_default(agent_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            parent_agent_id: None,
            recall_limit: 5,
            metadata: BTreeMap::new(),
            context_budget: None,
            memory_write_max_chars: Some(DEFAULT_MEMORY_WRITE_MAX_CHARS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelTurn {
    pub turn_id: String,
    pub user_input: String,
    pub result: RuntimeResult,
    pub report: SubagentReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChuangKernelSnapshot {
    pub agent_id: String,
    pub turn_count: u64,
    pub recall_limit: usize,
    pub metadata_keys: Vec<String>,
    pub context_budget_max_tokens: Option<u16>,
    pub memory_write_max_chars: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChuangKernelMemoryError {
    Store(MemoryStoreError),
    HardLimitExceeded {
        limit_chars: usize,
        attempted_chars: usize,
        existing_entries: Vec<MemoryEntryView>,
    },
}

pub struct ChuangKernel<S, R = FakeResponder> {
    config: ChuangKernelConfig,
    runtime: AgentRuntime<S, R>,
    turn_count: u64,
}

impl<S> ChuangKernel<S, FakeResponder> {
    pub fn new(config: ChuangKernelConfig, store: S) -> Self {
        Self::with_responder(config, store, FakeResponder::new("stub-responder"))
    }
}

impl<S, R> ChuangKernel<S, R> {
    pub fn with_responder(config: ChuangKernelConfig, store: S, responder: R) -> Self {
        Self {
            config,
            runtime: AgentRuntime::with_responder(store, responder),
            turn_count: 0,
        }
    }

    pub fn snapshot(&self) -> ChuangKernelSnapshot {
        ChuangKernelSnapshot {
            agent_id: self.config.agent_id.clone(),
            turn_count: self.turn_count,
            recall_limit: self.config.recall_limit,
            metadata_keys: self.config.metadata.keys().cloned().collect(),
            context_budget_max_tokens: self
                .config
                .context_budget
                .as_ref()
                .map(|budget| budget.max_tokens),
            memory_write_max_chars: self.config.memory_write_max_chars,
        }
    }
}

impl<S: MemoryStore, R: Responder> ChuangKernel<S, R> {
    pub fn run_turn(
        &mut self,
        user_input: impl Into<String>,
    ) -> Result<ChuangKernelTurn, AgentRuntimeError> {
        let next_turn = self.turn_count + 1;
        let turn_id = format!("turn-{next_turn}");
        let user_input = user_input.into();
        let result = self.runtime.run(&RuntimeRequest {
            user_input: user_input.clone(),
            recall_limit: self.config.recall_limit,
            metadata: self.config.metadata.clone(),
            context_budget: self.config.context_budget.clone(),
        })?;
        let report = build_runtime_report(
            &result,
            format!("report-{turn_id}"),
            turn_id.clone(),
            self.config.agent_id.clone(),
            self.config.parent_agent_id.clone(),
        );
        self.turn_count = next_turn;

        Ok(ChuangKernelTurn {
            turn_id,
            user_input,
            result,
            report,
        })
    }

    pub fn remember_turn(
        &mut self,
        turn: &ChuangKernelTurn,
    ) -> Result<String, ChuangKernelMemoryError> {
        let record_id = format!("turn-memory-{}", turn.turn_id);
        let content = format!(
            "user={}\nresponse={}\nsummary={}",
            turn.user_input, turn.result.response.body, turn.report.summary
        );

        if let Some(limit_chars) = self.config.memory_write_max_chars {
            match TextMemoryAdmission::new(limit_chars)
                .evaluate(&content, self.existing_turn_summary_entries()?)
            {
                TextMemoryAdmissionDecision::Accepted => {}
                TextMemoryAdmissionDecision::Rejected {
                    limit_chars,
                    attempted_chars,
                    existing_entries,
                } => {
                    return Err(ChuangKernelMemoryError::HardLimitExceeded {
                        limit_chars,
                        attempted_chars,
                        existing_entries,
                    });
                }
            }
        }

        self.runtime
            .memory_store_mut()
            .put(MemoryRecord {
                id: record_id.clone(),
                content,
                metadata: BTreeMap::from([
                    ("kind".to_string(), "turn_summary".to_string()),
                    ("agent_id".to_string(), self.config.agent_id.clone()),
                    ("turn_id".to_string(), turn.turn_id.clone()),
                ]),
                created_at: "2026-05-01T00:00:00Z".to_string(),
                expires_at: None,
            })
            .map_err(ChuangKernelMemoryError::Store)?;
        Ok(record_id)
    }

    fn existing_turn_summary_entries(
        &mut self,
    ) -> Result<Vec<MemoryEntryView>, ChuangKernelMemoryError> {
        let hits = self
            .runtime
            .memory_store_mut()
            .search(&MemoryQuery {
                text: None,
                metadata: BTreeMap::from([("kind".to_string(), "turn_summary".to_string())]),
                limit: 1000,
            })
            .map_err(ChuangKernelMemoryError::Store)?;

        Ok(hits
            .into_iter()
            .map(|hit| MemoryEntryView {
                id: hit.record.id,
                chars: hit.record.content.chars().count(),
                content_preview: preview_chars(&hit.record.content, 80),
            })
            .collect())
    }
}
