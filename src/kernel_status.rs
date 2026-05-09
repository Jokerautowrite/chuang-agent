use crate::atomic_tool::{ga_atomic_tool_manifests, AtomicToolManifest, AtomicToolStatus};
use crate::chuang_kernel::{ChuangKernelConfig, ChuangKernelSnapshot};
use crate::goal_mode::{GoalCheckpointPolicy, GoalFinalReportPolicy, GoalSpec};
use crate::goal_run::GoalRunStore;
use crate::governance::{
    risk_decision_parts, ActionKind, Governance, MarkdownRuleSet, ProposedAction,
    StaticRuleGovernance,
};
use crate::live_adapter_gate::{evaluate_live_adapter_gate, LiveAdapterSlot};
use crate::plugin_registry::{summarize_plugin_registry, PluginRegistrySummary};
use crate::runtime_config::{ConfigError, ConfigSummary, ProviderConfig, RuntimeConfig};
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
    pub local_contract_readiness: LocalContractReadinessStatus,
    pub provider_readiness: ProviderReadinessStatus,
    pub project_readiness: ProjectReadinessStatus,
    pub memory_readiness: MemoryReadinessStatus,
    pub memory_maintenance_receipt: MemoryMaintenanceReceiptStatus,
    pub channel_readiness: ChannelReadinessStatus,
    pub subagent_readiness: SubagentReadinessStatus,
    pub external_ai_readiness: ExternalAiReadinessStatus,
    pub live_adapter_gates: LiveAdapterGateStatus,
    pub atomic_tools: AtomicToolSurfaceStatus,
    pub governance: GovernanceReadinessStatus,
    pub release_readiness: ReleaseReadinessStatus,
    pub third_test_candidate: ThirdTestCandidateReadinessStatus,
    pub goal_mode: GoalModeStatus,
    pub goal_run: GoalRunReadinessStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub modules: Vec<ProjectModuleStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalContractReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub contract_count: usize,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub connects_real_external_services: bool,
    pub writes_core_memory: bool,
    pub executes_plugins: bool,
    pub contracts: Vec<LocalContractStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LocalContractStatus {
    pub name: String,
    pub state: String,
    pub evidence: String,
    pub boundary: String,
    pub next_action: String,
    pub read_only: bool,
    pub dry_run: bool,
    pub connects_real_service: bool,
    pub writes_core_memory: bool,
    pub writes_repo_files: bool,
    pub executes_plugins: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectModuleStatus {
    pub name: String,
    pub state: String,
    pub current: String,
    pub next_action: String,
    pub core_boundary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseReadinessStatus {
    pub ok: bool,
    pub release_name: String,
    pub overall_state: String,
    pub readiness_scope: String,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub acceptance_count: usize,
    pub acceptance_ready_count: usize,
    pub acceptance_partial_count: usize,
    pub acceptance_deferred_count: usize,
    pub connects_real_external_services: bool,
    pub verifies_real_external_services: bool,
    pub uses_stub_or_local_fixtures: bool,
    pub writes_repo_files: bool,
    pub current: String,
    pub next_action: String,
    pub acceptance: Vec<ReleaseAcceptanceStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReleaseAcceptanceStatus {
    pub name: String,
    pub state: String,
    pub evidence: String,
    pub boundary: String,
    pub read_only: bool,
    pub connects_real_service: bool,
    pub writes_repo_files: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ThirdTestCandidateReadinessStatus {
    pub ok: bool,
    pub candidate_name: String,
    pub overall_state: String,
    pub local_gate_ready: bool,
    pub smoke_script: String,
    pub marker: String,
    pub requires_manual_live_check: bool,
    pub connects_real_external_services: bool,
    pub verifies_real_external_services: bool,
    pub real_live_ready: bool,
    pub operator_env_blocks_100_percent: bool,
    pub current: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub layer_count: usize,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub layers: Vec<MemoryLayerStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryMaintenanceReceiptStatus {
    pub available: bool,
    pub readable: bool,
    pub state: String,
    pub experiences_path: String,
    pub receipt_count: usize,
    pub latest_entry_id: Option<String>,
    pub latest_source_record_id: Option<String>,
    pub latest_approval_source: Option<String>,
    pub latest_approved_at: Option<String>,
    pub latest_approval_note: Option<String>,
    pub latest_provenance_preserved: bool,
    pub current: String,
    pub next_action: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryLayerStatus {
    pub name: String,
    pub state: String,
    pub storage: String,
    pub current: String,
    pub next_action: String,
    pub writes_automatically: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub layer_count: usize,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub layers: Vec<ChannelLayerStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChannelLayerStatus {
    pub name: String,
    pub state: String,
    pub current: String,
    pub next_action: String,
    pub boundary: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubagentReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub mode: String,
    pub live_worker_available: bool,
    pub worker_runtime_state: String,
    pub worker_runtime_reason: String,
    pub worker_runtime_blocked_reason: String,
    pub capability_route_state: String,
    pub capability_mismatch_blocks_live: bool,
    pub capability_mismatch_reason: String,
    pub local_contract_ready: bool,
    pub local_contract_state: String,
    pub local_contract_reason: String,
    pub live_adapter_ready: bool,
    pub live_adapter_state: String,
    pub live_adapter_reason: String,
    pub layer_count: usize,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub layers: Vec<SubagentLayerStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SubagentLayerStatus {
    pub name: String,
    pub state: String,
    pub live_worker_available: bool,
    pub worker_runtime_state: String,
    pub blocked_reason: String,
    pub capability_route_state: String,
    pub capability_mismatch_blocks_live: bool,
    pub capability_mismatch_reason: String,
    pub local_contract_ready: bool,
    pub local_contract_state: String,
    pub local_contract_reason: String,
    pub live_adapter_ready: bool,
    pub live_adapter_state: String,
    pub live_adapter_reason: String,
    pub current: String,
    pub next_action: String,
    pub boundary: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalAiReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub layer_count: usize,
    pub ready_count: usize,
    pub partial_count: usize,
    pub deferred_count: usize,
    pub blocked_count: usize,
    pub layers: Vec<ExternalAiLayerStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExternalAiLayerStatus {
    pub name: String,
    pub state: String,
    pub current: String,
    pub next_action: String,
    pub boundary: String,
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveAdapterGateStatus {
    pub ok: bool,
    pub overall_state: String,
    pub gate_count: usize,
    pub enabled_count: usize,
    pub disabled_count: usize,
    pub gates: Vec<LiveAdapterGateLayerStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LiveAdapterGateLayerStatus {
    pub name: String,
    pub state: String,
    pub enabled: bool,
    pub default_enabled: bool,
    pub env_value_state: String,
    pub required_env: String,
    pub audit_label: String,
    pub preflight_checks: Vec<String>,
    pub must_reject_capabilities: Vec<String>,
    pub reason: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AtomicToolSurfaceStatus {
    pub source: String,
    pub ok: bool,
    pub total_count: usize,
    pub mapped_count: usize,
    pub interface_only_count: usize,
    pub governed_executable_atomic_tool_names: Vec<String>,
    pub mapped_atomic_tool_names: Vec<String>,
    pub interface_only_atomic_tool_names: Vec<String>,
    pub desktop_browser_interface_only_atomic_tool_names: Vec<String>,
    pub desktop_browser_live_gated_atomic_tool_names: Vec<String>,
    pub interface_only_reason: String,
    pub local_cli_self_check_entrypoints: Vec<String>,
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
pub struct GovernanceReadinessStatus {
    pub ok: bool,
    pub kind: String,
    pub rules_loaded: bool,
    pub rules_core_path: String,
    pub rule_count: usize,
    pub rules_fingerprint: String,
    pub tool_surface_governed: bool,
    pub read_only_decision: String,
    pub dangerous_write_decision: String,
    pub dangerous_shell_decision: String,
    pub secret_shell_decision: String,
    pub goal_run_executes: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoalModeStatus {
    pub ok: bool,
    pub kind: String,
    pub cli_entrypoint: String,
    pub context_source: String,
    pub default_goal_id: String,
    pub default_allowed_slots: Vec<String>,
    pub checkpoint_policy: GoalCheckpointPolicy,
    pub final_report_policy: GoalFinalReportPolicy,
    pub bypasses_governance: bool,
    pub adds_core_slot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderReadinessStatus {
    pub ok: bool,
    pub overall_state: String,
    pub provider_kind: String,
    pub provider_id: String,
    pub model_name: String,
    pub transport: String,
    pub fallback_configured: bool,
    pub fallback_policy: Option<String>,
    pub request_timeout_ms: Option<u64>,
    pub tls_ca_cert_path: Option<String>,
    pub api_key_state: Option<String>,
    pub placeholder_warning_count: usize,
    pub current: String,
    pub next_action: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GoalRunReadinessStatus {
    pub ok: bool,
    pub root: String,
    pub goal_id: String,
    pub path: String,
    pub plan_exists: bool,
    pub checkpoint_count: usize,
    pub worker_count: usize,
    pub validation_command_count: usize,
    pub checkpoint_log_complete: bool,
    pub last_checkpoint_id: Option<String>,
    pub last_checkpoint_summary: Option<String>,
    pub last_checkpoint_created_at: Option<String>,
    pub last_checkpoint_completed_worker_ids: Option<Vec<String>>,
    pub last_checkpoint_validation_notes: Option<Vec<String>>,
    pub incomplete_reasons: Vec<String>,
    pub read_error: Option<String>,
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

    let governance = governance_readiness_status(config)?;
    let goal_mode = goal_mode_status();
    let goal_run = summarize_goal_run_readiness(Path::new("./context/goal-runs"), "mainline-mvp");
    let plugin_registry = summarize_plugin_registry(Path::new("plugins/registry.example.json"));
    let local_contract_readiness = build_local_contract_readiness();
    let slots = summarize_runtime_slots(config);
    let config_summary = config.summary();
    let provider_readiness = build_provider_readiness(config, &config_summary);
    let atomic_tools = AtomicToolSurfaceStatus {
        source: "GenericAgent".to_string(),
        ok: atomic_manifests.len() == 9 && mapped_count == 9 && interface_only_count == 0,
        total_count: atomic_manifests.len(),
        mapped_count,
        interface_only_count,
        governed_executable_atomic_tool_names: atomic_manifests
            .iter()
            .filter(|tool| tool.status == AtomicToolStatus::Mapped)
            .map(|tool| tool.name.to_string())
            .collect(),
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
        desktop_browser_interface_only_atomic_tool_names: atomic_manifests
            .iter()
            .filter(|tool| {
                tool.status == AtomicToolStatus::InterfaceOnly
                    && matches!(
                        tool.name,
                        "mouse" | "keyboard" | "screenshot" | "locate"
                    )
            })
            .map(|tool| tool.name.to_string())
            .collect(),
        desktop_browser_live_gated_atomic_tool_names: atomic_manifests
            .iter()
            .filter(|tool| matches!(tool.name, "mouse" | "keyboard" | "screenshot" | "locate"))
            .map(|tool| tool.name.to_string())
            .collect(),
        interface_only_reason: "all GA atoms are mapped to governed runtime ports; real desktop/browser execution still requires an audited actuator adapter, live gate, allowlist, and receipt".to_string(),
        local_cli_self_check_entrypoints: vec![
            "status --json".to_string(),
            "doctor --json".to_string(),
            "app-server health --diagnostic --json".to_string(),
        ],
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
    };
    let project_readiness = build_project_readiness(
        &config_summary,
        &slots,
        &plugin_registry,
        &atomic_tools,
        &governance,
        &goal_mode,
        &goal_run,
    );
    let memory_readiness = build_memory_readiness(&config_summary);
    let memory_maintenance_receipt = build_memory_maintenance_receipt(&config_summary);
    let channel_readiness = build_channel_readiness();
    let subagent_readiness = build_subagent_readiness(&slots, &config_summary);
    let external_ai_readiness = build_external_ai_readiness();
    let live_adapter_gates = build_live_adapter_gate_status();
    let release_readiness = build_release_readiness(
        &project_readiness,
        &memory_readiness,
        &channel_readiness,
        &subagent_readiness,
        &external_ai_readiness,
        &atomic_tools,
        &governance,
        &goal_mode,
        &goal_run,
    );
    let third_test_candidate = build_third_test_candidate_readiness();

    Ok(ChuangMvpStatus {
        config: config_summary,
        slots,
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
        plugin_registry,
        local_contract_readiness,
        provider_readiness,
        project_readiness,
        memory_readiness,
        memory_maintenance_receipt,
        channel_readiness,
        subagent_readiness,
        external_ai_readiness,
        live_adapter_gates,
        atomic_tools,
        governance,
        release_readiness,
        third_test_candidate,
        goal_mode,
        goal_run,
    })
}

fn build_release_readiness(
    project_readiness: &ProjectReadinessStatus,
    memory_readiness: &MemoryReadinessStatus,
    channel_readiness: &ChannelReadinessStatus,
    subagent_readiness: &SubagentReadinessStatus,
    external_ai_readiness: &ExternalAiReadinessStatus,
    atomic_tools: &AtomicToolSurfaceStatus,
    governance: &GovernanceReadinessStatus,
    goal_mode: &GoalModeStatus,
    goal_run: &GoalRunReadinessStatus,
) -> ReleaseReadinessStatus {
    let acceptance = vec![
        release_acceptance(
            "status_json_readiness",
            if project_readiness.ok { "ready" } else { "blocked" },
            "status --json exposes project/module readiness and release readiness as structured data",
            "local_diagnostic",
            true,
            false,
            false,
        ),
        release_acceptance(
            "doctor_json_gate",
            if project_readiness.ok
                && memory_readiness.ok
                && channel_readiness.ok
                && subagent_readiness.ok
                && external_ai_readiness.ok
                && atomic_tools.ok
                && governance.ok
                && goal_mode.ok
                && goal_run.ok
            {
                "ready"
            } else {
                "blocked"
            },
            "doctor --json checks config, memory, channels, subagents, external AI boundaries, governance, tools, goal mode, and smoke probes",
            "local_diagnostic",
            true,
            false,
            false,
        ),
        release_acceptance(
            "mvp_smoke_stub_flow",
            "ready",
            "scripts/chuang-mvp-smoke.sh runs in a temporary workspace with provider transport=stub and example adapters",
            "temporary_fixture",
            false,
            false,
            false,
        ),
        release_acceptance(
            "memory_readiness_dry_run",
            if memory_readiness.ok { "ready" } else { "blocked" },
            "memory knowledge status/search and maintenance report remain read-only or dry-run and do not write core memory automatically",
            "memory_contract",
            true,
            false,
            false,
        ),
        release_acceptance(
            "channel_preflight_only",
            if channel_readiness.ok {
                "partial"
            } else {
                "blocked"
            },
            "Feishu readiness is limited to Chuang-scoped env/workspace preflight; live Feishu send/receive is not verified here",
            "adapter_preflight",
            true,
            false,
            false,
        ),
        release_acceptance(
            "subagent_protocol_acceptance",
            if subagent_readiness.overall_state == "ready" {
                "ready"
            } else if subagent_readiness.ok {
                "partial"
            } else {
                "blocked"
            },
            "queued dispatch/report/collect, approved command-runner path, and bounded multi-worker run-loop are covered",
            "protocol_boundary",
            false,
            false,
            false,
        ),
        release_acceptance(
            "real_external_services",
            "deferred",
            "status/doctor/smoke do not connect real provider, Feishu, desktop, browser, wiki, GBrain, or Hermes services",
            "live_service_boundary",
            true,
            false,
            false,
        ),
    ];
    let acceptance_ready_count = acceptance
        .iter()
        .filter(|item| item.state == "ready")
        .count();
    let acceptance_partial_count = acceptance
        .iter()
        .filter(|item| item.state == "partial")
        .count();
    let acceptance_deferred_count = acceptance
        .iter()
        .filter(|item| item.state == "deferred")
        .count();
    let acceptance_blocked_count = acceptance
        .iter()
        .filter(|item| item.state == "blocked")
        .count();
    let ready_count = usize::from(project_readiness.ok)
        + usize::from(memory_readiness.ok)
        + usize::from(channel_readiness.ok)
        + usize::from(subagent_readiness.ok)
        + usize::from(external_ai_readiness.ok)
        + usize::from(atomic_tools.ok)
        + usize::from(governance.ok)
        + usize::from(goal_mode.ok)
        + usize::from(goal_run.ok);
    let partial_count = project_readiness.partial_count
        + memory_readiness.partial_count
        + channel_readiness.partial_count
        + subagent_readiness.partial_count
        + external_ai_readiness.partial_count;
    let deferred_count = project_readiness.deferred_count
        + memory_readiness.deferred_count
        + channel_readiness.deferred_count
        + subagent_readiness.deferred_count
        + external_ai_readiness.deferred_count;
    let blocked_count = project_readiness.blocked_count
        + memory_readiness.blocked_count
        + channel_readiness.blocked_count
        + subagent_readiness.blocked_count
        + external_ai_readiness.blocked_count
        + usize::from(!atomic_tools.ok)
        + usize::from(!governance.ok)
        + usize::from(!goal_mode.ok)
        + usize::from(!goal_run.ok);
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "second_test_version_ready_with_partial_modules"
    } else {
        "second_test_version_ready"
    };

    ReleaseReadinessStatus {
        ok: blocked_count == 0 && acceptance_blocked_count == 0,
        release_name: "second_test_version".to_string(),
        overall_state: overall_state.to_string(),
        readiness_scope:
            "readiness_and_smoke_acceptance_only_no_live_external_service_connection".to_string(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        acceptance_count: acceptance.len(),
        acceptance_ready_count,
        acceptance_partial_count,
        acceptance_deferred_count,
        connects_real_external_services: false,
        verifies_real_external_services: false,
        uses_stub_or_local_fixtures: true,
        writes_repo_files: false,
        current: "second test version is checkpoint-continuable: readiness, smoke, goal/run, and subagent protocol surfaces are visible while partial modules remain adapter/plugin boundaries".to_string(),
        next_action: "keep readiness/smoke/goal-run/subagent protocol green while hardening real adapters without reopening core".to_string(),
        acceptance,
    }
}

fn build_local_contract_readiness() -> LocalContractReadinessStatus {
    let contracts = vec![
        local_contract(
            "knowledge_context_preview",
            "ready",
            "memory knowledge preview-context and run --enable-knowledge-context-preview expose local readonly context evidence",
            "local_markdown_text_preview_only",
            "wire live wiki/GBrain retrieval only after audited readonly adapters are configured",
            true,
            false,
            false,
            false,
            false,
            false,
        ),
        local_contract(
            "skill_proposal_review",
            "ready",
            "skill propose/judge keeps provenance, Darwin-style scoring, canonical identity, and duplicate-merge evidence visible for the writable lifecycle",
            "self_scored_review_and_dedup",
            "keep score thresholds, canonical identity, and merge evidence visible before every skill write or retirement decision",
            false,
            false,
            false,
            false,
            false,
            false,
        ),
        local_contract(
            "skill_lifecycle_write_retire",
            "ready",
            "skill lifecycle write/retire can self-approve policy-passing proposals, upsert canonical data/skills entries, and mark stale skills deprecated/retired without deletion",
            "self_maintained_upsert_and_retire",
            "preserve audit receipts, version history, and no-delete retirement state while monitoring stale or low-score skills",
            false,
            false,
            false,
            false,
            true,
            false,
        ),
        local_contract(
            "plugin_registry_evidence",
            "ready",
            "plugin list/check and status expose evidence, capabilities, check-only, and no-execution boundaries",
            "manifest_check_only",
            "enable real plugins only through explicit manifests, allowlists, and separate credentials",
            true,
            true,
            false,
            false,
            false,
            false,
        ),
        local_contract(
            "external_knowledge_source_contracts",
            "ready",
            "memory knowledge source-contract --source wiki|gbrain documents readonly adapter contracts",
            "adapter_contract_only",
            "connect live source adapters only after manual env/provider verification and provenance review",
            true,
            true,
            false,
            false,
            false,
            false,
        ),
        local_contract(
            "goal_mode_smoke_gate",
            "ready",
            "chuang-goal-mode-smoke and chuang-goal-mode-negative-smoke cover dispatch/step/collect/checkpoint happy path and not-ready rejection",
            "local_cli_smoke_only",
            "keep goal-mode smoke green while hardening live runner adapters",
            true,
            false,
            false,
            false,
            false,
            false,
        ),
    ];

    let ready_count = contracts
        .iter()
        .filter(|contract| contract.state == "ready")
        .count();
    let partial_count = contracts
        .iter()
        .filter(|contract| contract.state == "partial")
        .count();
    let deferred_count = contracts
        .iter()
        .filter(|contract| contract.state == "deferred")
        .count();
    let blocked_count = contracts
        .iter()
        .filter(|contract| contract.state == "blocked")
        .count();
    let connects_real_external_services = contracts
        .iter()
        .any(|contract| contract.connects_real_service);
    let writes_core_memory = contracts.iter().any(|contract| contract.writes_core_memory);
    let executes_plugins = contracts.iter().any(|contract| contract.executes_plugins);
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "ready_with_partial_contracts"
    } else {
        "ready"
    };

    LocalContractReadinessStatus {
        ok: blocked_count == 0,
        overall_state: overall_state.to_string(),
        contract_count: contracts.len(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        connects_real_external_services,
        writes_core_memory,
        executes_plugins,
        contracts,
    }
}

fn build_provider_readiness(
    config: &RuntimeConfig,
    summary: &ConfigSummary,
) -> ProviderReadinessStatus {
    let fallback_configured = matches!(config.provider, ProviderConfig::Fallback { .. });
    let provider_kind = summary.provider_kind.clone();
    let provider_id = summary.provider_id.clone();
    let model_name = summary.model_name.clone();
    let transport = provider_transport_label(&config.provider);
    let fallback_policy = summary.provider_fallback_policy.clone();
    let request_timeout_ms = summary.provider_request_timeout_ms;
    let tls_ca_cert_path = summary.provider_tls_ca_cert_path.clone();
    let api_key_state = summary.api_key_state.clone();
    let placeholder_warning_count = summary
        .placeholder_warnings
        .iter()
        .filter(|warning| warning.starts_with("provider "))
        .count();
    let missing_env = api_key_state
        .as_deref()
        .map(|state| state.contains("<missing") || state == "none")
        .unwrap_or(true);
    let uses_stub = provider_transport_label(&config.provider).contains("stub");
    let provider_kind_is_fake = provider_kind == "fake";
    let overall_state = if fallback_configured {
        "ready_with_fallback"
    } else if provider_kind_is_fake || uses_stub {
        "ready"
    } else if missing_env || placeholder_warning_count > 0 {
        "partial"
    } else {
        "ready"
    };

    ProviderReadinessStatus {
        ok: true,
        overall_state: overall_state.to_string(),
        provider_kind,
        provider_id,
        model_name,
        transport,
        fallback_configured,
        fallback_policy,
        request_timeout_ms,
        tls_ca_cert_path,
        api_key_state,
        placeholder_warning_count,
        current: if fallback_configured {
            "provider fallback is configured; live provider and fallback metadata remain locally observable".to_string()
        } else if provider_kind_is_fake {
            "provider fake mode is local-only and ready for smoke coverage".to_string()
        } else if uses_stub {
            "provider transport=stub is local-only and ready for smoke coverage".to_string()
        } else if missing_env {
            "provider api_key_env is missing; live provider readiness remains partial".to_string()
        } else {
            "provider configuration is locally ready for live requests".to_string()
        },
        next_action: if fallback_configured {
            "keep fallback metadata visible and verify primary diagnostics before promoting live provider traffic".to_string()
        } else if provider_kind_is_fake || uses_stub {
            "switch to a real provider transport only after live secrets and transport diagnostics are confirmed".to_string()
        } else if missing_env {
            "set the provider api_key_env before claiming live provider readiness".to_string()
        } else {
            "keep timeout, TLS, and fallback policy aligned with the selected provider transport"
                .to_string()
        },
    }
}

fn provider_transport_label(provider: &ProviderConfig) -> String {
    match provider {
        ProviderConfig::Fake { .. } => "fake".to_string(),
        ProviderConfig::OpenAICompatible(config) => config.transport.as_str().to_string(),
        ProviderConfig::Fallback {
            primary, fallback, ..
        } => format!(
            "{}->{}",
            provider_transport_label(primary),
            provider_transport_label(fallback)
        ),
    }
}

fn local_contract(
    name: &str,
    state: &str,
    evidence: &str,
    boundary: &str,
    next_action: &str,
    read_only: bool,
    dry_run: bool,
    connects_real_service: bool,
    writes_core_memory: bool,
    writes_repo_files: bool,
    executes_plugins: bool,
) -> LocalContractStatus {
    LocalContractStatus {
        name: name.to_string(),
        state: state.to_string(),
        evidence: evidence.to_string(),
        boundary: boundary.to_string(),
        next_action: next_action.to_string(),
        read_only,
        dry_run,
        connects_real_service,
        writes_core_memory,
        writes_repo_files,
        executes_plugins,
    }
}

fn release_acceptance(
    name: &str,
    state: &str,
    evidence: &str,
    boundary: &str,
    read_only: bool,
    connects_real_service: bool,
    writes_repo_files: bool,
) -> ReleaseAcceptanceStatus {
    ReleaseAcceptanceStatus {
        name: name.to_string(),
        state: state.to_string(),
        evidence: evidence.to_string(),
        boundary: boundary.to_string(),
        read_only,
        connects_real_service,
        writes_repo_files,
    }
}

fn build_third_test_candidate_readiness() -> ThirdTestCandidateReadinessStatus {
    ThirdTestCandidateReadinessStatus {
        ok: true,
        candidate_name: "third_test_candidate".to_string(),
        overall_state: "local_gate_ready_requires_manual_live_check".to_string(),
        local_gate_ready: true,
        smoke_script: "scripts/chuang-third-test-smoke.sh".to_string(),
        marker: "third_test_candidate_smoke_ok".to_string(),
        requires_manual_live_check: true,
        connects_real_external_services: false,
        verifies_real_external_services: false,
        real_live_ready: false,
        operator_env_blocks_100_percent: true,
        current: "third test candidate local gates are represented by the readonly smoke wrapper and status surfaces; real live service verification is still manual and not marked ready".to_string(),
        next_action: "run scripts/chuang-third-test-smoke.sh for local gate evidence, then collect an operator live receipt before claiming live readiness".to_string(),
    }
}

fn build_external_ai_readiness() -> ExternalAiReadinessStatus {
    let genesis_path = Path::new("src/genesis_actuator.rs");
    let browser_worker_path = Path::new("src/browser_worker/mod.rs");
    let dispatcher_doc_path = Path::new("data/skills/external_agent_dispatch_sop.md");
    let dispatch_adapter_path = Path::new("src/external_ai_dispatch.rs");
    let layers = vec![
        external_ai_layer(
            "genesis_actuator",
            if genesis_path.exists() { "ready" } else { "blocked" },
            "GenesisActuator trait plus AutoCli fallback runner exist behind a slot wrapper",
            "keep search/query ability behind adapter and audit before expanding channels",
            "adapter",
            Some(genesis_path.display().to_string()),
        ),
        external_ai_layer(
            "browser_worker_frozen",
            if browser_worker_path.exists() { "ready" } else { "blocked" },
            "BrowserWorker remains frozen and is not part of the mainline execution path",
            "do not revive the old browser-worker path; keep web AI on Genesis instead",
            "frozen_boundary",
            Some(browser_worker_path.display().to_string()),
        ),
        external_ai_layer(
            "agent_slot_boundary",
            "ready",
            "plugin registry and Genesis slot boundary already model external AI as an adapter/plugin line",
            "keep external AI outside the core slot count and behind explicit manifests",
            "slot",
            None,
        ),
        external_ai_layer(
            "dispatch_sop",
            if dispatcher_doc_path.exists() && dispatch_adapter_path.exists() {
                "ready"
            } else {
                "deferred"
            },
            "external_agent_dispatch_sop exists and external-ai dispatch can prepare bounded dry-run requests for subagent review",
            "wire live browser/HTTP adapters only after audited platform sessions are configured",
            "skill",
            Some(dispatcher_doc_path.display().to_string()),
        ),
        external_ai_layer(
            "unified_identity_engine",
            if Path::new("data/skills/unified_identity_engine_adapter.md").exists()
                && dispatch_adapter_path.exists()
            {
                "ready"
            } else {
                "deferred"
            },
            "the unified identity engine contract is documented and exposed through external-ai dispatch dry-run output",
            "add audited identity/session adapters without changing the dry-run contract",
            "adapter",
            Some("data/skills/unified_identity_engine_adapter.md".to_string()),
        ),
    ];

    let ready_count = layers.iter().filter(|layer| layer.state == "ready").count();
    let partial_count = layers
        .iter()
        .filter(|layer| layer.state == "partial")
        .count();
    let deferred_count = layers
        .iter()
        .filter(|layer| layer.state == "deferred")
        .count();
    let blocked_count = layers
        .iter()
        .filter(|layer| layer.state == "blocked")
        .count();
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "external_ai_adapter_partial"
    } else {
        "ready"
    };

    ExternalAiReadinessStatus {
        ok: blocked_count == 0,
        overall_state: overall_state.to_string(),
        layer_count: layers.len(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        layers,
    }
}

fn external_ai_layer(
    name: &str,
    state: &str,
    current: &str,
    next_action: &str,
    boundary: &str,
    path: Option<String>,
) -> ExternalAiLayerStatus {
    ExternalAiLayerStatus {
        name: name.to_string(),
        state: state.to_string(),
        current: current.to_string(),
        next_action: next_action.to_string(),
        boundary: boundary.to_string(),
        path,
    }
}

fn build_live_adapter_gate_status() -> LiveAdapterGateStatus {
    let gates = [
        LiveAdapterSlot::SubagentRunner,
        LiveAdapterSlot::ControlApply,
        LiveAdapterSlot::ActuatorOperation,
    ]
    .into_iter()
    .map(|slot| {
        let gate = evaluate_live_adapter_gate(slot);
        LiveAdapterGateLayerStatus {
            name: gate.name.to_string(),
            state: if gate.enabled {
                "enabled".to_string()
            } else {
                "disabled".to_string()
            },
            enabled: gate.enabled,
            default_enabled: gate.default_enabled,
            env_value_state: gate.env_value_state,
            required_env: gate.required_env.to_string(),
            audit_label: gate.audit_label.to_string(),
            preflight_checks: gate
                .preflight_checks
                .iter()
                .map(|check| check.to_string())
                .collect(),
            must_reject_capabilities: gate
                .must_reject_capabilities
                .iter()
                .map(|capability| capability.to_string())
                .collect(),
            reason: gate.reason,
            next_action: gate.next_action,
        }
    })
    .collect::<Vec<_>>();

    let enabled_count = gates.iter().filter(|gate| gate.enabled).count();
    let disabled_count = gates.len().saturating_sub(enabled_count);
    LiveAdapterGateStatus {
        ok: true,
        overall_state: if enabled_count == 0 {
            "disabled_by_default"
        } else {
            "live_gates_enabled"
        }
        .to_string(),
        gate_count: gates.len(),
        enabled_count,
        disabled_count,
        gates,
    }
}

fn build_subagent_readiness(
    slots: &RuntimeSlotsSummary,
    config: &ConfigSummary,
) -> SubagentReadinessStatus {
    let queued = slots.subagent == "queued_external";
    let live_runner_rehearsal_ready = Path::new("docs/subagent-runner-protocol.md").exists()
        && Path::new("src/live_subagent_rehearsal.rs").exists();
    let layers =
        vec![
        subagent_layer(
            "dispatch_queue",
            if queued { "ready" } else { "deferred" },
            queued,
            false,
            &format!("subagent slot={} queue_root={}", slots.subagent, config.subagent_queue_root),
            "keep dispatch files as the durable handoff format before adding live workers",
            "slot",
            "durable queue handoff is protocol-ready; live worker delivery is still deferred",
        ),
        subagent_layer(
            "report_collect",
            if queued { "ready" } else { "deferred" },
            queued,
            false,
            "collect path validates dispatch identity before accepting reports",
            "keep ReportAdmission separate from immutable SubagentReport",
            "protocol",
            "report admission is local-contract ready; live adapter collection is still deferred",
        ),
        subagent_layer(
            "command_runner",
            "ready",
            true,
            false,
            "run-once/run-loop command runner is governed by explicit --approve-exec, capability matching, report admission, output bounds, and dispatch timeouts",
            "connect additional real runners through the same report-admission boundary",
            "adapter",
            "local command-runner contract is ready; live runner adapters remain deferred until a real backend is connected",
        ),
        subagent_layer(
            "live_runner_rehearsal",
            if live_runner_rehearsal_ready { "ready" } else { "partial" },
            live_runner_rehearsal_ready,
            false,
            "subagent live-preflight rehearses live runner gate, command allowlist, capability routing, ReportAdmission, forbidden capabilities, and audit prerequisites without starting a worker",
            "run one approved live runner rehearsal only after operator enables CHUANG_CODEX_RUNNER_ENABLE=1 for an exact allowlisted command",
            "read_only_preflight",
            "read-only live runner rehearsal is ready; real worker execution remains gated and deferred",
        ),
        subagent_layer(
            "multi_worker",
            "ready",
            true,
            false,
            "run-loop supports bounded local multi-worker batches through durable queue claims and existing runner governance",
            "keep live external worker pools behind audited adapters",
            "orchestration",
            "bounded local multi-worker orchestration is ready; live external worker pools remain deferred",
        ),
        subagent_layer(
            "external_ai_downstream",
            if Path::new("src/external_ai_dispatch.rs").exists() {
                "ready"
            } else {
                "partial"
            },
            Path::new("src/external_ai_dispatch.rs").exists(),
            false,
            "external AI dispatch remains below subagents, with a dry-run unified-identity adapter contract available for review",
            "connect live agent-browser or HTTP sessions only through audited adapters",
            "adapter",
            "dry-run downstream contract is ready; live external AI sessions remain deferred behind audited adapters",
        ),
    ];

    let ready_count = layers.iter().filter(|layer| layer.state == "ready").count();
    let partial_count = layers
        .iter()
        .filter(|layer| layer.state == "partial")
        .count();
    let deferred_count = layers
        .iter()
        .filter(|layer| layer.state == "deferred")
        .count();
    let blocked_count = layers
        .iter()
        .filter(|layer| layer.state == "blocked")
        .count();
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "queued_protocol_partial"
    } else {
        "ready"
    };
    let local_contract_ready = layers
        .iter()
        .all(|layer| layer.local_contract_ready || layer.state == "deferred");
    let local_contract_state = if local_contract_ready {
        "ready"
    } else if layers.iter().any(|layer| layer.state == "blocked") {
        "blocked"
    } else {
        "partial"
    };
    let live_adapter_ready = layers.iter().all(|layer| layer.live_adapter_ready);
    let live_adapter_state = if live_adapter_ready {
        "ready"
    } else if layers.iter().any(|layer| layer.state == "blocked") {
        "blocked"
    } else {
        "partial"
    };
    let live_worker_available = live_adapter_ready;
    let worker_runtime_state = if live_worker_available {
        "live_worker_available"
    } else if local_contract_ready {
        "local_contract_only"
    } else if blocked_count > 0 {
        "blocked"
    } else {
        "deferred"
    };
    let worker_runtime_reason = if live_worker_available {
        "a live subagent worker adapter is configured and all layers report live availability"
            .to_string()
    } else if slots.subagent == "fake" {
        "subagent slot is fake; ready layers only prove local contracts and do not provide a live worker"
            .to_string()
    } else if queued {
        "queued_external provides durable dispatch/report contracts, but no live worker adapter is available yet"
            .to_string()
    } else {
        "subagent runtime is contract-only until an audited live worker adapter is configured"
            .to_string()
    };
    let worker_runtime_blocked_reason = if live_worker_available {
        "none".to_string()
    } else if blocked_count > 0 {
        "one or more subagent readiness layers are blocked".to_string()
    } else if slots.subagent == "fake" {
        "live_worker_unavailable: subagent slot is fake; local contracts are visible but no live worker can run"
            .to_string()
    } else if queued {
        "live_worker_unavailable: queued_external has durable dispatch/report contracts but no live worker adapter"
            .to_string()
    } else {
        "live_worker_unavailable: configure an audited live worker adapter before live execution"
            .to_string()
    };
    let capability_route_state = if live_worker_available {
        "ready_for_live"
    } else {
        "requires_dispatch_required_capabilities"
    };
    let capability_mismatch_reason = if live_worker_available {
        "capability route is satisfied for the configured live worker adapter".to_string()
    } else {
        "live subagent preflight must reject missing or mismatched dispatch required_capabilities before any worker starts"
            .to_string()
    };

    SubagentReadinessStatus {
        ok: blocked_count == 0,
        overall_state: overall_state.to_string(),
        mode: slots.subagent.clone(),
        live_worker_available,
        worker_runtime_state: worker_runtime_state.to_string(),
        worker_runtime_reason,
        worker_runtime_blocked_reason,
        capability_route_state: capability_route_state.to_string(),
        capability_mismatch_blocks_live: !live_worker_available,
        capability_mismatch_reason,
        local_contract_ready,
        local_contract_state: local_contract_state.to_string(),
        local_contract_reason: if local_contract_ready {
            "dispatch queue, report collect, command runner, and multi-worker orchestration are protocol-ready".to_string()
        } else if layers.iter().any(|layer| layer.state == "blocked") {
            "one or more local subagent contract layers are blocked".to_string()
        } else {
            "one or more local subagent contract layers are still deferred".to_string()
        },
        live_adapter_ready,
        live_adapter_state: live_adapter_state.to_string(),
        live_adapter_reason: if live_adapter_ready {
            "all subagent layers have live adapters".to_string()
        } else if layers.iter().any(|layer| layer.state == "blocked") {
            "one or more live subagent adapters are blocked".to_string()
        } else {
            "live adapters are not yet connected for the subagent layers; read-only live runner rehearsal is ready".to_string()
        },
        layer_count: layers.len(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        layers,
    }
}

fn subagent_layer(
    name: &str,
    state: &str,
    local_contract_ready: bool,
    live_adapter_ready: bool,
    current: &str,
    next_action: &str,
    boundary: &str,
    live_adapter_reason: &str,
) -> SubagentLayerStatus {
    let local_contract_state = if local_contract_ready {
        "ready"
    } else if state == "blocked" {
        "blocked"
    } else if state == "partial" {
        "partial"
    } else {
        "deferred"
    };
    let live_adapter_state = if live_adapter_ready {
        "ready"
    } else if state == "blocked" {
        "blocked"
    } else if state == "partial" {
        "partial"
    } else {
        "deferred"
    };
    let live_worker_available = live_adapter_ready;
    let worker_runtime_state = if live_worker_available {
        "live_worker_available"
    } else if local_contract_ready {
        "local_contract_only"
    } else {
        live_adapter_state
    };
    let blocked_reason = if live_worker_available {
        "none".to_string()
    } else if state == "blocked" {
        format!("{name} is blocked before live worker execution")
    } else if name == "live_runner_rehearsal" {
        "live_runner_rehearsal is read-only; missing or mismatched dispatch required_capabilities keep ready_for_live=false"
            .to_string()
    } else {
        format!("{name} has no live worker adapter; local contract evidence only")
    };
    let capability_route_state = if live_worker_available {
        "ready_for_live"
    } else if name == "live_runner_rehearsal" {
        "requires_dispatch_required_capabilities"
    } else {
        "not_live_routed"
    };
    let capability_mismatch_reason = if live_worker_available {
        format!("{name} capability route is satisfied for live execution")
    } else if name == "live_runner_rehearsal" {
        "capability mismatch or missing dispatch required_capabilities must block live runner readiness"
            .to_string()
    } else {
        format!("{name} does not start a live worker, so capability mismatch stays blocked at live-preflight")
    };
    SubagentLayerStatus {
        name: name.to_string(),
        state: state.to_string(),
        live_worker_available,
        worker_runtime_state: worker_runtime_state.to_string(),
        blocked_reason,
        capability_route_state: capability_route_state.to_string(),
        capability_mismatch_blocks_live: !live_worker_available,
        capability_mismatch_reason,
        local_contract_ready,
        local_contract_state: local_contract_state.to_string(),
        local_contract_reason: if local_contract_ready {
            format!("{name} local contract is protocol-ready")
        } else if state == "blocked" {
            format!("{name} local contract is blocked")
        } else if state == "partial" {
            format!("{name} local contract is partially ready")
        } else {
            format!("{name} local contract is deferred")
        },
        live_adapter_ready,
        live_adapter_state: live_adapter_state.to_string(),
        live_adapter_reason: live_adapter_reason.to_string(),
        current: current.to_string(),
        next_action: next_action.to_string(),
        boundary: boundary.to_string(),
    }
}

fn build_channel_readiness() -> ChannelReadinessStatus {
    let bridge_script = Path::new("scripts/chuang-feishu-bridge.sh");
    let bridge_js = Path::new("scripts/chuang-feishu-bridge.js");
    let client_adapter = Path::new("scripts/chuang-feishu-client-adapter.js");
    let env_example = Path::new("ops/systemd/chuang-feishu-bridge.env.example");
    let service_example = Path::new("ops/systemd/chuang-feishu-bridge.service.example");
    let checklist = Path::new("docs/feishu-dedicated-channel-checklist.md");
    let layers = vec![
        channel_layer(
            "app_server",
            "ready",
            "app-server JSON-RPC owns turn/start and workspace config loading",
            "keep provider/tool/runtime details behind app-server instead of channel code",
            "core_entrypoint",
            None,
        ),
        channel_layer(
            "channel_simulate",
            "ready",
            "local channel simulate can exercise inbound -> app-server -> outbound without Feishu",
            "keep smoke coverage on simulate before changing live bridge behavior",
            "adapter_test",
            None,
        ),
        channel_layer(
            "dedicated_feishu_bridge",
            if bridge_script.exists()
                && bridge_js.exists()
                && client_adapter.exists()
                && env_example.exists()
                && service_example.exists()
                && checklist.exists()
            {
                "ready"
            } else {
                "blocked"
            },
            "Chuang has repository-local bridge scripts, Chuang-only env names, env/service templates, and local feishu-check preflight",
            "verify live service with Chuang bot credentials after workspace/config/mode preflight passes",
            "adapter",
            Some(bridge_script.display().to_string()),
        ),
        channel_layer(
            "codex_hermes_isolation",
            "ready",
            "Chuang channel docs require a separate bot/service/session from Codex and Hermes",
            "keep service and credential names separate during every bridge change",
            "operational_boundary",
            Some("docs/feishu-dedicated-channel-checklist.md".to_string()),
        ),
        channel_layer(
            "rich_messages",
            if Path::new("scripts/chuang-feishu-rich-message-smoke.js").exists() {
                "ready"
            } else {
                "deferred"
            },
            "repo-local Feishu adapter can render Chuang replies as interactive cards and fall back to text on send failure",
            "keep rich-card rendering covered by local smoke before expanding non-text inbound events",
            "adapter",
            Some("scripts/chuang-feishu-rich-message-smoke.js".to_string()),
        ),
    ];

    let ready_count = layers.iter().filter(|layer| layer.state == "ready").count();
    let partial_count = layers
        .iter()
        .filter(|layer| layer.state == "partial")
        .count();
    let deferred_count = layers
        .iter()
        .filter(|layer| layer.state == "deferred")
        .count();
    let blocked_count = layers
        .iter()
        .filter(|layer| layer.state == "blocked")
        .count();
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "dedicated_adapter_partial"
    } else {
        "ready"
    };

    ChannelReadinessStatus {
        ok: blocked_count == 0,
        overall_state: overall_state.to_string(),
        layer_count: layers.len(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        layers,
    }
}

fn channel_layer(
    name: &str,
    state: &str,
    current: &str,
    next_action: &str,
    boundary: &str,
    path: Option<String>,
) -> ChannelLayerStatus {
    ChannelLayerStatus {
        name: name.to_string(),
        state: state.to_string(),
        current: current.to_string(),
        next_action: next_action.to_string(),
        boundary: boundary.to_string(),
        path,
    }
}

fn build_memory_readiness(config: &ConfigSummary) -> MemoryReadinessStatus {
    let layers = vec![
        memory_layer(
            "internal_identity",
            "ready",
            &format!(
                "{} at {}",
                config.identity_memory_kind, config.identity_memory_root
            ),
            "USER.md / MEMORY.md / experiences.md entrypoints exist behind explicit write commands",
            "keep hard-limit admission and explicit overwrite approval; do not auto-compress",
            false,
        ),
        memory_layer(
            "history_session",
            "ready",
            &format!("sqlite at {}", config.db_path),
            "turn_summary records support session-scoped search and isolated recall diagnostics",
            "keep channel thread id mapped to session id before Feishu adapter expansion",
            true,
        ),
        memory_layer(
            "lim_long_term",
            "ready",
            &config.identity_experiences_path,
            "memory lim extract produces provenance-bearing candidates and maintenance apply can write selected candidates to experiences.md with explicit approval",
            "keep automatic promotion disabled until admission policy is stronger",
            false,
        ),
        memory_layer(
            "external_knowledge",
            "ready",
            "docs/external-knowledge-adapter.md",
            "external-brain boundary is documented and local markdown/text search exposes read-only provenance-bearing hits without connecting live services",
            "wire live wiki/GBrain only through audited read-only adapters after local provenance stays stable",
            false,
        ),
        memory_layer(
            "maintenance_loop",
            "ready",
            "docs/memory-maintenance-loop.md",
            "maintenance report remains dry-run and maintenance apply requires explicit --approve-writeback for selected LIM candidates",
            "add scheduled maintainer only after approval/admission policy is explicit",
            false,
        ),
    ];

    let ready_count = layers.iter().filter(|layer| layer.state == "ready").count();
    let partial_count = layers
        .iter()
        .filter(|layer| layer.state == "partial")
        .count();
    let deferred_count = layers
        .iter()
        .filter(|layer| layer.state == "deferred")
        .count();
    let blocked_count = layers
        .iter()
        .filter(|layer| layer.state == "blocked")
        .count();
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "layered_mvp_partial"
    } else {
        "ready"
    };

    MemoryReadinessStatus {
        ok: blocked_count == 0,
        overall_state: overall_state.to_string(),
        layer_count: layers.len(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        layers,
    }
}

fn build_memory_maintenance_receipt(config: &ConfigSummary) -> MemoryMaintenanceReceiptStatus {
    let experiences_path = Path::new(&config.identity_experiences_path).to_path_buf();
    let experiences_path_text = experiences_path.display().to_string();
    match std::fs::read_to_string(&experiences_path) {
        Ok(content) => {
            let entries = parse_experience_entries(&content);
            let mut receipt_count = 0usize;
            let mut latest_receipt = None;

            for entry in entries.into_iter().rev() {
                let fields = parse_key_value_lines(&entry.body);
                if fields
                    .get("writeback")
                    .is_some_and(|value| value == "memory_maintenance_apply")
                    && fields
                        .get("approved_writeback")
                        .is_some_and(|value| value == "true")
                {
                    receipt_count += 1;
                    if latest_receipt.is_none() {
                        latest_receipt = Some((entry.id, fields));
                    }
                }
            }

            if let Some((entry_id, fields)) = latest_receipt {
                MemoryMaintenanceReceiptStatus {
                    available: true,
                    readable: true,
                    state: "ready".to_string(),
                    experiences_path: experiences_path_text,
                    receipt_count,
                    latest_entry_id: Some(entry_id),
                    latest_source_record_id: fields.get("source_record_id").cloned(),
                    latest_approval_source: fields.get("approval_source").cloned(),
                    latest_approved_at: fields.get("approved_at").cloned(),
                    latest_approval_note: fields.get("approval_note").cloned(),
                    latest_provenance_preserved: fields
                        .get("provenance_preserved")
                        .is_some_and(|value| value == "true"),
                    current: "latest approved memory maintenance writeback is visible in experiences.md".to_string(),
                    next_action: "keep using memory maintenance apply --approve-writeback for future receipts".to_string(),
                    error: None,
                }
            } else {
                MemoryMaintenanceReceiptStatus {
                    available: true,
                    readable: true,
                    state: "missing".to_string(),
                    experiences_path: experiences_path_text,
                    receipt_count: 0,
                    latest_entry_id: None,
                    latest_source_record_id: None,
                    latest_approval_source: None,
                    latest_approved_at: None,
                    latest_approval_note: None,
                    latest_provenance_preserved: false,
                    current: "no approved memory maintenance writeback found in experiences.md".to_string(),
                    next_action: "run memory maintenance apply --approve-writeback after operator review".to_string(),
                    error: None,
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            MemoryMaintenanceReceiptStatus {
                available: false,
                readable: false,
                state: "missing".to_string(),
                experiences_path: experiences_path_text,
                receipt_count: 0,
                latest_entry_id: None,
                latest_source_record_id: None,
                latest_approval_source: None,
                latest_approved_at: None,
                latest_approval_note: None,
                latest_provenance_preserved: false,
                current: "experiences.md is missing, so no memory maintenance receipt can be summarized".to_string(),
                next_action: "create the identity memory root and append an approved memory maintenance receipt".to_string(),
                error: Some("experiences.md missing".to_string()),
            }
        }
        Err(error) => MemoryMaintenanceReceiptStatus {
            available: false,
            readable: false,
            state: "unreadable".to_string(),
            experiences_path: experiences_path_text,
            receipt_count: 0,
            latest_entry_id: None,
            latest_source_record_id: None,
            latest_approval_source: None,
            latest_approved_at: None,
            latest_approval_note: None,
            latest_provenance_preserved: false,
            current: "experiences.md could not be read for receipt summary".to_string(),
            next_action: "fix identity memory permissions before checking the receipt summary".to_string(),
            error: Some(error.to_string()),
        },
    }
}

fn parse_experience_entries(content: &str) -> Vec<ExperienceEntry> {
    let mut entries = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(id) = line.strip_prefix("## ") {
            push_experience_entry(
                &mut entries,
                current_id.take().or_else(|| {
                    if current_body.trim().is_empty() {
                        None
                    } else {
                        Some("experiences.preamble".to_string())
                    }
                }),
                &current_body,
            );
            current_id = Some(id.trim().to_string());
            current_body.clear();
        } else {
            if !current_body.is_empty() {
                current_body.push('\n');
            }
            current_body.push_str(line);
        }
    }

    push_experience_entry(
        &mut entries,
        current_id.or_else(|| {
            if current_body.trim().is_empty() {
                None
            } else {
                Some("experiences.preamble".to_string())
            }
        }),
        &current_body,
    );

    entries
}

fn push_experience_entry(entries: &mut Vec<ExperienceEntry>, id: Option<String>, body: &str) {
    if let Some(id) = id {
        let body = body.trim();
        if !body.is_empty() {
            entries.push(ExperienceEntry {
                id,
                body: body.to_string(),
            });
        }
    }
}

fn parse_key_value_lines(body: &str) -> std::collections::BTreeMap<String, String> {
    let mut fields = std::collections::BTreeMap::new();
    for line in body.lines() {
        if let Some((key, value)) = line.split_once('=') {
            fields.insert(key.trim().to_string(), value.trim().to_string());
        }
    }
    fields
}

#[derive(Debug, Clone)]
struct ExperienceEntry {
    id: String,
    body: String,
}

fn memory_layer(
    name: &str,
    state: &str,
    storage: &str,
    current: &str,
    next_action: &str,
    writes_automatically: bool,
) -> MemoryLayerStatus {
    MemoryLayerStatus {
        name: name.to_string(),
        state: state.to_string(),
        storage: storage.to_string(),
        current: current.to_string(),
        next_action: next_action.to_string(),
        writes_automatically,
    }
}

fn build_project_readiness(
    config: &ConfigSummary,
    slots: &RuntimeSlotsSummary,
    plugin_registry: &PluginRegistrySummary,
    atomic_tools: &AtomicToolSurfaceStatus,
    governance: &GovernanceReadinessStatus,
    goal_mode: &GoalModeStatus,
    goal_run: &GoalRunReadinessStatus,
) -> ProjectReadinessStatus {
    let mut modules = vec![
        project_module(
            "main_chain",
            "ready",
            "input -> context -> governance -> execution slot -> report -> memory is wired",
            "keep smoke/doctor as the release gate before module-level optimization",
            "core",
        ),
        project_module(
            "identity",
            "ready",
            "Hermes dual-file identity memory and bootstrap identity files are configured",
            "continue filling SOUL/STORY/FIRST_WAKE content through explicit writes",
            "core",
        ),
        project_module(
            "memory",
            "ready",
            "hot memory, session recall, LIM candidate extraction, explicit maintenance apply, and local external-knowledge search are available",
            "keep wiki/GBrain live adapters behind explicit provenance contracts",
            "core+adapter",
        ),
        project_module(
            "context",
            "ready",
            &format!(
                "{} context engine is selected with deterministic budget diagnostics",
                config.context_engine_kind
            ),
            "improve summary_compression quality without changing the default engine",
            "core",
        ),
        project_module(
            "governance",
            if governance.ok { "ready" } else { "blocked" },
            "Markdown rules, risk probes, approval labels, and governed tool surface are visible",
            "move more adapter actions through the same approval/audit path",
            "core",
        ),
        project_module(
            "execution_tools",
            if atomic_tools.ok { "ready" } else { "blocked" },
            "GA atomic tool surface is registered; file_read/file_write/code_execute are mapped",
            "map desktop/browser atoms through explicit actuator adapters after mainline stabilizes",
            "slot",
        ),
        project_module(
            "reporting",
            "ready",
            "runtime reports, tool events, tool report schema, and subagent admission records are structured",
            "standardize human-facing report templates after protocol settles",
            "core",
        ),
        project_module(
            "channel",
            "ready",
            "app-server, channel simulate, dedicated Feishu bridge, rich message rendering, and local preflight are wired",
            "keep live Feishu verification in the adapter boundary without mixing in Codex or Hermes",
            "adapter",
        ),
        project_module(
            "subagent",
            if slots.subagent == "queued_external" {
                "ready"
            } else {
                "deferred"
            },
            "queued dispatch/report/collect protocol, approved command runner, capability matching, and bounded multi-worker run-loop are wired",
            "keep live worker pools behind audited adapters",
            "slot",
        ),
        project_module(
            "goal",
            if goal_mode.ok && goal_run.ok { "ready" } else { "blocked" },
            "GoalSpec runtime context and checkpoint-first GoalRun records are available",
            "keep it as planning/checkpoint primitive until execution governance is stronger",
            "core",
        ),
        project_module(
            "plugins",
            if plugin_registry.ok { "ready" } else { "blocked" },
            "plugin registry is manifest/readiness only and does not execute disabled plugins",
            "add real adapter manifests only with allowlists and separate credentials",
            "adapter",
        ),
        project_module(
            "external_ai",
            if Path::new("src/external_ai_dispatch.rs").exists() {
                "ready"
            } else {
                "partial"
            },
            "GenesisActuator exists, BrowserWorker is frozen, and external-ai dispatch exposes a dry-run unified identity engine contract",
            "connect live external platforms only through audited adapter manifests and subagent review",
            "adapter",
        ),
    ];

    let ready_count = modules
        .iter()
        .filter(|module| module.state == "ready")
        .count();
    let partial_count = modules
        .iter()
        .filter(|module| module.state == "partial")
        .count();
    let deferred_count = modules
        .iter()
        .filter(|module| module.state == "deferred")
        .count();
    let blocked_count = modules
        .iter()
        .filter(|module| module.state == "blocked")
        .count();
    let overall_state = if blocked_count > 0 {
        "blocked"
    } else if partial_count > 0 || deferred_count > 0 {
        "mvp_ready_with_partial_modules"
    } else {
        "ready"
    };

    modules.sort_by(|left, right| left.name.cmp(&right.name));

    ProjectReadinessStatus {
        ok: blocked_count == 0,
        overall_state: overall_state.to_string(),
        ready_count,
        partial_count,
        deferred_count,
        blocked_count,
        modules,
    }
}

fn project_module(
    name: &str,
    state: &str,
    current: &str,
    next_action: &str,
    core_boundary: &str,
) -> ProjectModuleStatus {
    ProjectModuleStatus {
        name: name.to_string(),
        state: state.to_string(),
        current: current.to_string(),
        next_action: next_action.to_string(),
        core_boundary: core_boundary.to_string(),
    }
}

fn governance_readiness_status(
    config: &RuntimeConfig,
) -> Result<GovernanceReadinessStatus, ConfigError> {
    let rules = MarkdownRuleSet::load(&config.rules.core_path).map_err(|message| ConfigError {
        field: "rules.core_path".to_string(),
        message,
    })?;
    let read_only_probe = governance_probe("read-only-status-probe", ActionKind::Observe);
    let rule_check = rules.check(&read_only_probe);
    let governance = StaticRuleGovernance::with_rules(rules);
    let read_only_decision = classify_probe(&governance, &read_only_probe)?;
    let dangerous_write_decision = classify_probe(
        &governance,
        &governance_probe("dangerous-write-probe", ActionKind::DeleteOrCleanup),
    )?;
    let dangerous_shell_decision = classify_probe(
        &governance,
        &governance_probe("dangerous-shell-probe", ActionKind::ServiceChange),
    )?;
    let secret_shell_decision = classify_probe(
        &governance,
        &governance_probe("secret-shell-probe", ActionKind::SecretAccess),
    )?;

    Ok(GovernanceReadinessStatus {
        ok: read_only_decision == "allowed"
            && dangerous_write_decision == "needs_approval"
            && dangerous_shell_decision == "needs_approval"
            && secret_shell_decision == "draft_only",
        kind: config.governance.kind().to_string(),
        rules_loaded: true,
        rules_core_path: config.rules.core_path.display().to_string(),
        rule_count: rule_check.rule_count,
        rules_fingerprint: rule_check.fingerprint,
        tool_surface_governed: true,
        read_only_decision,
        dangerous_write_decision,
        dangerous_shell_decision,
        secret_shell_decision,
        goal_run_executes: false,
    })
}

fn governance_probe(action_id: &str, kind: ActionKind) -> ProposedAction {
    ProposedAction {
        action_id: action_id.to_string(),
        kind,
        target: "readiness probe".to_string(),
        summary: "doctor/status readiness classification only".to_string(),
    }
}

fn classify_probe(
    governance: &StaticRuleGovernance,
    action: &ProposedAction,
) -> Result<String, ConfigError> {
    let decision = governance.classify(action).map_err(|error| ConfigError {
        field: "governance".to_string(),
        message: error.message,
    })?;
    Ok(risk_decision_parts(&decision).0.to_string())
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
        checkpoint_policy: default_goal.checkpoint_policy,
        final_report_policy: default_goal.final_report_policy,
        bypasses_governance: false,
        adds_core_slot: false,
    }
}

pub fn summarize_goal_run_readiness(
    root: impl AsRef<Path>,
    goal_id: &str,
) -> GoalRunReadinessStatus {
    let root = root.as_ref();
    let store = GoalRunStore::new(root);
    let path = match store.goal_path(goal_id) {
        Ok(path) => path,
        Err(error) => {
            return GoalRunReadinessStatus {
                ok: false,
                root: root.display().to_string(),
                goal_id: goal_id.to_string(),
                path: String::new(),
                plan_exists: false,
                checkpoint_count: 0,
                worker_count: 0,
                validation_command_count: 0,
                checkpoint_log_complete: false,
                last_checkpoint_id: None,
                last_checkpoint_summary: None,
                last_checkpoint_created_at: None,
                last_checkpoint_completed_worker_ids: None,
                last_checkpoint_validation_notes: None,
                incomplete_reasons: vec!["goal path could not be resolved".to_string()],
                read_error: Some(format!("{}: {}", error.field, error.message)),
            };
        }
    };
    let plan_exists = path.exists();
    if !plan_exists {
        return GoalRunReadinessStatus {
            ok: true,
            root: root.display().to_string(),
            goal_id: goal_id.to_string(),
            path: path.display().to_string(),
            plan_exists: false,
            checkpoint_count: 0,
            worker_count: 0,
            validation_command_count: 0,
            checkpoint_log_complete: false,
            last_checkpoint_id: None,
            last_checkpoint_summary: None,
            last_checkpoint_created_at: None,
            last_checkpoint_completed_worker_ids: None,
            last_checkpoint_validation_notes: None,
            incomplete_reasons: Vec::new(),
            read_error: None,
        };
    }

    match store.load(goal_id) {
        Ok(run) => {
            let diagnostics = run.diagnostics();
            GoalRunReadinessStatus {
                ok: true,
                root: root.display().to_string(),
                goal_id: goal_id.to_string(),
                path: path.display().to_string(),
                plan_exists: true,
                checkpoint_count: run.checkpoint_log.len(),
                worker_count: run.worker_plan.len(),
                validation_command_count: run.validation_plan.commands.len(),
                checkpoint_log_complete: diagnostics.checkpoint_log_complete,
                last_checkpoint_id: diagnostics.last_checkpoint_id,
                last_checkpoint_summary: diagnostics.last_checkpoint_summary,
                last_checkpoint_created_at: diagnostics.last_checkpoint_created_at,
                last_checkpoint_completed_worker_ids: diagnostics
                    .last_checkpoint_completed_worker_ids,
                last_checkpoint_validation_notes: diagnostics.last_checkpoint_validation_notes,
                incomplete_reasons: diagnostics.incomplete_reasons,
                read_error: None,
            }
        }
        Err(error) => GoalRunReadinessStatus {
            ok: false,
            root: root.display().to_string(),
            goal_id: goal_id.to_string(),
            path: path.display().to_string(),
            plan_exists: true,
            checkpoint_count: 0,
            worker_count: 0,
            validation_command_count: 0,
            checkpoint_log_complete: false,
            last_checkpoint_id: None,
            last_checkpoint_summary: None,
            last_checkpoint_created_at: None,
            last_checkpoint_completed_worker_ids: None,
            last_checkpoint_validation_notes: None,
            incomplete_reasons: vec!["goal run could not be loaded".to_string()],
            read_error: Some(format!("{}: {}", error.field, error.message)),
        },
    }
}
