use std::collections::BTreeMap;

use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver,
    SkillProposal, SkillSolidifyTicket, ValidationReport,
};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn skill_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("propose") => skill_propose_command(&args[1..]),
        Some("approve") => skill_approve_command(&args[1..]),
        Some("solidify") => skill_solidify_command(&args[1..]),
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
        writes_skills: false,
        solidifies_skill: false,
        approval_receipt_count: approval_tickets.len(),
        approval_receipts: approval_tickets,
        boundary: SkillApproveBoundary {
            validates_proposal: true,
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
                "skill_approve approved=true writes_skills=false solidifies_skill=false approval_receipts={}",
                output.approval_receipt_count
            );
            println!(
                "boundary validates_proposal=true emits_approval_receipt=true writes_skill_files=false solidifies_skill=false connects_llm=false connects_external_service=false"
            );
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

fn skill_solidify_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_solidify(args)?;
    let review = build_skill_review(&request.review)?;
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
    let output = SkillSolidifyOutput {
        solidify_requested: true,
        solidify_allowed: false,
        writes_skills: false,
        solidifies_skill: false,
        solidify_receipt_count: solidify_tickets.len(),
        solidify_receipts: solidify_tickets,
        boundary: SkillSolidifyBoundary {
            validates_proposal: true,
            emits_solidify_receipt: true,
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
                "skill_solidify solidify_requested=true solidify_allowed=false writes_skills=false solidifies_skill=false solidify_receipts={}",
                output.solidify_receipt_count
            );
            println!(
                "boundary validates_proposal=true emits_solidify_receipt=true writes_skill_files=false solidifies_skill=false connects_llm=false connects_external_service=false"
            );
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
    writes_skills: bool,
    solidifies_skill: bool,
    approval_receipt_count: usize,
    approval_receipts: Vec<SkillSolidifyTicket>,
    boundary: SkillApproveBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillApproveBoundary {
    validates_proposal: bool,
    emits_approval_receipt: bool,
    writes_skill_files: bool,
    solidifies_skill: bool,
    connects_llm: bool,
    connects_external_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillSolidifyOutput {
    solidify_requested: bool,
    solidify_allowed: bool,
    writes_skills: bool,
    solidifies_skill: bool,
    solidify_receipt_count: usize,
    solidify_receipts: Vec<SkillSolidifyTicket>,
    boundary: SkillSolidifyBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillSolidifyBoundary {
    validates_proposal: bool,
    emits_solidify_receipt: bool,
    writes_skill_files: bool,
    solidifies_skill: bool,
    connects_llm: bool,
    connects_external_service: bool,
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
