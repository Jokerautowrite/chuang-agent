use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::benchmark::BenchmarkStore;
use chuang_agent::goal_dispatch::{
    collect_goal_dispatch_reports, collect_goal_dispatch_reports_read_only, dispatch_goal_run,
    load_goal_dispatch_manifest, GoalCheckpointSuggestion, GoalDispatchCollectionReceipt,
    GoalDispatchDiagnostics, GoalDispatchManifest,
};
use chuang_agent::goal_mode::{AcceptanceCheck, GoalAcceptancePlan, GoalEvidence, GoalSpec};
use chuang_agent::goal_run::{
    check_evidence_plan, evaluate_acceptance_plan, ConvergenceStatus, ConvergenceVerdict,
    GoalCheckpoint, GoalCheckpointWriteback, GoalIntegrationPolicy, GoalRun, GoalRunDiagnostics,
    GoalRunReceipt, GoalRunStore, GoalValidationPlan, GoalWorkerPlan, GoalWriteScope,
};
use chuang_agent::runtime_config::RuntimeConfig;
use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver,
    SkillProposal, ValidationReport,
};
use chuang_agent::subagent_queue::{FileSubagentQueue, FileSubagentQueueConfig};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};
use crate::cli_skill::{
    build_skill_judgments, default_skills_root, enforce_benchmark_gate, solidify_skill_file,
    SkillJudgmentOutput, SkillReviewBuild, SkillWriteReceiptOutput,
};
use crate::cli_subagent::run_subagent_run_loop;
use crate::cli_types::{CliOptions, SubagentRunLoopCliOutput, SubagentRunOnceCliRequest};

pub(crate) fn goal_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("plan") => goal_plan_command(&args[1..]),
        Some("show") => goal_show_command(&args[1..]),
        Some("checkpoint") => goal_checkpoint_command(&args[1..]),
        Some("dispatch") => goal_dispatch_command(&args[1..]),
        Some("collect") => goal_collect_command(&args[1..]),
        Some("step") => goal_step_command(&args[1..]),
        Some("evolve") => goal_evolve_command(&args[1..]),
        Some("verify") => goal_verify_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn goal_plan_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_plan(args)?;
    let store = GoalRunStore::new(&request.root);
    let mut goal_spec = GoalSpec::mainline_mvp(request.objective);
    goal_spec.goal_id = request.goal_id;
    goal_spec.budget.max_subtasks = request.max_subtasks;
    goal_spec.acceptance_evidence = request.evidence;
    goal_spec.acceptance_plan = GoalAcceptancePlan::new(request.acceptance_plan);
    let run = GoalRun::new(
        goal_spec,
        request.worker_plan,
        request.write_scopes,
        GoalValidationPlan::new(request.validation_commands),
        GoalIntegrationPolicy::main_process_owned(),
    )
    .map_err(format_goal_run_error)?;
    let receipt = store.create(&run).map_err(format_goal_run_error)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_planned: {}", receipt.goal_id);
            println!("goal_path: {}", receipt.path);
            println!("goal_checkpoint_count: {}", receipt.checkpoint_count);
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

fn goal_show_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_show(args)?;
    let store = GoalRunStore::new(&request.root);
    let run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;
    let diagnostics = run.diagnostics();
    let operability = build_goal_operability_status(
        &run,
        &diagnostics,
        &request.root,
        &request.queue_root,
        &request.goal_id,
    );

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_id: {}", run.goal_spec.goal_id);
            println!("goal_objective: {}", run.goal_spec.objective);
            println!("goal_workers: {}", run.worker_plan.len());
            println!("goal_write_scopes: {}", run.disjoint_write_scopes.len());
            println!(
                "goal_validation_commands: {}",
                run.validation_plan.commands.len()
            );
            println!("goal_checkpoints: {}", run.checkpoint_log.len());
            let diagnostics = run.diagnostics();
            println!(
                "goal_worker_scope_complete: {}",
                diagnostics.worker_scope_complete
            );
            println!(
                "goal_worker_validation_complete: {}",
                diagnostics.worker_validation_complete
            );
            println!(
                "goal_validation_plan_complete: {}",
                diagnostics.validation_plan_complete
            );
            println!(
                "goal_checkpoint_log_complete: {}",
                diagnostics.checkpoint_log_complete
            );
            println!(
                "goal_last_checkpoint: {}",
                diagnostics.last_checkpoint_id.as_deref().unwrap_or("none")
            );
            println!(
                "goal_last_summary: {}",
                diagnostics
                    .last_checkpoint_summary
                    .as_deref()
                    .unwrap_or("none")
            );
            println!(
                "goal_last_checkpoint_created_at: {}",
                diagnostics
                    .last_checkpoint_created_at
                    .as_deref()
                    .unwrap_or("none")
            );
            println!(
                "goal_last_checkpoint_completed_worker_ids: {}",
                format_text_list(
                    diagnostics
                        .last_checkpoint_completed_worker_ids
                        .as_deref()
                        .unwrap_or(&[])
                )
            );
            println!(
                "goal_last_checkpoint_validation_notes: {}",
                format_text_list(
                    diagnostics
                        .last_checkpoint_validation_notes
                        .as_deref()
                        .unwrap_or(&[])
                )
            );
            print_goal_checkpoint_writeback("goal", &diagnostics.checkpoint_writeback);
            println!(
                "goal_incomplete_reasons: {}",
                format_text_list(&diagnostics.incomplete_reasons)
            );
            println!(
                "goal_executes_automatically: {}",
                diagnostics.executes_automatically
            );
            println!(
                "goal_bypasses_governance: {}",
                diagnostics.bypasses_governance
            );
            println!(
                "goal_convergence_status: {}",
                diagnostics.convergence_status
            );
            println!(
                "goal_convergence_repeated_fingerprint: {}",
                diagnostics
                    .convergence_repeated_fingerprint
                    .as_deref()
                    .unwrap_or("none")
            );
            println!(
                "goal_convergence_repeated_count: {}",
                diagnostics.convergence_repeated_count
            );
            println!(
                "goal_convergence_reason: {}",
                diagnostics.convergence_reason
            );
            println!("goal_evidence_expected: {}", diagnostics.evidence_expected);
            println!("goal_evidence_complete: {}", diagnostics.evidence_complete);
            println!(
                "goal_evidence_checked_at_checkpoint: {}",
                diagnostics
                    .evidence_checked_at_checkpoint
                    .as_deref()
                    .unwrap_or("none")
            );
            println!(
                "goal_evidence_missing: {}",
                format_text_list(&diagnostics.evidence_missing)
            );
            println!(
                "goal_acceptance_plan: {}",
                run.goal_spec.acceptance_plan.len()
            );
            for check in &run.goal_spec.acceptance_plan.checks {
                println!(
                    "goal_acceptance_check: [{}] {}",
                    check.evaluator(),
                    check.description()
                );
            }
            print_goal_operability_text(&operability);
        }
        ControlOutputFormat::Json => print_json(&GoalShowOutput {
            run: &run,
            goal_run_diagnostics: diagnostics,
            goal_operability: operability,
        })?,
    }
    Ok(())
}

/// 外环触发：goal 收敛判定为 blocked/spinning 时，把重复卡点转成
/// skill 进化提案（默认 dry-run，需显式 --approve 后才可固化），实现
/// "重复失败 → 自动提出改规则/换策略"的 harness 外环。
///
/// 治理边界：没有 --approve 时只产出 dry-run 提案，绝不落盘；
/// 落盘后若接了 --benchmark-gate 验证且结果不严格优于基线，
/// 自动回滚本次固化的规则文件并在输出中标记 reverted=true。
fn goal_evolve_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_evolve(args)?;
    let store = GoalRunStore::new(&request.root);
    let run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;
    let verdict = run.convergence_verdict();

    let mut output = if verdict.status == ConvergenceStatus::Blocked
        || verdict.status == ConvergenceStatus::Spinning
    {
        let mut metadata = BTreeMap::new();
        metadata.insert("goal_id".to_string(), request.goal_id.clone());
        metadata.insert(
            "convergence_status".to_string(),
            convergence_status_label(&verdict),
        );
        if let Some(fingerprint) = verdict.repeated_fingerprint.as_deref() {
            metadata.insert("repeated_fingerprint".to_string(), fingerprint.to_string());
        }
        metadata.insert(
            "repeated_count".to_string(),
            verdict.repeated_count.to_string(),
        );
        metadata.insert(
            "max_repeated_blockers".to_string(),
            run.goal_spec
                .convergence_policy
                .max_repeated_blockers
                .to_string(),
        );

        let event = RuntimeEvent {
            event_id: format!(
                "goal-evolve-{}-{}",
                sanitize_event_id_part(&request.goal_id),
                goal_evolve_token()
            ),
            task_id: request.goal_id.clone(),
            kind: RuntimeEventKind::ToolFailed,
            summary: verdict.reason.clone(),
            metadata,
        };
        let mut evolver = DryRunProposalEvolver::new();
        evolver
            .observe(event)
            .map_err(|err| format!("goal_evolve_observe_failed: {err:?}"))?;
        let proposals = evolver
            .propose(EvolutionScope {
                agent_id: "chuang-goal".to_string(),
                task_kind: Some("goal-convergence-blocker".to_string()),
                max_proposals: 1,
            })
            .map_err(|err| format!("goal_evolve_propose_failed: {err:?}"))?;
        let proposal_validations = proposals
            .iter()
            .map(|proposal| {
                evolver
                    .validate(proposal)
                    .map_err(|err| format!("goal_evolve_validate_failed: {err:?}"))
            })
            .collect::<Result<Vec<_>, _>>()?;

        GoalEvolveOutput {
            goal_id: request.goal_id.clone(),
            convergence_status: convergence_status_label(&verdict),
            convergence_repeated_fingerprint: verdict.repeated_fingerprint.clone(),
            convergence_repeated_count: verdict.repeated_count,
            convergence_reason: verdict.reason.clone(),
            evolved: true,
            proposal_count: proposals.len(),
            proposals,
            proposal_validations,
            approval: None,
            benchmark_verification: None,
        }
    } else {
        GoalEvolveOutput {
            goal_id: request.goal_id.clone(),
            convergence_status: convergence_status_label(&verdict),
            convergence_repeated_fingerprint: None,
            convergence_repeated_count: 0,
            convergence_reason: verdict.reason.clone(),
            evolved: false,
            proposal_count: 0,
            proposals: Vec::new(),
            proposal_validations: Vec::new(),
            approval: None,
            benchmark_verification: None,
        }
    };

    if request.approve {
        if !output.evolved || output.proposals.is_empty() {
            return Err(
                "goal_evolve_approve_rejected: no convergence blocker proposal to solidify (goal is not blocked/spinning)"
                    .to_string(),
            );
        }
        let (approval, benchmark_verification) = goal_evolve_approve_and_verify(&request, &output)?;
        output.approval = Some(approval);
        output.benchmark_verification = benchmark_verification;
    }

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_evolve_goal_id: {}", output.goal_id);
            println!(
                "goal_evolve_convergence_status: {}",
                output.convergence_status
            );
            println!("goal_evolve_reason: {}", output.convergence_reason);
            println!("goal_evolve_evolved: {}", output.evolved);
            println!("goal_evolve_proposal_count: {}", output.proposal_count);
            for (proposal, validation) in output
                .proposals
                .iter()
                .zip(output.proposal_validations.iter())
            {
                println!(
                    "goal_evolve_proposal id={} title={} trigger={} accepted={} reasons={}",
                    proposal.proposal_id,
                    proposal.title,
                    proposal.trigger,
                    validation.accepted,
                    validation.reasons.join("|")
                );
            }
            if let Some(approval) = &output.approval {
                println!(
                    "goal_evolve_approve requested=true approved={} approval_source={} approval_threshold={} writes_skills=true solidifies_skill=true skills_root={} judgments={} writes={}",
                    approval.approved,
                    approval.approval_source,
                    approval.approval_threshold,
                    approval.skills_root,
                    approval.judgment_count,
                    approval.write_count
                );
                for receipt in &approval.write_receipts {
                    println!(
                        "goal_evolve_write skill_id={} action={} duplicate_state={} path={} bytes_written={} status={} version={}",
                        receipt.skill_id,
                        receipt.action,
                        receipt.duplicate_state,
                        receipt.path,
                        receipt.bytes_written,
                        receipt.status,
                        receipt.version
                    );
                }
            }
            if let Some(benchmark) = &output.benchmark_verification {
                println!(
                    "goal_evolve_benchmark requested=true gate={} best_score={} after_score={} passed={} reverted={} revert_reason={}",
                    benchmark.benchmark_gate.as_deref().unwrap_or("none"),
                    benchmark
                        .best_score
                        .map(|score| score.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    benchmark
                        .after_score
                        .map(|score| score.to_string())
                        .unwrap_or_else(|| "none".to_string()),
                    benchmark.passed,
                    benchmark.reverted,
                    benchmark
                        .revert_reason
                        .as_deref()
                        .unwrap_or("none")
                );
                for receipt in &benchmark.revert_receipts {
                    println!(
                        "goal_evolve_revert skill_id={} path={} action={} reason={}",
                        receipt.skill_id, receipt.path, receipt.action, receipt.reason
                    );
                }
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }
    Ok(())
}

/// 审批固化 + 可选 benchmark 验证（post-write，失败自动回滚）。
/// 治理边界：只有显式 --approve（携带审批来源/阈值参数）才会走这里；
/// 固化前先自评打分，任一 judgment 不通过或 validation 不接受都拒绝落盘。
fn goal_evolve_approve_and_verify(
    request: &GoalEvolveCliRequest,
    output: &GoalEvolveOutput,
) -> Result<(GoalEvolveApprovalOutput, Option<GoalEvolveBenchmarkOutput>), String> {
    let review = SkillReviewBuild {
        proposals: output.proposals.clone(),
        proposal_validation_reports: output.proposal_validations.clone(),
    };
    let judgments = build_skill_judgments(
        &review,
        request.approval_threshold,
        Some(&request.skills_root),
    );
    if let Some(rejected) = judgments.iter().find(|judgment| !judgment.approved) {
        return Err(format!(
            "goal_evolve_approve_rejected: proposal {} self score {} below threshold {}",
            rejected.proposal_id, rejected.score_total, rejected.threshold
        ));
    }
    if let Some(rejected) = review
        .proposal_validation_reports
        .iter()
        .find(|report| !report.accepted)
    {
        return Err(format!(
            "goal_evolve_approve_rejected: proposal {} validation not accepted",
            rejected.proposal_id
        ));
    }

    // 固化前先记录原状（存在则保存原文，不存在则记为新建），供 auto-revert 精确还原。
    let mut write_states = Vec::new();
    let mut write_receipts = Vec::new();
    for (proposal, judgment) in output.proposals.iter().zip(judgments.iter()) {
        let path = request
            .skills_root
            .join(format!("{}.md", judgment.canonical_skill_id));
        let existed = path.exists();
        let original_content = if existed {
            Some(
                fs::read_to_string(&path)
                    .map_err(|err| format!("goal_evolve_approve_read_existing_failed: {err}"))?,
            )
        } else {
            None
        };
        let receipt = solidify_skill_file(
            &request.skills_root,
            proposal,
            judgment,
            &request.approval_source,
            request.approved_at.as_deref(),
            request.approval_note.as_deref(),
        )
        .map_err(|err| format!("goal_evolve_approve_solidify_failed: {err}"))?;
        write_states.push(GoalEvolveWriteState {
            skill_id: receipt.skill_id.clone(),
            path,
            existed,
            original_content,
        });
        write_receipts.push(receipt);
    }

    let approval = GoalEvolveApprovalOutput {
        requested: true,
        approved: true,
        approval_source: request.approval_source.clone(),
        approval_threshold: request.approval_threshold,
        writes_skills: true,
        solidifies_skill: true,
        skills_root: request.skills_root.display().to_string(),
        judgment_count: judgments.len(),
        write_count: write_receipts.len(),
        judgments,
        write_receipts,
    };

    let benchmark_verification = match &request.benchmark_gate {
        Some(gate_id) => {
            let benchmark_root = request
                .benchmark_root
                .clone()
                .unwrap_or_else(|| PathBuf::from(crate::cli_skill::DEFAULT_BENCHMARK_ROOT));
            let board = BenchmarkStore::new(&benchmark_root)
                .load_scoreboard(gate_id)
                .map_err(|err| format!("goal_evolve_benchmark_load_failed: {err}"))?;
            match enforce_benchmark_gate(&board, request.benchmark_after_score) {
                Ok(outcome) => Some(GoalEvolveBenchmarkOutput {
                    requested: true,
                    benchmark_gate: Some(gate_id.clone()),
                    best_score: outcome.best_score,
                    after_score: request.benchmark_after_score,
                    passed: true,
                    reverted: false,
                    revert_reason: None,
                    revert_receipts: Vec::new(),
                }),
                Err(reason) => {
                    // 验证不通过（无基线 / 未严格提升）：自动回滚本次固化的全部规则文件。
                    let mut revert_receipts = Vec::new();
                    for state in &write_states {
                        revert_receipts.push(revert_evolved_skill_write(state, &reason)?);
                    }
                    Some(GoalEvolveBenchmarkOutput {
                        requested: true,
                        benchmark_gate: Some(gate_id.clone()),
                        best_score: board.best.as_ref().map(|entry| entry.total_score),
                        after_score: request.benchmark_after_score,
                        passed: false,
                        reverted: true,
                        revert_reason: Some(reason),
                        revert_receipts,
                    })
                }
            }
        }
        None => None,
    };

    Ok((approval, benchmark_verification))
}

/// 把本次固化写入的规则文件还原到写前状态：
/// - 原本存在的文件：写回原始内容（精确还原，不残留回滚标记）。
/// - 本次新建的文件：移除刚创建的文件（属于同一命令自身的回滚，非用户数据删除）。
fn revert_evolved_skill_write(
    state: &GoalEvolveWriteState,
    reason: &str,
) -> Result<GoalEvolveRevertReceipt, String> {
    if state.existed {
        fs::write(
            &state.path,
            state.original_content.as_deref().unwrap_or_default(),
        )
        .map_err(|err| format!("goal_evolve_revert_restore_failed: {err}"))?;
        Ok(GoalEvolveRevertReceipt {
            skill_id: state.skill_id.clone(),
            path: state.path.display().to_string(),
            action: "restored_previous".to_string(),
            reason: reason.to_string(),
        })
    } else {
        fs::remove_file(&state.path)
            .map_err(|err| format!("goal_evolve_revert_remove_failed: {err}"))?;
        Ok(GoalEvolveRevertReceipt {
            skill_id: state.skill_id.clone(),
            path: state.path.display().to_string(),
            action: "removed_created".to_string(),
            reason: reason.to_string(),
        })
    }
}

fn convergence_status_label(verdict: &ConvergenceVerdict) -> String {
    match verdict.status {
        ConvergenceStatus::Unknown => "unknown".to_string(),
        ConvergenceStatus::Converging => "converging".to_string(),
        ConvergenceStatus::Spinning => "spinning".to_string(),
        ConvergenceStatus::Blocked => "blocked".to_string(),
    }
}

fn sanitize_event_id_part(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '-'
            }
        })
        .collect()
}

fn goal_evolve_token() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

#[derive(Serialize)]
struct GoalEvolveOutput {
    goal_id: String,
    convergence_status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    convergence_repeated_fingerprint: Option<String>,
    convergence_repeated_count: usize,
    convergence_reason: String,
    evolved: bool,
    proposal_count: usize,
    proposals: Vec<SkillProposal>,
    proposal_validations: Vec<ValidationReport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    approval: Option<GoalEvolveApprovalOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    benchmark_verification: Option<GoalEvolveBenchmarkOutput>,
}

#[derive(Serialize)]
struct GoalEvolveApprovalOutput {
    requested: bool,
    approved: bool,
    approval_source: String,
    approval_threshold: u16,
    writes_skills: bool,
    solidifies_skill: bool,
    skills_root: String,
    judgment_count: usize,
    write_count: usize,
    judgments: Vec<SkillJudgmentOutput>,
    write_receipts: Vec<SkillWriteReceiptOutput>,
}

#[derive(Serialize)]
struct GoalEvolveBenchmarkOutput {
    requested: bool,
    benchmark_gate: Option<String>,
    best_score: Option<u16>,
    after_score: Option<u16>,
    passed: bool,
    reverted: bool,
    revert_reason: Option<String>,
    revert_receipts: Vec<GoalEvolveRevertReceipt>,
}

#[derive(Serialize)]
struct GoalEvolveRevertReceipt {
    skill_id: String,
    path: String,
    action: String,
    reason: String,
}

struct GoalEvolveWriteState {
    skill_id: String,
    path: PathBuf,
    existed: bool,
    original_content: Option<String>,
}

fn goal_checkpoint_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_checkpoint(args)?;
    let store = GoalRunStore::new(&request.root);
    let run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;
    let fresh_evidence_verdicts = check_evidence_plan(&request.root, &run.goal_spec);
    let (summary, completed_worker_ids, validation_notes, evidence_verdicts, acceptance_verdicts, source_hint) =
        match request.source {
            GoalCheckpointCliSource::Manual {
                summary,
                completed_worker_ids,
                validation_notes,
                blocker_key,
            } => {
                // manual 分支保持行为不变：不携带 acceptance_verdicts（空）。
                let mut checkpoint = GoalCheckpoint::with_evidence(
                    request.checkpoint_id,
                    summary,
                    completed_worker_ids,
                    validation_notes,
                    fresh_evidence_verdicts,
                );
                if let Some(blocker_key) = blocker_key {
                    checkpoint.blocker_key = Some(blocker_key);
                }
                let receipt = store
                    .record_checkpoint(&request.goal_id, checkpoint)
                    .map_err(format_goal_run_error)?;
                return render_goal_checkpoint_receipt(receipt, None, request.output);
            }
            GoalCheckpointCliSource::FromCollect { queue_root } => {
                let suggestion =
                    load_goal_checkpoint_suggestion(&request.root, &queue_root, &request.goal_id)?;
                // verifier-first：优先落盘 collect 时产出的运行时证据判定快照；
                // 旧 suggestion 无该字段时回退为 checkpoint 时重新检查。
                let evidence_verdicts = if suggestion.evidence_verdicts.is_empty() {
                    fresh_evidence_verdicts
                } else {
                    suggestion.evidence_verdicts
                };
                (
                    suggestion.summary,
                    suggestion.completed_worker_ids,
                    suggestion.validation_notes,
                    evidence_verdicts,
                    suggestion.acceptance_verdicts,
                    Some("collect"),
                )
            }
        };
    let checkpoint = GoalCheckpoint::with_acceptance_verdicts(
        request.checkpoint_id,
        summary,
        completed_worker_ids,
        validation_notes,
        evidence_verdicts,
        acceptance_verdicts,
    );
    let receipt = store
        .record_checkpoint(&request.goal_id, checkpoint)
        .map_err(format_goal_run_error)?;

    render_goal_checkpoint_receipt(receipt, source_hint, request.output)
}

fn render_goal_checkpoint_receipt(
    receipt: GoalRunReceipt,
    source_hint: Option<&str>,
    output: ControlOutputFormat,
) -> Result<(), String> {
    match output {
        ControlOutputFormat::Text => {
            if let Some(source_hint) = source_hint {
                println!("goal_checkpoint_source: {}", source_hint);
            }
            println!("goal_checkpoint_recorded: {}", receipt.goal_id);
            println!("goal_path: {}", receipt.path);
            println!("goal_checkpoint_count: {}", receipt.checkpoint_count);
            println!(
                "goal_checkpoint_summary: {}",
                receipt.last_checkpoint_summary.as_deref().unwrap_or("none")
            );
            println!(
                "goal_checkpoint_acceptance_verdicts: {}",
                receipt
                    .last_checkpoint_acceptance_verdicts
                    .map(|count| count.to_string())
                    .unwrap_or_else(|| "none".to_string())
            );
            print_goal_checkpoint_writeback("goal", &receipt.checkpoint_writeback);
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

/// verifier-first 验收判定入口：按 goal 定义时声明的类型化验收计划
/// 逐条产出证据判定（文件证据只读检查；命令检查显式执行）。
/// 只读命令，不写 checkpoint、不修改 goal 文件；`--json` 输出判定详情。
fn goal_verify_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_verify(args)?;
    let store = GoalRunStore::new(&request.root);
    let run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;
    let verdicts = evaluate_acceptance_plan(&request.root, &run.goal_spec.acceptance_plan);
    let passed = verdicts.iter().all(|verdict| verdict.passed);
    let missing = verdicts
        .iter()
        .filter(|verdict| !verdict.passed)
        .map(|verdict| {
            format!(
                "[{}] {}: {}",
                verdict.evaluator, verdict.description, verdict.reason
            )
        })
        .collect::<Vec<_>>();

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_verify_goal_id: {}", request.goal_id);
            println!(
                "goal_verify_acceptance_checks: {}",
                run.goal_spec.acceptance_plan.len()
            );
            println!("goal_verify_passed: {}", passed);
            println!("goal_verify_missing: {}", format_text_list(&missing));
            for verdict in &verdicts {
                println!(
                    "goal_verify_verdict: [{}] [{}] {} passed={} reason={}",
                    verdict.check_index,
                    verdict.evaluator,
                    verdict.description,
                    verdict.passed,
                    verdict.reason
                );
            }
        }
        ControlOutputFormat::Json => print_json(&GoalVerifyOutput {
            goal_id: request.goal_id,
            acceptance_checks: run.goal_spec.acceptance_plan.len(),
            passed,
            missing,
            verdicts,
        })?,
    }
    Ok(())
}

#[derive(Serialize)]
struct GoalVerifyOutput {
    goal_id: String,
    acceptance_checks: usize,
    passed: bool,
    missing: Vec<String>,
    verdicts: Vec<chuang_agent::goal_mode::AcceptanceVerdict>,
}

struct GoalVerifyCliRequest {
    goal_id: String,
    root: PathBuf,
    output: ControlOutputFormat,
}

fn parse_goal_verify(args: &[String]) -> Result<GoalVerifyCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(GoalVerifyCliRequest {
        goal_id,
        root,
        output,
    })
}

fn goal_dispatch_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_dispatch(args)?;
    let store = GoalRunStore::new(&request.root);
    let run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;
    run.assert_time_budget_allows_continue()
        .map_err(format_goal_run_error)?;
    let receipt = dispatch_goal_run(
        &run,
        &request.root,
        &request.queue_root,
        request.parent_agent_id,
    )
    .map_err(format_goal_dispatch_error)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_dispatch_queued: {}", receipt.goal_id);
            println!("goal_dispatch_goal_objective: {}", receipt.goal_objective);
            println!("goal_dispatch_goal_root: {}", receipt.goal_root);
            println!("goal_dispatch_queue_root: {}", receipt.queue_root);
            println!("goal_dispatch_count: {}", receipt.dispatch_count);
            println!(
                "goal_dispatch_ready: {}",
                receipt.dispatch_diagnostics.ready_to_dispatch
            );
            println!(
                "goal_dispatch_worker_scope_complete: {}",
                receipt.dispatch_diagnostics.worker_scope_complete
            );
            println!(
                "goal_dispatch_worker_validation_complete: {}",
                receipt.dispatch_diagnostics.worker_validation_complete
            );
            println!(
                "goal_dispatch_validation_plan_complete: {}",
                receipt.dispatch_diagnostics.validation_plan_complete
            );
            println!(
                "goal_dispatch_incomplete_reasons: {}",
                format_text_list(&receipt.dispatch_diagnostics.incomplete_reasons)
            );
            println!(
                "goal_dispatch_workers: {}",
                format_text_list(
                    &receipt
                        .dispatches
                        .iter()
                        .map(|dispatch| dispatch.worker_id.clone())
                        .collect::<Vec<_>>()
                )
            );
            println!(
                "goal_dispatch_run_ids: {}",
                format_text_list(
                    &receipt
                        .dispatches
                        .iter()
                        .map(|dispatch| dispatch.run_id.clone())
                        .collect::<Vec<_>>()
                )
            );
            println!(
                "goal_dispatch_paths: {}",
                format_text_list(
                    &receipt
                        .dispatches
                        .iter()
                        .map(|dispatch| dispatch.dispatch_path.clone())
                        .collect::<Vec<_>>()
                )
            );
            println!(
                "goal_dispatch_manifest_path: {}",
                receipt.dispatch_manifest_path
            );
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

fn goal_collect_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_collect(args)?;
    let receipt =
        collect_goal_dispatch_reports(&request.root, &request.queue_root, &request.goal_id)
            .map_err(format_goal_dispatch_error)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_collect_goal_id: {}", receipt.goal_id);
            println!("goal_collect_goal_root: {}", receipt.goal_root);
            println!("goal_collect_queue_root: {}", receipt.queue_root);
            println!(
                "goal_collect_available_report_count: {}",
                receipt.available_report_count
            );
            println!(
                "goal_collect_missing_run_ids: {}",
                format_text_list(&receipt.missing_run_ids)
            );
            println!(
                "goal_collect_report_run_ids: {}",
                format_text_list(&receipt.report_run_ids)
            );
            println!(
                "goal_collect_completed_worker_ids: {}",
                format_text_list(&receipt.completed_worker_ids)
            );
            println!(
                "goal_collect_report_summaries: {}",
                format_text_list(&receipt.report_summaries)
            );
            println!(
                "goal_collect_parent_context_handoff_count: {}",
                receipt.parent_context_handoffs.len()
            );
            println!(
                "goal_collect_parent_context_handoff_refs: {}",
                format_text_list(&parent_context_handoff_refs(
                    &receipt.parent_context_handoffs
                ))
            );
            println!(
                "goal_collect_handoff_query_parent_context_handoff_count: {}",
                receipt.handoff_query_summary.parent_context_handoff_count
            );
            println!(
                "goal_collect_handoff_query_parent_context_handoff_refs: {}",
                format_text_list(&receipt.handoff_query_summary.parent_context_handoff_refs)
            );
            println!(
                "goal_collect_handoff_query_report_admission_ref_count: {}",
                receipt.handoff_query_summary.report_admission_ref_count
            );
            println!(
                "goal_collect_handoff_query_report_admission_reason_codes: {}",
                format_key_value_list(&receipt.handoff_query_summary.report_admission_reason_codes)
            );
            println!(
                "goal_collect_handoff_query_report_admission_refs: {}",
                format_admission_ref_list(&receipt.handoff_query_summary.report_admission_refs)
            );
            println!(
                "goal_collect_blocked_report_run_ids: {}",
                format_text_list(&receipt.blocked_report_run_ids)
            );
            println!(
                "goal_collect_blocked_report_reasons: {}",
                format_text_list(&receipt.blocked_report_reasons)
            );
            println!(
                "goal_collect_ready_to_checkpoint: {}",
                receipt.ready_to_checkpoint
            );
            println!(
                "goal_collect_acceptance_complete: {}",
                receipt.acceptance_complete
            );
            println!(
                "goal_collect_acceptance_missing: {}",
                format_text_list(&receipt.acceptance_missing)
            );
            if let Some(suggestion) = &receipt.checkpoint_suggestion {
                println!("goal_collect_checkpoint_summary: {}", suggestion.summary);
                println!(
                    "goal_collect_checkpoint_completed_worker_ids: {}",
                    format_text_list(&suggestion.completed_worker_ids)
                );
                println!(
                    "goal_collect_checkpoint_validation_notes: {}",
                    format_text_list(&suggestion.validation_notes)
                );
                println!(
                    "goal_collect_checkpoint_evidence_verdicts: {}",
                    suggestion.evidence_verdicts.len()
                );
            }
            println!("goal_collect_manifest_path: {}", receipt.manifest_path);
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

fn goal_step_command(args: &[String]) -> Result<(), String> {
    let request = parse_goal_step(args)?;
    let store = GoalRunStore::new(&request.root);
    let goal_run = store
        .load(&request.goal_id)
        .map_err(format_goal_run_error)?;
    goal_run
        .assert_time_budget_allows_continue()
        .map_err(format_goal_run_error)?;
    let manifest = load_goal_dispatch_manifest(&request.root, &request.goal_id)
        .map_err(format_goal_dispatch_error)?;
    let allowed_run_ids = manifest
        .dispatches
        .iter()
        .map(|dispatch| dispatch.run_id.clone())
        .collect::<BTreeSet<_>>();
    let requested_max_runs = request.max_runs.unwrap_or(allowed_run_ids.len().max(1));
    let max_runs = goal_run.step_run_cap(requested_max_runs);
    let queue = FileSubagentQueue::open(FileSubagentQueueConfig::new(&request.queue_root))
        .map_err(dispatch_error_from_queue)?;
    let run_once_request = request.run_once_request();
    let run_loop = run_subagent_run_loop(
        &queue,
        &run_once_request,
        max_runs,
        request.max_concurrency,
        Some(&allowed_run_ids),
    )?;
    let collection =
        collect_goal_dispatch_reports(&request.root, &request.queue_root, &request.goal_id)
            .map_err(format_goal_dispatch_error)?;
    let receipt = GoalStepReceipt {
        goal_id: request.goal_id,
        goal_root: request.root.display().to_string(),
        queue_root: request.queue_root.display().to_string(),
        manifest,
        run_loop,
        collection,
        checkpoint_recorded: false,
        writes_progress_log: false,
        writes_handoff: false,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("goal_step_goal_id: {}", receipt.goal_id);
            println!("goal_step_goal_root: {}", receipt.goal_root);
            println!("goal_step_queue_root: {}", receipt.queue_root);
            println!(
                "goal_step_manifest_dispatch_count: {}",
                receipt.manifest.dispatch_count
            );
            println!("goal_step_ran_count: {}", receipt.run_loop.ran_count);
            println!("goal_step_idle: {}", receipt.run_loop.idle);
            println!(
                "goal_step_run_ids: {}",
                format_text_list(&receipt.run_loop.run_ids)
            );
            println!(
                "goal_step_ready_to_checkpoint: {}",
                receipt.collection.ready_to_checkpoint
            );
            println!(
                "goal_step_acceptance_complete: {}",
                receipt.collection.acceptance_complete
            );
            println!(
                "goal_step_acceptance_missing: {}",
                format_text_list(&receipt.collection.acceptance_missing)
            );
            println!(
                "goal_step_missing_run_ids: {}",
                format_text_list(&receipt.collection.missing_run_ids)
            );
            println!(
                "goal_step_handoff_query_parent_context_handoff_count: {}",
                receipt
                    .collection
                    .handoff_query_summary
                    .parent_context_handoff_count
            );
            println!(
                "goal_step_handoff_query_parent_context_handoff_refs: {}",
                format_text_list(
                    &receipt
                        .collection
                        .handoff_query_summary
                        .parent_context_handoff_refs
                )
            );
            println!(
                "goal_step_handoff_query_report_admission_ref_count: {}",
                receipt
                    .collection
                    .handoff_query_summary
                    .report_admission_ref_count
            );
            println!(
                "goal_step_handoff_query_report_admission_reason_codes: {}",
                format_key_value_list(
                    &receipt
                        .collection
                        .handoff_query_summary
                        .report_admission_reason_codes
                )
            );
            println!(
                "goal_step_handoff_query_report_admission_refs: {}",
                format_admission_ref_list(
                    &receipt
                        .collection
                        .handoff_query_summary
                        .report_admission_refs
                )
            );
            println!("goal_step_checkpoint_recorded: false");
            println!("goal_step_writes_progress_log: false");
            println!("goal_step_writes_handoff: false");
            if let Some(suggestion) = &receipt.collection.checkpoint_suggestion {
                println!("goal_step_checkpoint_summary: {}", suggestion.summary);
            }
        }
        ControlOutputFormat::Json => print_json(&receipt)?,
    }
    Ok(())
}

struct GoalPlanCliRequest {
    goal_id: String,
    objective: String,
    root: PathBuf,
    write_scopes: Vec<GoalWriteScope>,
    worker_plan: Vec<GoalWorkerPlan>,
    validation_commands: Vec<String>,
    evidence: Vec<GoalEvidence>,
    acceptance_plan: Vec<AcceptanceCheck>,
    max_subtasks: Option<usize>,
    output: ControlOutputFormat,
}

struct GoalShowCliRequest {
    goal_id: String,
    root: PathBuf,
    queue_root: PathBuf,
    output: ControlOutputFormat,
}

struct GoalCheckpointCliRequest {
    goal_id: String,
    checkpoint_id: String,
    root: PathBuf,
    source: GoalCheckpointCliSource,
    output: ControlOutputFormat,
}

struct GoalEvolveCliRequest {
    goal_id: String,
    root: PathBuf,
    output: ControlOutputFormat,
    approve: bool,
    approval_source: String,
    approved_at: Option<String>,
    approval_note: Option<String>,
    approval_threshold: u16,
    skills_root: PathBuf,
    benchmark_gate: Option<String>,
    benchmark_after_score: Option<u16>,
    benchmark_root: Option<PathBuf>,
}

enum GoalCheckpointCliSource {
    Manual {
        summary: String,
        completed_worker_ids: Vec<String>,
        validation_notes: Vec<String>,
        blocker_key: Option<String>,
    },
    FromCollect {
        queue_root: PathBuf,
    },
}

struct GoalDispatchCliRequest {
    goal_id: String,
    root: PathBuf,
    queue_root: PathBuf,
    parent_agent_id: String,
    output: ControlOutputFormat,
}

struct GoalCollectCliRequest {
    goal_id: String,
    root: PathBuf,
    queue_root: PathBuf,
    output: ControlOutputFormat,
}

struct GoalStepCliRequest {
    goal_id: String,
    root: PathBuf,
    queue_root: PathBuf,
    output: ControlOutputFormat,
    runner: String,
    runner_command: Option<String>,
    runner_args: Vec<String>,
    worker_capabilities: Vec<String>,
    approve_exec: bool,
    max_runs: Option<usize>,
    max_concurrency: usize,
}

impl GoalStepCliRequest {
    fn run_once_request(&self) -> SubagentRunOnceCliRequest {
        let mut runtime = RuntimeConfig::new(self.queue_root.join("goal-step-memory.db"));
        runtime.subagent_queue.root = self.queue_root.clone();
        SubagentRunOnceCliRequest {
            options: CliOptions { runtime },
            output: self.output,
            runner: self.runner.clone(),
            runner_command: self.runner_command.clone(),
            runner_args: self.runner_args.clone(),
            worker_capabilities: self.worker_capabilities.clone(),
            approve_exec: self.approve_exec,
        }
    }
}

#[derive(Serialize)]
struct GoalStepReceipt {
    goal_id: String,
    goal_root: String,
    queue_root: String,
    manifest: GoalDispatchManifest,
    run_loop: SubagentRunLoopCliOutput,
    collection: GoalDispatchCollectionReceipt,
    checkpoint_recorded: bool,
    writes_progress_log: bool,
    writes_handoff: bool,
}

fn build_goal_operability_status(
    run: &GoalRun,
    diagnostics: &GoalRunDiagnostics,
    goal_root: &Path,
    queue_root: &Path,
    goal_id: &str,
) -> GoalOperabilityStatus {
    let dispatch_manifest_path = goal_root
        .join(format!("{goal_id}.dispatch.json"))
        .display()
        .to_string();
    match load_goal_dispatch_manifest(goal_root, goal_id) {
        Ok(manifest) => {
            let collect_result =
                collect_goal_dispatch_reports_read_only(goal_root, queue_root, goal_id);
            let (goal_collect, goal_collect_error_field, goal_collect_error_message) =
                match collect_result {
                    Ok(receipt) => (Some(receipt), None, None),
                    Err(error) => (None, Some(error.field), Some(error.message)),
                };
            let goal_checkpoint_ready = goal_collect
                .as_ref()
                .map(|receipt| receipt.ready_to_checkpoint)
                .unwrap_or(false);
            let goal_pipeline_state =
                if !run.checkpoint_log.is_empty() && diagnostics.checkpoint_log_complete {
                    "checkpointed".to_string()
                } else if goal_checkpoint_ready {
                    "checkpoint_ready".to_string()
                } else {
                    "step_pending".to_string()
                };
            let goal_next_command_reason =
                if !run.checkpoint_log.is_empty() && diagnostics.checkpoint_log_complete {
                    "latest checkpoint is already recorded".to_string()
                } else if goal_checkpoint_ready {
                    "dispatch reports are ready to checkpoint".to_string()
                } else {
                    "dispatch manifest is present but reports are not yet ready to checkpoint"
                        .to_string()
                };
            let goal_next_command =
                if !run.checkpoint_log.is_empty() && diagnostics.checkpoint_log_complete {
                    goal_show_command_line(goal_root, goal_id)
                } else if goal_checkpoint_ready {
                    goal_checkpoint_command_line(goal_root, queue_root, goal_id)
                } else {
                    goal_step_command_line(goal_root, queue_root, goal_id)
                };

            return GoalOperabilityStatus {
                goal_id: goal_id.to_string(),
                goal_root: goal_root.display().to_string(),
                queue_root: queue_root.display().to_string(),
                goal_dispatch_manifest_path: dispatch_manifest_path,
                goal_dispatch_manifest_state: "ready".to_string(),
                goal_dispatch_manifest_error_field: None,
                goal_dispatch_manifest_error_message: None,
                goal_dispatch_ready: manifest.dispatch_diagnostics.ready_to_dispatch,
                goal_step_ready: true,
                goal_collect_ready: true,
                goal_checkpoint_ready,
                goal_pipeline_state,
                goal_next_command,
                goal_next_command_reason,
                goal_dispatch_diagnostics: Some(manifest.dispatch_diagnostics),
                goal_collect,
                goal_collect_error_field,
                goal_collect_error_message,
            };
        }
        Err(error) => {
            let goal_dispatch_ready = diagnostics.worker_scope_complete
                && diagnostics.worker_validation_complete
                && diagnostics.validation_plan_complete;
            let goal_pipeline_state =
                if !run.checkpoint_log.is_empty() && diagnostics.checkpoint_log_complete {
                    "checkpointed".to_string()
                } else {
                    "dispatch_pending".to_string()
                };
            let goal_next_command_reason =
                if !run.checkpoint_log.is_empty() && diagnostics.checkpoint_log_complete {
                    "latest checkpoint is already recorded".to_string()
                } else {
                    "dispatch manifest is missing or invalid".to_string()
                };
            let goal_next_command =
                if !run.checkpoint_log.is_empty() && diagnostics.checkpoint_log_complete {
                    goal_show_command_line(goal_root, goal_id)
                } else {
                    goal_dispatch_command_line(goal_root, queue_root, goal_id)
                };
            let goal_dispatch_manifest_state = if error.field == "goal_dispatch_manifest.path" {
                "missing".to_string()
            } else {
                "invalid".to_string()
            };

            return GoalOperabilityStatus {
                goal_id: goal_id.to_string(),
                goal_root: goal_root.display().to_string(),
                queue_root: queue_root.display().to_string(),
                goal_dispatch_manifest_path: dispatch_manifest_path,
                goal_dispatch_manifest_state,
                goal_dispatch_manifest_error_field: Some(error.field),
                goal_dispatch_manifest_error_message: Some(error.message),
                goal_dispatch_ready,
                goal_step_ready: false,
                goal_collect_ready: false,
                goal_checkpoint_ready: false,
                goal_pipeline_state,
                goal_next_command,
                goal_next_command_reason,
                goal_dispatch_diagnostics: None,
                goal_collect: None,
                goal_collect_error_field: None,
                goal_collect_error_message: None,
            };
        }
    }
}

fn print_goal_operability_text(status: &GoalOperabilityStatus) {
    println!("goal_operability_goal_id: {}", status.goal_id);
    println!("goal_operability_goal_root: {}", status.goal_root);
    println!("goal_operability_queue_root: {}", status.queue_root);
    println!(
        "goal_operability_dispatch_manifest_path: {}",
        status.goal_dispatch_manifest_path
    );
    println!(
        "goal_operability_dispatch_manifest_state: {}",
        status.goal_dispatch_manifest_state
    );
    if let Some(field) = &status.goal_dispatch_manifest_error_field {
        println!("goal_operability_dispatch_manifest_error_field: {}", field);
    }
    if let Some(message) = &status.goal_dispatch_manifest_error_message {
        println!(
            "goal_operability_dispatch_manifest_error_message: {}",
            message
        );
    }
    println!(
        "goal_operability_dispatch_ready: {}",
        status.goal_dispatch_ready
    );
    println!("goal_operability_step_ready: {}", status.goal_step_ready);
    println!(
        "goal_operability_collect_ready: {}",
        status.goal_collect_ready
    );
    println!(
        "goal_operability_checkpoint_ready: {}",
        status.goal_checkpoint_ready
    );
    println!(
        "goal_operability_pipeline_state: {}",
        status.goal_pipeline_state
    );
    println!(
        "goal_operability_next_command: {}",
        status.goal_next_command
    );
    println!(
        "goal_operability_next_command_reason: {}",
        status.goal_next_command_reason
    );
    if let Some(dispatch_diagnostics) = &status.goal_dispatch_diagnostics {
        println!(
            "goal_operability_dispatch_worker_scope_complete: {}",
            dispatch_diagnostics.worker_scope_complete
        );
        println!(
            "goal_operability_dispatch_worker_validation_complete: {}",
            dispatch_diagnostics.worker_validation_complete
        );
        println!(
            "goal_operability_dispatch_validation_plan_complete: {}",
            dispatch_diagnostics.validation_plan_complete
        );
        println!(
            "goal_operability_dispatch_incomplete_reasons: {}",
            format_text_list(&dispatch_diagnostics.incomplete_reasons)
        );
    }
    if let Some(collect) = &status.goal_collect {
        println!(
            "goal_operability_collect_available_report_count: {}",
            collect.available_report_count
        );
        println!(
            "goal_operability_collect_missing_run_ids: {}",
            format_text_list(&collect.missing_run_ids)
        );
        println!(
            "goal_operability_collect_report_run_ids: {}",
            format_text_list(&collect.report_run_ids)
        );
        println!(
            "goal_operability_collect_blocked_report_run_ids: {}",
            format_text_list(&collect.blocked_report_run_ids)
        );
        println!(
            "goal_operability_collect_blocked_report_reasons: {}",
            format_text_list(&collect.blocked_report_reasons)
        );
        println!(
            "goal_operability_parent_context_handoff_count: {}",
            collect.parent_context_handoffs.len()
        );
        println!(
            "goal_operability_parent_context_handoff_refs: {}",
            format_text_list(&parent_context_handoff_refs(
                &collect.parent_context_handoffs
            ))
        );
        println!(
            "goal_operability_handoff_query_parent_context_handoff_count: {}",
            collect.handoff_query_summary.parent_context_handoff_count
        );
        println!(
            "goal_operability_handoff_query_parent_context_handoff_refs: {}",
            format_text_list(&collect.handoff_query_summary.parent_context_handoff_refs)
        );
        println!(
            "goal_operability_handoff_query_report_admission_ref_count: {}",
            collect.handoff_query_summary.report_admission_ref_count
        );
        println!(
            "goal_operability_handoff_query_report_admission_reason_codes: {}",
            format_key_value_list(&collect.handoff_query_summary.report_admission_reason_codes)
        );
        println!(
            "goal_operability_handoff_query_report_admission_refs: {}",
            format_admission_ref_list(&collect.handoff_query_summary.report_admission_refs)
        );
        println!(
            "goal_operability_collect_ready_to_checkpoint: {}",
            collect.ready_to_checkpoint
        );
        if let Some(suggestion) = &collect.checkpoint_suggestion {
            println!(
                "goal_operability_checkpoint_summary: {}",
                suggestion.summary
            );
            println!(
                "goal_operability_checkpoint_completed_worker_ids: {}",
                format_text_list(&suggestion.completed_worker_ids)
            );
            println!(
                "goal_operability_checkpoint_validation_notes: {}",
                format_text_list(&suggestion.validation_notes)
            );
        }
    }
    if let Some(field) = &status.goal_collect_error_field {
        println!("goal_operability_collect_error_field: {}", field);
    }
    if let Some(message) = &status.goal_collect_error_message {
        println!("goal_operability_collect_error_message: {}", message);
    }
}

fn parent_context_handoff_refs(
    handoffs: &[chuang_agent::subagent_report::ParentContextHandoff],
) -> Vec<String> {
    handoffs
        .iter()
        .map(|handoff| {
            handoff.provenance_ref.clone().unwrap_or_else(|| {
                format!("proposal_only:{}", handoff.admission_reason_code.as_str())
            })
        })
        .collect()
}

fn format_key_value_list(map: &std::collections::BTreeMap<String, usize>) -> String {
    if map.is_empty() {
        return "[]".to_string();
    }
    let mut parts = Vec::with_capacity(map.len());
    for (key, value) in map {
        parts.push(format!("{key}={value}"));
    }
    parts.join(" | ")
}

fn format_admission_ref_list(
    refs: &[chuang_agent::goal_dispatch::GoalReportAdmissionRef],
) -> String {
    if refs.is_empty() {
        return "[]".to_string();
    }
    refs.iter()
        .map(|entry| {
            format!(
                "admission_id={} report_id={} task_id={} agent_id={} status={} reason_code={} evidence_ref={}",
                entry.admission_id.as_deref().unwrap_or("none"),
                entry.report_id,
                entry.task_id,
                entry.agent_id,
                entry.admission_status,
                entry.reason_code,
                entry.evidence_ref.as_deref().unwrap_or("none")
            )
        })
        .collect::<Vec<_>>()
        .join(" | ")
}

fn goal_dispatch_command_line(goal_root: &Path, queue_root: &Path, goal_id: &str) -> String {
    format!(
        "cargo run -- goal dispatch --root {} --goal-id {} --subagent-queue-root {}",
        goal_root.display(),
        goal_id,
        queue_root.display()
    )
}

fn goal_step_command_line(goal_root: &Path, queue_root: &Path, goal_id: &str) -> String {
    format!(
        "cargo run -- goal step --root {} --goal-id {} --subagent-queue-root {}",
        goal_root.display(),
        goal_id,
        queue_root.display()
    )
}

fn goal_checkpoint_command_line(goal_root: &Path, queue_root: &Path, goal_id: &str) -> String {
    format!(
        "cargo run -- goal checkpoint --from-collect --root {} --goal-id {} --subagent-queue-root {} --checkpoint-id <checkpoint-id>",
        goal_root.display(),
        goal_id,
        queue_root.display()
    )
}

fn goal_show_command_line(goal_root: &Path, goal_id: &str) -> String {
    format!(
        "cargo run -- goal show --root {} --goal-id {}",
        goal_root.display(),
        goal_id
    )
}

#[derive(Serialize)]
struct GoalShowOutput<'a> {
    #[serde(flatten)]
    run: &'a GoalRun,
    goal_run_diagnostics: GoalRunDiagnostics,
    goal_operability: GoalOperabilityStatus,
}

#[derive(Serialize)]
struct GoalOperabilityStatus {
    goal_id: String,
    goal_root: String,
    queue_root: String,
    goal_dispatch_manifest_path: String,
    goal_dispatch_manifest_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal_dispatch_manifest_error_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal_dispatch_manifest_error_message: Option<String>,
    goal_dispatch_ready: bool,
    goal_step_ready: bool,
    goal_collect_ready: bool,
    goal_checkpoint_ready: bool,
    goal_pipeline_state: String,
    goal_next_command: String,
    goal_next_command_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal_dispatch_diagnostics: Option<GoalDispatchDiagnostics>,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal_collect: Option<GoalDispatchCollectionReceipt>,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal_collect_error_field: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    goal_collect_error_message: Option<String>,
}

fn parse_goal_plan(args: &[String]) -> Result<GoalPlanCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut objective: Option<String> = None;
    let mut root = default_goal_root();
    let mut write_paths: Vec<String> = Vec::new();
    let mut write_scopes: Vec<GoalWriteScope> = Vec::new();
    let mut worker_plan: Vec<GoalWorkerPlan> = Vec::new();
    let mut validation_commands: Vec<String> = Vec::new();
    let mut evidence: Vec<GoalEvidence> = Vec::new();
    let mut acceptance_plan: Vec<AcceptanceCheck> = Vec::new();
    let mut max_subtasks: Option<usize> = None;
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--objective" => objective = Some(take_value(args, &mut index, "--objective")?),
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--write-path" => write_paths.push(take_value(args, &mut index, "--write-path")?),
            "--scope" => write_scopes.push(parse_write_scope(&take_value(
                args, &mut index, "--scope",
            )?)?),
            "--worker" => worker_plan.push(parse_worker_plan(&take_value(
                args, &mut index, "--worker",
            )?)?),
            "--validation" => {
                validation_commands.push(take_value(args, &mut index, "--validation")?)
            }
            "--evidence" => {
                let evidence_item =
                    parse_goal_evidence(&take_value(args, &mut index, "--evidence")?)?;
                // legacy 字段与类型化验收计划同步：--evidence 声明即验收检查。
                evidence.push(evidence_item.clone());
                acceptance_plan.push(AcceptanceCheck::Evidence(evidence_item));
            }
            "--acceptance" => acceptance_plan.push(parse_goal_acceptance_check(&take_value(
                args,
                &mut index,
                "--acceptance",
            )?)?),
            "--max-subtasks" => {
                max_subtasks = Some(
                    take_value(args, &mut index, "--max-subtasks")?
                        .parse::<usize>()
                        .map_err(|_| "--max-subtasks expects a positive integer".to_string())?,
                )
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    if write_paths.is_empty() {
        if write_scopes.is_empty() {
            write_paths.push(".".to_string());
        }
    }
    if !write_paths.is_empty() {
        write_scopes.push(GoalWriteScope::new("mainline", write_paths));
    }
    if validation_commands.is_empty() {
        validation_commands = GoalSpec::mainline_mvp("validation defaults").acceptance_checks;
    }
    if worker_plan.is_empty() {
        let default_worker_scope_ids = if write_scopes.is_empty() {
            vec!["mainline".to_string()]
        } else {
            write_scopes
                .iter()
                .map(|scope| scope.scope_id.clone())
                .collect()
        };
        worker_plan.push(GoalWorkerPlan::new(
            "main-process",
            "continue the goal from checkpoints",
            default_worker_scope_ids,
            validation_commands.clone(),
        ));
    }
    for worker in &mut worker_plan {
        if worker.validation_checks.is_empty() {
            worker.validation_checks = validation_commands.clone();
        }
    }

    if let Some(max_subtasks) = max_subtasks {
        if max_subtasks == 0 {
            return Err("--max-subtasks must be greater than zero".to_string());
        }
    }

    Ok(GoalPlanCliRequest {
        goal_id,
        objective: objective.ok_or_else(|| "goal plan requires --objective".to_string())?,
        root,
        write_scopes,
        worker_plan,
        validation_commands,
        evidence,
        acceptance_plan,
        max_subtasks,
        output,
    })
}

/// 解析类型化验收检查（verifier-first 声明）：
/// - `evidence:path|min_lines|min_content|description`
/// - `command:CMD`
fn parse_goal_acceptance_check(raw: &str) -> Result<AcceptanceCheck, String> {
    let (kind, payload) = raw
        .split_once(':')
        .ok_or_else(|| "--acceptance expects evidence:... or command:...".to_string())?;
    match kind.trim() {
        "evidence" => Ok(AcceptanceCheck::Evidence(parse_goal_evidence(payload)?)),
        "command" => {
            let command = payload.trim().to_string();
            if command.is_empty() {
                return Err("--acceptance command must not be empty".to_string());
            }
            Ok(AcceptanceCheck::Command(command))
        }
        _ => Err(
            "--acceptance kind must be evidence (path[|min_lines|min_content|description]) or command (CMD)"
                .to_string(),
        ),
    }
}

/// 解析验收证据：`path|min_lines|min_content|description`
/// - min_lines 空或 0 表示不检查行数
/// - min_content 空表示不检查内容
/// - description 空时用 path
fn parse_goal_evidence(raw: &str) -> Result<GoalEvidence, String> {
    let parts = raw.split('|').map(str::trim).collect::<Vec<_>>();
    if parts.is_empty() || parts[0].is_empty() {
        return Err("--evidence expects path[|min_lines|min_content|description]".to_string());
    }
    let path = parts[0].to_string();
    let mut evidence = GoalEvidence::new(path);
    if parts.len() > 1 && !parts[1].is_empty() && parts[1] != "0" {
        let min_lines = parts[1]
            .parse::<usize>()
            .map_err(|_| "--evidence min_lines must be a positive integer".to_string())?;
        evidence = evidence.with_min_lines(min_lines);
    }
    if parts.len() > 2 && !parts[2].is_empty() {
        evidence = evidence.with_min_content(parts[2]);
    }
    if parts.len() > 3 && !parts[3].is_empty() {
        evidence = evidence.with_description(parts[3]);
    }
    Ok(evidence)
}

fn parse_write_scope(raw: &str) -> Result<GoalWriteScope, String> {
    let (scope_id, raw_paths) = raw
        .split_once('=')
        .ok_or_else(|| "--scope expects scope_id=path[,path...]".to_string())?;
    let paths = raw_paths
        .split(',')
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if paths.is_empty() {
        return Err("--scope requires at least one path".to_string());
    }
    Ok(GoalWriteScope::new(scope_id.trim(), paths))
}

fn parse_worker_plan(raw: &str) -> Result<GoalWorkerPlan, String> {
    let parts = raw.split('|').map(str::trim).collect::<Vec<_>>();
    if parts.len() != 3 {
        return Err("--worker expects worker_id|scope_id[,scope_id...]|objective".to_string());
    }
    let write_scope_ids = parts[1]
        .split(',')
        .map(str::trim)
        .filter(|scope_id| !scope_id.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if write_scope_ids.is_empty() {
        return Err("--worker requires at least one scope id".to_string());
    }
    Ok(GoalWorkerPlan::new(
        parts[0],
        parts[2],
        write_scope_ids,
        Vec::new(),
    ))
}

fn parse_goal_show(args: &[String]) -> Result<GoalShowCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut queue_root = PathBuf::from("./context/subagent-queue");
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--subagent-queue-root" => {
                queue_root = PathBuf::from(take_value(args, &mut index, "--subagent-queue-root")?)
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(GoalShowCliRequest {
        goal_id,
        root,
        queue_root,
        output,
    })
}

fn parse_goal_checkpoint(args: &[String]) -> Result<GoalCheckpointCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut checkpoint_id: Option<String> = None;
    let mut summary: Option<String> = None;
    let mut completed_worker_ids: Vec<String> = Vec::new();
    let mut validation_notes: Vec<String> = Vec::new();
    let mut blocker_key: Option<String> = None;
    let mut root = default_goal_root();
    let mut queue_root: Option<PathBuf> = None;
    let mut from_collect = false;
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--checkpoint-id" => {
                checkpoint_id = Some(take_value(args, &mut index, "--checkpoint-id")?)
            }
            "--summary" => summary = Some(take_value(args, &mut index, "--summary")?),
            "--completed-worker-id" => {
                completed_worker_ids.push(take_value(args, &mut index, "--completed-worker-id")?)
            }
            "--validation-note" => {
                validation_notes.push(take_value(args, &mut index, "--validation-note")?)
            }
            "--blocker-key" => blocker_key = Some(take_value(args, &mut index, "--blocker-key")?),
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--subagent-queue-root" => {
                queue_root = Some(PathBuf::from(take_value(
                    args,
                    &mut index,
                    "--subagent-queue-root",
                )?))
            }
            "--from-collect" => {
                from_collect = true;
                index += 1;
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let source = if from_collect {
        if blocker_key.is_some() {
            return Err(format_goal_checkpoint_cli_error(
                "checkpoint.blocker_key",
                "--blocker-key is manual-only; --from-collect derives suggestions from reports",
            ));
        }
        if let Some(queue_root) = queue_root {
            GoalCheckpointCliSource::FromCollect { queue_root }
        } else {
            return Err(format_goal_checkpoint_cli_error(
                "collect.subagent_queue_root",
                "--from-collect requires --subagent-queue-root",
            ));
        }
    } else {
        if queue_root.is_some() {
            return Err(usage());
        }
        GoalCheckpointCliSource::Manual {
            summary: summary.ok_or_else(|| "goal checkpoint requires --summary".to_string())?,
            completed_worker_ids,
            validation_notes,
            blocker_key,
        }
    };

    Ok(GoalCheckpointCliRequest {
        goal_id,
        checkpoint_id: checkpoint_id.unwrap_or_else(default_checkpoint_id),
        root,
        source,
        output,
    })
}

fn parse_goal_evolve(args: &[String]) -> Result<GoalEvolveCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut output = ControlOutputFormat::Text;
    let mut approve = false;
    let mut approval_source = "cli goal evolve".to_string();
    let mut approved_at: Option<String> = None;
    let mut approval_note: Option<String> = None;
    let mut approval_threshold = 80u16;
    let mut skills_root: Option<PathBuf> = None;
    let mut benchmark_gate: Option<String> = None;
    let mut benchmark_after_score: Option<u16> = None;
    let mut benchmark_root: Option<PathBuf> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--approve" => {
                approve = true;
                index += 1;
            }
            "--approval-source" => {
                approval_source = take_value(args, &mut index, "--approval-source")?;
            }
            "--approved-at" => {
                approved_at = Some(take_value(args, &mut index, "--approved-at")?);
            }
            "--approval-note" => {
                approval_note = Some(take_value(args, &mut index, "--approval-note")?);
            }
            "--approval-threshold" => {
                let value = take_value(args, &mut index, "--approval-threshold")?;
                approval_threshold = value
                    .parse::<u16>()
                    .map_err(|_| "--approval-threshold requires numeric value".to_string())?;
            }
            "--skills-root" => {
                skills_root = Some(PathBuf::from(take_value(
                    args,
                    &mut index,
                    "--skills-root",
                )?));
            }
            "--benchmark-gate" => {
                benchmark_gate = Some(take_value(args, &mut index, "--benchmark-gate")?);
            }
            "--benchmark-after-score" => {
                let value = take_value(args, &mut index, "--benchmark-after-score")?;
                benchmark_after_score =
                    Some(value.parse::<u16>().map_err(|_| {
                        "--benchmark-after-score requires numeric value".to_string()
                    })?);
            }
            "--benchmark-root" => {
                benchmark_root = Some(PathBuf::from(take_value(
                    args,
                    &mut index,
                    "--benchmark-root",
                )?));
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    if benchmark_gate.is_some() || benchmark_after_score.is_some() {
        if !approve {
            return Err(
                "goal evolve --benchmark-gate/--benchmark-after-score require --approve (verification runs after solidification)"
                    .to_string(),
            );
        }
        if benchmark_after_score.is_some() && benchmark_gate.is_none() {
            return Err(
                "goal evolve --benchmark-after-score requires --benchmark-gate".to_string(),
            );
        }
    }
    if approve && approval_source.trim().is_empty() {
        return Err("goal evolve --approve requires non-empty --approval-source".to_string());
    }

    Ok(GoalEvolveCliRequest {
        goal_id,
        root,
        output,
        approve,
        approval_source,
        approved_at,
        approval_note,
        approval_threshold,
        skills_root: skills_root.unwrap_or_else(default_skills_root),
        benchmark_gate,
        benchmark_after_score,
        benchmark_root,
    })
}

fn parse_goal_dispatch(args: &[String]) -> Result<GoalDispatchCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut queue_root = PathBuf::from("./context/subagent-queue");
    let mut parent_agent_id = "chuang-goal".to_string();
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--subagent-queue-root" => {
                queue_root = PathBuf::from(take_value(args, &mut index, "--subagent-queue-root")?)
            }
            "--parent-agent-id" => {
                parent_agent_id = take_value(args, &mut index, "--parent-agent-id")?
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(GoalDispatchCliRequest {
        goal_id,
        root,
        queue_root,
        parent_agent_id,
        output,
    })
}

fn parse_goal_collect(args: &[String]) -> Result<GoalCollectCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut queue_root = PathBuf::from("./context/subagent-queue");
    let mut output = ControlOutputFormat::Text;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--subagent-queue-root" => {
                queue_root = PathBuf::from(take_value(args, &mut index, "--subagent-queue-root")?)
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    Ok(GoalCollectCliRequest {
        goal_id,
        root,
        queue_root,
        output,
    })
}

fn parse_goal_step(args: &[String]) -> Result<GoalStepCliRequest, String> {
    let mut goal_id = "mainline-mvp".to_string();
    let mut root = default_goal_root();
    let mut queue_root = PathBuf::from("./context/subagent-queue");
    let mut output = ControlOutputFormat::Text;
    let mut runner: Option<String> = None;
    let mut runner_command: Option<String> = None;
    let mut runner_args: Vec<String> = Vec::new();
    let mut worker_capabilities: Vec<String> = Vec::new();
    let mut approve_exec = false;
    let mut max_runs: Option<usize> = None;
    let mut max_concurrency = 1usize;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--goal-id" => goal_id = take_value(args, &mut index, "--goal-id")?,
            "--root" => root = PathBuf::from(take_value(args, &mut index, "--root")?),
            "--subagent-queue-root" => {
                queue_root = PathBuf::from(take_value(args, &mut index, "--subagent-queue-root")?)
            }
            "--runner" => runner = Some(take_value(args, &mut index, "--runner")?),
            "--runner-command" => {
                runner_command = Some(take_value(args, &mut index, "--runner-command")?)
            }
            "--runner-arg" => runner_args.push(take_value(args, &mut index, "--runner-arg")?),
            "--capability" => {
                push_unique_string(
                    &mut worker_capabilities,
                    take_value(args, &mut index, "--capability")?,
                    "--capability",
                )?;
            }
            "--approve-exec" => {
                approve_exec = true;
                index += 1;
            }
            "--max-runs" => {
                max_runs = Some(parse_positive_usize(
                    "--max-runs",
                    &take_value(args, &mut index, "--max-runs")?,
                )?)
            }
            "--max-concurrency" => {
                max_concurrency = parse_positive_usize(
                    "--max-concurrency",
                    &take_value(args, &mut index, "--max-concurrency")?,
                )?;
                if max_concurrency > 32 {
                    return Err(
                        "--max-concurrency above 32 is not supported by the worker loop"
                            .to_string(),
                    );
                }
            }
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    let runner = runner.unwrap_or_else(|| "fake".to_string());
    match runner.as_str() {
        "fake" => {
            if runner_command.is_some() || !runner_args.is_empty() || approve_exec {
                return Err(
                    "goal step fake runner does not accept command execution flags".to_string(),
                );
            }
        }
        "command" => {
            if !approve_exec {
                return Err("command_runner_requires_approve_exec: pass --approve-exec".to_string());
            }
            if runner_command
                .as_deref()
                .map(str::trim)
                .unwrap_or_default()
                .is_empty()
            {
                return Err("command_runner_requires_runner_command".to_string());
            }
        }
        _ => {
            return Err(format!(
                "unsupported goal step runner: {runner} (supported: fake, command)"
            ))
        }
    }

    Ok(GoalStepCliRequest {
        goal_id,
        root,
        queue_root,
        output,
        runner,
        runner_command,
        runner_args,
        worker_capabilities,
        approve_exec,
        max_runs,
        max_concurrency,
    })
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires a value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn default_checkpoint_id() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    format!("checkpoint-{nanos}")
}

fn default_goal_root() -> PathBuf {
    PathBuf::from("./context/goal-runs")
}

fn format_goal_run_error(error: chuang_agent::goal_run::GoalRunError) -> String {
    format!("goal_run_invalid: {}: {}", error.field, error.message)
}

fn format_goal_dispatch_error(error: chuang_agent::goal_dispatch::GoalDispatchError) -> String {
    format!("goal_dispatch_invalid: {}: {}", error.field, error.message)
}

fn format_goal_checkpoint_cli_error(field: &str, message: &str) -> String {
    format!("goal_checkpoint_invalid: {}: {}", field, message)
}

fn dispatch_error_from_queue(
    error: chuang_agent::subagent_queue::FileSubagentQueueError,
) -> String {
    format!("goal_dispatch_invalid: subagent_queue: {error:?}")
}

fn load_goal_checkpoint_suggestion(
    root: &PathBuf,
    queue_root: &PathBuf,
    goal_id: &str,
) -> Result<GoalCheckpointSuggestion, String> {
    let receipt = collect_goal_dispatch_reports(root.as_path(), queue_root.as_path(), goal_id)
        .map_err(format_goal_dispatch_error)?;

    if !receipt.ready_to_checkpoint {
        return Err(format_goal_checkpoint_cli_error(
            "collect.ready_to_checkpoint",
            &format!(
                "collect state is not ready: available_report_count={} dispatch_count={} missing_run_ids={} report_run_ids={} blocked_report_run_ids={} blocked_report_reasons={} manifest_path={}",
                receipt.available_report_count,
                receipt.dispatch_count,
                format_text_list(&receipt.missing_run_ids),
                format_text_list(&receipt.report_run_ids),
                format_text_list(&receipt.blocked_report_run_ids),
                format_text_list(&receipt.blocked_report_reasons),
                receipt.manifest_path
            ),
        ));
    }

    receipt.checkpoint_suggestion.ok_or_else(|| {
        format_goal_checkpoint_cli_error(
            "collect.checkpoint_suggestion",
            &format!(
                "ready collect state did not include a checkpoint suggestion: manifest_path={}",
                receipt.manifest_path
            ),
        )
    })
}

fn format_text_list(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(" | ")
    }
}
fn parse_positive_usize(flag: &str, raw: &str) -> Result<usize, String> {
    let value = raw
        .parse::<usize>()
        .map_err(|_| format!("{flag} expects a positive integer"))?;
    if value == 0 {
        return Err(format!("{flag} must be greater than zero"));
    }
    Ok(value)
}

fn push_unique_string(values: &mut Vec<String>, value: String, flag: &str) -> Result<(), String> {
    let value = value.trim().to_string();
    if value.is_empty() {
        return Err(format!("{flag} must not be empty"));
    }
    if !values.contains(&value) {
        values.push(value);
    }
    Ok(())
}

fn print_goal_checkpoint_writeback(prefix: &str, writeback: &GoalCheckpointWriteback) {
    println!(
        "{prefix}_checkpoint_writeback_manual_only: {}",
        writeback.manual_only
    );
    println!(
        "{prefix}_checkpoint_writeback_update_progress_log: {}",
        writeback.update_progress_log
    );
    println!(
        "{prefix}_checkpoint_writeback_update_handoff: {}",
        writeback.update_handoff
    );
    println!(
        "{prefix}_checkpoint_writeback_commit_checkpoint: {}",
        writeback.commit_checkpoint
    );
    println!(
        "{prefix}_checkpoint_writeback_targets: {}",
        format_text_list(&writeback.documentation_targets)
    );
}
