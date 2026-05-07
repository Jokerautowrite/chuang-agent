use std::collections::BTreeMap;

use chuang_agent::skill_evolver::{
    DryRunProposalEvolver, EvolutionScope, RuntimeEvent, RuntimeEventKind, SkillEvolver,
    SkillProposal,
};
use serde::Serialize;

use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn skill_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("propose") => skill_propose_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn skill_propose_command(args: &[String]) -> Result<(), String> {
    let request = parse_skill_propose(args)?;
    let mut evolver = DryRunProposalEvolver::new();
    evolver
        .observe(RuntimeEvent {
            event_id: request.event_id.clone(),
            task_id: request.task_id.clone(),
            kind: request.kind,
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
    let output = SkillProposeOutput {
        dry_run: true,
        writes_skills: false,
        requires_approval: true,
        proposal_count: proposals.len(),
        proposals,
        boundary: SkillProposeBoundary {
            observes_runtime_event: true,
            reads_existing_skills: false,
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
                "skill_propose dry_run=true writes_skills=false requires_approval=true proposals={}",
                output.proposal_count
            );
            println!(
                "boundary observes_runtime_event=true reads_existing_skills=false writes_skill_files=false solidifies_skill=false connects_llm=false connects_external_service=false"
            );
            for proposal in &output.proposals {
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
            }
        }
    }

    Ok(())
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

    let event_id = require_non_empty("--event-id", event_id)?;
    let task_id = require_non_empty("--task-id", task_id)?;
    let summary = require_non_empty("--summary", summary)?;
    if agent_id.trim().is_empty() {
        return Err("--agent-id must not be empty".to_string());
    }
    if let Some(task_kind) = &task_kind {
        if task_kind.trim().is_empty() {
            return Err("--task-kind must not be empty".to_string());
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

fn require_non_empty(flag: &str, value: Option<String>) -> Result<String, String> {
    let value = value.ok_or_else(|| format!("skill propose requires {flag}"))?;
    if value.trim().is_empty() {
        return Err(format!("{flag} must not be empty"));
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillProposeOutput {
    dry_run: bool,
    writes_skills: bool,
    requires_approval: bool,
    proposal_count: usize,
    proposals: Vec<SkillProposal>,
    boundary: SkillProposeBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SkillProposeBoundary {
    observes_runtime_event: bool,
    reads_existing_skills: bool,
    writes_skill_files: bool,
    solidifies_skill: bool,
    connects_llm: bool,
    connects_external_service: bool,
}
