use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use chuang_agent::benchmark::{BenchmarkStore, Scoreboard};
use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver,
    SkillProposal, SkillSolidifyTicket, ValidationReport,
};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

/// Default benchmark root, aligned with `src/cli_benchmark.rs`.
pub(crate) const DEFAULT_BENCHMARK_ROOT: &str = "benchmarks";

pub(crate) fn skill_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("propose") => skill_propose_command(&args[1..]),
        Some("judge") => skill_judge_command(&args[1..]),
        Some("approve") => skill_approve_command(&args[1..]),
        Some("solidify") => skill_solidify_command(&args[1..]),
        Some("retire") | Some("deprecate") => skill_retire_command(&args[1..]),
        // Curator: read-only hygiene alias for monitor (never auto-writes).
        Some("monitor") | Some("curator") => skill_monitor_command(&args[1..]),
        Some("rollback") => skill_rollback_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn skill_propose_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_propose(args)?;
    let review = build_skill_review(&request)?;
    let proposal_validation_reports = review.proposal_validation_reports.clone();
    let proposal_validations = proposal_validation_reports
        .iter()
        .cloned()
        .map(SkillProposalValidationOutput::from)
        .collect::<Vec<_>>();
    let proposal_validation_accepted_count = proposal_validations
        .iter()
        .filter(|validation| validation.accepted)
        .count();
    let approval_tickets = review
        .proposals
        .iter()
        .zip(proposal_validation_reports)
        .map(|(proposal, validation)| SkillSolidifyTicket::pending_review(proposal, validation))
        .collect::<Vec<_>>();
    let output = SkillProposeOutput {
        dry_run: true,
        writes_skills: false,
        requires_approval: true,
        approval_boundary_explicit: true,
        proposal_count: review.proposals.len(),
        validation_count: proposal_validations.len(),
        approval_ticket_count: approval_tickets.len(),
        validation_accepted_count: proposal_validation_accepted_count,
        proposals: review.proposals,
        proposal_validations,
        approval_tickets,
        boundary: SkillProposeBoundary {
            observes_runtime_event: true,
            reads_existing_skills: false,
            writes_skill_files: false,
            solidifies_skill: false,
            emits_approval_ticket: true,
            connects_llm: false,
            connects_external_service: false,
        },
    };

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "skill_propose dry_run=true writes_skills=false requires_approval=true approval_boundary_explicit=true proposals={} validations={} accepted={} approval_tickets={}",
                output.proposal_count,
                output.validation_count,
                output.validation_accepted_count,
                output.approval_ticket_count
            );
            println!(
                "boundary observes_runtime_event=true reads_existing_skills=false writes_skill_files=false solidifies_skill=false emits_approval_ticket=true connects_llm=false connects_external_service=false"
            );
            for ((proposal, validation), ticket) in output
                .proposals
                .iter()
                .zip(output.proposal_validations.iter())
                .zip(output.approval_tickets.iter())
            {
                println!(
                    "proposal id={} title={} trigger={} evidence={} provenance={} dry_run={} writes_skills={} requires_approval={}",
                    proposal.proposal_id,
                    proposal.title,
                    proposal.trigger,
                    proposal.evidence_event_ids.join(","),
                    proposal
                        .provenance
                        .iter()
                        .map(|item| item.source_event_id.clone())
                        .collect::<Vec<_>>()
                        .join(","),
                    proposal.dry_run,
                    proposal.writes_skills,
                    proposal.requires_approval
                );
                println!(
                    "validation proposal_id={} accepted={} reasons={}",
                    validation.proposal_id,
                    validation.accepted,
                    validation.reasons.join("|")
                );
                println!(
                    "approval_ticket id={} proposal_id={} approved={} source={} dry_run={} writes_skills={} solidifies_skill={} local_only={}",
                    ticket.ticket_id,
                    ticket.proposal_id,
                    ticket.approval_receipt.approved,
                    ticket.approval_receipt.approval_source,
                    ticket.dry_run,
                    ticket.writes_skills,
                    ticket.solidifies_skill,
                    ticket.local_only
                );
            }
        }
    }

    Ok(())
}

fn skill_approve_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_approve(args)?;
    let review = build_skill_review(&request.review)?;
    let judgments = build_skill_judgments(&review, request.approval_threshold, None);
    if let Some(rejected) = judgments.iter().find(|judgment| !judgment.approved) {
        return Err(format!(
            "skill_approve_rejected: proposal {} self score {} below threshold {}",
            rejected.proposal_id, rejected.score_total, rejected.threshold
        ));
    }
    if let Some(rejected) = review
        .proposal_validation_reports
        .iter()
        .find(|report| !report.accepted)
    {
        return Err(format!(
            "skill_approve_rejected: proposal {} validation not accepted",
            rejected.proposal_id
        ));
    }

    let approval_tickets = review
        .proposals
        .iter()
        .zip(review.proposal_validation_reports)
        .map(|(proposal, validation)| {
            SkillSolidifyTicket::approval_receipt(
                proposal,
                validation,
                request.approval_source.clone(),
                request.approved_at.clone(),
                request.approval_note.clone(),
            )
        })
        .collect::<Vec<_>>();
    let output = SkillApproveOutput {
        approved: true,
        self_scored: true,
        approval_policy: "darwin_style_cli_rubric".to_string(),
        approval_threshold: request.approval_threshold,
        writes_skills: false,
        solidifies_skill: false,
        judgment_count: judgments.len(),
        judgments,
        approval_receipt_count: approval_tickets.len(),
        approval_receipts: approval_tickets,
        boundary: SkillApproveBoundary {
            validates_proposal: true,
            self_scores_proposal: true,
            emits_approval_receipt: true,
            writes_skill_files: false,
            solidifies_skill: false,
            connects_llm: false,
            connects_external_service: false,
        },
    };

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "skill_approve approved=true self_scored=true approval_policy={} approval_threshold={} writes_skills=false solidifies_skill=false judgments={} approval_receipts={}",
                output.approval_policy,
                output.approval_threshold,
                output.judgment_count,
                output.approval_receipt_count
            );
            println!(
                "boundary validates_proposal=true self_scores_proposal=true emits_approval_receipt=true writes_skill_files=false solidifies_skill=false connects_llm=false connects_external_service=false"
            );
            for judgment in &output.judgments {
                print_skill_judgment_text("judgment", judgment);
            }
            for receipt in &output.approval_receipts {
                println!(
                    "approval_receipt id={} proposal_id={} approved={} source={} approved_at={} note={} dry_run={} writes_skills={} solidifies_skill={} local_only={}",
                    receipt.ticket_id,
                    receipt.proposal_id,
                    receipt.approval_receipt.approved,
                    receipt.approval_receipt.approval_source,
                    receipt
                        .approval_receipt
                        .approved_at
                        .as_deref()
                        .unwrap_or("none"),
                    receipt
                        .approval_receipt
                        .approval_note
                        .as_deref()
                        .unwrap_or("none"),
                    receipt.dry_run,
                    receipt.writes_skills,
                    receipt.solidifies_skill,
                    receipt.local_only
                );
            }
        }
    }

    Ok(())
}

fn skill_judge_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_judge(args)?;
    let review = build_skill_review(&request.review)?;
    let skills_root = request
        .skills_root
        .clone()
        .unwrap_or_else(default_skills_root);
    let judgments = build_skill_judgments(&review, request.approval_threshold, Some(&skills_root));
    let approved_count = judgments
        .iter()
        .filter(|judgment| judgment.approved)
        .count();
    let output = SkillJudgeOutput {
        judged: true,
        self_scored: true,
        approval_policy: "darwin_style_cli_rubric".to_string(),
        approval_threshold: request.approval_threshold,
        proposal_count: review.proposals.len(),
        judgment_count: judgments.len(),
        approved_count,
        writes_skills: false,
        solidifies_skill: false,
        skills_root: skills_root.display().to_string(),
        judgments,
        boundary: SkillJudgeBoundary {
            validates_proposal: true,
            self_scores_proposal: true,
            reads_existing_skills: true,
            writes_skill_files: false,
            solidifies_skill: false,
            connects_llm: false,
            connects_external_service: false,
        },
    };

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "skill_judge judged=true self_scored=true approval_policy={} approval_threshold={} proposals={} judgments={} approved={} writes_skills=false solidifies_skill=false skills_root={}",
                output.approval_policy,
                output.approval_threshold,
                output.proposal_count,
                output.judgment_count,
                output.approved_count,
                output.skills_root
            );
            println!(
                "boundary validates_proposal=true self_scores_proposal=true reads_existing_skills=true writes_skill_files=false solidifies_skill=false connects_llm=false connects_external_service=false"
            );
            for judgment in &output.judgments {
                print_skill_judgment_text("judgment", judgment);
            }
        }
    }

    Ok(())
}

fn skill_solidify_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_solidify(args)?;
    let skills_root = request
        .skills_root
        .clone()
        .unwrap_or_else(default_skills_root);
    let output = run_skill_solidify(
        &request.review,
        &skills_root,
        &request.approval_source,
        request.approved_at.as_deref(),
        request.approval_note.as_deref(),
        request.approval_threshold,
        request.benchmark_gate.as_deref(),
        request.benchmark_after_score,
        request.benchmark_root.as_deref(),
    )?;

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => print_skill_solidify_text(&output),
    }

    Ok(())
}

/// Shared solidify pipeline, reused by `skill solidify` and by the
/// self-experiment closure (`experiment complete --outcome success` with a
/// benchmark gate). Owns: review -> self-score -> benchmark gate -> write.
pub(crate) fn run_skill_solidify(
    review_request: &SkillProposeRequest,
    skills_root: &Path,
    approval_source: &str,
    approved_at: Option<&str>,
    approval_note: Option<&str>,
    approval_threshold: u16,
    benchmark_gate: Option<&str>,
    benchmark_after_score: Option<u16>,
    benchmark_root: Option<&Path>,
) -> Result<SkillSolidifyOutput, String> {
    let review = build_skill_review(review_request)?;
    let judgments = build_skill_judgments(&review, approval_threshold, Some(skills_root));
    if let Some(rejected) = judgments.iter().find(|judgment| !judgment.approved) {
        return Err(format!(
            "skill_solidify_rejected: proposal {} self score {} below threshold {}",
            rejected.proposal_id, rejected.score_total, rejected.threshold
        ));
    }
    if let Some(rejected) = review
        .proposal_validation_reports
        .iter()
        .find(|report| !report.accepted)
    {
        return Err(format!(
            "skill_solidify_rejected: proposal {} validation not accepted",
            rejected.proposal_id
        ));
    }

    // Benchmark score gate (Penguin: no baseline -> no optimize; only a
    // strictly improving score may solidify a skill). Rejects before any
    // file write. approve/judge never reach this code path.
    let (benchmark_gate, benchmark_gate_passed, benchmark_best_score, benchmark_required_score) =
        match benchmark_gate {
            Some(gate_id) => {
                let root = benchmark_root
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| PathBuf::from(DEFAULT_BENCHMARK_ROOT));
                let board = BenchmarkStore::new(&root)
                    .load_scoreboard(gate_id)
                    .map_err(|err| format!("benchmark_gate_load_failed: {err}"))?;
                let outcome = enforce_benchmark_gate(&board, benchmark_after_score)?;
                (
                    Some(gate_id.to_string()),
                    outcome.passed,
                    outcome.best_score,
                    outcome.required_score,
                )
            }
            None => (None, false, None, None),
        };

    let solidify_tickets = review
        .proposals
        .iter()
        .zip(review.proposal_validation_reports)
        .map(|(proposal, validation)| {
            SkillSolidifyTicket::solidify_refusal_receipt(
                proposal,
                validation,
                approval_source.to_string(),
                approved_at.map(str::to_string),
                approval_note.map(str::to_string),
            )
        })
        .collect::<Vec<_>>();
    let write_receipts = review
        .proposals
        .iter()
        .zip(judgments.iter())
        .map(|(proposal, judgment)| {
            solidify_skill_file(
                skills_root,
                proposal,
                judgment,
                approval_source,
                approved_at,
                approval_note,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output = SkillSolidifyOutput {
        solidify_requested: true,
        solidify_allowed: true,
        self_scored: true,
        approval_policy: "darwin_style_cli_rubric".to_string(),
        approval_threshold,
        writes_skills: true,
        solidifies_skill: true,
        skills_root: skills_root.display().to_string(),
        judgment_count: judgments.len(),
        judgments,
        write_count: write_receipts.len(),
        write_receipts,
        solidify_receipt_count: solidify_tickets.len(),
        solidify_receipts: solidify_tickets,
        benchmark_gate: benchmark_gate.clone(),
        benchmark_gate_passed,
        benchmark_best_score,
        benchmark_required_score,
        boundary: SkillSolidifyBoundary {
            validates_proposal: true,
            self_scores_proposal: true,
            emits_solidify_receipt: true,
            reads_existing_skills: true,
            writes_skill_files: true,
            upserts_canonical_skill: true,
            solidifies_skill: true,
            enforces_benchmark_gate: benchmark_gate.is_some(),
            connects_llm: false,
            connects_external_service: false,
        },
    };

    Ok(output)
}

/// Build a minimal skill proposal request for the self-experiment closure.
/// The event/task ids derive from the experiment id so the solidified skill
/// can be traced back to the experiment report.
pub(crate) fn skill_propose_request_for_experiment(
    experiment_id: &str,
    summary: &str,
    agent_id: &str,
) -> SkillProposeRequest {
    SkillProposeRequest {
        output: ControlOutputFormat::Json,
        event_id: format!("experiment-{experiment_id}"),
        task_id: format!("experiment-{experiment_id}"),
        kind: RuntimeEventKind::ManualObservation,
        summary: summary.to_string(),
        metadata: BTreeMap::new(),
        agent_id: agent_id.to_string(),
        task_kind: Some("self-experiment".to_string()),
        max_proposals: 1,
    }
}

fn print_skill_solidify_text(output: &SkillSolidifyOutput) {
    println!(
        "skill_solidify solidify_requested=true solidify_allowed=true self_scored=true approval_policy={} approval_threshold={} writes_skills=true solidifies_skill=true skills_root={} judgments={} writes={} solidify_receipts={}",
        output.approval_policy,
        output.approval_threshold,
        output.skills_root,
        output.judgment_count,
        output.write_count,
        output.solidify_receipt_count
    );
    println!(
        "skill_solidify_gate benchmark_gate={} passed={} best_score={} required_score={}",
        output.benchmark_gate.as_deref().unwrap_or("none"),
        output.benchmark_gate_passed,
        output
            .benchmark_best_score
            .map(|score| score.to_string())
            .unwrap_or_else(|| "none".to_string()),
        output
            .benchmark_required_score
            .map(|score| score.to_string())
            .unwrap_or_else(|| "none".to_string()),
    );
    println!(
        "boundary validates_proposal=true self_scores_proposal=true emits_solidify_receipt=true reads_existing_skills=true writes_skill_files=true upserts_canonical_skill=true solidifies_skill=true enforces_benchmark_gate={} connects_llm=false connects_external_service=false",
        output.boundary.enforces_benchmark_gate
    );
    for judgment in &output.judgments {
        print_skill_judgment_text("judgment", judgment);
    }
    for receipt in &output.write_receipts {
        println!(
            "skill_write skill_id={} action={} duplicate_state={} path={} bytes_written={} status={} version={} provenance={}",
            receipt.skill_id,
            receipt.action,
            receipt.duplicate_state,
            receipt.path,
            receipt.bytes_written,
            receipt.status,
            receipt.version,
            receipt.provenance_event_ids.join(",")
        );
    }
    for receipt in &output.solidify_receipts {
        println!(
            "solidify_receipt id={} proposal_id={} approved={} source={} approved_at={} note={} dry_run={} writes_skills={} solidifies_skill={} local_only={}",
            receipt.ticket_id,
            receipt.proposal_id,
            receipt.approval_receipt.approved,
            receipt.approval_receipt.approval_source,
            receipt
                .approval_receipt
                .approved_at
                .as_deref()
                .unwrap_or("none"),
            receipt
                .approval_receipt
                .approval_note
                .as_deref()
                .unwrap_or("none"),
            receipt.dry_run,
            receipt.writes_skills,
            receipt.solidifies_skill,
            receipt.local_only
        );
    }
}

impl SkillSolidifyOutput {
    /// Compact gate summary for cross-command reporting without exposing
    /// private fields outside this module.
    pub(crate) fn benchmark_gate_summary(
        &self,
    ) -> (Option<&str>, bool, Option<u16>, Option<u16>, usize, &str) {
        (
            self.benchmark_gate.as_deref(),
            self.benchmark_gate_passed,
            self.benchmark_best_score,
            self.benchmark_required_score,
            self.write_count,
            self.skills_root.as_str(),
        )
    }
}

fn skill_retire_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_retire(args)?;
    let receipt = retire_skill_file(&request)?;
    let output = SkillRetireOutput {
        lifecycle_updated: true,
        writes_skill_files: true,
        deletes_skill_files: false,
        receipt,
        boundary: SkillRetireBoundary {
            reads_existing_skills: true,
            writes_skill_files: true,
            deletes_skill_files: false,
            connects_llm: false,
            connects_external_service: false,
        },
    };

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "skill_retire lifecycle_updated=true writes_skill_files=true deletes_skill_files=false skill_id={} status={} path={} reason={} previous_status={} bytes_written={}",
                output.receipt.skill_id,
                output.receipt.status,
                output.receipt.path,
                output.receipt.reason,
                output.receipt.previous_status.as_deref().unwrap_or("unknown"),
                output.receipt.bytes_written
            );
            println!(
                "boundary reads_existing_skills=true writes_skill_files=true deletes_skill_files=false connects_llm=false connects_external_service=false"
            );
        }
    }

    Ok(())
}

fn skill_monitor_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_monitor(args)?;
    let output = build_skill_monitor_output(&request.skills_root)?;

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "skill_monitor monitored=true skills_root={} skills={} active={} deprecated={} retired={} decay_candidates={} rollback_candidates={}",
                output.skills_root,
                output.skill_count,
                output.active_count,
                output.deprecated_count,
                output.retired_count,
                output.decay_candidate_count,
                output.rollback_candidate_count
            );
            println!(
                "boundary reads_existing_skills=true writes_skill_files=false emits_decay_candidates=true emits_rollback_candidates=true connects_llm=false connects_external_service=false"
            );
            for skill in &output.skills {
                println!(
                    "skill id={} status={} version={} path={} score={} snapshot={} decay_candidate={} rollback_available={}",
                    skill.skill_id,
                    skill.status,
                    skill.version,
                    skill.path,
                    skill.score.map(|value| value.to_string()).unwrap_or_else(|| "none".to_string()),
                    skill.has_previous_version_snapshot,
                    skill.decay_candidate,
                    skill.rollback_available
                );
            }
            // Curator footer: recommendations only — no automatic retire/write.
            println!(
                "curator_mode=read_only auto_write=false decay_candidates={} rollback_candidates={}",
                output.decay_candidate_count, output.rollback_candidate_count
            );
            if output.decay_candidate_count > 0 {
                println!(
                    "curator_hint: review decay_candidate=true skills; retire/deprecate only with explicit --reason (never auto)"
                );
            }
            if output.rollback_candidate_count > 0 {
                println!(
                    "curator_hint: rollback_available=true skills can restore previous snapshot via skill rollback"
                );
            }
            if output.skill_count == 0 {
                println!("curator_hint: skills root empty — nothing to curate");
            }
        }
    }

    Ok(())
}

fn skill_rollback_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_rollback(args)?;
    let receipt = rollback_skill_file(&request)?;
    let output = SkillRollbackOutput {
        lifecycle_updated: true,
        writes_skill_files: true,
        deletes_skill_files: false,
        receipt,
        boundary: SkillRollbackBoundary {
            reads_existing_skills: true,
            writes_skill_files: true,
            deletes_skill_files: false,
            restores_previous_version: true,
            connects_llm: false,
            connects_external_service: false,
        },
    };

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "skill_rollback lifecycle_updated=true writes_skill_files=true deletes_skill_files=false skill_id={} status={} previous_status={} version={} previous_version={} restored_from_snapshot={} reason={} path={}",
                output.receipt.skill_id,
                output.receipt.status,
                output.receipt.previous_status.as_deref().unwrap_or("unknown"),
                output.receipt.version,
                output.receipt.previous_version,
                output.receipt.restored_from_snapshot,
                output.receipt.reason,
                output.receipt.path
            );
            println!(
                "boundary reads_existing_skills=true writes_skill_files=true deletes_skill_files=false restores_previous_version=true connects_llm=false connects_external_service=false"
            );
        }
    }

    Ok(())
}

fn build_skill_review(request: &SkillProposeRequest) -> Result<SkillReviewBuild, String> {
    let mut evolver = DryRunProposalEvolver::new();
    evolver
        .observe(RuntimeEvent {
            event_id: request.event_id.clone(),
            task_id: request.task_id.clone(),
            kind: request.kind.clone(),
            summary: request.summary.clone(),
            metadata: request.metadata.clone(),
        })
        .map_err(|err| format!("skill_observe_failed: {err:?}"))?;
    let proposals = evolver
        .propose(EvolutionScope {
            agent_id: request.agent_id.clone(),
            task_kind: request.task_kind.clone(),
            max_proposals: request.max_proposals,
        })
        .map_err(|err| format!("skill_propose_failed: {err:?}"))?;
    let proposal_validation_reports = proposals
        .iter()
        .map(|proposal| {
            evolver
                .validate(proposal)
                .map_err(|err| format!("skill_validate_failed: {err:?}"))
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(SkillReviewBuild {
        proposals,
        proposal_validation_reports,
    })
}

fn parse_skill_propose(args: &[String]) -> Result<SkillProposeRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut event_id: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut kind = RuntimeEventKind::ManualObservation;
    let mut summary: Option<String> = None;
    let mut metadata = BTreeMap::new();
    let mut agent_id = "xiaoce".to_string();
    let mut task_kind: Option<String> = None;
    let mut max_proposals = 1usize;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--event-id" => {
                event_id = Some(take_value(args, &mut index, "--event-id")?);
            }
            "--task-id" => {
                task_id = Some(take_value(args, &mut index, "--task-id")?);
            }
            "--kind" => {
                kind = parse_event_kind(&take_value(args, &mut index, "--kind")?)?;
            }
            "--summary" => {
                summary = Some(take_value(args, &mut index, "--summary")?);
            }
            "--metadata" => {
                let value = take_value(args, &mut index, "--metadata")?;
                let (key, value) = value
                    .split_once('=')
                    .ok_or_else(|| "--metadata requires key=value".to_string())?;
                if key.trim().is_empty() {
                    return Err("--metadata key must not be empty".to_string());
                }
                metadata.insert(key.trim().to_string(), value.trim().to_string());
            }
            "--agent-id" => {
                agent_id = take_value(args, &mut index, "--agent-id")?;
            }
            "--task-kind" => {
                task_kind = Some(take_value(args, &mut index, "--task-kind")?);
            }
            "--max-proposals" => {
                let value = take_value(args, &mut index, "--max-proposals")?;
                max_proposals = value
                    .parse::<usize>()
                    .map_err(|_| "--max-proposals requires numeric value".to_string())?;
                if max_proposals == 0 {
                    return Err("--max-proposals must be greater than zero".to_string());
                }
            }
            _ => return Err(usage()),
        }
    }

    let event_id = require_non_empty("skill propose", "--event-id", event_id)?;
    let task_id = require_non_empty("skill propose", "--task-id", task_id)?;
    let summary = require_non_empty("skill propose", "--summary", summary)?;
    if agent_id.trim().is_empty() {
        return Err("skill propose --agent-id must not be empty".to_string());
    }
    if let Some(task_kind) = &task_kind {
        if task_kind.trim().is_empty() {
            return Err("skill propose --task-kind must not be empty".to_string());
        }
    }
    metadata
        .entry("task_kind".to_string())
        .or_insert_with(|| task_kind.clone().unwrap_or_else(|| "manual".to_string()));

    Ok(SkillProposeRequest {
        output,
        event_id,
        task_id,
        kind,
        summary,
        metadata,
        agent_id,
        task_kind,
        max_proposals,
    })
}

fn parse_skill_approve(args: &[String]) -> Result<SkillApproveRequest, String> {
    parse_skill_review_request(args, "skill approve", "cli skill approve", false)
}

fn parse_skill_judge(args: &[String]) -> Result<SkillApproveRequest, String> {
    parse_skill_review_request(args, "skill judge", "cli skill judge", false)
}

fn parse_skill_solidify(args: &[String]) -> Result<SkillApproveRequest, String> {
    parse_skill_review_request(args, "skill solidify", "cli skill solidify", true)
}

fn parse_skill_review_request(
    args: &[String],
    command_name: &str,
    default_approval_source: &str,
    allow_benchmark_flags: bool,
) -> Result<SkillApproveRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut event_id: Option<String> = None;
    let mut task_id: Option<String> = None;
    let mut kind = RuntimeEventKind::ManualObservation;
    let mut summary: Option<String> = None;
    let mut metadata = BTreeMap::new();
    let mut agent_id = "xiaoce".to_string();
    let mut task_kind: Option<String> = None;
    let mut max_proposals = 1usize;
    let mut approval_source = default_approval_source.to_string();
    let mut approved_at: Option<String> = None;
    let mut approval_note: Option<String> = None;
    let mut skills_root: Option<PathBuf> = None;
    let mut approval_threshold = 80u16;
    let mut benchmark_gate: Option<String> = None;
    let mut benchmark_after_score: Option<u16> = None;
    let mut benchmark_root: Option<PathBuf> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--event-id" => {
                event_id = Some(take_value(args, &mut index, "--event-id")?);
            }
            "--task-id" => {
                task_id = Some(take_value(args, &mut index, "--task-id")?);
            }
            "--kind" => {
                kind = parse_event_kind(&take_value(args, &mut index, "--kind")?)?;
            }
            "--summary" => {
                summary = Some(take_value(args, &mut index, "--summary")?);
            }
            "--metadata" => {
                let value = take_value(args, &mut index, "--metadata")?;
                let (key, value) = value
                    .split_once('=')
                    .ok_or_else(|| "--metadata requires key=value".to_string())?;
                if key.trim().is_empty() {
                    return Err("--metadata key must not be empty".to_string());
                }
                metadata.insert(key.trim().to_string(), value.trim().to_string());
            }
            "--agent-id" => {
                agent_id = take_value(args, &mut index, "--agent-id")?;
            }
            "--task-kind" => {
                task_kind = Some(take_value(args, &mut index, "--task-kind")?);
            }
            "--max-proposals" => {
                let value = take_value(args, &mut index, "--max-proposals")?;
                max_proposals = value
                    .parse::<usize>()
                    .map_err(|_| "--max-proposals requires numeric value".to_string())?;
                if max_proposals == 0 {
                    return Err("--max-proposals must be greater than zero".to_string());
                }
            }
            "--approval-source" => {
                approval_source = take_value(args, &mut index, "--approval-source")?;
                if approval_source.trim().is_empty() {
                    return Err(format!(
                        "{command_name} --approval-source must not be empty"
                    ));
                }
            }
            "--approved-at" => {
                approved_at = Some(take_value(args, &mut index, "--approved-at")?);
            }
            "--approval-note" => {
                approval_note = Some(take_value(args, &mut index, "--approval-note")?);
            }
            "--skills-root" => {
                let value = take_value(args, &mut index, "--skills-root")?;
                if value.trim().is_empty() {
                    return Err(format!("{command_name} --skills-root must not be empty"));
                }
                skills_root = Some(PathBuf::from(value));
            }
            "--approval-threshold" => {
                let value = take_value(args, &mut index, "--approval-threshold")?;
                approval_threshold = value
                    .parse::<u16>()
                    .map_err(|_| "--approval-threshold requires numeric value".to_string())?;
                if approval_threshold > 100 {
                    return Err("--approval-threshold must be <= 100".to_string());
                }
            }
            "--benchmark-gate" => {
                if !allow_benchmark_flags {
                    return Err(format!(
                        "{command_name} does not accept --benchmark-gate; only skill solidify supports benchmark gating"
                    ));
                }
                let value = take_value(args, &mut index, "--benchmark-gate")?;
                if value.trim().is_empty() {
                    return Err(format!("{command_name} --benchmark-gate must not be empty"));
                }
                benchmark_gate = Some(value);
            }
            "--benchmark-after-score" => {
                if !allow_benchmark_flags {
                    return Err(format!(
                        "{command_name} does not accept --benchmark-after-score; only skill solidify supports benchmark gating"
                    ));
                }
                let value = take_value(args, &mut index, "--benchmark-after-score")?;
                benchmark_after_score =
                    Some(value.parse::<u16>().map_err(|_| {
                        "--benchmark-after-score requires numeric value".to_string()
                    })?);
            }
            "--benchmark-root" => {
                if !allow_benchmark_flags {
                    return Err(format!(
                        "{command_name} does not accept --benchmark-root; only skill solidify supports benchmark gating"
                    ));
                }
                let value = take_value(args, &mut index, "--benchmark-root")?;
                if value.trim().is_empty() {
                    return Err(format!("{command_name} --benchmark-root must not be empty"));
                }
                benchmark_root = Some(PathBuf::from(value));
            }
            _ => return Err(usage()),
        }
    }

    let event_id = require_non_empty(command_name, "--event-id", event_id)?;
    let task_id = require_non_empty(command_name, "--task-id", task_id)?;
    let summary = require_non_empty(command_name, "--summary", summary)?;
    if agent_id.trim().is_empty() {
        return Err(format!("{command_name} --agent-id must not be empty"));
    }
    if let Some(task_kind) = &task_kind {
        if task_kind.trim().is_empty() {
            return Err(format!("{command_name} --task-kind must not be empty"));
        }
    }
    metadata
        .entry("task_kind".to_string())
        .or_insert_with(|| task_kind.clone().unwrap_or_else(|| "manual".to_string()));

    Ok(SkillApproveRequest {
        output,
        review: SkillProposeRequest {
            output,
            event_id,
            task_id,
            kind,
            summary,
            metadata,
            agent_id,
            task_kind,
            max_proposals,
        },
        approval_source,
        approved_at,
        approval_note,
        skills_root,
        approval_threshold,
        benchmark_gate,
        benchmark_after_score,
        benchmark_root,
    })
}

/// Outcome of a benchmark score gate check, used by skill solidify and by
/// `goal evolve --approve` post-write verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BenchmarkGateOutcome {
    pub(crate) passed: bool,
    pub(crate) best_score: Option<u16>,
    pub(crate) required_score: Option<u16>,
}

/// Penguin gate: no baseline -> no optimize; a skill may only solidify when
/// the submitted score strictly exceeds the recorded best. Pure function so
/// the rule is unit-testable without touching the filesystem.
pub(crate) fn enforce_benchmark_gate(
    board: &Scoreboard,
    after_score: Option<u16>,
) -> Result<BenchmarkGateOutcome, String> {
    let best = board
        .best
        .as_ref()
        .ok_or_else(|| "benchmark_gate_rejected: no best score recorded yet".to_string())?;
    let best_score = best.total_score;
    let required_score = after_score.ok_or_else(|| {
        "benchmark_gate_rejected: --benchmark-after-score required (no improvement proof -> no accept)"
            .to_string()
    })?;
    if required_score <= best_score {
        return Err(format!(
            "benchmark_gate_rejected: after_score {} does not strictly exceed best {} (no improvement -> no accept)",
            required_score, best_score
        ));
    }
    Ok(BenchmarkGateOutcome {
        passed: true,
        best_score: Some(best_score),
        required_score: Some(required_score),
    })
}

fn parse_skill_retire(args: &[String]) -> Result<SkillRetireRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut skills_root = default_skills_root();
    let mut skill_id: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut status = "deprecated".to_string();
    let mut retired_at: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--skills-root" => {
                let value = take_value(args, &mut index, "--skills-root")?;
                if value.trim().is_empty() {
                    return Err("skill retire --skills-root must not be empty".to_string());
                }
                skills_root = PathBuf::from(value);
            }
            "--skill-id" => {
                skill_id = Some(take_value(args, &mut index, "--skill-id")?);
            }
            "--reason" => {
                reason = Some(take_value(args, &mut index, "--reason")?);
            }
            "--status" => {
                status = take_value(args, &mut index, "--status")?;
                if status != "deprecated" && status != "retired" {
                    return Err("skill retire --status must be deprecated or retired".to_string());
                }
            }
            "--retired-at" => {
                retired_at = Some(take_value(args, &mut index, "--retired-at")?);
            }
            _ => return Err(usage()),
        }
    }

    let skill_id = require_non_empty("skill retire", "--skill-id", skill_id)?;
    if !is_safe_skill_id(&skill_id) {
        return Err(
            "skill retire --skill-id must be ascii letters, numbers, '-' or '_'".to_string(),
        );
    }
    let reason = require_non_empty("skill retire", "--reason", reason)?;

    Ok(SkillRetireRequest {
        output,
        skills_root,
        skill_id,
        reason,
        status,
        retired_at,
    })
}

fn parse_skill_monitor(args: &[String]) -> Result<SkillMonitorRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut skills_root = default_skills_root();

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--skills-root" => {
                let value = take_value(args, &mut index, "--skills-root")?;
                if value.trim().is_empty() {
                    return Err("skill monitor --skills-root must not be empty".to_string());
                }
                skills_root = PathBuf::from(value);
            }
            _ => return Err(usage()),
        }
    }

    Ok(SkillMonitorRequest {
        output,
        skills_root,
    })
}

fn parse_skill_rollback(args: &[String]) -> Result<SkillRollbackRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut skills_root = default_skills_root();
    let mut skill_id: Option<String> = None;
    let mut reason: Option<String> = None;
    let mut rollback_at: Option<String> = None;

    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--skills-root" => {
                let value = take_value(args, &mut index, "--skills-root")?;
                if value.trim().is_empty() {
                    return Err("skill rollback --skills-root must not be empty".to_string());
                }
                skills_root = PathBuf::from(value);
            }
            "--skill-id" => {
                skill_id = Some(take_value(args, &mut index, "--skill-id")?);
            }
            "--reason" => {
                reason = Some(take_value(args, &mut index, "--reason")?);
            }
            "--rollback-at" => {
                rollback_at = Some(take_value(args, &mut index, "--rollback-at")?);
            }
            _ => return Err(usage()),
        }
    }

    let skill_id = require_non_empty("skill rollback", "--skill-id", skill_id)?;
    if !is_safe_skill_id(&skill_id) {
        return Err(
            "skill rollback --skill-id must be ascii letters, numbers, '-' or '_'".to_string(),
        );
    }
    let reason = require_non_empty("skill rollback", "--reason", reason)?;

    Ok(SkillRollbackRequest {
        output,
        skills_root,
        skill_id,
        reason,
        rollback_at,
    })
}

fn parse_event_kind(raw: &str) -> Result<RuntimeEventKind, String> {
    match raw {
        "turn_completed" => Ok(RuntimeEventKind::TurnCompleted),
        "tool_succeeded" => Ok(RuntimeEventKind::ToolSucceeded),
        "tool_failed" => Ok(RuntimeEventKind::ToolFailed),
        "user_correction" => Ok(RuntimeEventKind::UserCorrection),
        "manual_observation" => Ok(RuntimeEventKind::ManualObservation),
        _ => Err(format!("unsupported skill event kind: {raw}")),
    }
}

fn take_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires value"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn require_non_empty(
    command_name: &str,
    flag: &str,
    value: Option<String>,
) -> Result<String, String> {
    let value = value.ok_or_else(|| format!("{command_name} requires {flag}"))?;
    if value.trim().is_empty() {
        return Err(format!("{command_name} {flag} must not be empty"));
    }
    Ok(value)
}

pub(crate) fn build_skill_judgments(
    review: &SkillReviewBuild,
    threshold: u16,
    skills_root: Option<&Path>,
) -> Vec<SkillJudgmentOutput> {
    review
        .proposals
        .iter()
        .zip(review.proposal_validation_reports.iter())
        .map(|(proposal, validation)| {
            let canonical_skill_id = canonical_skill_id(proposal);
            let target_path = skills_root.map(|root| root.join(format!("{canonical_skill_id}.md")));
            let duplicate_state = target_path
                .as_ref()
                .map(|path| {
                    if path.exists() {
                        "updates_existing".to_string()
                    } else {
                        "new_canonical_skill".to_string()
                    }
                })
                .unwrap_or_else(|| "not_checked".to_string());
            let rubric_scores = score_skill_proposal(proposal, validation);
            let score_total = rubric_scores.iter().map(|score| score.score).sum::<u16>();
            SkillJudgmentOutput {
                proposal_id: proposal.proposal_id.clone(),
                canonical_skill_id,
                approved: validation.accepted && score_total >= threshold,
                score_total,
                threshold,
                policy: "darwin_style_cli_rubric".to_string(),
                duplicate_state,
                target_path: target_path.map(|path| path.display().to_string()),
                reasons: build_judgment_reasons(validation, score_total, threshold),
                rubric_scores,
            }
        })
        .collect()
}

fn score_skill_proposal(
    proposal: &SkillProposal,
    validation: &ValidationReport,
) -> Vec<SkillRubricScoreOutput> {
    let accepted_bonus = if validation.accepted { 1 } else { 0 };
    vec![
        SkillRubricScoreOutput::new(
            "frontmatter_quality",
            12,
            if proposal.title.trim().is_empty() {
                4
            } else {
                11
            },
            "generated canonical metadata is available",
        ),
        SkillRubricScoreOutput::new(
            "workflow_clarity",
            16,
            if proposal.procedure.len() >= 2 { 15 } else { 8 },
            "proposal carries ordered procedure steps",
        ),
        SkillRubricScoreOutput::new(
            "boundary_coverage",
            14,
            13,
            "generated skill keeps governance, memory, and secret boundaries explicit",
        ),
        SkillRubricScoreOutput::new(
            "checkpoint_design",
            12,
            10 + accepted_bonus,
            "solidify writes provenance and lifecycle metadata for later review",
        ),
        SkillRubricScoreOutput::new(
            "instruction_specificity",
            14,
            if proposal.trigger.trim().is_empty() {
                7
            } else {
                13
            },
            "trigger and procedure are concrete enough for reuse",
        ),
        SkillRubricScoreOutput::new(
            "resource_integration",
            10,
            if proposal.evidence_event_ids.is_empty() {
                5
            } else {
                9
            },
            "source event ids are preserved as provenance",
        ),
        SkillRubricScoreOutput::new(
            "overall_architecture",
            12,
            11,
            "canonical id prevents duplicate skill creation",
        ),
        SkillRubricScoreOutput::new(
            "real_world_test_performance",
            10,
            if validation.accepted { 8 } else { 3 },
            "CLI validation is a local smoke gate before solidify",
        ),
    ]
}

fn build_judgment_reasons(
    validation: &ValidationReport,
    score_total: u16,
    threshold: u16,
) -> Vec<String> {
    let mut reasons = validation.reasons.clone();
    if score_total >= threshold {
        reasons.push(format!(
            "self score {score_total} meets approval threshold {threshold}"
        ));
    } else {
        reasons.push(format!(
            "self score {score_total} is below approval threshold {threshold}"
        ));
    }
    reasons.push("canonical skill id selected before write to prevent duplicates".to_string());
    reasons
}

pub(crate) fn solidify_skill_file(
    skills_root: &Path,
    proposal: &SkillProposal,
    judgment: &SkillJudgmentOutput,
    approval_source: &str,
    approved_at: Option<&str>,
    approval_note: Option<&str>,
) -> Result<SkillWriteReceiptOutput, String> {
    fs::create_dir_all(skills_root)
        .map_err(|err| format!("skill_solidify_create_root_failed: {err}"))?;
    let path = skill_path(skills_root, &judgment.canonical_skill_id)?;
    let existed = path.exists();
    let previous_content = if existed {
        Some(
            fs::read_to_string(&path)
                .map_err(|err| format!("skill_solidify_read_existing_failed: {err}"))?,
        )
    } else {
        None
    };
    let previous_version = previous_content
        .as_deref()
        .and_then(extract_skill_version)
        .unwrap_or(0);
    let version = previous_version + 1;
    let content = render_skill_markdown(
        proposal,
        judgment,
        version,
        approval_source,
        approved_at,
        approval_note,
        previous_content.as_deref(),
    );
    fs::write(&path, content.as_bytes())
        .map_err(|err| format!("skill_solidify_write_failed: {err}"))?;

    Ok(SkillWriteReceiptOutput {
        skill_id: judgment.canonical_skill_id.clone(),
        action: if existed {
            "updated".to_string()
        } else {
            "created".to_string()
        },
        duplicate_state: if existed {
            "updated_existing_canonical_skill".to_string()
        } else {
            "created_new_canonical_skill".to_string()
        },
        path: path.display().to_string(),
        bytes_written: content.len(),
        status: "active".to_string(),
        version,
        provenance_event_ids: proposal.evidence_event_ids.clone(),
    })
}

fn retire_skill_file(request: &SkillRetireRequest) -> Result<SkillRetireReceiptOutput, String> {
    let path = skill_path(&request.skills_root, &request.skill_id)?;
    if !path.exists() {
        return Err(format!("skill_retire_not_found: {}", path.display()));
    }
    let previous_content =
        fs::read_to_string(&path).map_err(|err| format!("skill_retire_read_failed: {err}"))?;
    let previous_status = extract_lifecycle_value(&previous_content, "status");
    let version = extract_skill_version(&previous_content).unwrap_or(0) + 1;
    let content = render_retired_skill_markdown(
        &request.skill_id,
        &previous_content,
        &request.status,
        &request.reason,
        request.retired_at.as_deref(),
        version,
    );
    fs::write(&path, content.as_bytes())
        .map_err(|err| format!("skill_retire_write_failed: {err}"))?;

    Ok(SkillRetireReceiptOutput {
        skill_id: request.skill_id.clone(),
        status: request.status.clone(),
        reason: request.reason.clone(),
        path: path.display().to_string(),
        previous_status,
        bytes_written: content.len(),
        version,
    })
}

fn render_skill_markdown(
    proposal: &SkillProposal,
    judgment: &SkillJudgmentOutput,
    version: u32,
    approval_source: &str,
    approved_at: Option<&str>,
    approval_note: Option<&str>,
    previous_content: Option<&str>,
) -> String {
    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&format!("skill_id: {}\n", judgment.canonical_skill_id));
    content.push_str(&format!(
        "title: {}\n",
        sanitize_yaml_scalar(&proposal.title)
    ));
    content.push_str("status: active\n");
    content.push_str(&format!("version: {version}\n"));
    content.push_str("approval_policy: darwin_style_cli_rubric\n");
    content.push_str(&format!("score_total: {}\n", judgment.score_total));
    content.push_str(&format!("approval_threshold: {}\n", judgment.threshold));
    content.push_str(&format!(
        "approval_source: {}\n",
        sanitize_yaml_scalar(approval_source)
    ));
    content.push_str(&format!(
        "approved_at: {}\n",
        approved_at.unwrap_or("unspecified")
    ));
    content.push_str(&format!(
        "approval_note: {}\n",
        sanitize_yaml_scalar(approval_note.unwrap_or("none"))
    ));
    content.push_str(&format!(
        "provenance_event_ids: {}\n",
        proposal.evidence_event_ids.join(",")
    ));
    content.push_str("---\n\n");
    content.push_str(&format!("# {}\n\n", proposal.title));
    content.push_str("## Lifecycle\n\n");
    content.push_str(&format!("- skill_id: `{}`\n", judgment.canonical_skill_id));
    content.push_str("- status: `active`\n");
    content.push_str(&format!("- version: `{version}`\n"));
    content.push_str(&format!(
        "- self_score: `{}` / `100`, threshold `{}`\n",
        judgment.score_total, judgment.threshold
    ));
    content.push_str("- duplicate_policy: `upsert_canonical_skill_id`\n\n");
    content.push_str("## When To Use\n\n");
    content.push_str(&format!("{}\n\n", proposal.trigger));
    content.push_str("## Procedure\n\n");
    for (index, step) in proposal.procedure.iter().enumerate() {
        content.push_str(&format!("{}. {}\n", index + 1, step));
    }
    content.push_str("\n## Boundaries\n\n");
    content.push_str("- Keep governance mandatory for risky actions.\n");
    content.push_str("- Do not write core memory directly from skill output.\n");
    content.push_str("- Do not expose secrets, tokens, credentials, or private env values.\n");
    content.push_str("- Prefer updating this canonical skill over creating duplicates.\n\n");
    content.push_str("## Rubric\n\n");
    for score in &judgment.rubric_scores {
        content.push_str(&format!(
            "- {}: {}/{} - {}\n",
            score.dimension, score.score, score.max_score, score.reason
        ));
    }
    content.push_str("\n## Provenance\n\n");
    for source in &proposal.provenance {
        content.push_str(&format!(
            "- event_id: `{}` task_id: `{}` kind: `{:?}` summary: {}\n",
            source.source_event_id,
            source.source_task_id,
            source.source_kind,
            source.source_summary
        ));
    }
    if let Some(previous_content) = previous_content {
        if !previous_content.trim().is_empty() {
            content.push_str("\n## Previous Version Snapshot\n\n");
            content.push_str(&render_previous_version_snapshot(previous_content));
        }
    }
    content
}

fn render_retired_skill_markdown(
    skill_id: &str,
    previous_content: &str,
    status: &str,
    reason: &str,
    retired_at: Option<&str>,
    version: u32,
) -> String {
    let body = strip_frontmatter(previous_content);
    let mut content = String::new();
    content.push_str("---\n");
    content.push_str(&format!("skill_id: {skill_id}\n"));
    content.push_str(&format!("status: {status}\n"));
    content.push_str(&format!("version: {version}\n"));
    content.push_str(&format!(
        "retired_at: {}\n",
        retired_at.unwrap_or("unspecified")
    ));
    content.push_str(&format!(
        "retirement_reason: {}\n",
        sanitize_yaml_scalar(reason)
    ));
    content.push_str("deletes_skill_file: false\n");
    content.push_str("---\n\n");
    content.push_str(&format!(
        "> Lifecycle notice: this skill is `{status}`. Reason: {reason}. The file is preserved for audit and possible future restoration.\n\n"
    ));
    content.push_str(body.trim_start());
    if !previous_content.trim().is_empty() {
        content.push_str("\n\n## Previous Version Snapshot\n\n");
        content.push_str(&render_previous_version_snapshot(previous_content));
    }
    content
}

const SKILL_SNAPSHOT_BEGIN: &str = "<<<CHUANG-SNAPSHOT-BEGIN>>>";
const SKILL_SNAPSHOT_END: &str = "<<<CHUANG-SNAPSHOT-END>>>";

fn render_previous_version_snapshot(previous_content: &str) -> String {
    let encoded =
        serde_json::to_string(previous_content.trim_end()).unwrap_or_else(|_| "\"\"".to_string());
    format!(
        "{begin}\n{encoded}\n{end}\n",
        begin = SKILL_SNAPSHOT_BEGIN,
        end = SKILL_SNAPSHOT_END
    )
}

fn extract_previous_version_snapshot(content: &str) -> Option<String> {
    let begin = content.find(SKILL_SNAPSHOT_BEGIN)? + SKILL_SNAPSHOT_BEGIN.len();
    let after_begin = content[begin..]
        .strip_prefix('\n')
        .unwrap_or(&content[begin..]);
    let end = after_begin.find(SKILL_SNAPSHOT_END)?;
    let encoded = after_begin[..end].trim();
    serde_json::from_str(encoded).ok()
}

fn rollback_skill_file(
    request: &SkillRollbackRequest,
) -> Result<SkillRollbackReceiptOutput, String> {
    let path = skill_path(&request.skills_root, &request.skill_id)?;
    if !path.exists() {
        return Err(format!("skill_rollback_not_found: {}", path.display()));
    }
    let current_content =
        fs::read_to_string(&path).map_err(|err| format!("skill_rollback_read_failed: {err}"))?;
    let restored_content = extract_previous_version_snapshot(&current_content)
        .ok_or_else(|| format!("skill_rollback_snapshot_missing: {}", path.display()))?;
    let current_version = extract_skill_version(&current_content).unwrap_or(0);
    let source_version = extract_skill_version(&restored_content).unwrap_or(0);
    let restored_version = current_version + 1;
    let mut rewritten = rewrite_skill_frontmatter_for_rollback(
        &restored_content,
        restored_version,
        source_version,
        &request.reason,
        request.rollback_at.as_deref(),
        current_version,
    )?;
    if !current_content.trim().is_empty() {
        rewritten.push_str("\n## Previous Version Snapshot\n\n");
        rewritten.push_str(&render_previous_version_snapshot(&current_content));
    }
    fs::write(&path, rewritten.as_bytes())
        .map_err(|err| format!("skill_rollback_write_failed: {err}"))?;

    Ok(SkillRollbackReceiptOutput {
        skill_id: request.skill_id.clone(),
        path: path.display().to_string(),
        previous_status: extract_lifecycle_value(&current_content, "status"),
        status: "active".to_string(),
        reason: request.reason.clone(),
        previous_version: current_version,
        source_version,
        version: restored_version,
        restored_from_snapshot: true,
        bytes_written: rewritten.len(),
    })
}

fn build_skill_monitor_output(skills_root: &Path) -> Result<SkillMonitorOutput, String> {
    let mut skills = Vec::new();
    if skills_root.exists() {
        for entry in fs::read_dir(skills_root)
            .map_err(|err| format!("skill_monitor_read_dir_failed: {err}"))?
        {
            let entry = entry.map_err(|err| format!("skill_monitor_read_dir_failed: {err}"))?;
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("md") {
                continue;
            }
            let content = fs::read_to_string(&path)
                .map_err(|err| format!("skill_monitor_read_failed: {err}"))?;
            skills.push(build_skill_monitor_entry(&path, &content));
        }
    }
    skills.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
    let active_count = skills
        .iter()
        .filter(|skill| skill.status == "active")
        .count();
    let deprecated_count = skills
        .iter()
        .filter(|skill| skill.status == "deprecated")
        .count();
    let retired_count = skills
        .iter()
        .filter(|skill| skill.status == "retired")
        .count();
    let decay_candidate_count = skills.iter().filter(|skill| skill.decay_candidate).count();
    let rollback_candidate_count = skills
        .iter()
        .filter(|skill| skill.rollback_available)
        .count();

    Ok(SkillMonitorOutput {
        monitored: true,
        skills_root: skills_root.display().to_string(),
        skill_count: skills.len(),
        active_count,
        deprecated_count,
        retired_count,
        decay_candidate_count,
        rollback_candidate_count,
        skills,
        boundary: SkillMonitorBoundary {
            reads_existing_skills: true,
            writes_skill_files: false,
            emits_decay_candidates: true,
            emits_rollback_candidates: true,
            connects_llm: false,
            connects_external_service: false,
        },
    })
}

fn build_skill_monitor_entry(path: &Path, content: &str) -> SkillMonitorEntry {
    let skill_id = extract_lifecycle_value(content, "skill_id").unwrap_or_else(|| {
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("skill")
            .to_string()
    });
    let status = extract_lifecycle_value(content, "status").unwrap_or_else(|| "active".to_string());
    let version = extract_skill_version(content).unwrap_or(0);
    let score = extract_lifecycle_value(content, "score")
        .or_else(|| extract_lifecycle_value(content, "retirement_score"))
        .and_then(|raw| raw.parse::<u16>().ok());
    let has_previous_version_snapshot = extract_previous_version_snapshot(content).is_some();
    let decay_candidate = status != "active" || score.map_or(false, |score| score < 75);
    let rollback_available = has_previous_version_snapshot;

    SkillMonitorEntry {
        skill_id,
        path: path.display().to_string(),
        status,
        version,
        score,
        has_previous_version_snapshot,
        decay_candidate,
        rollback_available,
    }
}

fn rewrite_skill_frontmatter_for_rollback(
    content: &str,
    version: u32,
    source_version: u32,
    reason: &str,
    rollback_at: Option<&str>,
    previous_version: u32,
) -> Result<String, String> {
    let mut lines = content.lines().map(String::from).collect::<Vec<_>>();
    if lines.first().map(|line| line.as_str()) != Some("---") {
        return Err("skill_rollback_snapshot_missing_frontmatter".to_string());
    }

    let mut saw_status = false;
    let mut saw_version = false;
    let mut insert_at = None;
    for (index, line) in lines.iter_mut().enumerate().skip(1) {
        if line == "---" {
            insert_at = Some(index);
            break;
        }
        if let Some((key, _)) = line.split_once(':') {
            match key.trim() {
                "status" => {
                    *line = "status: active".to_string();
                    saw_status = true;
                }
                "version" => {
                    *line = format!("version: {version}");
                    saw_version = true;
                }
                _ => {}
            }
        }
    }
    let insert_at = insert_at
        .ok_or_else(|| "skill_rollback_snapshot_missing_frontmatter_closure".to_string())?;

    let mut additions = Vec::new();
    if !saw_status {
        additions.push("status: active".to_string());
    }
    if !saw_version {
        additions.push(format!("version: {version}"));
    }
    additions.push(format!("rollback_reason: {}", sanitize_yaml_scalar(reason)));
    additions.push(format!("rollback_from_version: {previous_version}"));
    additions.push(format!("rollback_source_version: {source_version}"));
    if let Some(rollback_at) = rollback_at {
        additions.push(format!(
            "rollback_at: {}",
            sanitize_yaml_scalar(rollback_at)
        ));
    }

    let mut has_version_inserted = false;
    let mut has_status_inserted = false;
    for addition in additions.into_iter().rev() {
        if addition.starts_with("version:") {
            has_version_inserted = true;
        }
        if addition.starts_with("status:") {
            has_status_inserted = true;
        }
        lines.insert(insert_at, addition);
    }

    if !has_status_inserted && !saw_status {
        lines.insert(insert_at, "status: active".to_string());
    }
    if !has_version_inserted && !saw_version {
        lines.insert(insert_at, format!("version: {version}"));
    }

    let mut updated = lines.join("\n");
    updated.push('\n');
    Ok(updated)
}

fn strip_frontmatter(content: &str) -> &str {
    if let Some(rest) = content.strip_prefix("---\n") {
        if let Some(end) = rest.find("\n---\n") {
            return &rest[end + "\n---\n".len()..];
        }
    }
    content
}

fn extract_skill_version(content: &str) -> Option<u32> {
    extract_lifecycle_value(content, "version").and_then(|raw| raw.parse::<u32>().ok())
}

fn extract_lifecycle_value(content: &str, key: &str) -> Option<String> {
    content.lines().find_map(|line| {
        let (line_key, value) = line.split_once(':')?;
        if line_key.trim() == key {
            Some(value.trim().trim_matches('"').to_string())
        } else {
            None
        }
    })
}

fn print_skill_judgment_text(prefix: &str, judgment: &SkillJudgmentOutput) {
    println!(
        "{} proposal_id={} skill_id={} approved={} score_total={} threshold={} duplicate_state={} target_path={} reasons={}",
        prefix,
        judgment.proposal_id,
        judgment.canonical_skill_id,
        judgment.approved,
        judgment.score_total,
        judgment.threshold,
        judgment.duplicate_state,
        judgment.target_path.as_deref().unwrap_or("none"),
        judgment.reasons.join("|")
    );
    for score in &judgment.rubric_scores {
        println!(
            "rubric proposal_id={} dimension={} score={} max_score={} reason={}",
            judgment.proposal_id, score.dimension, score.score, score.max_score, score.reason
        );
    }
}

fn canonical_skill_id(proposal: &SkillProposal) -> String {
    let slug = slugify(&proposal.title);
    if slug.is_empty() {
        slugify(&proposal.proposal_id)
    } else {
        slug
    }
}

fn slugify(raw: &str) -> String {
    let mut slug = String::new();
    let mut last_was_separator = false;
    for ch in raw.chars().flat_map(char::to_lowercase) {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch);
            last_was_separator = false;
        } else if !last_was_separator && !slug.is_empty() {
            slug.push('_');
            last_was_separator = true;
        }
    }
    while slug.ends_with('_') {
        slug.pop();
    }
    slug
}

fn skill_path(skills_root: &Path, skill_id: &str) -> Result<PathBuf, String> {
    if !is_safe_skill_id(skill_id) {
        return Err("skill_id must be ascii letters, numbers, '-' or '_'".to_string());
    }
    Ok(skills_root.join(format!("{skill_id}.md")))
}

fn is_safe_skill_id(skill_id: &str) -> bool {
    !skill_id.trim().is_empty()
        && skill_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
}

pub(crate) fn default_skills_root() -> PathBuf {
    PathBuf::from("data/skills")
}

fn sanitize_yaml_scalar(raw: &str) -> String {
    raw.replace('\n', " ").replace('\r', " ")
}

pub(crate) struct SkillProposeRequest {
    output: ControlOutputFormat,
    event_id: String,
    task_id: String,
    kind: RuntimeEventKind,
    summary: String,
    metadata: BTreeMap<String, String>,
    agent_id: String,
    task_kind: Option<String>,
    max_proposals: usize,
}

struct SkillApproveRequest {
    output: ControlOutputFormat,
    review: SkillProposeRequest,
    approval_source: String,
    approved_at: Option<String>,
    approval_note: Option<String>,
    skills_root: Option<PathBuf>,
    approval_threshold: u16,
    benchmark_gate: Option<String>,
    benchmark_after_score: Option<u16>,
    benchmark_root: Option<PathBuf>,
}

struct SkillRetireRequest {
    output: ControlOutputFormat,
    skills_root: PathBuf,
    skill_id: String,
    reason: String,
    status: String,
    retired_at: Option<String>,
}

struct SkillMonitorRequest {
    output: ControlOutputFormat,
    skills_root: PathBuf,
}

struct SkillRollbackRequest {
    output: ControlOutputFormat,
    skills_root: PathBuf,
    skill_id: String,
    reason: String,
    rollback_at: Option<String>,
}

pub(crate) struct SkillReviewBuild {
    pub(crate) proposals: Vec<SkillProposal>,
    pub(crate) proposal_validation_reports: Vec<ValidationReport>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillProposeOutput {
    dry_run: bool,
    writes_skills: bool,
    requires_approval: bool,
    approval_boundary_explicit: bool,
    proposal_count: usize,
    validation_count: usize,
    approval_ticket_count: usize,
    validation_accepted_count: usize,
    proposals: Vec<SkillProposal>,
    proposal_validations: Vec<SkillProposalValidationOutput>,
    approval_tickets: Vec<SkillSolidifyTicket>,
    boundary: SkillProposeBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillApproveOutput {
    approved: bool,
    self_scored: bool,
    approval_policy: String,
    approval_threshold: u16,
    writes_skills: bool,
    solidifies_skill: bool,
    judgment_count: usize,
    judgments: Vec<SkillJudgmentOutput>,
    approval_receipt_count: usize,
    approval_receipts: Vec<SkillSolidifyTicket>,
    boundary: SkillApproveBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillApproveBoundary {
    validates_proposal: bool,
    self_scores_proposal: bool,
    emits_approval_receipt: bool,
    writes_skill_files: bool,
    solidifies_skill: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillJudgeOutput {
    judged: bool,
    self_scored: bool,
    approval_policy: String,
    approval_threshold: u16,
    proposal_count: usize,
    judgment_count: usize,
    approved_count: usize,
    writes_skills: bool,
    solidifies_skill: bool,
    skills_root: String,
    judgments: Vec<SkillJudgmentOutput>,
    boundary: SkillJudgeBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillJudgeBoundary {
    validates_proposal: bool,
    self_scores_proposal: bool,
    reads_existing_skills: bool,
    writes_skill_files: bool,
    solidifies_skill: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SkillSolidifyOutput {
    solidify_requested: bool,
    solidify_allowed: bool,
    self_scored: bool,
    approval_policy: String,
    approval_threshold: u16,
    writes_skills: bool,
    solidifies_skill: bool,
    skills_root: String,
    judgment_count: usize,
    judgments: Vec<SkillJudgmentOutput>,
    write_count: usize,
    write_receipts: Vec<SkillWriteReceiptOutput>,
    solidify_receipt_count: usize,
    solidify_receipts: Vec<SkillSolidifyTicket>,
    benchmark_gate: Option<String>,
    benchmark_gate_passed: bool,
    benchmark_best_score: Option<u16>,
    benchmark_required_score: Option<u16>,
    boundary: SkillSolidifyBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillSolidifyBoundary {
    validates_proposal: bool,
    self_scores_proposal: bool,
    emits_solidify_receipt: bool,
    reads_existing_skills: bool,
    writes_skill_files: bool,
    upserts_canonical_skill: bool,
    solidifies_skill: bool,
    enforces_benchmark_gate: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRetireOutput {
    lifecycle_updated: bool,
    writes_skill_files: bool,
    deletes_skill_files: bool,
    receipt: SkillRetireReceiptOutput,
    boundary: SkillRetireBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRetireBoundary {
    reads_existing_skills: bool,
    writes_skill_files: bool,
    deletes_skill_files: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SkillJudgmentOutput {
    pub(crate) proposal_id: String,
    pub(crate) canonical_skill_id: String,
    pub(crate) approved: bool,
    pub(crate) score_total: u16,
    pub(crate) threshold: u16,
    pub(crate) policy: String,
    pub(crate) duplicate_state: String,
    pub(crate) target_path: Option<String>,
    pub(crate) reasons: Vec<String>,
    pub(crate) rubric_scores: Vec<SkillRubricScoreOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SkillRubricScoreOutput {
    pub(crate) dimension: String,
    pub(crate) score: u16,
    pub(crate) max_score: u16,
    pub(crate) reason: String,
}

impl SkillRubricScoreOutput {
    fn new(dimension: &str, max_score: u16, score: u16, reason: &str) -> Self {
        Self {
            dimension: dimension.to_string(),
            score,
            max_score,
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SkillWriteReceiptOutput {
    pub(crate) skill_id: String,
    pub(crate) action: String,
    pub(crate) duplicate_state: String,
    pub(crate) path: String,
    pub(crate) bytes_written: usize,
    pub(crate) status: String,
    pub(crate) version: u32,
    pub(crate) provenance_event_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRetireReceiptOutput {
    skill_id: String,
    status: String,
    reason: String,
    path: String,
    previous_status: Option<String>,
    bytes_written: usize,
    version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillMonitorOutput {
    monitored: bool,
    skills_root: String,
    skill_count: usize,
    active_count: usize,
    deprecated_count: usize,
    retired_count: usize,
    decay_candidate_count: usize,
    rollback_candidate_count: usize,
    skills: Vec<SkillMonitorEntry>,
    boundary: SkillMonitorBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillMonitorBoundary {
    reads_existing_skills: bool,
    writes_skill_files: bool,
    emits_decay_candidates: bool,
    emits_rollback_candidates: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillMonitorEntry {
    skill_id: String,
    path: String,
    status: String,
    version: u32,
    score: Option<u16>,
    has_previous_version_snapshot: bool,
    decay_candidate: bool,
    rollback_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRollbackOutput {
    lifecycle_updated: bool,
    writes_skill_files: bool,
    deletes_skill_files: bool,
    receipt: SkillRollbackReceiptOutput,
    boundary: SkillRollbackBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRollbackBoundary {
    reads_existing_skills: bool,
    writes_skill_files: bool,
    deletes_skill_files: bool,
    restores_previous_version: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRollbackReceiptOutput {
    skill_id: String,
    status: String,
    reason: String,
    path: String,
    previous_status: Option<String>,
    previous_version: u32,
    source_version: u32,
    version: u32,
    restored_from_snapshot: bool,
    bytes_written: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillProposalValidationOutput {
    proposal_id: String,
    accepted: bool,
    reasons: Vec<String>,
}

impl From<ValidationReport> for SkillProposalValidationOutput {
    fn from(report: ValidationReport) -> Self {
        Self {
            proposal_id: report.proposal_id,
            accepted: report.accepted,
            reasons: report.reasons,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillProposeBoundary {
    observes_runtime_event: bool,
    reads_existing_skills: bool,
    writes_skill_files: bool,
    solidifies_skill: bool,
    emits_approval_ticket: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chuang_agent::benchmark::{CaseScore, ScoreEntry};

    fn scoreboard_with_best(total: u16) -> Scoreboard {
        Scoreboard {
            benchmark_id: "memory-recall".to_string(),
            version: 1,
            best: Some(ScoreEntry {
                run_id: "run-1".to_string(),
                benchmark_id: "memory-recall".to_string(),
                version: 1,
                tested_at: "2026-08-10".to_string(),
                case_scores: vec![CaseScore {
                    case_id: "case-1".to_string(),
                    score: total,
                    max_score: total,
                    reason: "baseline".to_string(),
                }],
                total_score: total,
                max_score: total,
            }),
            latest: None,
            history: vec![],
        }
    }

    #[test]
    fn benchmark_gate_rejects_when_no_best_score() {
        let board = Scoreboard::default();
        let err = enforce_benchmark_gate(&board, Some(5)).unwrap_err();
        assert!(err.contains("no best score"));
    }

    #[test]
    fn benchmark_gate_rejects_without_after_score() {
        let board = scoreboard_with_best(4);
        let err = enforce_benchmark_gate(&board, None).unwrap_err();
        assert!(err.contains("--benchmark-after-score required"));
    }

    #[test]
    fn benchmark_gate_rejects_without_strict_improvement() {
        let board = scoreboard_with_best(4);
        let err = enforce_benchmark_gate(&board, Some(4)).unwrap_err();
        assert!(err.contains("does not strictly exceed"));
        let err = enforce_benchmark_gate(&board, Some(3)).unwrap_err();
        assert!(err.contains("does not strictly exceed"));
    }

    #[test]
    fn benchmark_gate_passes_on_strict_improvement() {
        let board = scoreboard_with_best(4);
        let outcome = enforce_benchmark_gate(&board, Some(5)).unwrap();
        assert!(outcome.passed);
        assert_eq!(outcome.best_score, Some(4));
        assert_eq!(outcome.required_score, Some(5));
    }
}
