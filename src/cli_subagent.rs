use std::io::{ErrorKind, Read, Write};
use std::process::{Command, Stdio};
use std::thread::JoinHandle;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, Timestamp};
use chuang_agent::slot_registry::SubagentRuntimeSlot;
use chuang_agent::subagent_queue::FileSubagentQueue;
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{QueuedSubagentSpawner, RunId, SubagentSpawner};

use crate::cli_args::{
    parse_subagent_collect, parse_subagent_dispatch, parse_subagent_list,
    parse_subagent_release_claim, parse_subagent_report, parse_subagent_run_loop,
    parse_subagent_run_once,
};
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_types::*;

const COMMAND_RUNNER_OUTPUT_CAPTURE_LIMIT_BYTES: usize = 64 * 1024;
const COMMAND_RUNNER_PREVIEW_CHARS: usize = 1200;

pub(crate) fn subagent_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("dispatch") => subagent_dispatch_command(&args[1..]),
        Some("report") => subagent_report_command(&args[1..]),
        Some("collect") => subagent_collect_command(&args[1..]),
        Some("release-claim") => subagent_release_claim_command(&args[1..]),
        Some("list") => subagent_list_command(&args[1..]),
        Some("run-once") => subagent_run_once_command(&args[1..]),
        Some("run-loop") => subagent_run_loop_command(&args[1..]),
        _ => Err(usage()),
    }
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
                    "subagent_run_once runner={} capabilities={} run_id={} report_path={}",
                    output.runner,
                    output.worker_capabilities.join(","),
                    output.run_id.as_deref().unwrap_or("none"),
                    output.report_path.as_deref().unwrap_or("none")
                );
            } else {
                println!(
                    "subagent_run_once idle runner={} capabilities={}",
                    output.runner,
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
    let mut run_ids = Vec::new();
    let mut report_paths = Vec::new();
    let mut idle = false;

    for _ in 0..request.max_runs {
        let output = run_one_pending_subagent(&queue, &run_once_request)?;
        if !output.ran {
            idle = true;
            break;
        }
        if let Some(run_id) = output.run_id {
            run_ids.push(run_id);
        }
        if let Some(path) = output.report_path {
            report_paths.push(path);
        }
    }

    let output = SubagentRunLoopCliOutput {
        runner: request.runner,
        worker_capabilities: request.worker_capabilities,
        max_runs: request.max_runs,
        max_concurrency: request.max_concurrency,
        ran_count: run_ids.len(),
        idle,
        run_ids,
        report_paths,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_run_loop runner={} capabilities={} ran_count={} max_runs={} max_concurrency={} idle={}",
                output.runner,
                output.worker_capabilities.join(","),
                output.ran_count,
                output.max_runs,
                output.max_concurrency,
                output.idle
            );
            for run_id in &output.run_ids {
                println!("subagent_run_loop_ran run_id={run_id}");
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
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
            summary,
        });
    };

    let report = match request.runner.as_str() {
        "fake" => build_fake_runner_report(&dispatch)?,
        "command" => build_command_runner_report(&dispatch, request)?,
        runner => return Err(format!("unsupported subagent runner: {runner}")),
    };
    let report_path = queue
        .write_report(&dispatch.run_id, &report)
        .map_err(|e| format!("subagent_report_write_failed: {e:?}"))?;
    Ok(SubagentRunOnceCliOutput {
        runner: request.runner.clone(),
        worker_capabilities: request.worker_capabilities.clone(),
        ran: true,
        run_id: Some(dispatch.run_id.0),
        report_path: Some(report_path.display().to_string()),
        summary: report.summary,
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
    let report = queue
        .read_report(&request.run_id)
        .map_err(|e| format!("subagent_report_read_failed: {e:?}"))?;
    let output = SubagentReportCliOutput {
        run_id: request.run_id.0.clone(),
        available: report.is_some(),
        report,
    };

    match request.output {
        ControlOutputFormat::Text => {
            if let Some(report) = &output.report {
                println!(
                    "subagent_report_available run_id={} status={:?} summary={}",
                    output.run_id, report.status, report.summary
                );
            } else {
                println!("subagent_report_missing run_id={}", output.run_id);
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
        };
        return match request.output {
            ControlOutputFormat::Text => {
                println!("subagent_collect_missing_dispatch run_id={}", output.run_id);
                Ok(())
            }
            ControlOutputFormat::Json => print_json(&output),
        };
    };

    let mut spawner = QueuedSubagentSpawner::new();
    spawner
        .restore_dispatch(dispatch)
        .map_err(|e| format!("subagent_dispatch_restore_failed: {e:?}"))?;
    let mut slot = SubagentRuntimeSlot::QueuedExternal { spawner, queue };
    let report = slot
        .collect(&request.run_id)
        .map_err(|e| format!("subagent_collect_failed: {e:?}"))?;
    let output = SubagentCollectCliOutput {
        run_id: request.run_id.0.clone(),
        dispatch_available: true,
        report_available: report.is_some(),
        report,
    };

    match request.output {
        ControlOutputFormat::Text => {
            if let Some(report) = &output.report {
                println!(
                    "subagent_collect_available run_id={} status={:?} summary={}",
                    output.run_id, report.status, report.summary
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

fn build_fake_runner_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
) -> Result<SubagentReport, String> {
    let timestamp = current_rfc3339_timestamp()?;
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
    })
}

fn build_command_runner_report(
    dispatch: &chuang_agent::subagent_spawner::SubagentDispatch,
    request: &SubagentRunOnceCliRequest,
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
        governance_decision: None,
        truncated: output.stdout_truncated
            || output.stderr_truncated
            || stdout.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS
            || stderr.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS,
    })
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
    if !(trimmed.starts_with('{')
        && trimmed.contains("\"schema_version\"")
        && trimmed.contains("\"task_id\"")
        && trimmed.contains("\"status\""))
    {
        return None;
    }

    Some(
        serde_json::from_str::<SubagentReport>(trimmed)
            .map_err(|e| format!("command_runner_report_decode_failed: {e}"))
            .and_then(|mut report| {
                validate_protocol_report_identity(dispatch, &report)?;
                apply_protocol_report_bounds(&mut report);
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
        governance_decision: None,
        truncated: stderr_preview.chars().count() > COMMAND_RUNNER_PREVIEW_CHARS,
    }
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
