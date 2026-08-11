//! `cli_memory` 模块。内部实现模块（无公开顶层项）。

use std::collections::{BTreeMap, BTreeSet};

use chrono::Utc;
use chuang_agent::diary::{today_local, DiaryConfig, FileDiaryStore};
use chuang_agent::experience_policy::{ExperienceCandidate, ExperienceWritePolicy};
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
        Some("diary") => diary_memory_command(&args[1..]),
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
        Some("preview-context") => memory_knowledge_preview_context_command(&args[1..]),
        Some("source-contract") => memory_knowledge_source_contract_command(&args[1..]),
        _ => Err(usage()),
    }
}

fn diary_memory_command(args: &[String]) -> Result<(), String> {
    match args.first().map(String::as_str) {
        Some("show") => diary_show_command(&args[1..]),
        Some("distill") => diary_distill_command(&args[1..]),
        Some("append") => diary_append_command(&args[1..]),
        _ => Err(usage()),
    }
}

/// 手动追加一条日记（节点总结）。seq 缺省时取当天最大 seq+1。
fn diary_append_command(args: &[String]) -> Result<(), String> {
    let request = parse_diary_append(args)?;
    let mut store = open_diary_store(&request.runtime_args)?;

    let seq = match request.seq {
        Some(seq) => seq,
        None => {
            let entries = store
                .read_date(&request.date)
                .map_err(|e| format!("diary_read_failed: {e:?}"))?;
            entries.iter().map(|entry| entry.seq).max().unwrap_or(0) + 1
        }
    };

    let entry = chuang_agent::diary::DiaryEntry {
        date: request.date.clone(),
        seq,
        created_at: chuang_agent::diary::now_local_hm(),
        session_id: request.session_id.clone(),
        trigger: request.trigger.clone(),
        completed: request.completed.clone(),
        in_progress: request.in_progress.clone(),
        pending: request.pending.clone(),
        constraints: request.constraints.clone(),
    };
    store
        .append(entry.clone())
        .map_err(|e| format!("diary_append_failed: {e:?}"))?;

    let output = DiaryAppendOutput {
        date: request.date,
        seq,
        session_id: request.session_id,
        trigger: request.trigger,
        written: true,
    };
    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "diary_appended date={} seq={} session={} trigger={}",
                output.date, output.seq, output.session_id, output.trigger
            );
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }
    Ok(())
}

fn diary_show_command(args: &[String]) -> Result<(), String> {
    let request = parse_diary_show(args)?;
    let store = open_diary_store(&request.runtime_args)?;
    let root = store.config().diary_root().display().to_string();
    let entries = store
        .read_date(&request.date)
        .map_err(|e| format!("diary_read_failed: {e:?}"))?;
    let output = DiaryShowOutput {
        date: request.date.clone(),
        root,
        entry_count: entries.len(),
        entries: entries
            .iter()
            .map(|entry| DiaryEntryOutput {
                created_at: entry.created_at.clone(),
                seq: entry.seq,
                session_id: entry.session_id.clone(),
                trigger: entry.trigger.clone(),
                completed: entry.completed.clone(),
                in_progress: entry.in_progress.clone(),
                pending: entry.pending.clone(),
                constraints: entry.constraints.clone(),
            })
            .collect(),
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!("diary_date: {}", output.date);
            println!("diary_root: {}", output.root);
            println!("diary_entry_count: {}", output.entry_count);
            for entry in &output.entries {
                println!(
                    "--- {} [seq={} trigger={}] session={} ---",
                    entry.created_at, entry.seq, entry.trigger, entry.session_id
                );
                println!("completed: {}", entry.completed);
                println!("in_progress: {}", entry.in_progress);
                println!("pending: {}", entry.pending);
                println!("constraints: {}", entry.constraints);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

/// 每日提炼：读取某天日记，用确定性经验策略过滤，把「值得沉淀」的条目
/// 追加进 experiences.md（每日从日记提炼经验，不再每轮直写）。
fn diary_distill_command(args: &[String]) -> Result<(), String> {
    let request = parse_diary_distill(args)?;
    let store = open_diary_store(&request.runtime_args)?;
    let entries = store
        .read_date(&request.date)
        .map_err(|e| format!("diary_read_failed: {e:?}"))?;

    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for entry in &entries {
        let candidate = ExperienceCandidate {
            user_input: "",
            summary: &entry.as_candidate_text(),
            lesson: "",
        };
        let decision = ExperienceWritePolicy::Deterministic.evaluate(&candidate);
        if decision.should_write() {
            accepted.push(entry.clone());
        } else {
            rejected.push(format!(
                "seq={} reason={}",
                entry.seq,
                decision.reason().as_str()
            ));
        }
    }

    let mut written_count = 0usize;
    if !request.dry_run {
        let dual_file_config = options_dual_file_config(&request.runtime_args)?;
        let mut memory_store = FileDualFileMemoryStore::open(dual_file_config.clone())
            .map_err(|e| format!("identity_memory_open_failed: {e:?}"))?;
        // 已提炼过的条目跳过（幂等：每日重复跑不产生重复 id）。
        let existing = memory_store
            .read_experiences()
            .map_err(|e| format!("identity_memory_read_failed: {e:?}"))?;
        let existing_ids = parse_experience_ids(&existing);
        for entry in &accepted {
            let id = format!("diary-{}-{}", entry.date, entry.seq);
            if existing_ids.contains(&id) {
                continue;
            }
            memory_store
                .append_experience(HotMemoryEntry {
                    id: id.clone(),
                    content: format!(
                        "source=diary_distill\ndate={}\nseq={}\nsession={}\ntrigger={}\n{}",
                        entry.date,
                        entry.seq,
                        entry.session_id,
                        entry.trigger,
                        entry.as_candidate_text()
                    ),
                })
                .map_err(format_identity_memory_error)?;
            written_count += 1;
        }
    }

    let output = DiaryDistillOutput {
        date: request.date.clone(),
        dry_run: request.dry_run,
        read_count: entries.len(),
        accepted_count: accepted.len(),
        rejected_count: rejected.len(),
        written_count,
        rejected,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "diary_distill date={} dry_run={} read={} accepted={} rejected={} written={}",
                output.date,
                output.dry_run,
                output.read_count,
                output.accepted_count,
                output.rejected_count,
                output.written_count
            );
            for reason in &output.rejected {
                println!("rejected {reason}");
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

/// 从 experiences.md 文本里提取已存在的条目 id（`**<id>**` 或 `id=<id>` 两种格式）。
fn parse_experience_ids(content: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in content.lines() {
        let line = line.trim();
        if let Some(id) = line.strip_prefix("## ") {
            let id = id.trim().to_string();
            if !id.is_empty() {
                ids.insert(id);
            }
        }
        if let Some(rest) = line.strip_prefix("**") {
            if let Some(id) = rest.split("**").next() {
                if !id.is_empty() {
                    ids.insert(id.to_string());
                }
            }
        }
        if let Some(id) = line.strip_prefix("id=") {
            let id = id.trim().to_string();
            if !id.is_empty() {
                ids.insert(id);
            }
        }
    }
    ids
}

fn open_diary_store(runtime_args: &[String]) -> Result<FileDiaryStore, String> {
    let dual_file_config = options_dual_file_config(runtime_args)?;
    let diary_config = DiaryConfig::new(dual_file_config.root);
    FileDiaryStore::open(diary_config).map_err(|e| format!("diary_open_failed: {e:?}"))
}

fn options_dual_file_config(
    runtime_args: &[String],
) -> Result<chuang_agent::hermes_memory::DualFileMemoryConfig, String> {
    let options = parse_cli_options(runtime_args)?;
    options
        .runtime
        .identity_memory
        .build_dual_file_config()
        .map_err(|e| format!("config_invalid: {}: {}", e.field, e.message))
}

fn parse_diary_show(args: &[String]) -> Result<DiaryShowRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut date = today_local();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--date" => {
                date = take_local_value(args, &mut index, "--date")?;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }
    Ok(DiaryShowRequest {
        runtime_args,
        output,
        date,
    })
}

fn parse_diary_distill(args: &[String]) -> Result<DiaryDistillRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut date = today_local();
    let mut dry_run = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--date" => {
                date = take_local_value(args, &mut index, "--date")?;
            }
            "--dry-run" => {
                dry_run = true;
                index += 1;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }
    Ok(DiaryDistillRequest {
        runtime_args,
        output,
        date,
        dry_run,
    })
}

fn parse_diary_append(args: &[String]) -> Result<DiaryAppendRequest, String> {
    let mut runtime_args = Vec::new();
    let mut output = ControlOutputFormat::Text;
    let mut date = today_local();
    let mut session_id = "manual".to_string();
    let mut trigger = "manual".to_string();
    let mut seq = None;
    let mut completed = String::new();
    let mut in_progress = String::new();
    let mut pending = String::new();
    let mut constraints = String::new();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--date" => {
                date = take_local_value(args, &mut index, "--date")?;
            }
            "--seq" => {
                let raw = take_local_value(args, &mut index, "--seq")?;
                seq = Some(raw.parse::<u64>().map_err(|_| {
                    format!("memory diary append requires numeric --seq, got: {raw}")
                })?);
            }
            "--session-id" => {
                session_id = take_local_value(args, &mut index, "--session-id")?;
            }
            "--trigger" => {
                trigger = take_local_value(args, &mut index, "--trigger")?;
            }
            "--completed" => {
                completed = take_local_value(args, &mut index, "--completed")?;
            }
            "--in-progress" => {
                in_progress = take_local_value(args, &mut index, "--in-progress")?;
            }
            "--pending" => {
                pending = take_local_value(args, &mut index, "--pending")?;
            }
            "--constraints" => {
                constraints = take_local_value(args, &mut index, "--constraints")?;
            }
            "--config" | "--identity-memory-root" | "--db" => {
                push_value_arg(args, &mut index, &mut runtime_args)?
            }
            _ => return Err(usage()),
        }
    }
    if completed.trim().is_empty() && in_progress.trim().is_empty() {
        return Err(
            "memory diary append requires --completed or --in-progress (both empty is useless)"
                .to_string(),
        );
    }
    Ok(DiaryAppendRequest {
        runtime_args,
        output,
        date,
        seq,
        session_id,
        trigger,
        completed,
        in_progress,
        pending,
        constraints,
    })
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
                    "hit path={} line={} score={} source={} query={} read_only={} connects_real_service={}",
                    hit.path,
                    hit.line,
                    hit.score,
                    hit.source,
                    hit.provenance.query,
                    hit.provenance.read_only,
                    hit.provenance.connects_real_service
                );
                println!(
                    "evidence local_file={} line={} score={} read_only={} connects_real_service={}",
                    hit.evidence.local_file,
                    hit.evidence.line,
                    hit.evidence.score,
                    hit.evidence.read_only,
                    hit.evidence.connects_real_service
                );
                println!("{}", hit.preview);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

fn memory_knowledge_preview_context_command(args: &[String]) -> Result<(), String> {
    let request = parse_memory_knowledge_preview_context(args)?;
    let output = preview_local_knowledge_context(&request.root, &request.query, request.limit)?;

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "memory_knowledge_preview_context adapter={} preview=true read_only=true root={} query={} segments={}",
                output.adapter, output.root, output.query, output.segment_count
            );
            println!(
                "preview_only=true connects_real_service=false writes_automatically=false runtime_injection_applied=false runtime_retrieval_wired=false"
            );
            println!(
                "runtime context preview only; no runtime injection was applied and runtime retrieval is not wired"
            );
            for segment in &output.segments {
                println!(
                    "segment id={} source={} path={} line={} score={} token_estimate={} read_only={} connects_real_service={}",
                    segment.segment_id,
                    segment.source,
                    segment.path,
                    segment.line,
                    segment.score,
                    segment.token_estimate,
                    segment.read_only,
                    segment.connects_real_service
                );
                println!(
                    "provenance source={} adapter={} local_file={} line={} score={} writes_automatically={}",
                    segment.provenance.source,
                    segment.provenance.adapter,
                    segment.provenance.local_file,
                    segment.provenance.line,
                    segment.provenance.score,
                    segment.provenance.writes_automatically
                );
                println!(
                    "evidence kind={} local_file={} line={} score={} read_only={} connects_real_service={}",
                    segment.evidence.kind,
                    segment.evidence.local_file,
                    segment.evidence.line,
                    segment.evidence.score,
                    segment.evidence.read_only,
                    segment.evidence.connects_real_service
                );
                println!("{}", segment.preview);
            }
        }
        ControlOutputFormat::Json => print_json(&output)?,
    }

    Ok(())
}

pub(crate) fn preview_local_knowledge_context(
    root: &std::path::Path,
    query: &str,
    limit: usize,
) -> Result<MemoryKnowledgePreviewContextOutput, String> {
    let hits = search_local_knowledge_root(root, query, limit)?;
    let segments = build_memory_knowledge_context_segments(hits);
    Ok(MemoryKnowledgePreviewContextOutput {
        adapter: "local_external_knowledge".to_string(),
        preview: true,
        read_only: true,
        connects_real_service: false,
        writes_automatically: false,
        runtime_injection_applied: false,
        runtime_retrieval_wired: false,
        root: root.display().to_string(),
        query: query.to_string(),
        limit,
        segment_count: segments.len(),
        segments,
    })
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
                current: "external-brain source is documented but live retrieval is pending/gated"
                    .to_string(),
            },
            MemoryKnowledgeSourceOutput {
                name: "gbrain".to_string(),
                state: "documented_only".to_string(),
                current:
                    "knowledge base boundary is documented; runtime retrieval stays pending/gated"
                        .to_string(),
            },
        ],
        next_actions: vec![
            "keep local read-only preview and source-contract explicit".to_string(),
            "add provenance-bearing search before any live runtime injection".to_string(),
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
                "runtime_retrieval_wired: {} doc: {} live_retrieval_pending_gated=true",
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

fn memory_knowledge_source_contract_command(args: &[String]) -> Result<(), String> {
    let request = parse_memory_knowledge_source_contract(args)?;
    let output = build_memory_knowledge_source_contract(&request.source);

    match request.output {
        ControlOutputFormat::Json => print_json(&output)?,
        ControlOutputFormat::Text => {
            println!(
                "memory_knowledge_source_contract source={} adapter={} read_only={} live_adapter_configured={} connects_real_service={} writes_automatically={} runtime_retrieval_wired={}",
                output.source,
                output.adapter,
                output.read_only,
                output.live_adapter_configured,
                output.connects_real_service,
                output.writes_automatically,
                output.runtime_retrieval_wired
            );
            println!(
                "boundary requires_operator_credentials={} stores_secret_in_repo={} writes_core_memory={} requires_provenance={} requires_evidence={}",
                output.boundary.requires_operator_credentials,
                output.boundary.stores_secret_in_repo,
                output.boundary.writes_core_memory,
                output.boundary.requires_provenance,
                output.boundary.requires_evidence
            );
            for field in &output.request_fields {
                println!("request_field: {field}");
            }
            for field in &output.response_fields {
                println!("response_field: {field}");
            }
        }
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
        boundary: memory_maintenance_boundary(),
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
                "memory_layering archive={} maintenance={} decay={} writeback={} core_memory_rewrite_allowed={} archive_mutation_allowed={} automatic_writeback={}",
                output.boundary.archive_layer,
                output.boundary.maintenance_layer,
                output.boundary.decay_boundary,
                output.boundary.writeback_target,
                output.boundary.core_memory_rewrite_allowed,
                output.boundary.archive_mutation_allowed,
                output.boundary.automatic_writeback
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
    let requested_candidate_ids = if request.candidate_ids.is_empty() {
        plan.lim_candidates
            .iter()
            .map(|candidate| candidate.candidate_id.clone())
            .collect::<Vec<_>>()
    } else {
        request.candidate_ids.clone()
    };
    let requested_candidate_count = requested_candidate_ids.len();

    let mut unique_selected_candidate_ids = Vec::new();
    let mut duplicate_candidate_ids = Vec::new();
    let mut seen = BTreeSet::new();
    for candidate_id in requested_candidate_ids {
        if seen.insert(candidate_id.clone()) {
            unique_selected_candidate_ids.push(candidate_id);
        } else {
            duplicate_candidate_ids.push(candidate_id);
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

    let selection_state = if unique_selected_candidate_ids.is_empty() {
        "empty".to_string()
    } else if !duplicate_candidate_ids.is_empty() {
        "deduplicated".to_string()
    } else {
        "selected".to_string()
    };
    let selection_reason = if unique_selected_candidate_ids.is_empty() {
        "no_lim_candidates".to_string()
    } else if !request.candidate_ids.is_empty() {
        if !duplicate_candidate_ids.is_empty() {
            "duplicate_candidate_ids_deduplicated".to_string()
        } else {
            "explicit_candidate_ids".to_string()
        }
    } else {
        "plan_candidates".to_string()
    };

    let approval = MemoryMaintenanceApprovalOutput {
        required: true,
        approved: request.approve_writeback,
        approval_source: if request.approve_writeback {
            Some("cli --approve-writeback".to_string())
        } else {
            None
        },
        approval_note: request.approval_note.clone(),
        approved_at: request.approve_writeback.then(|| Utc::now().to_rfc3339()),
        writeback_scope: "experiences".to_string(),
        writes_automatically: false,
    };

    let mut applied_candidate_ids = Vec::new();
    let mut skipped_candidate_ids = Vec::new();
    if request.approve_writeback && !selected_candidates.is_empty() {
        let mut store = open_identity_memory_store(&request.runtime_args)?;
        for candidate in &selected_candidates {
            match store.append_experience(HotMemoryEntry {
                id: candidate.candidate_id.clone(),
                content: build_memory_maintenance_writeback_content(candidate, &approval),
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
        boundary: memory_maintenance_boundary(),
        query: request.queries.first().cloned().unwrap_or_default(),
        queries: request.queries,
        session_id: request.session_id,
        limit: request.limit,
        identity_health: plan.identity_health,
        batch_count: plan.batches.len(),
        batches: plan.batches,
        lim_candidate_count: plan.lim_candidate_count,
        decay_candidate_count: plan.decay_candidate_count,
        requested_candidate_count,
        duplicate_candidate_count: duplicate_candidate_ids.len(),
        duplicate_candidate_ids,
        selection_state,
        selection_reason,
        selected_candidates,
        selected_candidate_ids: unique_selected_candidate_ids,
        approval,
        applied_candidate_ids,
        skipped_candidate_ids,
        recommendations: plan.recommendations,
    };

    match request.output {
        ControlOutputFormat::Text => {
            println!(
                "memory_maintenance_apply dry_run={} writes_automatically=false approved_writeback={} query={} session_id={} requested={} unique={} duplicates={} selection_state={} selection_reason={} applied={} skipped={}",
                output.dry_run,
                output.approved_writeback,
                output.query,
                output.session_id.as_deref().unwrap_or("any"),
                output.requested_candidate_count,
                output.selected_candidate_ids.len(),
                output.duplicate_candidate_count,
                output.selection_state,
                output.selection_reason,
                output.applied_candidate_ids.len(),
                output.skipped_candidate_ids.len()
            );
            println!(
                "approval required={} approved={} source={} scope={} writes_automatically={}",
                output.approval.required,
                output.approval.approved,
                output.approval.approval_source.as_deref().unwrap_or("none"),
                output.approval.writeback_scope,
                output.approval.writes_automatically
            );
            println!(
                "memory_layering archive={} maintenance={} decay={} writeback={} core_memory_rewrite_allowed={} archive_mutation_allowed={} automatic_writeback={}",
                output.boundary.archive_layer,
                output.boundary.maintenance_layer,
                output.boundary.decay_boundary,
                output.boundary.writeback_target,
                output.boundary.core_memory_rewrite_allowed,
                output.boundary.archive_mutation_allowed,
                output.boundary.automatic_writeback
            );
            for candidate_id in &output.duplicate_candidate_ids {
                println!("duplicate_candidate_id: {candidate_id}");
            }
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

fn build_memory_maintenance_writeback_content(
    candidate: &LimExtractionCandidateOutput,
    approval: &MemoryMaintenanceApprovalOutput,
) -> String {
    let mut lines = vec![
        "writeback=memory_maintenance_apply".to_string(),
        "approved_writeback=true".to_string(),
        format!(
            "approval_source={}",
            approval.approval_source.as_deref().unwrap_or("none")
        ),
        format!(
            "approved_at={}",
            approval.approved_at.as_deref().unwrap_or("unknown")
        ),
        "provenance_preserved=true".to_string(),
    ];
    if let Some(note) = &approval.approval_note {
        lines.push(format!("approval_note={note}"));
    }
    lines.push(candidate.content.clone());
    lines.join("\n")
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

fn memory_maintenance_boundary() -> MemoryMaintenanceBoundaryOutput {
    MemoryMaintenanceBoundaryOutput {
        archive_layer: "history_session_archive".to_string(),
        archive_source: "sqlite turn_summary records".to_string(),
        archive_read_only: true,
        archive_mutation_allowed: false,
        maintenance_layer: "maintenance_runtime".to_string(),
        maintenance_mode: "dry_run_report_then_explicit_apply".to_string(),
        decay_boundary: "review_only_not_writeback_candidate".to_string(),
        decay_writeback_allowed: false,
        writeback_target: "experiences.md".to_string(),
        lim_writeback_requires_approval: true,
        core_memory_rewrite_allowed: false,
        automatic_writeback: false,
    }
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
            // CLI 诊断搜索要求精确短语命中，避免近似内容（如"锚点A" vs "锚点B"）
            // 被 token 模糊召回误判为命中；agent 自动召回仍走 Token 模糊模式。
            match_mode: chuang_agent::memory_store::MemoryMatchMode::ExactPhrase,
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
    let mut approval_note = None;
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
            "--approval-note" => {
                let value = take_local_value(args, &mut index, "--approval-note")?;
                let normalized = normalize_approval_note(&value);
                if normalized.is_empty() {
                    return Err(
                        "memory maintenance apply requires non-empty --approval-note".to_string(),
                    );
                }
                approval_note = Some(normalized);
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
        approval_note,
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
    parse_memory_knowledge_query(args, "search")
}

fn parse_memory_knowledge_preview_context(
    args: &[String],
) -> Result<MemoryKnowledgeSearchRequest, String> {
    parse_memory_knowledge_query(args, "preview-context")
}

fn parse_memory_knowledge_source_contract(
    args: &[String],
) -> Result<MemoryKnowledgeSourceContractRequest, String> {
    let mut output = ControlOutputFormat::Text;
    let mut source: Option<String> = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--json" => {
                output = ControlOutputFormat::Json;
                index += 1;
            }
            "--source" => {
                source = Some(take_local_value(args, &mut index, "--source")?);
            }
            _ => return Err(usage()),
        }
    }
    let source =
        source.ok_or_else(|| "memory knowledge source-contract requires --source".to_string())?;
    if !matches!(source.as_str(), "wiki" | "gbrain") {
        return Err("memory knowledge source-contract supports --source wiki|gbrain".to_string());
    }
    Ok(MemoryKnowledgeSourceContractRequest { output, source })
}

fn build_memory_knowledge_source_contract(source: &str) -> MemoryKnowledgeSourceContractOutput {
    MemoryKnowledgeSourceContractOutput {
        source: source.to_string(),
        adapter: format!("{source}_readonly_external_knowledge"),
        read_only: true,
        live_adapter_configured: false,
        connects_real_service: false,
        writes_automatically: false,
        runtime_retrieval_wired: false,
        request_fields: vec![
            "query".to_string(),
            "limit".to_string(),
            "operator_scope".to_string(),
            "audit_label".to_string(),
        ],
        response_fields: vec![
            "hits[].source".to_string(),
            "hits[].path_or_url".to_string(),
            "hits[].score".to_string(),
            "hits[].preview".to_string(),
            "hits[].provenance".to_string(),
            "hits[].evidence".to_string(),
        ],
        boundary: MemoryKnowledgeSourceContractBoundary {
            requires_operator_credentials: true,
            stores_secret_in_repo: false,
            writes_core_memory: false,
            requires_provenance: true,
            requires_evidence: true,
            approval_required_for_writeback: true,
        },
    }
}

fn parse_memory_knowledge_query(
    args: &[String],
    command_name: &str,
) -> Result<MemoryKnowledgeSearchRequest, String> {
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
                limit = value.parse::<usize>().map_err(|_| {
                    format!("memory knowledge {command_name} requires numeric --limit")
                })?;
                if limit == 0 {
                    return Err(format!(
                        "memory knowledge {command_name} requires --limit > 0"
                    ));
                }
            }
            _ => return Err(usage()),
        }
    }

    let root = root.ok_or_else(|| format!("memory knowledge {command_name} requires --root"))?;
    let query = query.ok_or_else(|| format!("memory knowledge {command_name} requires --query"))?;
    if query.trim().is_empty() {
        return Err(format!(
            "memory knowledge {command_name} requires non-empty --query"
        ));
    }

    Ok(MemoryKnowledgeSearchRequest {
        output,
        root,
        query,
        limit,
    })
}

fn build_memory_knowledge_context_segments(
    hits: Vec<MemoryKnowledgeSearchHitOutput>,
) -> Vec<MemoryKnowledgeContextSegmentOutput> {
    hits.into_iter()
        .enumerate()
        .map(|(index, hit)| MemoryKnowledgeContextSegmentOutput {
            segment_id: format!("knowledge-segment-{}", index + 1),
            source: hit.source,
            path: hit.path,
            line: hit.line,
            score: hit.score,
            preview: hit.preview.clone(),
            token_estimate: estimate_tokenish_count(&hit.preview),
            read_only: true,
            connects_real_service: false,
            writes_automatically: false,
            runtime_injection_applied: false,
            runtime_retrieval_wired: false,
            provenance: hit.provenance,
            evidence: hit.evidence,
        })
        .collect()
}

fn estimate_tokenish_count(text: &str) -> usize {
    let chars = text.chars().count();
    ((chars + 3) / 4).max(1)
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
                let score = score_knowledge_line(&line_lower, &needle);
                let line_number = line_index + 1;
                let preview: String = line.trim().chars().take(240).collect();
                hits.push(MemoryKnowledgeSearchHitOutput {
                    source: "local_file".to_string(),
                    path: relative.clone(),
                    line: line_number,
                    score,
                    preview: preview.clone(),
                    provenance: MemoryKnowledgeSearchProvenanceOutput {
                        source: "local_file".to_string(),
                        adapter: "local_external_knowledge".to_string(),
                        local_file: relative.clone(),
                        line: line_number,
                        score,
                        query: query.to_string(),
                        read_only: true,
                        connects_real_service: false,
                        writes_automatically: false,
                    },
                    evidence: MemoryKnowledgeSearchEvidenceOutput {
                        kind: "line_match".to_string(),
                        local_file: relative,
                        line: line_number,
                        score,
                        query: query.to_string(),
                        preview,
                        read_only: true,
                        connects_real_service: false,
                    },
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
    approval_note: Option<String>,
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
    boundary: MemoryMaintenanceBoundaryOutput,
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
    boundary: MemoryMaintenanceBoundaryOutput,
    query: String,
    queries: Vec<String>,
    session_id: Option<String>,
    limit: usize,
    identity_health: IdentityMaintenanceHealthOutput,
    batch_count: usize,
    batches: Vec<MemoryMaintenanceBatchOutput>,
    lim_candidate_count: usize,
    decay_candidate_count: usize,
    requested_candidate_count: usize,
    duplicate_candidate_count: usize,
    duplicate_candidate_ids: Vec<String>,
    selection_state: String,
    selection_reason: String,
    selected_candidates: Vec<LimExtractionCandidateOutput>,
    selected_candidate_ids: Vec<String>,
    approval: MemoryMaintenanceApprovalOutput,
    applied_candidate_ids: Vec<String>,
    skipped_candidate_ids: Vec<String>,
    recommendations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryMaintenanceApprovalOutput {
    required: bool,
    approved: bool,
    approval_source: Option<String>,
    approval_note: Option<String>,
    approved_at: Option<String>,
    writeback_scope: String,
    writes_automatically: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryMaintenanceBoundaryOutput {
    archive_layer: String,
    archive_source: String,
    archive_read_only: bool,
    archive_mutation_allowed: bool,
    maintenance_layer: String,
    maintenance_mode: String,
    decay_boundary: String,
    decay_writeback_allowed: bool,
    writeback_target: String,
    lim_writeback_requires_approval: bool,
    core_memory_rewrite_allowed: bool,
    automatic_writeback: bool,
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

struct MemoryKnowledgeSourceContractRequest {
    output: ControlOutputFormat,
    source: String,
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
pub(crate) struct MemoryKnowledgePreviewContextOutput {
    pub(crate) adapter: String,
    pub(crate) preview: bool,
    pub(crate) read_only: bool,
    pub(crate) connects_real_service: bool,
    pub(crate) writes_automatically: bool,
    pub(crate) runtime_injection_applied: bool,
    pub(crate) runtime_retrieval_wired: bool,
    pub(crate) root: String,
    pub(crate) query: String,
    pub(crate) limit: usize,
    pub(crate) segment_count: usize,
    pub(crate) segments: Vec<MemoryKnowledgeContextSegmentOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MemoryKnowledgeContextSegmentOutput {
    pub(crate) segment_id: String,
    pub(crate) source: String,
    pub(crate) path: String,
    pub(crate) line: usize,
    pub(crate) score: u32,
    pub(crate) preview: String,
    pub(crate) token_estimate: usize,
    pub(crate) read_only: bool,
    pub(crate) connects_real_service: bool,
    pub(crate) writes_automatically: bool,
    pub(crate) runtime_injection_applied: bool,
    pub(crate) runtime_retrieval_wired: bool,
    pub(crate) provenance: MemoryKnowledgeSearchProvenanceOutput,
    pub(crate) evidence: MemoryKnowledgeSearchEvidenceOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeSearchHitOutput {
    source: String,
    path: String,
    line: usize,
    score: u32,
    preview: String,
    provenance: MemoryKnowledgeSearchProvenanceOutput,
    evidence: MemoryKnowledgeSearchEvidenceOutput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MemoryKnowledgeSearchProvenanceOutput {
    pub(crate) source: String,
    pub(crate) adapter: String,
    pub(crate) local_file: String,
    pub(crate) line: usize,
    pub(crate) score: u32,
    pub(crate) query: String,
    pub(crate) read_only: bool,
    pub(crate) connects_real_service: bool,
    pub(crate) writes_automatically: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct MemoryKnowledgeSearchEvidenceOutput {
    pub(crate) kind: String,
    pub(crate) local_file: String,
    pub(crate) line: usize,
    pub(crate) score: u32,
    pub(crate) query: String,
    pub(crate) preview: String,
    pub(crate) read_only: bool,
    pub(crate) connects_real_service: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeSourceContractOutput {
    source: String,
    adapter: String,
    read_only: bool,
    live_adapter_configured: bool,
    connects_real_service: bool,
    writes_automatically: bool,
    runtime_retrieval_wired: bool,
    request_fields: Vec<String>,
    response_fields: Vec<String>,
    boundary: MemoryKnowledgeSourceContractBoundary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct MemoryKnowledgeSourceContractBoundary {
    requires_operator_credentials: bool,
    stores_secret_in_repo: bool,
    writes_core_memory: bool,
    requires_provenance: bool,
    requires_evidence: bool,
    approval_required_for_writeback: bool,
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

fn normalize_approval_note(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiaryShowRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    date: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiaryDistillRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    date: String,
    dry_run: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct DiaryAppendRequest {
    runtime_args: Vec<String>,
    output: ControlOutputFormat,
    date: String,
    seq: Option<u64>,
    session_id: String,
    trigger: String,
    completed: String,
    in_progress: String,
    pending: String,
    constraints: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiaryShowOutput {
    date: String,
    root: String,
    entry_count: usize,
    entries: Vec<DiaryEntryOutput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiaryEntryOutput {
    created_at: String,
    seq: u64,
    session_id: String,
    trigger: String,
    completed: String,
    in_progress: String,
    pending: String,
    constraints: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiaryDistillOutput {
    date: String,
    dry_run: bool,
    read_count: usize,
    accepted_count: usize,
    rejected_count: usize,
    written_count: usize,
    rejected: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct DiaryAppendOutput {
    date: String,
    seq: u64,
    session_id: String,
    trigger: String,
    written: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chuang_agent::diary::DiaryEntry;

    fn temp_identity_root(tag: &str) -> std::path::PathBuf {
        let root = std::env::temp_dir().join(format!(
            "chuang-cli-diary-{tag}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&root).expect("identity root should be created");
        root
    }

    fn write_sample_diary(root: &std::path::Path, date: &str) {
        let config = DiaryConfig::new(root.to_path_buf());
        let mut store = FileDiaryStore::open(config).expect("diary store should open");
        // 一条「可沉淀」：跨会话约束/偏好
        store
            .append(DiaryEntry {
                date: date.to_string(),
                seq: 3,
                created_at: "10:00".to_string(),
                session_id: "s1".to_string(),
                trigger: "completion_signal".to_string(),
                completed: "完成了跨会话记忆链路的调整，经验从日记每日提炼".to_string(),
                in_progress: "验证每日提炼命令".to_string(),
                pending: "后续由用户指令决定".to_string(),
                constraints: "经验只从日记提炼，不每轮直写；禁止删除任何文件".to_string(),
            })
            .expect("append should succeed");
        // 一条「噪音」：一次性调试信息（应被过滤）
        store
            .append(DiaryEntry {
                date: date.to_string(),
                seq: 5,
                created_at: "10:30".to_string(),
                session_id: "s1".to_string(),
                trigger: "turn_threshold".to_string(),
                completed: "临时修复了一次端口占用，debug 堆栈如下".to_string(),
                in_progress: "继续观察".to_string(),
                pending: "临时方案".to_string(),
                constraints: "本次报错是一次性调试，无跨会话价值".to_string(),
            })
            .expect("append should succeed");
    }

    #[test]
    fn diary_distill_writes_only_durable_entries_to_experiences() {
        // parse_cli_options 会加载 config.toml 并校验 provider env（测试用桩值）
        unsafe {
            std::env::set_var("CHUANG_PROXY_API_KEY", "test-key");
            std::env::set_var("CHUANG_PROXY_STATIC_KEY", "test-static-key");
        }
        let root = temp_identity_root("distill");
        let date = "2026-08-11";
        write_sample_diary(&root, date);

        let mut args = vec![
            "--identity-memory-root".to_string(),
            root.display().to_string(),
            "--date".to_string(),
            date.to_string(),
        ];
        diary_distill_command(&args).expect("distill should succeed");

        let store = FileDualFileMemoryStore::open(
            chuang_agent::hermes_memory::DualFileMemoryConfig::new(root.clone()),
        )
        .expect("memory store should open");
        let experiences = store
            .read_experiences()
            .expect("experiences should be readable");
        assert!(
            experiences.contains("diary-2026-08-11-3"),
            "durable diary entry should be distilled into experiences"
        );
        assert!(
            !experiences.contains("diary-2026-08-11-5"),
            "noisy diary entry should be filtered out"
        );

        // 幂等：重复 distill 不产生重复 id
        diary_distill_command(&args).expect("second distill should succeed");
        let experiences2 = store.read_experiences().expect("experiences readable");
        assert_eq!(experiences, experiences2, "distill should be idempotent");

        // dry-run 不写盘
        args.push("--dry-run".to_string());
        diary_distill_command(&args).expect("dry-run distill should succeed");
        let experiences3 = store.read_experiences().expect("experiences readable");
        assert_eq!(experiences, experiences3, "dry-run must not write");
    }
}
