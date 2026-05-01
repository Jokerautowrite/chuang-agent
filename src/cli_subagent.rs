use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::common::{AgentId, ReportId, Timestamp};
use chuang_agent::slot_registry::SubagentRuntimeSlot;
use chuang_agent::subagent_queue::FileSubagentQueue;
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{QueuedSubagentSpawner, RunId, SubagentSpawner};

use crate::cli_args::{
    parse_subagent_collect, parse_subagent_dispatch, parse_subagent_list, parse_subagent_report,
    parse_subagent_run_once,
};
use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_types::*;

pub(crate) fn subagent_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("dispatch") => subagent_dispatch_command(&args[1..]),
        Some("report") => subagent_report_command(&args[1..]),
        Some("collect") => subagent_collect_command(&args[1..]),
        Some("list") => subagent_list_command(&args[1..]),
        Some("run-once") => subagent_run_once_command(&args[1..]),
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
        .map(|dispatch| SubagentListItem {
            run_id: dispatch.run_id.0.clone(),
            agent_id: dispatch.agent_id.0,
            task_id: dispatch.task_id.0,
            agent_name: dispatch.agent_name,
            tool_policy: format!("{:?}", dispatch.tool_policy),
            has_report: report_lookup.contains(&dispatch.run_id.0),
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
                    "run_id={} agent_id={} task_id={} policy={} has_report={}",
                    item.run_id, item.agent_id, item.task_id, item.tool_policy, item.has_report
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn subagent_run_once_command(args: &[String]) -> Result<(), String> {
    let request = parse_subagent_run_once(args)?;
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
        .map_err(|e| format!("subagent_report_list_failed: {e:?}"))?
        .into_iter()
        .map(|run_id| run_id.0)
        .collect::<std::collections::BTreeSet<_>>();
    let Some(dispatch) = dispatches
        .into_iter()
        .find(|dispatch| !report_run_ids.contains(&dispatch.run_id.0))
    else {
        let output = SubagentRunOnceCliOutput {
            runner: request.runner,
            ran: false,
            run_id: None,
            report_path: None,
            summary: "no pending dispatch".to_string(),
        };
        return match request.output {
            ControlOutputFormat::Text => {
                println!("subagent_run_once idle runner={}", output.runner);
                Ok(())
            }
            ControlOutputFormat::Json => print_json(&output),
        };
    };

    let report = build_fake_runner_report(&dispatch)?;
    let report_path = queue
        .write_report(&dispatch.run_id, &report)
        .map_err(|e| format!("subagent_report_write_failed: {e:?}"))?;
    let output = SubagentRunOnceCliOutput {
        runner: request.runner,
        ran: true,
        run_id: Some(dispatch.run_id.0),
        report_path: Some(report_path.display().to_string()),
        summary: report.summary,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "subagent_run_once runner={} run_id={} report_path={}",
                output.runner,
                output.run_id.as_deref().unwrap_or("none"),
                output.report_path.as_deref().unwrap_or("none")
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
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
