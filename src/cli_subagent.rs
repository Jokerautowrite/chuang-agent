use std::collections::BTreeSet;
use std::io::{ErrorKind, Read, Write};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, Timestamp};
use chuang_agent::runtime_config::EvolutionConfig;
use chuang_agent::slot_registry::build_runtime_slots;
use chuang_agent::live_adapter_gate::{require_live_adapter_enabled, LiveAdapterSlot};
use chuang_agent::skill_evolver::{EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver};
use chuang_agent::live_subagent_rehearsal::{
    rehearse_live_subagent_adapter, LiveSubagentRehearsalInput,
};
use chuang_agent::subagent_queue::FileSubagentQueue;
use chuang_agent::subagent_report::{
    build_parent_context_handoff, ExecutionStatus, GovernanceDecisionSummary,
    ReportAdmissionStatus, ResourceUsage, SubagentReport, SubagentReportValidator,
};
use chuang_agent::subagent_spawner::{QueuedSubagentSpawner, RunId};

use crate::cli_args::{
    parse_subagent_collect, parse_subagent_dispatch, parse_subagent_list,
    parse_subagent_live_preflight, parse_subagent_release_claim, parse_subagent_report,
    parse_subagent_run_loop, parse_subagent_run_once,
};
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_types::*;

const COMMAND_RUNNER_OUTPUT_CAPTURE_LIMIT_BYTES: usize = 64 * 1024;
const COMMAND_RUNNER_PREVIEW_CHARS: usize = 1200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SubagentEvolutionSource {
    RuntimeConfig,
    DefaultDryRunPromotion,
}

impl SubagentEvolutionSource {
    fn as_str(self) -> &'static str {
        match self {
            Self::RuntimeConfig => "runtime_config",
            Self::DefaultDryRunPromotion => "default_dry_run_promotion",
        }
    }
}

pub(crate) fn subagent_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("dispatch") => subagent_dispatch_command(&args[1..]),
        Some("report") => subagent_report_command(&args[1..]),
        Some("collect") => subagent_collect_command(&args[1..]),
        Some("release-claim") => subagent_release_claim_command(&args[1..]),
        Some("list") => subagent_list_command(&args[1..]),
        Some("run-once") => subagent_run_once_command(&args[1..]),
        Some("run-loop") => subagent_run_loop_command(&args[1..]),
        Some("live-preflight") => subagent_live_preflight_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn subagent_live_preflight_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_live_preflight(args)?;
    let rehearsal = rehearse_live_subagent_adapter(LiveSubagentRehearsalInput {
        runner: request.runner,
        runner_command: request.runner_command,
        allowed_runner_commands: request.allowed_runner_commands,
        required_capabilities: request.required_capabilities,
        worker_capabilities: request.worker_capabilities,
    });
    let output = SubagentLivePreflightCliOutput { rehearsal };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_live_preflight ok={} ready_for_live={} readonly={} starts_external_worker={} gate_enabled={} runner_allowlist_ok={} capability_routing_ok={} report_admission_ok={} forbidden_capabilities_ok={} approval_audit_prerequisites_ok={} next_action={}",
                output.rehearsal.ok,
                output.rehearsal.ready_for_live,
                output.rehearsal.readonly,
                output.rehearsal.starts_external_worker,
                output.rehearsal.gate_enabled,
                output.rehearsal.runner_allowlist_ok,
                output.rehearsal.capability_routing_ok,
                output.rehearsal.report_admission_ok,
                output.rehearsal.forbidden_capabilities_ok,
                output.rehearsal.approval_audit_prerequisites_ok,
                output.rehearsal.next_action
            );
            println!(
                "worker_runtime live_worker_available={} worker_runtime_state={} adapter_entrypoint={} reason={}",
                output.rehearsal.live_worker_available,
                output.rehearsal.worker_runtime_state,
                output.rehearsal.adapter_entrypoint,
                output.rehearsal.worker_runtime_reason
            );
            println!(
                "gate enabled={} env_value_state={} required_env={} default_enabled={} audit_label={} preflight_checks={} reason={}",
                output.rehearsal.gate.enabled,
                output.rehearsal.gate.env_value_state,
                output.rehearsal.gate.required_env,
                output.rehearsal.gate.default_enabled,
                output.rehearsal.gate.audit_label,
                output.rehearsal.gate.preflight_checks.join("|"),
                output.rehearsal.gate.reason
            );
            println!(
                "runner_allowlist ok={} runner={} runner_command={} exact_match_required={} allowed_runner_commands={} matched_runner_command={} reason={}",
                output.rehearsal.runner_allowlist.ok,
                output.rehearsal.runner_allowlist.runner,
                output.rehearsal.runner_allowlist.runner_command,
                output.rehearsal.runner_allowlist.exact_match_required,
                output
                    .rehearsal
                    .runner_allowlist
                    .allowed_runner_commands
                    .join(","),
                output
                    .rehearsal
                    .runner_allowlist
                    .matched_runner_command
                    .as_deref()
                    .unwrap_or("none"),
                output.rehearsal.runner_allowlist.reason
            );
            println!(
                "capability_routing ok={} required_capabilities={} worker_capabilities={} matched_capabilities={} missing_capabilities={} reason={}",
                output.rehearsal.capability_routing.ok,
                output.rehearsal.capability_routing.required_capabilities.join(","),
                output.rehearsal.capability_routing.worker_capabilities.join(","),
                output.rehearsal.capability_routing.matched_capabilities.join(","),
                output.rehearsal.capability_routing.missing_capabilities.join(","),
                output.rehearsal.capability_routing.reason
            );
            println!(
                "report_admission ok={} required={} covered_commands={} stable_reason_codes={} evidence={}",
                output.rehearsal.report_admission.ok,
                output.rehearsal.report_admission.required,
                output.rehearsal.report_admission.covered_commands.join(","),
                output.rehearsal.report_admission.stable_reason_codes.join(","),
                output.rehearsal.report_admission.evidence
            );
            println!(
                "forbidden_capabilities ok={} must_reject_capabilities={} requested_forbidden_capabilities={} checked_capability_sources={} reason={}",
                output.rehearsal.forbidden_capabilities.ok,
                output
                    .rehearsal
                    .forbidden_capabilities
                    .must_reject_capabilities
                    .join("|"),
                output
                    .rehearsal
                    .forbidden_capabilities
                    .requested_forbidden_capabilities
                    .join(","),
                output
                    .rehearsal
                    .forbidden_capabilities
                    .checked_capability_sources
                    .join("|"),
                output.rehearsal.forbidden_capabilities.reason
            );
            println!(
                "approval_audit_prerequisites ok={} explicit_operator_approval_required={} governance_approval_required={} audit_receipt_required={} dispatch_evidence_required={} audit_label={} prerequisites={} reason={}",
                output.rehearsal.approval_audit_prerequisites.ok,
                output
                    .rehearsal
                    .approval_audit_prerequisites
                    .explicit_operator_approval_required,
                output
                    .rehearsal
                    .approval_audit_prerequisites
                    .governance_approval_required,
                output
                    .rehearsal
                    .approval_audit_prerequisites
                    .audit_receipt_required,
                output
                    .rehearsal
                    .approval_audit_prerequisites
                    .dispatch_evidence_required,
                output.rehearsal.approval_audit_prerequisites.audit_label,
                output
                    .rehearsal
                    .approval_audit_prerequisites
                    .prerequisites
                    .join("|"),
                output.rehearsal.approval_audit_prerequisites.reason
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_dispatch_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_dispatch(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let mut spawner = QueuedSubagentSpawner::new();
    let dispatch_ids = unique_cli_subagent_ids(&request.spawn.agent_name)?;
    let receipt = spawner
        .spawn_with_ids(request.spawn, dispatch_ids.run_id, dispatch_ids.agent_id)
        .map_err(|e| format!("subagent_spawn_failed: {e:?}"))?;
    let paths = queue
        .flush_pending_dispatches(&spawner)
        .map_err(|e| format!("subagent_dispatch_write_failed: {e:?}"))?;
    let dispatch_path = paths
        .first()
        .ok_or_else(|| "subagent_dispatch_write_failed: no dispatch was produced".to_string())?
        .clone();
    let output = SubagentDispatchCliOutput {
        run_id: receipt.run_id.0,
        agent_id: receipt.agent_id.0,
        task_id: request.task_id.0,
        dispatch_path: dispatch_path.display().to_string(),
        queue_root: request
            .options
            .runtime
            .subagent_queue
            .root
            .display()
            .to_string(),
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_dispatch_queued run_id={} agent_id={} task_id={} path={}",
                output.run_id, output.agent_id, output.task_id, output.dispatch_path
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_list_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_list(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let dispatches = queue
        .list_dispatches()
        .map_err(|e| format!("subagent_dispatch_list_failed: {e:?}"))?;
    let report_run_ids = queue
        .list_report_run_ids()
        .map_err(|e| format!("subagent_report_list_failed: {e:?}"))?;
    let report_lookup = report_run_ids
        .iter()
        .map(|run_id| run_id.0.clone())
        .collect::<std::collections::BTreeSet<_>>();
    let items = dispatches
        .into_iter()
        .map(|dispatch| {
            let is_claimed = queue.is_claimed(&dispatch.run_id).unwrap_or(false);
            SubagentListItem {
                required_capabilities: dispatch_required_capabilities(&dispatch),
                is_claim_stale: queue
                    .is_claim_stale(&dispatch.run_id, dispatch.idle_timeout_ms)
                    .unwrap_or(false),
                run_id: dispatch.run_id.0.clone(),
                agent_id: dispatch.agent_id.0,
                task_id: dispatch.task_id.0,
                agent_name: dispatch.agent_name,
                tool_policy: format!("{:?}", dispatch.tool_policy),
                is_claimed,
                has_report: report_lookup.contains(&dispatch.run_id.0),
            }
        })
        .collect::<Vec<_>>();
    let output = SubagentListCliOutput {
        queue_root: request
            .options
            .runtime
            .subagent_queue
            .root
            .display()
            .to_string(),
        dispatch_count: items.len(),
        report_count: report_run_ids.len(),
        items,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_queue queue_root={} dispatch_count={} report_count={}",
                output.queue_root, output.dispatch_count, output.report_count
            );
            for item in &output.items {
                println!(
                    "run_id={} agent_id={} task_id={} policy={} required_capabilities={} is_claimed={} is_claim_stale={} has_report={}",
                    item.run_id,
                    item.agent_id,
                    item.task_id,
                    item.tool_policy,
                    item.required_capabilities.join(","),
                    item.is_claimed,
                    item.is_claim_stale,
                    item.has_report
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_run_once_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_run_once(args)?;
    let queue = open_subagent_queue(&request.options)?;
    let output = run_one_pending_subagent(&queue, &request)?;

    match request.output {
        ControlOutputFormat::Text => {
            if output.ran {
                println!(
                    "subagent_run_once runner={} evolution_kind={} evolution_source={} capabilities={} run_id={} report_path={} admission={}",
                    output.runner,
                    output.evolution_kind,
                    output.evolution_source,
                    output.worker_capabilities.join(","),
                    output.run_id.as_deref().unwrap_or("none"),
                    output.report_path.as_deref().unwrap_or("none"),
                    output
                        .report_admission
                        .as_ref()
                        .map(|admission| format!("{:?}", admission.status))
                        .unwrap_or_else(|| "none".to_string())
                );
            } else {
                println!(
                    "subagent_run_once idle runner={} evolution_kind={} evolution_source={} capabilities={}",
                    output.runner,
                    output.evolution_kind,
                    output.evolution_source,
                    output.worker_capabilities.join(",")
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_run_loop_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_run_loop(args)?;
    validate_live_run_loop_gate(&request)?;
    let queue = open_subagent_queue(&request.options)?;
    let run_once_request = SubagentRunOnceCliRequest {
        options: request.options.clone(),
        output: request.output,
        runner: request.runner.clone(),
        runner_command: request.runner_command.clone(),
        runner_args: request.runner_args.clone(),
        worker_capabilities: request.worker_capabilities.clone(),
        approve_exec: request.approve_exec,
    };

    let output = run_subagent_run_loop(
        &queue,
        &run_once_request,
        request.max_runs,
        request.max_concurrency,
        None,
    )?;

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_run_loop runner={} evolution_kind={} evolution_source={} capabilities={} ran_count={} max_runs={} max_concurrency={} idle={} admissions={}",
                output.runner,
                output.evolution_kind,
                output.evolution_source,
                output.worker_capabilities.join(","),
                output.ran_count,
                output.max_runs,
                output.max_concurrency,
                output.idle,
                output.report_admissions.len()
            );
            for run_id in &output.run_ids {
                println!("subagent_run_loop_ran run_id={run_id}");
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn validate_live_run_loop_gate(request: &SubagentRunLoopCliRequest) -> Result<(), String> {
    if !request.require_live_gate {
        return Ok(());
    }
    if request.runner != "command" {
        return Err("--require-live-gate requires --runner command".to_string());
    }
    let runner_command = request
        .runner_command
        .as_deref()
        .ok_or_else(|| "--require-live-gate requires --runner-command".to_string())?;
    if request.allowed_runner_commands.is_empty() {
        return Err("--require-live-gate requires at least one --allow-runner-command".to_string());
    }
    if !request
        .allowed_runner_commands
        .iter()
        .any(|allowed| allowed == runner_command)
    {
        return Err(format!(
            "live_runner_command_not_allowlisted: runner_command={runner_command}"
        ));
    }
    require_live_adapter_enabled(LiveAdapterSlot::SubagentRunner).map_err(|error| {
        format!(
            "live_runner_gate_disabled: required_env={} audit_label={} reason={}",
            error.required_env, error.audit_label, error.reason
        )
    })?;
    Ok(())
}

pub(crate) fn run_subagent_run_loop(
    queue: &FileSubagentQueue,
    request: &SubagentRunOnceCliRequest,
    max_runs: usize,
    max_concurrency: usize,
    allowed_run_ids: Option<&BTreeSet<String>>,
) -> Result<SubagentRunLoopCliOutput, String> {
    if max_runs == 0 {
        return Err("--max-runs must be greater than zero".to_string());
    }
    if max_concurrency == 0 {
        return Err("--max-concurrency must be greater than zero".to_string());
    }

    let allowed_run_ids = allowed_run_ids.cloned().map(Arc::new);
    let mut run_ids = Vec::new();
    let mut report_paths = Vec::new();
    let mut report_admissions = Vec::new();
    let mut idle = false;

    while run_ids.len() < max_runs {
        let remaining_runs = max_runs - run_ids.len();
        let batch_size = max_concurrency.min(remaining_runs);
        let outputs = if batch_size == 1 {
            vec![run_one_pending_subagent_with_allowlist(
                queue,
                request,
                allowed_run_ids.as_deref(),
            )?]
        } else {
            run_subagent_worker_batch(queue, request, batch_size, allowed_run_ids.clone())?
        };
        let mut batch_ran = false;

        for output in outputs {
            if !output.ran {
                idle = true;
                continue;
            }
            batch_ran = true;
            if let Some(run_id) = output.run_id {
                run_ids.push(run_id);
            }
            if let Some(path) = output.report_path {
                report_paths.push(path);
            }
            if let Some(admission) = output.report_admission {
                report_admissions.push(admission);
            }
        }

        if !batch_ran || run_ids.len() >= max_runs {
            break;
        }
    }

    let (_, evolution_kind, evolution_source) = resolve_subagent_evolution_runtime(request);

    Ok(SubagentRunLoopCliOutput {
        runner: request.runner.clone(),
        evolution_kind: evolution_kind.to_string(),
        evolution_source: evolution_source.to_string(),
        worker_capabilities: request.worker_capabilities.clone(),
        max_runs,
        max_concurrency,
        ran_count: run_ids.len(),
        idle,
        run_ids,
        report_paths,
        report_admissions,
    })
}

fn run_subagent_worker_batch(
    queue: &FileSubagentQueue,
    request: &SubagentRunOnceCliRequest,
    batch_size: usize,
    allowed_run_ids: Option<Arc<BTreeSet<String>>>,
) -> Result<Vec<SubagentRunOnceCliOutput>, String> {
    let mut handles = Vec::with_capacity(batch_size);
    for _ in 0..batch_size {
        let worker_queue = queue.clone();
        let worker_request = request.clone();
        let worker_allowed_run_ids = allowed_run_ids.clone();
        handles.push(std::thread::spawn(move || {
            run_one_pending_subagent_with_allowlist(
                &worker_queue,
                &worker_request,
                worker_allowed_run_ids.as_deref(),
            )
        }));
    }

    let mut outputs = Vec::with_capacity(batch_size);
    for handle in handles {
        let output = handle
            .join()
            .map_err(|_| "subagent_worker_thread_panicked".to_string())??;
        outputs.push(output);
    }
    Ok(outputs)
}

fn open_subagent_queue(options: &CliOptions) -> Result<FileSubagentQueue, String> {
    let queue_config = options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    FileSubagentQueue::open(queue_config).map_err(|e| format!("subagent_queue_open_failed: {e:?}"))
}

fn run_one_pending_subagent(
    queue: &FileSubagentQueue,
    request: &SubagentRunOnceCliRequest,
) -> Result<SubagentRunOnceCliOutput, String> {
    run_one_pending_subagent_with_allowlist(queue, request, None)
}

fn run_one_pending_subagent_with_allowlist(
    queue: &FileSubagentQueue,
    request: &SubagentRunOnceCliRequest,
    allowed_run_ids: Option<&BTreeSet<String>>,
) -> Result<SubagentRunOnceCliOutput, String> {
    let (mut evolution, evolution_kind, evolution_source) = build_subagent_evolution_slot(request)?;
    let owner = unique_worker_owner()?;
    let dispatches = queue
        .list_dispatches()
        .map_err(|e| format!("subagent_dispatch_list_failed: {e:?}"))?;
    let report_run_ids = queue
        .list_report_run_ids()
        .map_err(|e| format!("subagent_report_list_failed: {e:?}"))?
        .into_iter()
        .map(|run_id| run_id.0)
        .collect::<std::collections::BTreeSet<_>>();
    let mut claimed_dispatch = None;
    for dispatch in dispatches {
        if let Some(allowed_run_ids) = allowed_run_ids {
            if !allowed_run_ids.contains(&dispatch.run_id.0) {
                continue;
            }
        }
        if report_run_ids.contains(&dispatch.run_id.0) {
            continue;
        }
        if !dispatch_matches_worker_capabilities(&dispatch, &request.worker_capabilities) {
            continue;
        }
        let claimed = queue
            .claim_dispatch_with_timeout(&dispatch.run_id, &owner, Some(dispatch.idle_timeout_ms))
            .map_err(|e| format!("subagent_claim_failed: {e:?}"))?;
        if claimed.is_some() {
            claimed_dispatch = Some(dispatch);
            break;
        }
    }
    let Some(dispatch) = claimed_dispatch else {
        let summary = if request.worker_capabilities.is_empty() {
            "no pending dispatch".to_string()
        } else {
            format!(
                "no pending dispatch matching capabilities: {}",
                request.worker_capabilities.join(",")
            )
        };
        return Ok(SubagentRunOnceCliOutput {
            runner: request.runner.clone(),
            worker_capabilities: request.worker_capabilities.clone(),
            ran: false,
            run_id: None,
            report_path: None,
            report_admission: None,
            summary,
            evolution_kind: evolution_kind.to_string(),
            evolution_source: evolution_source.to_string(),
        });
    };

    let report = match request.runner.as_str() {
        "fake" => build_fake_runner_report(&dispatch, &mut evolution)?,
        "command" => build_command_runner_report(&dispatch, request, &mut evolution)?,
        runner => return Err(format!("unsupported subagent runner: {runner}")),
    };
    let report_admission = build_report_admission(&report)?;
    let report_path = queue
        .write_report(&dispatch.run_id, &report)
        .map_err(|e| format!("subagent_report_write_failed: {e:?}"))?;
    Ok(SubagentRunOnceCliOutput {
        runner: request.runner.clone(),
        worker_capabilities: request.worker_capabilities.clone(),
        ran: true,
        run_id: Some(dispatch.run_id.0),
        report_path: Some(report_path.display().to_string()),
        report_admission: Some(report_admission),
        summary: report.summary,
        evolution_kind: evolution_kind.to_string(),
        evolution_source: evolution_source.to_string(),
    })
}

fn dispatch_matches_worker_capabilities(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    worker_capabilities: &[String],
) -> bool {
    let required = dispatch_required_capabilities(dispatch);
    if required.is_empty() {
        return true;
    }
    required.iter().all(|required| {
        worker_capabilities
            .iter()
            .any(|capability| capability == required)
    })
}

fn dispatch_required_capabilities(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
) -> Vec<String> {
    dispatch
        .metadata
        .get("required_capabilities")
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|capability| !capability.is_empty())
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn unique_worker_owner() -> Result<String, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    Ok(format!("cli-worker-{}-{nanos}", std::process::id()))
}

fn subagent_report_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_report(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let report_raw = queue
        .read_report_raw(&request.run_id)
        .map_err(|e| format!("subagent_report_read_failed: {e:?}"))?;
    let mut report_admission = report_raw
        .as_deref()
        .map(build_report_admission_raw)
        .transpose()?;
    let report = report_raw
        .as_deref()
        .and_then(|raw| serde_json::from_slice::<SubagentReport>(raw).ok());
    if let Some(report) = &report {
        if let Some(admission) = command_protocol_reject_admission(report)? {
            report_admission = Some(admission);
        }
    }
    let parent_context_handoff = report.as_ref().and_then(|report| {
        report_admission
            .as_ref()
            .map(|admission| build_parent_context_handoff(report, admission))
    });
    let output = SubagentReportCliOutput {
        run_id: request.run_id.0.clone(),
        available: report_raw.is_some(),
        report,
        report_admission,
        parent_context_handoff,
    };

    match request.output {
        ControlOutputFormat::Text => {
            if let Some(report) = &output.report {
                let handoff = output
                    .parent_context_handoff
                    .as_ref()
                    .map(parent_context_handoff_state)
                    .unwrap_or_else(|| "none".to_string());
                println!(
                    "subagent_report_available run_id={} status={:?} summary={} admission={} handoff={}",
                    output.run_id,
                    report.status,
                    report.summary,
                    output
                        .report_admission
                        .as_ref()
                        .map(|admission| format!("{:?}", admission.status))
                        .unwrap_or_else(|| "none".to_string()),
                    handoff
                );
            } else {
                println!(
                    "subagent_report_{} run_id={} admission={}",
                    if output.available {
                        "unavailable"
                    } else {
                        "missing"
                    },
                    output.run_id,
                    output
                        .report_admission
                        .as_ref()
                        .map(|admission| format!("{:?}", admission.status))
                        .unwrap_or_else(|| "none".to_string())
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_collect_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_collect(args)?;
    let queue_config = request
        .options
        .runtime
        .subagent_queue
        .build_file_queue_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    let queue = FileSubagentQueue::open(queue_config)
        .map_err(|e| format!("subagent_queue_open_failed: {e:?}"))?;
    let Some(dispatch) = queue
        .read_dispatch(&request.run_id)
        .map_err(|e| format!("subagent_dispatch_read_failed: {e:?}"))?
    else {
        let output = SubagentCollectCliOutput {
            run_id: request.run_id.0.clone(),
            dispatch_available: false,
            report_available: false,
            report: None,
            report_admission: None,
            parent_context_handoff: None,
        };
        return match request.output {
            ControlOutputFormat::Text => {
                println!("subagent_collect_missing_dispatch run_id={}", output.run_id);
                Ok(())
            }
            ControlOutputFormat::Json => print_json(&output),
        };
    };

    let report_raw = queue
        .read_report_raw(&request.run_id)
        .map_err(|e| format!("subagent_report_read_failed: {e:?}"))?;
    let mut report_admission = report_raw
        .as_deref()
        .map(build_report_admission_raw)
        .transpose()?;
    let report = report_raw
        .as_deref()
        .and_then(|raw| serde_json::from_slice::<SubagentReport>(raw).ok());
    if let Some(report) = &report {
        if let Some(admission) = command_protocol_reject_admission(report)? {
            report_admission = Some(admission);
        }
        if report.task_id != dispatch.task_id
            || report.agent_id != dispatch.agent_id
            || report.parent_agent_id.as_ref() != Some(&dispatch.parent_agent_id)
        {
            return Err(
                "subagent_collect_failed: InvalidRequest(\"report identity does not match queued run\")"
                    .to_string(),
            );
        }
    }
    let parent_context_handoff = report.as_ref().and_then(|report| {
        report_admission
            .as_ref()
            .map(|admission| build_parent_context_handoff(report, admission))
    });
    let output = SubagentCollectCliOutput {
        run_id: request.run_id.0.clone(),
        dispatch_available: true,
        report_available: report_raw.is_some(),
        report,
        report_admission,
        parent_context_handoff,
    };

    match request.output {
        ControlOutputFormat::Text => {
            if let Some(report) = &output.report {
                let handoff = output
                    .parent_context_handoff
                    .as_ref()
                    .map(parent_context_handoff_state)
                    .unwrap_or_else(|| "none".to_string());
                println!(
                    "subagent_collect_available run_id={} status={:?} summary={} admission={} handoff={}",
                    output.run_id,
                    report.status,
                    report.summary,
                    output
                        .report_admission
                        .as_ref()
                        .map(|admission| format!("{:?}", admission.status))
                        .unwrap_or_else(|| "none".to_string()),
                    handoff
                );
            } else {
                println!("subagent_collect_pending run_id={}", output.run_id);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_release_claim_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_release_claim(args)?;
    let queue = open_subagent_queue(&request.options)?;
    let owner = unique_worker_owner()?;
    let path = queue
        .release_claim(&request.run_id, &owner, &request.reason)
        .map_err(|e| format!("subagent_claim_release_failed: {e:?}"))?;
    let output = SubagentReleaseClaimCliOutput {
        run_id: request.run_id.0,
        released: true,
        release_path: path.display().to_string(),
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_claim_released run_id={} path={}",
                output.run_id, output.release_path
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn unique_cli_subagent_ids(agent_name: &str) -> Result<CliSubagentIds, String> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_nanos();
    let pid = std::process::id();
    let safe_agent_name = agent_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    let safe_agent_name = if safe_agent_name.is_empty() {
        "worker".to_string()
    } else {
        safe_agent_name
    };

    Ok(CliSubagentIds {
        run_id: RunId(format!("queued-cli-{pid}-{nanos}")),
        agent_id: AgentId(format!("{safe_agent_name}-{pid}-{nanos}")),
    })
}

fn parent_context_handoff_state(
    handoff: &chuang_agent::subagent_report::ParentContextHandoff,
) -> String {
    if handoff.memory_proposal_only {
        format!("proposal_only reason={}", handoff.admission_reason_code)
    } else {
        format!(
            "accepted report_id={} task_id={} agent_id={}",
            handoff
                .report_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or("none"),
            handoff
                .task_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or("none"),
            handoff
                .agent_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or("none")
        )
    }
}

fn build_fake_runner_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    evolution: &mut impl SkillEvolver,
) -> Result<SubagentReport, String> {
    let timestamp = current_rfc3339_timestamp()?;
    let skill_proposals = collect_dry_run_proposals(
        evolution,
        &dispatch.agent_id.0,
        &dispatch.task_id.0,
        &format!("fake_runner_turn_completed:{}", dispatch.run_id.0),
        &format!("fake runner completed task {}", dispatch.task_id.0),
    );
    Ok(SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", dispatch.run_id.0)),
        task_id: dispatch.task_id.clone(),
        agent_id: dispatch.agent_id.clone(),
        parent_agent_id: Some(dispatch.parent_agent_id.clone()),
        status: ExecutionStatus::Success,
        started_at: Timestamp(timestamp.clone()),
        finished_at: Timestamp(timestamp),
        summary: format!(
            "fake runner completed {} with {:?} policy",
            dispatch.task_id.0, dispatch.tool_policy
        ),
        exit_code: Some(0),
        stdout_preview: Some(format!("task={}", dispatch.task)),
        stderr_preview: None,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("queued-subagent://{}", dispatch.run_id.0)),
        context_debug: None,
        governance_decision: None,
        truncated: false,
        skill_proposals,
    })
}

fn build_command_runner_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    request: &SubagentRunOnceCliRequest,
    evolution: &mut impl SkillEvolver,
) -> Result<SubagentReport, String> {
    let command = request
        .runner_command
        .as_deref()
        .ok_or_else(|| "command_runner_requires_runner_command".to_string())?;
    let dispatch_json = serde_json::to_string_pretty(dispatch)
        .map_err(|e| format!("command_runner_dispatch_encode_failed: {e}"))?;
    let started_at = current_rfc3339_timestamp()?;
    let mut child = Command::new(command)
        .args(&request.runner_args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("command_runner_spawn_failed: {e}"))?;

    let stdout_reader = child
        .stdout
        .take()
        .map(|stdout| spawn_limited_pipe_reader(stdout, COMMAND_RUNNER_OUTPUT_CAPTURE_LIMIT_BYTES));
    let stderr_reader = child
        .stderr
        .take()
        .map(|stderr| spawn_limited_pipe_reader(stderr, COMMAND_RUNNER_OUTPUT_CAPTURE_LIMIT_BYTES));

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| "command_runner_stdin_unavailable".to_string())?;
    write_dispatch_to_runner_stdin(&mut stdin, &dispatch_json)?;
    drop(stdin);

    let output = wait_command_runner_with_timeout(
        child,
        dispatch.idle_timeout_ms,
        stdout_reader,
        stderr_reader,
    )
    .map_err(|e| format!("command_runner_wait_failed: {e}"))?;
    let finished_at = current_rfc3339_timestamp()?;
    let status = if output.timed_out {
        ExecutionStatus::Failed
    } else if output.status_success {
        ExecutionStatus::Success
    } else {
        ExecutionStatus::Failed
    };
    let exit_code = output.exit_code;
    let stdout = output.stdout;
    let stderr = if output.timed_out {
        let timeout = format!(
            "command runner timed out after {}ms",
            dispatch.idle_timeout_ms
        );
        if output.stderr.trim().is_empty() {
            timeout
        } else {
            format!("{timeout}\n{}", output.stderr)
        }
    } else {
        output.stderr
    };

    if let Some(protocol_report) = try_build_protocol_report(
        dispatch,
        &stdout,
        &stderr,
        output.timed_out,
        output.exit_code,
        &started_at,
        &finished_at,
    ) {
        return protocol_report;
    }

    let skill_proposals = collect_dry_run_proposals(
        evolution,
        &dispatch.agent_id.0,
        &dispatch.task_id.0,
        &format!("command_runner_turn_completed:{}", dispatch.run_id.0),
        &format!(
            "command runner completed task {} stdout_bytes={}",
            dispatch.task_id.0,
            stdout.len()
        ),
    );
    Ok(SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", dispatch.run_id.0)),
        task_id: dispatch.task_id.clone(),
        agent_id: dispatch.agent_id.clone(),
        parent_agent_id: Some(dispatch.parent_agent_id.clone()),
        status,
        started_at: Timestamp(started_at),
        finished_at: Timestamp(finished_at),
        summary: format!(
            "command runner completed {} exit_code={}{}",
            dispatch.task_id.0,
            exit_code
                .map(|code| code.to_string())
                .unwrap_or_else(|| "unknown".to_string()),
            if output.timed_out {
                " timed_out=true"
            } else {
                ""
            }
        ),
        exit_code,
        stdout_preview: non_empty_preview(&stdout),
        stderr_preview: non_empty_preview(&stderr),
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("queued-subagent-command://{}", dispatch.run_id.0)),
        context_debug: None,
        governance_decision: Some(command_runner_governance_decision(dispatch)),
        truncated: output.stdout_truncated
            || output.stderr_truncated
            || stdout.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS
            || stderr.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS,
        skill_proposals,
    })
}

fn command_runner_governance_decision(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
) -> GovernanceDecisionSummary {
    GovernanceDecisionSummary {
        action_id: format!("subagent-command-runner:{}", dispatch.run_id.0),
        decision: "needs_approval".to_string(),
        reason: "approved_by_cli_flag: --approve-exec".to_string(),
    }
}

fn build_report_admission(
    report: &SubagentReport,
) -> Result<chuang_agent::subagent_report::ReportAdmission, String> {
    if let Some(admission) = command_protocol_reject_admission(report)? {
        return Ok(admission);
    }

    let validator = SubagentReportValidator::default();
    let raw =
        serde_json::to_vec(report).map_err(|e| format!("report_admission_encode_failed: {e}"))?;
    Ok(validator.admit_raw(
        &raw,
        AgentId("cli-subagent-controller".to_string()),
        Timestamp(current_rfc3339_timestamp()?),
    ))
}

fn build_report_admission_raw(
    raw: &[u8],
) -> Result<chuang_agent::subagent_report::ReportAdmission, String> {
    let validator = SubagentReportValidator::default();
    Ok(validator.admit_raw(
        raw,
        AgentId("cli-subagent-controller".to_string()),
        Timestamp(current_rfc3339_timestamp()?),
    ))
}

fn command_protocol_reject_admission(
    report: &SubagentReport,
) -> Result<Option<chuang_agent::subagent_report::ReportAdmission>, String> {
    if !is_command_protocol_reject_report(report) {
        return Ok(None);
    }

    Ok(Some(chuang_agent::subagent_report::ReportAdmission {
        schema_version: "1.0.0".to_string(),
        report_id: Some(report.report_id.clone()),
        task_id: Some(report.task_id.clone()),
        agent_id: Some(report.agent_id.clone()),
        controller_agent_id: report
            .parent_agent_id
            .clone()
            .unwrap_or_else(|| AgentId("cli-subagent-controller".to_string())),
        status: ReportAdmissionStatus::Rejected,
        reason_code: "command_protocol_report_rejected".to_string(),
        upstream_reason_code: command_protocol_upstream_reason_code(report),
        reason: report
            .stderr_preview
            .clone()
            .unwrap_or_else(|| report.summary.clone()),
        decided_at: Timestamp(current_rfc3339_timestamp()?),
    }))
}

fn is_command_protocol_reject_report(report: &SubagentReport) -> bool {
    matches!(report.status, ExecutionStatus::Failed)
        && report
            .replay_ref
            .as_deref()
            .map(|value| value.starts_with("queued-subagent-command://"))
            .unwrap_or(false)
        && report.summary.contains("command runner protocol rejected")
}

fn write_dispatch_to_runner_stdin(
    stdin: &mut impl Write,
    dispatch_json: &str,
) -> Result<(), String> {
    match stdin.write_all(dispatch_json.as_bytes()) {
        Ok(()) => {}
        Err(error) if error.kind() == ErrorKind::BrokenPipe => return Ok(()),
        Err(error) => return Err(format!("command_runner_stdin_write_failed: {error}")),
    }

    match stdin.flush() {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(format!("command_runner_stdin_write_failed: {error}")),
    }
}

fn try_build_protocol_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    stdout: &str,
    stderr: &str,
    timed_out: bool,
    exit_code: Option<i32>,
    started_at: &str,
    finished_at: &str,
) -> Option<Result<SubagentReport, String>> {
    if timed_out {
        return None;
    }

    let trimmed = stdout.trim();
    if !looks_like_protocol_report(trimmed) {
        return None;
    }

    Some(
        {
            let validator = SubagentReportValidator::default();
            let admission = validator.admit_raw(
                trimmed.as_bytes(),
                dispatch.parent_agent_id.clone(),
                Timestamp(finished_at.to_string()),
            );
            if admission.status == ReportAdmissionStatus::Rejected {
                Err(format!(
                    "report_admission_rejected:{}:{}",
                    admission.reason_code, admission.reason
                ))
            } else {
                Ok(())
            }
        }
        .and_then(|_| {
            serde_json::from_str::<SubagentReport>(trimmed)
                .map_err(|e| format!("command_runner_report_decode_failed: {e}"))
        })
        .and_then(|mut report| {
            validate_protocol_report_identity(dispatch, &report)?;
            apply_protocol_report_bounds(&mut report);
            if report.governance_decision.is_none() {
                report.governance_decision = Some(command_runner_governance_decision(dispatch));
            }
            Ok(report)
        })
        .or_else(|error| {
            Ok(build_command_runner_protocol_reject_report(
                dispatch,
                started_at,
                finished_at,
                exit_code,
                &error,
                stderr,
            ))
        }),
    )
}

fn looks_like_protocol_report(trimmed_stdout: &str) -> bool {
    trimmed_stdout.starts_with('{')
        && trimmed_stdout.contains("\"schema_version\"")
        && (trimmed_stdout.contains("\"report_id\"")
            || trimmed_stdout.contains("\"task_id\"")
            || trimmed_stdout.contains("\"agent_id\""))
}

fn validate_protocol_report_identity(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    report: &SubagentReport,
) -> Result<(), String> {
    if !report.schema_version.starts_with("1.") {
        return Err(format!(
            "unsupported_schema_version:{}",
            report.schema_version
        ));
    }
    if report.task_id != dispatch.task_id {
        return Err(format!(
            "task_id_mismatch expected={} found={}",
            dispatch.task_id.0, report.task_id.0
        ));
    }
    if report.agent_id != dispatch.agent_id {
        return Err(format!(
            "agent_id_mismatch expected={} found={}",
            dispatch.agent_id.0, report.agent_id.0
        ));
    }
    if report.parent_agent_id.as_ref() != Some(&dispatch.parent_agent_id) {
        return Err(format!(
            "parent_agent_id_mismatch expected={} found={}",
            dispatch.parent_agent_id.0,
            report
                .parent_agent_id
                .as_ref()
                .map(|id| id.0.as_str())
                .unwrap_or("none")
        ));
    }
    if report.summary.trim().is_empty() {
        return Err("summary_must_not_be_empty".to_string());
    }
    Ok(())
}

fn apply_protocol_report_bounds(report: &mut SubagentReport) {
    let stdout_too_large = report
        .stdout_preview
        .as_ref()
        .map(|value| value.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS)
        .unwrap_or(false);
    if stdout_too_large {
        report.stdout_preview = report
            .stdout_preview
            .as_ref()
            .map(|value| value.chars().take(COMMAND_RUNNER_PREVIEW_CHARS).collect());
        report.truncated = true;
    }

    let stderr_too_large = report
        .stderr_preview
        .as_ref()
        .map(|value| value.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS)
        .unwrap_or(false);
    if stderr_too_large {
        report.stderr_preview = report
            .stderr_preview
            .as_ref()
            .map(|value| value.chars().take(COMMAND_RUNNER_PREVIEW_CHARS).collect());
        report.truncated = true;
    }
}

fn build_command_runner_protocol_reject_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    started_at: &str,
    finished_at: &str,
    exit_code: Option<i32>,
    error: &str,
    stderr: &str,
) -> SubagentReport {
    let stderr_preview = if stderr.trim().is_empty() {
        error.to_string()
    } else {
        format!("{error}\n{}", stderr.trim())
    };

    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{}", dispatch.run_id.0)),
        task_id: dispatch.task_id.clone(),
        agent_id: dispatch.agent_id.clone(),
        parent_agent_id: Some(dispatch.parent_agent_id.clone()),
        status: ExecutionStatus::Failed,
        started_at: Timestamp(started_at.to_string()),
        finished_at: Timestamp(finished_at.to_string()),
        summary: format!(
            "command runner protocol rejected {} reason={}",
            dispatch.task_id.0, error
        ),
        exit_code,
        stdout_preview: None,
        stderr_preview: non_empty_preview(&stderr_preview),
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("queued-subagent-command://{}", dispatch.run_id.0)),
        context_debug: None,
        governance_decision: Some(command_runner_governance_decision(dispatch)),
        truncated: stderr_preview.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS,
        skill_proposals: vec![],
    }
}

fn command_protocol_upstream_reason_code(report: &SubagentReport) -> Option<String> {
    let raw = report
        .stderr_preview
        .as_deref()
        .unwrap_or(report.summary.as_str());
    extract_command_protocol_upstream_reason_code(raw)
}

fn extract_command_protocol_upstream_reason_code(raw: &str) -> Option<String> {
    let normalized = raw.to_ascii_lowercase();
    if let Some(rest) = normalized.strip_prefix("report_admission_rejected:") {
        return rest
            .split(':')
            .next()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string);
    }
    for code in [
        "agent_id_mismatch",
        "task_id_mismatch",
        "parent_agent_id_mismatch",
        "invalid_timestamp_order",
        "missing_required_field",
        "empty_required_field",
        "invalid_json",
        "invalid_utf8",
        "unsupported_schema_version",
        "invalid_enum_format",
        "invalid_timestamp_format",
        "size_limit_exceeded",
    ] {
        if normalized.contains(code) {
            return Some(code.to_string());
        }
    }
    None
}

struct CommandRunnerOutput {
    status_success: bool,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    timed_out: bool,
    stdout_truncated: bool,
    stderr_truncated: bool,
}

struct PipeReadResult {
    text: String,
    truncated: bool,
}

fn wait_command_runner_with_timeout(
    mut child: std::process::Child,
    timeout_ms: u64,
    stdout_reader: Option<JoinHandle<std::io::Result<PipeReadResult>>>,
    stderr_reader: Option<JoinHandle<std::io::Result<PipeReadResult>>>,
) -> std::io::Result<CommandRunnerOutput> {
    let deadline = Instant::now() + Duration::from_millis(timeout_ms.max(1));
    loop {
        if let Some(status) = child.try_wait()? {
            return command_runner_output(status, false, stdout_reader, stderr_reader);
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let status = child.wait()?;
            return command_runner_output(status, true, stdout_reader, stderr_reader);
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn command_runner_output(
    status: std::process::ExitStatus,
    timed_out: bool,
    stdout_reader: Option<JoinHandle<std::io::Result<PipeReadResult>>>,
    stderr_reader: Option<JoinHandle<std::io::Result<PipeReadResult>>>,
) -> std::io::Result<CommandRunnerOutput> {
    let stdout = join_pipe_reader(stdout_reader)?;
    let stderr = join_pipe_reader(stderr_reader)?;

    Ok(CommandRunnerOutput {
        status_success: status.success(),
        exit_code: status.code(),
        stdout: stdout.text,
        stderr: stderr.text,
        timed_out,
        stdout_truncated: stdout.truncated,
        stderr_truncated: stderr.truncated,
    })
}

fn spawn_limited_pipe_reader<R>(
    mut reader: R,
    capture_limit_bytes: usize,
) -> JoinHandle<std::io::Result<PipeReadResult>>
where
    R: Read + Send + 'static,
{
    std::thread::spawn(move || {
        let mut captured = Vec::new();
        let mut truncated = false;
        let mut buffer = [0u8; 8192];

        loop {
            let bytes = reader.read(&mut buffer)?;
            if bytes == 0 {
                break;
            }
            let remaining = capture_limit_bytes.saturating_sub(captured.len());
            if remaining > 0 {
                let take = remaining.min(bytes);
                captured.extend_from_slice(&buffer[..take]);
                if take < bytes {
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }

        Ok(PipeReadResult {
            text: String::from_utf8_lossy(&captured).to_string(),
            truncated,
        })
    })
}

fn join_pipe_reader(
    reader: Option<JoinHandle<std::io::Result<PipeReadResult>>>,
) -> std::io::Result<PipeReadResult> {
    let Some(reader) = reader else {
        return Ok(PipeReadResult {
            text: String::new(),
            truncated: false,
        });
    };

    reader.join().map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::Other,
            "command runner pipe reader panicked",
        )
    })?
}

fn non_empty_preview(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    Some(trimmed.chars().take(COMMAND_RUNNER_PREVIEW_CHARS).collect())
}

fn current_rfc3339_timestamp() -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| format!("clock_error: {e}"))?
        .as_secs();
    Ok(format_unix_rfc3339(now))
}

fn format_unix_rfc3339(seconds: u64) -> String {
    let datetime = chrono::DateTime::<chrono::Utc>::from_timestamp(seconds as i64, 0)
        .unwrap_or_else(|| {
            chrono::DateTime::<chrono::Utc>::from_timestamp(0, 0)
                .expect("unix epoch timestamp should be valid")
        });
    datetime.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    use chuang_agent::common::TaskId;
    use chuang_agent::runtime_config::RuntimeConfig;
    use chuang_agent::subagent_queue::FileSubagentQueueConfig;
    use chuang_agent::subagent_spawner::{ContextIsolation, SubagentDispatch, SubagentToolPolicy};

    #[test]
    fn run_subagent_run_loop_respects_allowed_run_ids() {
        let queue_root = temp_queue_root("run-loop-allowlist");
        let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&queue_root))
            .expect("queue should open");
        queue
            .write_dispatch(&sample_dispatch("queued-run-1", "task-1"))
            .expect("first dispatch should write");
        queue
            .write_dispatch(&sample_dispatch("queued-run-2", "task-2"))
            .expect("second dispatch should write");

        let request = sample_run_once_request(&queue_root);
        let allowed_run_ids = BTreeSet::from(["queued-run-2".to_string()]);

        let output = run_subagent_run_loop(&queue, &request, 2, 2, Some(&allowed_run_ids))
            .expect("run loop should execute");

        assert_eq!(output.ran_count, 1);
        assert_eq!(output.run_ids, vec!["queued-run-2".to_string()]);
        assert!(output.idle);
        assert!(!queue_root
            .join("reports")
            .join("queued-run-1.json")
            .exists());
        assert!(queue_root
            .join("reports")
            .join("queued-run-2.json")
            .exists());
    }

    fn temp_queue_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock should be valid")
            .as_nanos();
        std::env::temp_dir().join(format!("chuang-agent-cli-subagent-{name}-{nanos}"))
    }

    fn sample_run_once_request(queue_root: &std::path::Path) -> SubagentRunOnceCliRequest {
        let mut runtime = RuntimeConfig::new(queue_root.join("memory.db"));
        runtime.subagent_queue.root = queue_root.to_path_buf();
        SubagentRunOnceCliRequest {
            options: CliOptions { runtime },
            output: ControlOutputFormat::Json,
            runner: "fake".to_string(),
            runner_command: None,
            runner_args: Vec::new(),
            worker_capabilities: Vec::new(),
            approve_exec: false,
        }
    }

    fn sample_dispatch(run_id: &str, task_id: &str) -> SubagentDispatch {
        SubagentDispatch {
            run_id: RunId(run_id.to_string()),
            agent_id: AgentId(format!("worker-{run_id}")),
            task_id: TaskId(task_id.to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: format!("queued task {task_id}"),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 512,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: BTreeMap::new(),
        }
    }
}

fn resolve_subagent_evolution_runtime(
    request: &SubagentRunOnceCliRequest,
) -> (chuang_agent::runtime_config::RuntimeConfig, &'static str, &'static str) {
    let mut runtime = request.options.runtime.clone();
    let source = if matches!(runtime.evolution, EvolutionConfig::Noop) {
        runtime.evolution = EvolutionConfig::DryRun;
        SubagentEvolutionSource::DefaultDryRunPromotion
    } else {
        SubagentEvolutionSource::RuntimeConfig
    };
    let evolution_kind = runtime.evolution.kind();
    (runtime, evolution_kind, source.as_str())
}

fn build_subagent_evolution_slot(
    request: &SubagentRunOnceCliRequest,
) -> Result<(impl SkillEvolver, &'static str, &'static str), String> {
    let (runtime, evolution_kind, evolution_source) =
        resolve_subagent_evolution_runtime(request);
    let slots = build_runtime_slots(&runtime)
        .map_err(|e| format!("subagent_evolution_slot_invalid: {}: {}", e.field, e.message))?;
    Ok((slots.evolution, evolution_kind, evolution_source))
}

/// Observe a `TurnCompleted` event through the configured evolution slot and
/// return at most one dry-run skill candidate. The slot remains readonly here;
/// no skill is written or solidified by `subagent run-once`.
fn collect_dry_run_proposals(
    evolution: &mut impl SkillEvolver,
    agent_id: &str,
    task_id: &str,
    event_id: &str,
    summary: &str,
) -> Vec<chuang_agent::skill_evolver::SkillProposal> {
    let event = RuntimeEvent {
        event_id: event_id.to_string(),
        task_id: task_id.to_string(),
        kind: RuntimeEventKind::TurnCompleted,
        summary: summary.to_string(),
        metadata: std::collections::BTreeMap::new(),
    };
    if evolution.observe(event).is_err() {
        return vec![];
    }
    let scope = EvolutionScope {
        agent_id: agent_id.to_string(),
        task_kind: None,
        max_proposals: 1,
    };
    evolution.propose(scope).unwrap_or_default()
}
