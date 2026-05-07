use chuang_agent::control_workflow::{build_unit_views, ControlUnitView};
use chuang_agent::kernel_status::{build_chuang_mvp_status, ChuangMvpStatus};
use chuang_agent::plugin_registry::{load_plugin_registry, PluginKind, PluginManifest};
use chuang_agent::runtime_config::ConfigSummary;
use chuang_agent::slot_registry::build_runtime_slots;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::app_server::{
    app_server_health_diagnostic_status, app_server_health_diagnostic_summary,
    app_server_health_next_actions,
};
use crate::cli_args::{effective_config_source, parse_status_cli_options, parse_status_output};
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_runtime::kernel_config_from_runtime;

const DEFAULT_TERMINAL_WATCHDOG_REPORT_SUFFIX: &str =
    ".codex/chuang-goal-interactive/latest-watchdog-report.json";
const TERMINAL_WATCHDOG_REPORT_ENV: &str = "CHUANG_GOAL_WATCHDOG_REPORT_FILE";
const TERMINAL_WATCHDOG_STALE_SECONDS_ENV: &str = "CHUANG_GOAL_WATCHDOG_STALE_SECONDS";
const DEFAULT_TERMINAL_WATCHDOG_STALE_SECONDS: i64 = 3600;

pub(crate) fn console_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("snapshot") => console_snapshot_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn console_snapshot_command(args: &[String]) -> Result<(), String> {
    let output = parse_status_output(args)?;
    let options = parse_status_cli_options(args)?;
    let kernel = kernel_config_from_runtime(&options.runtime)?;
    let status = build_chuang_mvp_status(&options.runtime, &kernel)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let slots = build_runtime_slots(&options.runtime)
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let control_units = build_unit_views(
        slots
            .control_plane
            .try_list_units()
            .map_err(|err| format!("console_control_list_failed: {err:?}"))?,
    );
    let plugins = load_console_plugins()?;
    let terminal_watchdog = load_terminal_watchdog_status();
    let app_server_health = build_app_server_health_snapshot(&status.config);

    let snapshot = ConsoleSnapshot {
        ok: true,
        source: effective_config_source(args)?.unwrap_or_else(|| "defaults".to_string()),
        status,
        control_units,
        plugins,
        terminal_watchdog,
        app_server_health,
    };

    match output {
        ControlOutputFormat::Text => print_console_snapshot(&snapshot),
        ControlOutputFormat::Json => print_json(&snapshot)?,
    }

    Ok(())
}

fn print_console_snapshot(snapshot: &ConsoleSnapshot) {
    println!("console_ok: {}", snapshot.ok);
    println!("config_source: {}", snapshot.source);
    println!("provider: {}", snapshot.status.config.provider_kind);
    println!("model: {}", snapshot.status.config.model_name);
    println!("execution: {}", snapshot.status.slots.execution);
    println!(
        "atomic_tools: ok={} total={} mapped={} interface_only={} action_schema_version={} report_schema_version={}",
        snapshot.status.atomic_tools.ok,
        snapshot.status.atomic_tools.total_count,
        snapshot.status.atomic_tools.mapped_count,
        snapshot.status.atomic_tools.interface_only_count,
        snapshot.status.atomic_tools.tool_action_schema_version,
        snapshot.status.atomic_tools.tool_report_schema_version
    );
    println!(
        "atomic_tools_mapped: {}",
        format_name_list(&snapshot.status.atomic_tools.mapped_atomic_tool_names)
    );
    println!(
        "atomic_tools_interface_only: {}",
        format_name_list(
            &snapshot
                .status
                .atomic_tools
                .interface_only_atomic_tool_names
        )
    );
    println!("subagent: {}", snapshot.status.config.subagent_kind);
    println!(
        "project_readiness: ok={} state={}",
        snapshot.status.project_readiness.ok, snapshot.status.project_readiness.overall_state
    );
    println!(
        "channel_readiness: ok={} state={}",
        snapshot.status.channel_readiness.ok, snapshot.status.channel_readiness.overall_state
    );
    println!(
        "subagent_readiness: ok={} state={}",
        snapshot.status.subagent_readiness.ok, snapshot.status.subagent_readiness.overall_state
    );
    println!(
        "external_ai_readiness: ok={} state={}",
        snapshot.status.external_ai_readiness.ok,
        snapshot.status.external_ai_readiness.overall_state
    );
    println!(
        "release_readiness: ok={} name={} state={}",
        snapshot.status.release_readiness.ok,
        snapshot.status.release_readiness.release_name,
        snapshot.status.release_readiness.overall_state
    );
    println!(
        "release_acceptance: count={} connects_real_external_services={} verifies_real_external_services={} uses_stub_or_local_fixtures={}",
        snapshot.status.release_readiness.acceptance_count,
        snapshot
            .status
            .release_readiness
            .connects_real_external_services,
        snapshot
            .status
            .release_readiness
            .verifies_real_external_services,
        snapshot.status.release_readiness.uses_stub_or_local_fixtures
    );
    println!("control_units: {}", snapshot.control_units.len());
    println!("plugins: {}", snapshot.plugins.len());
    println!(
        "terminal_watchdog: available={} readable={} fresh={} diagnostic_status={} readonly={} session={} tmux_session_present={} codex_process_count={} git_dirty={} next_action={}",
        snapshot.terminal_watchdog.available,
        snapshot.terminal_watchdog.readable,
        snapshot.terminal_watchdog.fresh,
        snapshot.terminal_watchdog.diagnostic_status,
        snapshot.terminal_watchdog.readonly,
        optional_text(&snapshot.terminal_watchdog.session),
        optional_bool(snapshot.terminal_watchdog.tmux_session_present),
        optional_usize(snapshot.terminal_watchdog.codex_process_count),
        optional_bool(snapshot.terminal_watchdog.git_dirty),
        optional_text(&snapshot.terminal_watchdog.next_action)
    );
    println!(
        "app_server_health: status={} summary={} next_actions={}",
        snapshot.app_server_health.diagnostic_status,
        snapshot.app_server_health.diagnostic_summary,
        if snapshot.app_server_health.next_actions.is_empty() {
            "none".to_string()
        } else {
            snapshot.app_server_health.next_actions.join(";")
        }
    );
    println!(
        "plugin_registry: available={} ok={} plugin_count={} enabled_count={} issue_count={}",
        snapshot.status.plugin_registry.available,
        snapshot.status.plugin_registry.ok,
        snapshot.status.plugin_registry.plugin_count,
        snapshot.status.plugin_registry.enabled_count,
        snapshot.status.plugin_registry.issue_count
    );
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct ConsoleSnapshot {
    ok: bool,
    source: String,
    status: ChuangMvpStatus,
    control_units: Vec<ControlUnitView>,
    plugins: Vec<PluginOverview>,
    terminal_watchdog: TerminalWatchdogStatus,
    app_server_health: AppServerHealthSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct AppServerHealthSnapshot {
    diagnostic_status: String,
    diagnostic_summary: String,
    next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct PluginOverview {
    id: String,
    kind: PluginKind,
    display_name: String,
    enabled: bool,
    capabilities: Vec<String>,
}

fn format_name_list(names: &[String]) -> String {
    if names.is_empty() {
        "none".to_string()
    } else {
        names.join(",")
    }
}

fn optional_text(value: &Option<String>) -> &str {
    value.as_deref().unwrap_or("unknown")
}

fn optional_bool(value: Option<bool>) -> String {
    value
        .map(|inner| inner.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn optional_usize(value: Option<usize>) -> String {
    value
        .map(|inner| inner.to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

fn load_console_plugins() -> Result<Vec<PluginOverview>, String> {
    let path = PathBuf::from("plugins/registry.example.json");
    if !path.exists() {
        return Ok(Vec::new());
    }
    let registry = load_plugin_registry(&path)?;
    Ok(registry
        .plugins
        .into_iter()
        .map(|plugin: PluginManifest| PluginOverview {
            id: plugin.id,
            kind: plugin.kind,
            display_name: plugin.display_name,
            enabled: plugin.enabled,
            capabilities: plugin.capabilities,
        })
        .collect())
}

fn load_terminal_watchdog_status() -> TerminalWatchdogStatus {
    let report_file = terminal_watchdog_report_file();
    TerminalWatchdogStatus::from_report_file(
        &report_file,
        terminal_watchdog_stale_threshold_seconds(),
    )
}

fn build_app_server_health_snapshot(summary: &ConfigSummary) -> AppServerHealthSnapshot {
    AppServerHealthSnapshot {
        diagnostic_status: app_server_health_diagnostic_status(summary).to_string(),
        diagnostic_summary: app_server_health_diagnostic_summary(summary, false),
        next_actions: app_server_health_next_actions(summary),
    }
}

fn terminal_watchdog_report_file() -> PathBuf {
    if let Some(path) = std::env::var_os(TERMINAL_WATCHDOG_REPORT_ENV) {
        return PathBuf::from(path);
    }
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/"))
        .join(DEFAULT_TERMINAL_WATCHDOG_REPORT_SUFFIX)
}

fn terminal_watchdog_stale_threshold_seconds() -> i64 {
    std::env::var(TERMINAL_WATCHDOG_STALE_SECONDS_ENV)
        .ok()
        .and_then(|value| value.parse::<i64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(DEFAULT_TERMINAL_WATCHDOG_STALE_SECONDS)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct TerminalWatchdogStatus {
    available: bool,
    readable: bool,
    fresh: bool,
    diagnostic_status: String,
    report_file: String,
    error: Option<String>,
    schema_version: Option<u64>,
    generated_at: Option<String>,
    age_seconds: Option<i64>,
    stale_after_seconds: i64,
    readonly: bool,
    project_root: Option<String>,
    session: Option<String>,
    tmux_session_present: Option<bool>,
    pane_bytes: Option<u64>,
    codex_process_count: Option<usize>,
    git_dirty: Option<bool>,
    git_status_count: Option<usize>,
    next_action: Option<String>,
    attach_command: Option<String>,
    review_command: Option<String>,
    dispatches_tasks: bool,
    modifies_repo: bool,
    restarts_worker: bool,
    touches_services: bool,
}

impl TerminalWatchdogStatus {
    fn from_report_file(path: &Path, stale_after_seconds: i64) -> Self {
        let report_file = path.display().to_string();
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Self::diagnostic(
                    report_file,
                    stale_after_seconds,
                    "missing",
                    "report_missing",
                    "run_watchdog_once_before_console_review",
                    false,
                    false,
                    false,
                );
            }
            Err(_) => {
                return Self::diagnostic(
                    report_file,
                    stale_after_seconds,
                    "unreadable",
                    "report_read_failed",
                    "check_watchdog_report_file_permissions",
                    true,
                    false,
                    false,
                );
            }
        };
        let report = match serde_json::from_str::<TerminalWatchdogReport>(&content) {
            Ok(report) => report,
            Err(_) => {
                return Self::diagnostic(
                    report_file,
                    stale_after_seconds,
                    "invalid",
                    "report_parse_failed",
                    "inspect_or_regenerate_watchdog_report",
                    true,
                    true,
                    false,
                );
            }
        };
        let freshness =
            terminal_watchdog_freshness(report.generated_at.as_deref(), stale_after_seconds);
        let diagnostic_status = if freshness.fresh { "fresh" } else { "stale" };
        let next_action = if freshness.fresh {
            report
                .takeover
                .as_ref()
                .and_then(|takeover| takeover.next_action.clone())
                .unwrap_or_else(|| "observe_watchdog_report".to_string())
        } else {
            "run_watchdog_once_before_console_review".to_string()
        };

        Self {
            available: true,
            readable: true,
            fresh: freshness.fresh,
            diagnostic_status: diagnostic_status.to_string(),
            report_file,
            error: freshness.error,
            schema_version: report.schema_version,
            generated_at: report.generated_at,
            age_seconds: freshness.age_seconds,
            stale_after_seconds,
            readonly: report.readonly.unwrap_or(false),
            project_root: report.project_root,
            session: report.session,
            tmux_session_present: report.tmux_session_present,
            pane_bytes: report.pane.and_then(|pane| pane.bytes),
            codex_process_count: report.codex_processes.and_then(|processes| processes.count),
            git_dirty: report.git.as_ref().and_then(|git| git.dirty),
            git_status_count: report
                .git
                .and_then(|git| git.status_short)
                .map(|status_short| status_short.len()),
            next_action: Some(next_action),
            attach_command: report
                .takeover
                .as_ref()
                .and_then(|takeover| takeover.attach_command.clone()),
            review_command: report.takeover.and_then(|takeover| takeover.review_command),
            dispatches_tasks: report
                .boundaries
                .as_ref()
                .and_then(|boundaries| boundaries.dispatches_tasks)
                .unwrap_or(false),
            modifies_repo: report
                .boundaries
                .as_ref()
                .and_then(|boundaries| boundaries.modifies_repo)
                .unwrap_or(false),
            restarts_worker: report
                .boundaries
                .as_ref()
                .and_then(|boundaries| boundaries.restarts_worker)
                .unwrap_or(false),
            touches_services: report
                .boundaries
                .and_then(|boundaries| boundaries.touches_services)
                .unwrap_or(false),
        }
    }

    fn diagnostic(
        report_file: String,
        stale_after_seconds: i64,
        diagnostic_status: &str,
        error: &str,
        next_action: &str,
        available: bool,
        readable: bool,
        fresh: bool,
    ) -> Self {
        Self {
            available,
            readable,
            fresh,
            diagnostic_status: diagnostic_status.to_string(),
            report_file,
            error: Some(error.to_string()),
            schema_version: None,
            generated_at: None,
            age_seconds: None,
            stale_after_seconds,
            readonly: true,
            project_root: None,
            session: None,
            tmux_session_present: None,
            pane_bytes: None,
            codex_process_count: None,
            git_dirty: None,
            git_status_count: None,
            next_action: Some(next_action.to_string()),
            attach_command: None,
            review_command: None,
            dispatches_tasks: false,
            modifies_repo: false,
            restarts_worker: false,
            touches_services: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalWatchdogFreshness {
    fresh: bool,
    age_seconds: Option<i64>,
    error: Option<String>,
}

fn terminal_watchdog_freshness(
    generated_at: Option<&str>,
    stale_after_seconds: i64,
) -> TerminalWatchdogFreshness {
    let Some(generated_at) = generated_at else {
        return TerminalWatchdogFreshness {
            fresh: false,
            age_seconds: None,
            error: Some("generated_at_missing".to_string()),
        };
    };
    let generated_at = match chrono::DateTime::parse_from_rfc3339(generated_at) {
        Ok(value) => value.with_timezone(&chrono::Utc),
        Err(_) => {
            return TerminalWatchdogFreshness {
                fresh: false,
                age_seconds: None,
                error: Some("generated_at_invalid".to_string()),
            };
        }
    };
    let age_seconds = chrono::Utc::now()
        .signed_duration_since(generated_at)
        .num_seconds()
        .max(0);
    TerminalWatchdogFreshness {
        fresh: age_seconds <= stale_after_seconds,
        age_seconds: Some(age_seconds),
        error: if age_seconds <= stale_after_seconds {
            None
        } else {
            Some("report_stale".to_string())
        },
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TerminalWatchdogReport {
    schema_version: Option<u64>,
    generated_at: Option<String>,
    readonly: Option<bool>,
    project_root: Option<String>,
    session: Option<String>,
    tmux_session_present: Option<bool>,
    pane: Option<TerminalWatchdogPaneReport>,
    codex_processes: Option<TerminalWatchdogProcessReport>,
    git: Option<TerminalWatchdogGitReport>,
    takeover: Option<TerminalWatchdogTakeoverReport>,
    boundaries: Option<TerminalWatchdogBoundaryReport>,
}

#[derive(Debug, Clone, Deserialize)]
struct TerminalWatchdogPaneReport {
    bytes: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct TerminalWatchdogProcessReport {
    count: Option<usize>,
}

#[derive(Debug, Clone, Deserialize)]
struct TerminalWatchdogGitReport {
    dirty: Option<bool>,
    status_short: Option<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize)]
struct TerminalWatchdogTakeoverReport {
    next_action: Option<String>,
    attach_command: Option<String>,
    review_command: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct TerminalWatchdogBoundaryReport {
    dispatches_tasks: Option<bool>,
    modifies_repo: Option<bool>,
    restarts_worker: Option<bool>,
    touches_services: Option<bool>,
}
