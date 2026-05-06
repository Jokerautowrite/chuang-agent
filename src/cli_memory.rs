use std::collections::{BTreeMap, BTreeSet};

use chuang_agent::hermes_memory::{
    DualFileMemoryError, DualFileMemoryStore, FileDualFileMemoryStore, HotMemoryEntry,
};
use chuang_agent::memory_store::{MemoryQuery, MemoryStore, SearchHit};
use chuang_agent::memory_store_sqlite::SqliteMemoryStore;
use serde::Serialize;

use crate::cli_args::parse_cli_options;
use crate::cli_output::{print_json, usage, ControlOutputFormat};

pub(crate) fn memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("identity") => identity_memory_command(&args[1..]),
        Some("session") => session_memory_command(&args[1..]),
        Some("lim") => lim_memory_command(&args[1..]),
        Some("maintenance") => maintenance_memory_command(&args[1..]),
        Some("knowledge") => knowledge_memory_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn identity_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("show") => identity_memory_show_command(&args[1..]),
        Some("append") => identity_memory_append_command(&args[1..]),
        Some("append-experience") => identity_memory_append_experience_command(&args[1..]),
        Some("write-user") => {
            identity_memory_write_command(IdentityMemoryWriteScope::User, &args[1..])
        }
        Some("write-memory") => {
            identity_memory_write_command(IdentityMemoryWriteScope::Memory, &args[1..])
        }
        _ => Err(usage()),
    }
}

fn session_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("search") => session_memory_search_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn lim_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("extract") => lim_memory_extract_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn maintenance_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("report") => memory_maintenance_report_command(&args[1..]),
        Some("apply") => memory_maintenance_apply_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn knowledge_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("status") => memory_knowledge_status_command(&args[1..]),
        Some("search") => memory_knowledge_search_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn memory_knowledge_search_command(args: &[String]) -> Result<(), String> {
    let request = parse_memory_knowledge_search(args)?;
    let hits = search_local_knowledge_root(&request.root, &request.query, request.limit)?;
    let output = MemoryKnowledgeSearchOutput {
        adapter: "local_external_knowledge".to_string(),
        dry_run: true,
        read_only: true,
        connects_real_service: false,
        writes_automatically: false,
        runtime_retrieval_wired: false,
        root: request.root.display().to_string(),
        query: request.query,
        limit: request.limit,
        hit_count: hits.len(),
        hits,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "memory_knowledge_search adapter={} dry_run=true read_only=true root={} query={} hits={}",
                output.adapter, output.root, output.query, output.hit_count
            );
            println!(
                "connects_real_service=false writes_automatically=false runtime_retrieval_wired=false"
            );
            for hit in &output.hits {
                println!(
                    "hit path={} line={} score={} source={}",
                    hit.path, hit.line, hit.score, hit.source
                );
                println!("{}", hit.preview);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn memory_knowledge_status_command(args: &[String]) -> Result<(), String> {
    let output = parse_memory_knowledge_status(args)?;
    let status = MemoryKnowledgeStatusOutput {
        adapter: "external_knowledge".to_string(),
        dry_run: true,
        read_only: true,
        connects_real_service: false,
        writes_automatically: false,
        runtime_retrieval_wired: false,
        doc: "docs/external-knowledge-adapter.md".to_string(),
        sources: vec![
            MemoryKnowledgeSourceOutput {
                name: "wiki".to_string(),
                state: "documented_only".to_string(),
                current: "external-brain source is documented but no live adapter is configured"
                    .to_string(),
            },
            MemoryKnowledgeSourceOutput {
                name: "gbrain".to_string(),
                state: "documented_only".to_string(),
                current: "knowledge base boundary is documented; runtime retrieval stays deferred"
                    .to_string(),
            },
        ],
        next_actions: vec![
            "add a read-only local knowledge adapter contract".to_string(),
            "add provenance-bearing search before runtime injection".to_string(),
            "keep automatic sync and memory writeback disabled".to_string(),
        ],
    };

    match output {
        ControlOutputFormat::Text => {
            println!(
                "memory_knowledge_status adapter={} dry_run={} read_only={} connects_real_service={} writes_automatically={}",
                status.adapter,
                status.dry_run,
                status.read_only,
                status.connects_real_service,
                status.writes_automatically
            );
            println!(
                "runtime_retrieval_wired: {} doc: {}",
                status.runtime_retrieval_wired, status.doc
            );
            for source in &status.sources {
                println!(
                    "source name={} state={} current={}",
                    source.name, source.state, source.current
                );
            }
            for next_action in &status.next_actions {
                println!("next_action: {next_action}");
            }
        }
        ControlOutputFormat::Json => print_json(&status)?,
    }

    Ok(())
}

fn memory_maintenance_report_command(args: &[String]) -> Result<(), String> {
    let request = parse_memory_maintenance_report(args)?;
    let plan = build_memory_maintenance_plan(
        &request.runtime_args,
        &request.queries,
        request.session_id.as_deref(),
        request.limit,
    )?;
    let output = MemoryMaintenanceReportOutput {
        dry_run: true,
        writes_automatically: false,
        explicit_writeback_required: true,
        query: request.queries.first().cloned().unwrap_or_default(),
        queries: request.queries,
        session_id: request.session_id,
        limit: request.limit,
        identity_health: plan.identity_health,
        batch_count: plan.batches.len(),
        batches: plan.batches.clone(),
        lim_candidate_count: plan.lim_candidate_count,
        lim_candidates: plan.lim_candidates,
        decay_candidate_count: plan.decay_candidate_count,
        decay_candidates: plan.decay_candidates,
        recommendations: plan.recommendations,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "memory_maintenance_report dry_run=true writes_automatically=false query={} session_id={} candidates={}",
                output.query,
                output.session_id.as_deref().unwrap_or("any"),
                output.lim_candidate_count
            );
            println!(
                "maintenance_boundary explicit_writeback_required={} batch_count={} decay_candidates={}",
                output.explicit_writeback_required,
                output.batch_count,
                output.decay_candidate_count
            );
            println!(
                "identity_health root={} user={}/{} memory={}/{} experiences_chars={}",
                output.identity_health.root,
                output.identity_health.user_chars,
                output.identity_health.user_max_chars,
                output.identity_health.memory_chars,
                output.identity_health.memory_max_chars,
                output.identity_health.experiences_chars
            );
            for recommendation in &output.recommendations {
                println!("recommendation: {recommendation}");
            }
            for candidate in &output.lim_candidates {
                println!(
                    "candidate id={} source_record_id={} confidence={}",
                    candidate.candidate_id, candidate.source_record_id, candidate.confidence
                );
            }
            for candidate in &output.decay_candidates {
                println!(
                    "decay_candidate id={} source_scope={} reason={}",
                    candidate.candidate_id, candidate.source_scope, candidate.reason_code
                );
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn memory_maintenance_apply_command(args: &[String]) -> Result<(), String> {
    let request = parse_memory_maintenance_apply(args)?;
    if request.dry_run && request.approve_writeback {
        return Err(
            "memory_maintenance_apply_dry_run_conflicts_with_approve_writeback".to_string(),
        );
    }
    if !request.dry_run && !request.approve_writeback {
        return Err("memory_maintenance_apply_requires_approve_writeback".to_string());
    }

    let plan = build_memory_maintenance_plan(
        &request.runtime_args,
        &request.queries,
        request.session_id.as_deref(),
        request.limit,
    )?;
    let selected_candidate_ids = if request.candidate_ids.is_empty() {
        plan.lim_candidates
            .iter()
            .map(|candidate| candidate.candidate_id.clone())
            .collect::<Vec<_>>()
    } else {
        request.candidate_ids.clone()
    };

    let mut unique_selected_candidate_ids = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate_id in selected_candidate_ids {
        if seen.insert(candidate_id.clone()) {
            unique_selected_candidate_ids.push(candidate_id);
        }
    }

    let mut selected_candidates = Vec::new();
    for candidate_id in &unique_selected_candidate_ids {
        let candidate = plan
            .lim_candidates
            .iter()
            .find(|candidate| candidate.candidate_id == *candidate_id);
        if let Some(candidate) = candidate {
            selected_candidates.push(candidate.clone());
            continue;
        }
        if plan
            .decay_candidates
            .iter()
            .any(|candidate| candidate.candidate_id == *candidate_id)
        {
            return Err(format!(
                "memory_maintenance_apply_candidate_not_writeback_candidate: {candidate_id}"
            ));
        }
        return Err(format!(
            "memory_maintenance_apply_unknown_candidate_id: {candidate_id}"
        ));
    }

    let mut applied_candidate_ids = Vec::new();
    let mut skipped_candidate_ids = Vec::new();
    if request.approve_writeback {
        let mut store = open_identity_memory_store(&request.runtime_args)?;
        for candidate in &selected_candidates {
            match store.append_experience(HotMemoryEntry {
                id: candidate.candidate_id.clone(),
                content: candidate.content.clone(),
            }) {
                Ok(()) => applied_candidate_ids.push(candidate.candidate_id.clone()),
                Err(DualFileMemoryError::DuplicateEntry { .. }) => {
                    skipped_candidate_ids.push(candidate.candidate_id.clone());
                }
                Err(err) => return Err(format_identity_memory_error(err)),
            }
        }
    }

    let output = MemoryMaintenanceApplyOutput {
        dry_run: request.dry_run,
        writes_automatically: false,
        explicit_writeback_required: true,
        approved_writeback: request.approve_writeback,
        query: request.queries.first().cloned().unwrap_or_default(),
        queries: request.queries,
        session_id: request.session_id,
        limit: request.limit,
        identity_health: plan.identity_health,
        batch_count: plan.batches.len(),
        batches: plan.batches,
        lim_candidate_count: plan.lim_candidate_count,
        decay_candidate_count: plan.decay_candidate_count,
        selected_candidate_ids: unique_selected_candidate_ids,
        applied_candidate_ids,
        skipped_candidate_ids,
        recommendations: plan.recommendations,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "memory_maintenance_apply dry_run={} writes_automatically=false approved_writeback={} query={} session_id={} applied={} skipped={}",
                output.dry_run,
                output.approved_writeback,
                output.query,
                output.session_id.as_deref().unwrap_or("any"),
                output.applied_candidate_ids.len(),
                output.skipped_candidate_ids.len()
            );
            for candidate_id in &output.applied_candidate_ids {
                println!("applied_candidate_id: {candidate_id}");
            }
            for candidate_id in &output.skipped_candidate_ids {
                println!("skipped_candidate_id: {candidate_id}");
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn lim_memory_extract_command(args: &[String]) -> Result<(), String> {
    let request = parse_lim_memory_extract(args)?;
    let hits = search_turn_summaries(
        &request.runtime_args,
        &request.query,
        request.session_id.as_deref(),
        request.limit,
    )?;
    let candidates = build_lim_candidates(hits);
    let output = LimExtractionOutput {
        query: request.query,
        session_id: request.session_id,
        limit: request.limit,
        dry_run: true,
        candidate_count: candidates.len(),
        candidates,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "lim_extract dry_run=true query={} session_id={} candidates={}",
                output.query,
                output.session_id.as_deref().unwrap_or("any"),
                output.candidate_count
            );
            for candidate in &output.candidates {
                println!(
                    "candidate id={} source_record_id={} confidence={}",
                    candidate.candidate_id, candidate.source_record_id, candidate.confidence
                );
                println!("{}", candidate.content);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn build_lim_candidates(hits: Vec<SearchHit>) -> Vec<LimExtractionCandidateOutput> {
    hits.into_iter()
        .map(|hit| {
            let source_record_id = hit.record.id;
            let created_at = hit.record.created_at;
            let lesson = first_non_empty_line(&hit.record.content);
            let metadata = hit.record.metadata;
            LimExtractionCandidateOutput {
                candidate_id: format!("lim-candidate-{}", sanitize_candidate_id(&source_record_id)),
                source_record_id: source_record_id.clone(),
                confidence: if hit.score > 0 { "medium" } else { "low" }.to_string(),
                proposed_scope: "experiences".to_string(),
                content: format!(
                    "source=lim_dry_run\nsource_record_id={}\ncreated_at={}\nlesson={}",
                    source_record_id, created_at, lesson
                ),
                metadata,
            }
        })
        .collect()
}

fn build_decay_candidates(
    health: &IdentityMaintenanceHealthOutput,
) -> Vec<MemoryDecayCandidateOutput> {
    let mut candidates = Vec::new();
    if health.memory_chars > health.memory_max_chars.saturating_mul(8) / 10 {
        candidates.push(MemoryDecayCandidateOutput {
            candidate_id: "decay-hot-memory-review".to_string(),
            source_scope: "MEMORY.md".to_string(),
            reason_code: "memory_over_80_percent".to_string(),
            recommendation: "review and compact hot memory manually; do not auto-rewrite"
                .to_string(),
            writeback_allowed: false,
        });
    }
    if health.user_chars > health.user_max_chars.saturating_mul(8) / 10 {
        candidates.push(MemoryDecayCandidateOutput {
            candidate_id: "decay-user-memory-review".to_string(),
            source_scope: "USER.md".to_string(),
            reason_code: "user_over_80_percent".to_string(),
            recommendation: "review fixed user facts manually; do not auto-rewrite".to_string(),
            writeback_allowed: false,
        });
    }
    candidates
}

fn build_maintenance_recommendations(
    health: &IdentityMaintenanceHealthOutput,
    lim_candidates: &[LimExtractionCandidateOutput],
    decay_candidates: &[MemoryDecayCandidateOutput],
) -> Vec<String> {
    let mut recommendations = Vec::new();
    if health.memory_chars > health.memory_max_chars.saturating_mul(8) / 10 {
        recommendations.push("review MEMORY.md before appending more hot memory".to_string());
    }
    if health.user_chars > health.user_max_chars.saturating_mul(8) / 10 {
        recommendations
            .push("review USER.md fixed facts before adding more user memory".to_string());
    }
    if lim_candidates.is_empty() {
        recommendations.push("no LIM candidates found for this query/session".to_string());
    } else {
        recommendations.push(
            "review LIM candidates manually before append-experience or write-memory".to_string(),
        );
    }
    if !decay_candidates.is_empty() {
        recommendations.push(
            "review decay candidates manually; maintenance apply never rewrites hot memory"
                .to_string(),
        );
    }
    recommendations.push("do not run automatic rewrite from maintenance report".to_string());
    recommendations
}

fn session_memory_search_command(args: &[String]) -> Result<(), String> {
    let request = parse_session_memory_search(args)?;
    let hits = search_turn_summaries(
        &request.runtime_args,
        &request.query,
        request.session_id.as_deref(),
        request.limit,
    )?;
    let output = SessionMemorySearchOutput {
        query: request.query,
        session_id: request.session_id,
        limit: request.limit,
        hit_count: hits.len(),
        hits: hits
            .into_iter()
            .map(|hit| SessionMemorySearchHitOutput {
                id: hit.record.id,
                score: hit.score,
                content: hit.record.content,
                metadata: hit.record.metadata,
                created_at: hit.record.created_at,
            })
            .collect(),
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "session_memory_search query={} session_id={} hits={}",
                output.query,
                output.session_id.as_deref().unwrap_or("any"),
                output.hit_count
            );
            for hit in &output.hits {
                println!(
                    "hit id={} score={} created_at={}",
                    hit.id, hit.score, hit.created_at
                );
                println!("{}", hit.content);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn search_turn_summaries(
    runtime_args: &[String],
    query: &str,
    session_id: Option<&str>,
    limit: usize,
) -> Result<Vec<SearchHit>, String> {
    let options = parse_cli_options(runtime_args)?;
    let store = SqliteMemoryStore::open(&options.runtime.db_path)
        .map_err(|e| format!("session_memory_open_failed: {e:?}"))?;
    let mut metadata = BTreeMap::from([("kind".to_string(), "turn_summary".to_string())]);
    if let Some(session_id) = session_id {
        metadata.insert("memory_scope".to_string(), "session".to_string());
        metadata.insert("session_id".to_string(), session_id.to_string());
    }
    store
        .search(&MemoryQuery {
            text: Some(query.to_string()),
            metadata,
            limit,
        })
        .map_err(|e| format!("session_memory_search_failed: {e:?}"))
}

fn identity_memory_show_command(args: &[String]) -> Result<(), String> {
    let request = parse_identity_memory_show(args)?;
    let store = open_identity_memory_store(&request.runtime_args)?;
    let config = store.config().clone();
    let snapshot = store
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;
    let output = IdentityMemoryShowOutput {
        root: config.root.display().to_string(),
        user_file: config.user_file,
        memory_file: config.memory_file,
        experiences_file: config.experiences_file,
        user_max_chars: config.user_max_chars,
        memory_max_chars: config.memory_max_chars,
        user_chars: snapshot.user.chars().count(),
        memory_chars: snapshot.memory.chars().count(),
        experiences_chars: snapshot.experiences.chars().count(),
        user: snapshot.user,
        memory: snapshot.memory,
        experiences: snapshot.experiences,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("identity_memory_root: {}", output.root);
            println!(
                "identity_memory_limits: user={} memory={}",
                output.user_max_chars, output.memory_max_chars
            );
            println!(
                "identity_memory_chars: user={} memory={}",
                output.user_chars, output.memory_chars
            );
            println!("--- USER.md ---");
            println!("{}", output.user);
            println!("--- MEMORY.md ---");
            println!("{}", output.memory);
            println!("--- experiences.md ---");
            println!("{}", output.experiences);
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_append_command(args: &[String]) -> Result<(), String> {
    let request = parse_identity_memory_append(args)?;
    let mut store = open_identity_memory_store(&request.runtime_args)?;
    store
        .append_memory(HotMemoryEntry {
            id: request.id.clone(),
            content: request.content,
        })
        .map_err(format_identity_memory_error)?;
    let output = IdentityMemoryMutationOutput {
        scope: "memory".to_string(),
        id: Some(request.id),
        written: true,
        replaced: false,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "identity_memory_appended scope={} id={}",
                output.scope,
                output.id.as_deref().unwrap_or("none")
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_append_experience_command(args: &[String]) -> Result<(), String> {
    let request = parse_identity_memory_append(args)?;
    let mut store = open_identity_memory_store(&request.runtime_args)?;
    store
        .append_experience(HotMemoryEntry {
            id: request.id.clone(),
            content: request.content,
        })
        .map_err(format_identity_memory_error)?;
    let output = IdentityMemoryMutationOutput {
        scope: "experiences".to_string(),
        id: Some(request.id),
        written: true,
        replaced: false,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "identity_memory_appended scope={} id={}",
                output.scope,
                output.id.as_deref().unwrap_or("none")
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn identity_memory_write_command(
    scope: IdentityMemoryWriteScope,
    args: &[String],
) -> Result<(), String> {
    let request = parse_identity_memory_write(args)?;
    if !request.approve_overwrite {
        return Err("identity_memory_write_requires_approve_overwrite".to_string());
    }
    let mut store = open_identity_memory_store(&request.runtime_args)?;
    match scope {
        IdentityMemoryWriteScope::User => store.write_user(&request.content),
        IdentityMemoryWriteScope::Memory => store.write_memory(&request.content),
    }
    .map_err(format_identity_memory_error)?;

    let output = IdentityMemoryMutationOutput {
        scope: scope.as_str().to_string(),
        id: None,
        written: true,
        replaced: true,
    };
    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "identity_memory_written scope={} replaced={}",
                output.scope, output.replaced
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn open_identity_memory_store(runtime_args: &[String]) -> Result<FileDualFileMemoryStore, String> {
    let options = parse_cli_options(runtime_args)?;
    let config = options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))?;
    FileDualFileMemoryStore::open(config).map_err(|e| format!("identity_memory_open_failed: {e:?}"))
}

fn parse_identity_memory_show(args: &[String]) -> Result<IdentityMemoryShowRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }
    Ok(IdentityMemoryShowRequest {
        runtime_args,
        output,
    })
}

fn parse_session_memory_search(args: &[String]) -> Result<SessionMemorySearchRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut query = None;
    let mut session_id = None;
    let mut limit = 5usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--query" => {
                query = Some(take_local_value(args, &mut index, "--query")?);
            }
            "--session-id" => {
                let value = take_local_value(args, &mut index, "--session-id")?;
                if value.trim().is_empty() {
                    return Err("session memory search requires non-empty --session-id".to_string());
                }
                session_id = Some(value);
            }
            "--limit" => {
                let value = take_local_value(args, &mut index, "--limit")?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| "session memory search requires numeric --limit".to_string())?;
                if limit == 0 {
                    return Err("session memory search requires --limit > 0".to_string());
                }
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let query = query.ok_or_else(|| "session memory search requires --query".to_string())?;
    if query.trim().is_empty() {
        return Err("session memory search requires non-empty --query".to_string());
    }

    Ok(SessionMemorySearchRequest {
        runtime_args,
        output,
        query,
        session_id,
        limit,
    })
}

fn parse_lim_memory_extract(args: &[String]) -> Result<LimExtractionRequest, String> {
    let request = parse_session_memory_search(args)?;
    Ok(LimExtractionRequest {
        runtime_args: request.runtime_args,
        output: request.output,
        query: request.query,
        session_id: request.session_id,
        limit: request.limit,
    })
}

fn parse_memory_maintenance_report(
    args: &[String],
) -> Result<MemoryMaintenanceReportRequest, String> {
    let request = parse_memory_maintenance_query_args(args, "report")?;
    Ok(MemoryMaintenanceReportRequest {
        runtime_args: request.runtime_args,
        output: request.output,
        queries: request.queries,
        session_id: request.session_id,
        limit: request.limit,
    })
}

fn parse_memory_maintenance_apply(
    args: &[String],
) -> Result<MemoryMaintenanceApplyRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut queries = Vec::new();
    let mut session_id = None;
    let mut limit = 5usize;
    let mut candidate_ids = Vec::new();
    let mut dry_run = false;
    let mut approve_writeback = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--query" => {
                let value = take_local_value(args, &mut index, "--query")?;
                if value.trim().is_empty() {
                    return Err("memory maintenance apply requires non-empty --query".to_string());
                }
                queries.push(value);
            }
            "--session-id" => {
                let value = take_local_value(args, &mut index, "--session-id")?;
                if value.trim().is_empty() {
                    return Err(
                        "memory maintenance apply requires non-empty --session-id".to_string()
                    );
                }
                session_id = Some(value);
            }
            "--limit" => {
                let value = take_local_value(args, &mut index, "--limit")?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| "memory maintenance apply requires numeric --limit".to_string())?;
                if limit == 0 {
                    return Err("memory maintenance apply requires --limit > 0".to_string());
                }
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            "--candidate-id" => {
                let value = take_local_value(args, &mut index, "--candidate-id")?;
                if value.trim().is_empty() {
                    return Err(
                        "memory maintenance apply requires non-empty --candidate-id".to_string()
                    );
                }
                candidate_ids.push(value);
            }
            "--approve-writeback" => {
                approve_writeback = true;
                index += 1;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }

    if queries.is_empty() {
        return Err("memory maintenance apply requires --query".to_string());
    }

    Ok(MemoryMaintenanceApplyRequest {
        runtime_args,
        output,
        queries,
        session_id,
        limit,
        candidate_ids,
        dry_run,
        approve_writeback,
    })
}

fn parse_memory_maintenance_query_args(
    args: &[String],
    command_name: &str,
) -> Result<MemoryMaintenanceQueryRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut queries = Vec::new();
    let mut session_id = None;
    let mut limit = 5usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--query" => {
                let value = take_local_value(args, &mut index, "--query")?;
                if value.trim().is_empty() {
                    return Err(format!(
                        "memory maintenance {command_name} requires non-empty --query"
                    ));
                }
                queries.push(value);
            }
            "--session-id" => {
                let value = take_local_value(args, &mut index, "--session-id")?;
                if value.trim().is_empty() {
                    return Err(format!(
                        "memory maintenance {command_name} requires non-empty --session-id"
                    ));
                }
                session_id = Some(value);
            }
            "--limit" => {
                let value = take_local_value(args, &mut index, "--limit")?;
                limit = value.parse::<usize>().map_err(|_| {
                    format!("memory maintenance {command_name} requires numeric --limit")
                })?;
                if limit == 0 {
                    return Err(format!(
                        "memory maintenance {command_name} requires --limit > 0"
                    ));
                }
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }
    if queries.is_empty() {
        return Err(format!(
            "memory maintenance {command_name} requires --query"
        ));
    }
    Ok(MemoryMaintenanceQueryRequest {
        runtime_args,
        output,
        queries,
        session_id,
        limit,
    })
}

fn build_memory_maintenance_plan(
    runtime_args: &[String],
    queries: &[String],
    session_id: Option<&str>,
    limit: usize,
) -> Result<MemoryMaintenancePlan, String> {
    let store = open_identity_memory_store(runtime_args)?;
    let config = store.config().clone();
    let snapshot = store
        .snapshot()
        .map_err(|e| format!("identity_memory_snapshot_failed: {e:?}"))?;
    let identity_health = IdentityMaintenanceHealthOutput {
        root: config.root.display().to_string(),
        user_chars: snapshot.user.chars().count(),
        user_max_chars: config.user_max_chars,
        memory_chars: snapshot.memory.chars().count(),
        memory_max_chars: config.memory_max_chars,
        experiences_chars: snapshot.experiences.chars().count(),
        experiences_file: config.experiences_file,
    };

    let mut batches = Vec::new();
    let mut lim_candidates = Vec::new();
    let mut seen_candidate_ids = BTreeSet::new();
    for query in queries {
        let hits = search_turn_summaries(runtime_args, query, session_id, limit)?;
        let candidates = build_lim_candidates(hits);
        for candidate in &candidates {
            if seen_candidate_ids.insert(candidate.candidate_id.clone()) {
                lim_candidates.push(candidate.clone());
            }
        }
        batches.push(MemoryMaintenanceBatchOutput {
            query: query.clone(),
            lim_candidate_count: candidates.len(),
            lim_candidate_ids: candidates
                .iter()
                .map(|candidate| candidate.candidate_id.clone())
                .collect(),
        });
    }

    let decay_candidates = build_decay_candidates(&identity_health);
    let recommendations =
        build_maintenance_recommendations(&identity_health, &lim_candidates, &decay_candidates);
    Ok(MemoryMaintenancePlan {
        identity_health,
        batches,
        lim_candidate_count: lim_candidates.len(),
        lim_candidates,
        decay_candidate_count: decay_candidates.len(),
        decay_candidates,
        recommendations,
    })
}

fn parse_memory_knowledge_status(args: &[String]) -> Result<ControlOutputFormat, String> {
    let mut output = ControlOutputFormat::Text;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            _ => return Err(usage()),
        }
    }
    Ok(output)
}

fn parse_memory_knowledge_search(args: &[String]) -> Result<MemoryKnowledgeSearchRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut root = None;
    let mut query = None;
    let mut limit = 5usize;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--root" => {
                root = Some(std::path::PathBuf::from(take_local_value(
                    args, &mut index, "--root",
                )?));
            }
            "--query" => {
                query = Some(take_local_value(args, &mut index, "--query")?);
            }
            "--limit" => {
                let value = take_local_value(args, &mut index, "--limit")?;
                limit = value
                    .parse::<usize>()
                    .map_err(|_| "memory knowledge search requires numeric --limit".to_string())?;
                if limit == 0 {
                    return Err("memory knowledge search requires --limit > 0".to_string());
                }
            }
            _ => return Err(usage()),
        }
    }

    let root = root.ok_or_else(|| "memory knowledge search requires --root".to_string())?;
    let query = query.ok_or_else(|| "memory knowledge search requires --query".to_string())?;
    if query.trim().is_empty() {
        return Err("memory knowledge search requires non-empty --query".to_string());
    }

    Ok(MemoryKnowledgeSearchRequest {
        output,
        root,
        query,
        limit,
    })
}

fn search_local_knowledge_root(
    root: &std::path::Path,
    query: &str,
    limit: usize,
) -> Result<Vec<MemoryKnowledgeSearchHitOutput>, String> {
    let root = root
        .canonicalize()
        .map_err(|e| format!("memory_knowledge_root_unavailable: {e}"))?;
    if !root.is_dir() {
        return Err("memory_knowledge_root_must_be_directory".to_string());
    }

    let needle = query.to_lowercase();
    let mut files = Vec::new();
    collect_knowledge_files(&root, &root, &mut files)?;
    let mut hits = Vec::new();
    for file in files {
        let content = std::fs::read_to_string(&file)
            .map_err(|e| format!("memory_knowledge_read_failed path={}: {e}", file.display()))?;
        for (line_index, line) in content.lines().enumerate() {
            let line_lower = line.to_lowercase();
            if line_lower.contains(&needle) {
                let relative = file
                    .strip_prefix(&root)
                    .unwrap_or(file.as_path())
                    .display()
                    .to_string();
                hits.push(MemoryKnowledgeSearchHitOutput {
                    source: "local_file".to_string(),
                    path: relative,
                    line: line_index + 1,
                    score: score_knowledge_line(&line_lower, &needle),
                    preview: line.trim().chars().take(240).collect(),
                });
            }
        }
    }
    hits.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.path.cmp(&right.path))
            .then_with(|| left.line.cmp(&right.line))
    });
    hits.truncate(limit);
    Ok(hits)
}

fn collect_knowledge_files(
    root: &std::path::Path,
    dir: &std::path::Path,
    files: &mut Vec<std::path::PathBuf>,
) -> Result<(), String> {
    let mut entries = std::fs::read_dir(dir)
        .map_err(|e| {
            format!(
                "memory_knowledge_read_dir_failed path={}: {e}",
                dir.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| format!("memory_knowledge_read_dir_entry_failed: {e}"))?;
    entries.sort_by_key(|entry| entry.path());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || is_sensitive_knowledge_path(&path) {
            continue;
        }
        let file_type = entry.file_type().map_err(|e| {
            format!(
                "memory_knowledge_file_type_failed path={}: {e}",
                path.display()
            )
        })?;
        if file_type.is_dir() {
            let canonical = path.canonicalize().map_err(|e| {
                format!(
                    "memory_knowledge_canonicalize_failed path={}: {e}",
                    path.display()
                )
            })?;
            if canonical.starts_with(root) {
                collect_knowledge_files(root, &path, files)?;
            }
        } else if file_type.is_file() && is_supported_knowledge_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_supported_knowledge_file(path: &std::path::Path) -> bool {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| matches!(ext.to_ascii_lowercase().as_str(), "md" | "markdown" | "txt"))
        .unwrap_or(false)
}

fn is_sensitive_knowledge_path(path: &std::path::Path) -> bool {
    let path_text = path.display().to_string().to_ascii_lowercase();
    [
        "secret",
        "token",
        "password",
        "private",
        ".env",
        "credential",
    ]
    .iter()
    .any(|marker| path_text.contains(marker))
}

fn score_knowledge_line(line_lower: &str, needle: &str) -> u32 {
    let occurrences = line_lower.matches(needle).count() as u32;
    10 + occurrences.saturating_mul(5)
}

fn parse_identity_memory_append(args: &[String]) -> Result<IdentityMemoryAppendRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut id = None;
    let mut content = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--id" => {
                id = Some(take_local_value(args, &mut index, "--id")?);
            }
            "--content" => {
                content = Some(take_local_value(args, &mut index, "--content")?);
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let id = id.ok_or_else(|| "identity memory append requires --id".to_string())?;
    if id.trim().is_empty() {
        return Err("identity memory append requires non-empty --id".to_string());
    }
    let content = content.ok_or_else(|| "identity memory append requires --content".to_string())?;
    if content.trim().is_empty() {
        return Err("identity memory append requires non-empty --content".to_string());
    }

    Ok(IdentityMemoryAppendRequest {
        runtime_args,
        output,
        id,
        content,
    })
}

fn parse_identity_memory_write(args: &[String]) -> Result<IdentityMemoryWriteRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut content = None;
    let mut approve_overwrite = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--content" => {
                content = Some(take_local_value(args, &mut index, "--content")?);
            }
            "--approve-overwrite" => {
                approve_overwrite = true;
                index += 1;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }

    let content = content.ok_or_else(|| "identity memory write requires --content".to_string())?;

    Ok(IdentityMemoryWriteRequest {
        runtime_args,
        output,
        content,
        approve_overwrite,
    })
}

fn push_value_arg(
    args: &[String],
    index: &mut usize,
    target: &mut Vec<String>,
) -> Result<(), String> {
    let flag = args
        .get(*index)
        .ok_or_else(|| "missing flag".to_string())?
        .clone();
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("{flag} requires value"))?
        .clone();
    target.push(flag);
    target.push(value);
    *index += 2;
    Ok(())
}

fn take_local_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index + 1)
        .ok_or_else(|| format!("memory command requires value after {flag}"))?
        .clone();
    *index += 2;
    Ok(value)
}

fn format_identity_memory_error(err: DualFileMemoryError) -> String {
    match err {
        DualFileMemoryError::StorageUnavailable { path } => {
            format!("identity_memory_write_failed path={}", path.display())
        }
        DualFileMemoryError::DuplicateEntry { id } => {
            format!("identity_memory_duplicate_entry id={id}")
        }
        DualFileMemoryError::HardLimitExceeded {
            scope,
            limit_chars,
            attempted_chars,
            existing_entries,
        } => format!(
            "identity_memory_hard_limit_exceeded scope={scope:?} limit_chars={} attempted_chars={} existing_entries={}",
            limit_chars,
            attempted_chars,
            if existing_entries.is_empty() {
                "none".to_string()
            } else {
                existing_entries
                    .into_iter()
                    .map(|entry| {
                        format!(
                            "{}:{}chars:preview={}",
                            entry.id, entry.chars, entry.content_preview
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",")
            }
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityMemoryWriteScope {
    User,
    Memory,
}

impl IdentityMemoryWriteScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Memory => "memory",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityMemoryShowRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityMemoryAppendRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    id: String,
    content: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct IdentityMemoryWriteRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    content: String,
    approve_overwrite: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionMemorySearchRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    query: String,
    session_id: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionMemorySearchOutput {
    query: String,
    session_id: Option<String>,
    limit: usize,
    hit_count: usize,
    hits: Vec<SessionMemorySearchHitOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct SessionMemorySearchHitOutput {
    id: String,
    score: u32,
    content: String,
    metadata: BTreeMap<String, String>,
    created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LimExtractionRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    query: String,
    session_id: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryMaintenanceReportRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    queries: Vec<String>,
    session_id: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryMaintenanceApplyRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    queries: Vec<String>,
    session_id: Option<String>,
    limit: usize,
    candidate_ids: Vec<String>,
    dry_run: bool,
    approve_writeback: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryMaintenanceQueryRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    queries: Vec<String>,
    session_id: Option<String>,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LimExtractionOutput {
    query: String,
    session_id: Option<String>,
    limit: usize,
    dry_run: bool,
    candidate_count: usize,
    candidates: Vec<LimExtractionCandidateOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct LimExtractionCandidateOutput {
    candidate_id: String,
    source_record_id: String,
    confidence: String,
    proposed_scope: String,
    content: String,
    metadata: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryDecayCandidateOutput {
    candidate_id: String,
    source_scope: String,
    reason_code: String,
    recommendation: String,
    writeback_allowed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryMaintenanceBatchOutput {
    query: String,
    lim_candidate_count: usize,
    lim_candidate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryMaintenanceReportOutput {
    dry_run: bool,
    writes_automatically: bool,
    explicit_writeback_required: bool,
    query: String,
    queries: Vec<String>,
    session_id: Option<String>,
    limit: usize,
    identity_health: IdentityMaintenanceHealthOutput,
    batch_count: usize,
    batches: Vec<MemoryMaintenanceBatchOutput>,
    lim_candidate_count: usize,
    lim_candidates: Vec<LimExtractionCandidateOutput>,
    decay_candidate_count: usize,
    decay_candidates: Vec<MemoryDecayCandidateOutput>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryMaintenanceApplyOutput {
    dry_run: bool,
    writes_automatically: bool,
    explicit_writeback_required: bool,
    approved_writeback: bool,
    query: String,
    queries: Vec<String>,
    session_id: Option<String>,
    limit: usize,
    identity_health: IdentityMaintenanceHealthOutput,
    batch_count: usize,
    batches: Vec<MemoryMaintenanceBatchOutput>,
    lim_candidate_count: usize,
    decay_candidate_count: usize,
    selected_candidate_ids: Vec<String>,
    applied_candidate_ids: Vec<String>,
    skipped_candidate_ids: Vec<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeStatusOutput {
    adapter: String,
    dry_run: bool,
    read_only: bool,
    connects_real_service: bool,
    writes_automatically: bool,
    runtime_retrieval_wired: bool,
    doc: String,
    sources: Vec<MemoryKnowledgeSourceOutput>,
    next_actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeSourceOutput {
    name: String,
    state: String,
    current: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemoryKnowledgeSearchRequest {
    output: ControlOutputFormat,
    root: std::path::PathBuf,
    query: String,
    limit: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeSearchOutput {
    adapter: String,
    dry_run: bool,
    read_only: bool,
    connects_real_service: bool,
    writes_automatically: bool,
    runtime_retrieval_wired: bool,
    root: String,
    query: String,
    limit: usize,
    hit_count: usize,
    hits: Vec<MemoryKnowledgeSearchHitOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeSearchHitOutput {
    source: String,
    path: String,
    line: usize,
    score: u32,
    preview: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IdentityMaintenanceHealthOutput {
    root: String,
    user_chars: usize,
    user_max_chars: usize,
    memory_chars: usize,
    memory_max_chars: usize,
    experiences_chars: usize,
    experiences_file: String,
}

struct MemoryMaintenanceContext {
    identity_health: IdentityMaintenanceHealthOutput,
    batches: Vec<MemoryMaintenanceBatchOutput>,
    lim_candidate_count: usize,
    lim_candidates: Vec<LimExtractionCandidateOutput>,
    decay_candidate_count: usize,
    decay_candidates: Vec<MemoryDecayCandidateOutput>,
    recommendations: Vec<String>,
}

type MemoryMaintenancePlan = MemoryMaintenanceContext;

fn first_non_empty_line(content: &str) -> String {
    content
        .lines()
        .find(|line| !line.trim().is_empty())
        .map(|line| line.trim().chars().take(180).collect())
        .unwrap_or_else(|| "empty_turn_summary".to_string())
}

fn sanitize_candidate_id(raw: &str) -> String {
    let sanitized = raw
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string();
    if sanitized.is_empty() {
        "record".to_string()
    } else {
        sanitized
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IdentityMemoryShowOutput {
    root: String,
    user_file: String,
    memory_file: String,
    experiences_file: String,
    user_max_chars: usize,
    memory_max_chars: usize,
    user_chars: usize,
    memory_chars: usize,
    experiences_chars: usize,
    user: String,
    memory: String,
    experiences: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct IdentityMemoryMutationOutput {
    scope: String,
    id: Option<String>,
    written: bool,
    replaced: bool,
}
