use chuang_agent::context_engine::SegmentSource;
use chuang_agent::goal_mode::{GoalControlFile, GoalSpec, GoalStatus};

#[test]
fn goal_status_default_is_active_and_renders_snake_case() {
    assert_eq!(GoalStatus::default(), GoalStatus::Active);
    assert_eq!(GoalStatus::Active.as_str(), "active");
    assert_eq!(GoalStatus::Complete.as_str(), "complete");
    assert_eq!(GoalStatus::Blocked.as_str(), "blocked");
}

#[test]
fn goal_control_file_parse_accepts_complete_and_blocked() {
    let complete = GoalControlFile::parse("objective: do the thing\nstatus: complete\n");
    assert_eq!(complete.status, GoalStatus::Complete);
    assert_eq!(complete.objective.as_deref(), Some("do the thing"));

    let blocked = GoalControlFile::parse("objective: do the thing\nstatus: blocked\n");
    assert_eq!(blocked.status, GoalStatus::Blocked);
}

#[test]
fn goal_control_file_parse_is_case_insensitive_and_skips_comments() {
    let upper = GoalControlFile::parse("# GOAL control file\nstatus: COMPLETE\n");
    assert_eq!(upper.status, GoalStatus::Complete);

    let mixed = GoalControlFile::parse("status: Blocked\n");
    assert_eq!(mixed.status, GoalStatus::Blocked);
}

#[test]
fn goal_control_file_parse_invalid_value_fails_closed_to_blocked() {
    let invalid = GoalControlFile::parse("status: done\n");
    assert_eq!(invalid.status, GoalStatus::Blocked);

    let nonsense = GoalControlFile::parse("status: active\n");
    assert_eq!(nonsense.status, GoalStatus::Blocked);
}

#[test]
fn goal_control_file_parse_missing_or_malformed_fails_closed_to_blocked() {
    let missing = GoalControlFile::parse("objective: do the thing\n");
    assert_eq!(missing.status, GoalStatus::Blocked);

    let empty = GoalControlFile::parse("");
    assert_eq!(empty.status, GoalStatus::Blocked);

    let malformed = GoalControlFile::parse("not: yaml\nstatus:\n");
    assert_eq!(malformed.status, GoalStatus::Blocked);
}

#[test]
fn goal_control_file_render_writes_system_objective_and_status() {
    let rendered = GoalControlFile::render("system objective", GoalStatus::Complete);
    assert!(rendered.contains("objective: system objective"));
    assert!(rendered.contains("status: complete"));

    let rendered_blocked = GoalControlFile::render("system objective", GoalStatus::Blocked);
    assert!(rendered_blocked.contains("status: blocked"));
}

#[test]
fn goal_spec_control_file_contents_roundtrips_status() {
    let mut goal = GoalSpec::mainline_mvp("roundtrip control file");
    assert_eq!(goal.status, GoalStatus::Active);

    let contents = goal.control_file_contents();
    assert!(contents.contains(&format!("objective: {}", goal.objective)));

    goal.status = GoalStatus::Complete;
    let parsed = GoalControlFile::parse(&goal.control_file_contents());
    assert_eq!(parsed.status, GoalStatus::Complete);
    assert_eq!(parsed.objective.as_deref(), Some(goal.objective.as_str()));
}

#[test]
fn goal_spec_mainline_mvp_has_safe_defaults() {
    let goal = GoalSpec::mainline_mvp("stabilize the main execution chain");

    goal.validate().expect("default goal should be valid");
    assert_eq!(goal.goal_id, "mainline-mvp");
    assert_eq!(goal.status, GoalStatus::Active);
    assert!(goal.allowed_slots.contains(&"execution".to_string()));
    assert!(goal.allowed_slots.contains(&"governance".to_string()));
    assert_eq!(goal.budget.max_subtasks, Some(4));
    assert!(goal.checkpoint_policy.update_progress_log);
    assert!(goal.checkpoint_policy.update_handoff);
    assert!(goal.checkpoint_policy.commit_checkpoint);
    assert!(goal.final_report_policy.include_validation);
    assert!(goal.final_report_policy.include_next_steps);
}

#[test]
fn goal_spec_rejects_missing_required_fields() {
    let mut goal = GoalSpec::mainline_mvp("stabilize");
    goal.objective.clear();

    let err = goal.validate().expect_err("empty objective should fail");

    assert_eq!(err.field, "objective");
}

#[test]
fn goal_spec_rejects_empty_acceptance_checks_or_slots() {
    let mut goal = GoalSpec::mainline_mvp("stabilize");
    goal.acceptance_checks.clear();

    let err = goal
        .validate()
        .expect_err("empty acceptance checks should fail");

    assert_eq!(err.field, "acceptance_checks");

    let mut goal = GoalSpec::mainline_mvp("stabilize");
    goal.allowed_slots.clear();

    let err = goal
        .validate()
        .expect_err("empty allowed slots should fail");

    assert_eq!(err.field, "allowed_slots");
}

#[test]
fn goal_spec_renders_context_block_for_runtime_injection() {
    let goal = GoalSpec::mainline_mvp("stabilize the main execution chain");

    let block = goal
        .render_context_block()
        .expect("context block should render");

    assert!(block.contains("GOAL_SPEC"));
    assert!(block.contains("goal_id: mainline-mvp"));
    assert!(block.contains("objective: stabilize the main execution chain"));
    assert!(block.contains("- cargo fmt --all"));
    assert!(block.contains("allowed_slots: context,governance,execution,report,memory"));
    assert!(block.contains("max_subtasks=4"));
    assert!(block.contains("checkpoint_policy: progress_log=true handoff=true commit=true"));
    assert!(block.contains("final_report_policy: validation=true next_steps=true"));
}

#[test]
fn goal_spec_renders_context_segment_for_runtime_extra_context() {
    let goal = GoalSpec::mainline_mvp("inject goal without new slot");

    let segment = goal
        .render_context_segment()
        .expect("context segment should render");

    assert_eq!(segment.id, "goal-spec-mainline-mvp");
    assert_eq!(segment.source, SegmentSource::Goal);
    assert_eq!(
        segment.metadata.get("kind").map(String::as_str),
        Some("goal_spec")
    );
    assert_eq!(
        segment.metadata.get("goal_id").map(String::as_str),
        Some("mainline-mvp")
    );
    assert!(segment.content.contains("GOAL_SPEC"));
    assert!(segment
        .content
        .contains("objective: inject goal without new slot"));
}
