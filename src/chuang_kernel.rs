use std::collections::BTreeMap;

use crate::agent_runtime::{AgentRuntime, AgentRuntimeError, RuntimeRequest, RuntimeResult};
use crate::context_engine::ContextBudget;
use crate::memory_store::MemoryStore;
use crate::responder::{FakeResponder, Responder};
use crate::runtime_report::build_runtime_report;
use crate::subagent_report::SubagentReport;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelConfig {
    pub agent_id: String,
    pub parent_agent_id: Option<String>,
    pub recall_limit: usize,
    pub metadata: BTreeMap<String, String>,
    pub context_budget: Option<ContextBudget>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelTurn {
    pub turn_id: String,
    pub result: RuntimeResult,
    pub report: SubagentReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChuangKernelSnapshot {
    pub agent_id: String,
    pub turn_count: u64,
    pub recall_limit: usize,
    pub metadata_keys: Vec<String>,
    pub context_budget_max_tokens: Option<u16>,
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
        let result = self.runtime.run(&RuntimeRequest {
            user_input: user_input.into(),
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
            result,
            report,
        })
    }
}
