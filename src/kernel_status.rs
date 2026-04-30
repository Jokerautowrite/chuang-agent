use crate::chuang_kernel::{ChuangKernelConfig, ChuangKernelSnapshot};
use crate::runtime_config::{ConfigError, ConfigSummary, RuntimeConfig};
use crate::slot_registry::{summarize_runtime_slots, RuntimeSlotsSummary};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChuangMvpStatus {
    pub config: ConfigSummary,
    pub slots: RuntimeSlotsSummary,
    pub kernel: ChuangKernelSnapshot,
}

pub fn build_chuang_mvp_status(
    config: &RuntimeConfig,
    kernel: &ChuangKernelConfig,
) -> Result<ChuangMvpStatus, ConfigError> {
    config.validate()?;

    Ok(ChuangMvpStatus {
        config: config.summary(),
        slots: summarize_runtime_slots(config),
        kernel: ChuangKernelSnapshot {
            agent_id: kernel.agent_id.clone(),
            turn_count: 0,
            recall_limit: kernel.recall_limit,
            metadata_keys: kernel.metadata.keys().cloned().collect(),
            context_budget_max_tokens: kernel
                .context_budget
                .as_ref()
                .map(|budget| budget.max_tokens),
        },
    })
}
