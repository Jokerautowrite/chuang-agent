use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver,
    SkillProposal, SkillSolidifyTicket, ValidationReport,
};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn skill_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("propose") => skill_propose_command(&args[1..]),
        Some("judge") => skill_judge_command(&args[1..]),
        Some("approve") => skill_approve_command(&args[1..]),
        Some("solidify") => skill_solidify_command(&args[1..]),
        Some("retire") | Some("deprecate") => skill_retire_command(&args[1..]),
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
    let review = build_skill_review(&request.review)?;
    let skills_root = request
        .skills_root
        .clone()
        .unwrap_or_else(default_skills_root);
    let judgments = build_skill_judgments(&review, request.approval_threshold, Some(&skills_root));
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

    let solidify_tickets = review
        .proposals
        .iter()
        .zip(review.proposal_validation_reports)
        .map(|(proposal, validation)| {
            SkillSolidifyTicket::solidify_refusal_receipt(
                proposal,
                validation,
                request.approval_source.clone(),
                request.approved_at.clone(),
                request.approval_note.clone(),
            )
        })
        .collect::<Vec<_>>();
    let write_receipts = review
        .proposals
        .iter()
        .zip(judgments.iter())
        .map(|(proposal, judgment)| {
            solidify_skill_file(
                &skills_root,
                proposal,
                judgment,
                &request.approval_source,
                request.approved_at.as_deref(),
                request.approval_note.as_deref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let output = SkillSolidifyOutput {
        solidify_requested: true,
        solidify_allowed: true,
        self_scored: true,
        approval_policy: "darwin_style_cli_rubric".to_string(),
        approval_threshold: request.approval_threshold,
        writes_skills: true,
        solidifies_skill: true,
        skills_root: skills_root.display().to_string(),
        judgment_count: judgments.len(),
        judgments,
        write_count: write_receipts.len(),
        write_receipts,
        solidify_receipt_count: solidify_tickets.len(),
        solidify_receipts: solidify_tickets,
        boundary: SkillSolidifyBoundary {
            validates_proposal: true,
            self_scores_proposal: true,
            emits_solidify_receipt: true,
            reads_existing_skills: true,
            writes_skill_files: true,
            upserts_canonical_skill: true,
            solidifies_skill: true,
            connects_llm: false,
            connects_external_service: false,
        },
    };

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
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
                "boundary validates_proposal=true self_scores_proposal=true emits_solidify_receipt=true reads_existing_skills=true writes_skill_files=true upserts_canonical_skill=true solidifies_skill=true connects_llm=false connects_external_service=false"
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
    }

    Ok(())
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
    parse_skill_review_request(args, "skill approve", "cli skill approve")
}

fn parse_skill_judge(args: &[String]) -> Result<SkillApproveRequest, String> {
    parse_skill_review_request(args, "skill judge", "cli skill judge")
}

fn parse_skill_solidify(args: &[String]) -> Result<SkillApproveRequest, String> {
    parse_skill_review_request(args, "skill solidify", "cli skill solidify")
}

fn parse_skill_review_request(
    args: &[String],
    command_name: &str,
    default_approval_source: &str,
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

fn build_skill_judgments(
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

fn solidify_skill_file(
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
            content.push_str("Previous content was replaced by this canonical upsert. The latest active form above is authoritative.\n");
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
    content
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

fn default_skills_root() -> PathBuf {
    PathBuf::from("data/skills")
}

fn sanitize_yaml_scalar(raw: &str) -> String {
    raw.replace('\n', " ").replace('\r', " ")
}

struct SkillProposeRequest {
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
}

struct SkillRetireRequest {
    output: ControlOutputFormat,
    skills_root: PathBuf,
    skill_id: String,
    reason: String,
    status: String,
    retired_at: Option<String>,
}

struct SkillReviewBuild {
    proposals: Vec<SkillProposal>,
    proposal_validation_reports: Vec<ValidationReport>,
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
struct SkillSolidifyOutput {
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
struct SkillJudgmentOutput {
    proposal_id: String,
    canonical_skill_id: String,
    approved: bool,
    score_total: u16,
    threshold: u16,
    policy: String,
    duplicate_state: String,
    target_path: Option<String>,
    reasons: Vec<String>,
    rubric_scores: Vec<SkillRubricScoreOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillRubricScoreOutput {
    dimension: String,
    score: u16,
    max_score: u16,
    reason: String,
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
struct SkillWriteReceiptOutput {
    skill_id: String,
    action: String,
    duplicate_state: String,
    path: String,
    bytes_written: usize,
    status: String,
    version: u32,
    provenance_event_ids: Vec<String>,
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
