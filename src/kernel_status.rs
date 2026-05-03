use crate::atomic_tool::{ga_atomic_tool_manifests, AtomicToolManifest, AtomicToolStatus};
use crate::chuang_kernel::{ChuangKernelConfig, ChuangKernelSnapshot};
use crate::goal_mode::GoalSpec;
use crate::plugin_registry::{summarize_plugin_registry, PluginRegistrySummary};
use crate::runtime_config::{ConfigError, ConfigSummary, RuntimeConfig};
use crate::slot_registry::{summarize_runtime_slots, RuntimeSlotsSummary};
use crate::tool_runtime::{ToolActionEnvelope, ToolLoopReport};
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChuangMvpStatus {
    pub config: ConfigSummary,
    pub slots: RuntimeSlotsSummary,
    pub kernel: ChuangKernelSnapshot,
    pub plugin_registry: PluginRegistrySummary,
    pub atomic_tools: AtomicToolSurfaceStatus,
    pub goal_mode: GoalModeStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtomicToolSurfaceStatus {
    pub source: String,
    pub ok: bool,
    pub total_count: usize,
    pub mapped_count: usize,
    pub interface_only_count: usize,
    pub mapped_atomic_tool_names: Vec<String>,
    pub interface_only_atomic_tool_names: Vec<String>,
    pub manifest_schema_version: u16,
    pub manifest_schema_fields: Vec<String>,
    pub tool_action_schema_version: u16,
    pub tool_action_schema_fields: Vec<String>,
    pub tool_action_call_schema_fields: Vec<String>,
    pub tool_report_schema_version: u16,
    pub tool_report_schema_fields: Vec<String>,
    pub tool_call_schema_fields: Vec<String>,
    pub manifests: Vec<AtomicToolManifest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoalModeStatus {
    pub ok: bool,
    pub kind: String,
    pub cli_entrypoint: String,
    pub context_source: String,
    pub default_goal_id: String,
    pub default_allowed_slots: Vec<String>,
    pub bypasses_governance: bool,
    pub adds_core_slot: bool,
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
            identity_soul_exists: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.soul_exists),
            identity_story_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.story.chars().count()),
            identity_story_exists: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.story_exists),
            identity_first_wake_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.first_wake.chars().count()),
            identity_first_wake_exists: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.first_wake_exists),
            identity_agents_registry_chars: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.agents_registry.chars().count()),
            identity_agents_registry_exists: kernel
                .identity_bootstrap_snapshot
                .as_ref()
                .map(|snapshot| snapshot.agents_registry_exists),
        },
        plugin_registry: summarize_plugin_registry(Path::new("plugins/registry.example.json")),
        atomic_tools: AtomicToolSurfaceStatus {
            source: "GenericAgent".to_string(),
            ok: atomic_manifests.len() == 9 && mapped_count == 3 && interface_only_count == 6,
            total_count: atomic_manifests.len(),
            mapped_count,
            interface_only_count,
            mapped_atomic_tool_names: atomic_manifests
                .iter()
                .filter(|tool| tool.status == AtomicToolStatus::Mapped)
                .map(|tool| tool.name.to_string())
                .collect(),
            interface_only_atomic_tool_names: atomic_manifests
                .iter()
                .filter(|tool| tool.status == AtomicToolStatus::InterfaceOnly)
                .map(|tool| tool.name.to_string())
                .collect(),
            manifest_schema_version: AtomicToolManifest::schema_version(),
            manifest_schema_fields: AtomicToolManifest::schema_fields()
                .iter()
                .map(|field| field.to_string())
                .collect(),
            tool_action_schema_version: ToolActionEnvelope::schema_version(),
            tool_action_schema_fields: ToolActionEnvelope::schema_fields()
                .iter()
                .map(|field| field.to_string())
                .collect(),
            tool_action_call_schema_fields: ToolActionEnvelope::call_schema_fields()
                .iter()
                .map(|field| field.to_string())
                .collect(),
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
        goal_mode: goal_mode_status(),
    })
}

fn goal_mode_status() -> GoalModeStatus {
    let default_goal = GoalSpec::mainline_mvp("status probe");
    GoalModeStatus {
        ok: default_goal.validate().is_ok(),
        kind: "lightweight_runtime_context".to_string(),
        cli_entrypoint: "run --goal TEXT".to_string(),
        context_source: "goal".to_string(),
        default_goal_id: default_goal.goal_id,
        default_allowed_slots: default_goal.allowed_slots,
        bypasses_governance: false,
        adds_core_slot: false,
    }
}
