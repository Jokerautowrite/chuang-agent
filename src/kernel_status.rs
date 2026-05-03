use crate::atomic_tool::{ga_atomic_tool_manifests, AtomicToolManifest, AtomicToolStatus};
use crate::chuang_kernel::{ChuangKernelConfig, ChuangKernelSnapshot};
use crate::plugin_registry::{summarize_plugin_registry, PluginRegistrySummary};
use crate::runtime_config::{ConfigError, ConfigSummary, RuntimeConfig};
use crate::slot_registry::{summarize_runtime_slots, RuntimeSlotsSummary};
use crate::tool_runtime::ToolLoopReport;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChuangMvpStatus {
    pub config: ConfigSummary,
    pub slots: RuntimeSlotsSummary,
    pub kernel: ChuangKernelSnapshot,
    pub plugin_registry: PluginRegistrySummary,
    pub atomic_tools: AtomicToolSurfaceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtomicToolSurfaceStatus {
    pub source: String,
    pub ok: bool,
    pub total_count: usize,
    pub mapped_count: usize,
    pub interface_only_count: usize,
    pub tool_report_schema_version: u16,
    pub tool_report_schema_fields: Vec<String>,
    pub tool_call_schema_fields: Vec<String>,
    pub manifests: Vec<AtomicToolManifest>,
}

pub fn build_chuang_mvp_status(
    config: &RuntimeConfig,
    kernel: &ChuangKernelConfig,
) -> Result<ChuangMvpStatus, ConfigError> {
    config.validate()?;

    let atomic_manifests = ga_atomic_tool_manifests();
    let mapped_count = atomic_manifests
        .iter()
        .filter(|tool| tool.status == AtomicToolStatus::Mapped)
        .count();
    let interface_only_count = atomic_manifests
        .iter()
        .filter(|tool| tool.status == AtomicToolStatus::InterfaceOnly)
        .count();

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
            memory_write_max_chars: kernel.memory_write_max_chars,
            identity_user_chars: kernel
                .identity_snapshot
                .as_ref()
                .map(|snapshot| snapshot.user.chars().count()),
            identity_memory_chars: kernel
                .identity_snapshot
                .as_ref()
                .map(|snapshot| snapshot.memory.chars().count()),
            identity_soul_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.soul.chars().count()),
            identity_story_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.story.chars().count()),
            identity_first_wake_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.first_wake.chars().count()),
            identity_agents_registry_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.agents_registry.chars().count()),
        },
        plugin_registry: summarize_plugin_registry(Path::new("plugins/registry.example.json")),
        atomic_tools: AtomicToolSurfaceStatus {
            source: "GenericAgent".to_string(),
            ok: atomic_manifests.len() == 9 && mapped_count == 3 && interface_only_count == 6,
            total_count: atomic_manifests.len(),
            mapped_count,
            interface_only_count,
            tool_report_schema_version: ToolLoopReport::schema_version(),
            tool_report_schema_fields: ToolLoopReport::schema_fields()
                .iter()
                .map(|field| field.to_string())
                .collect(),
            tool_call_schema_fields: ToolLoopReport::call_schema_fields()
                .iter()
                .map(|field| field.to_string())
                .collect(),
            manifests: atomic_manifests,
        },
    })
}
