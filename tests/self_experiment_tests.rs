use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::self_experiment::{
    ExperimentCompleteRequest, ExperimentOutcome, ExperimentRequest, SelfExperimentPlanner,
};

fn temp_root(name: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-self-experiment-{name}-{nanos}"))
}

#[test]
fn self_experiment_planner_writes_safe_experiment_plan() {
    let root = temp_root("plan");
    let planner = SelfExperimentPlanner::new(&root);

    let receipt = planner
        .create_plan(&ExperimentRequest {
            goal: "验证 provider fallback 策略".to_string(),
            success_criteria: "生成可审计报告，不修改主工作区".to_string(),
            time_budget_minutes: 20,
        })
        .expect("plan should be written");

    assert_eq!(receipt.status, "planned");
    assert_eq!(receipt.time_budget_minutes, 20);
    assert!(receipt.plan_path.ends_with("experiment.md"));
    let content = fs::read_to_string(&receipt.plan_path).expect("plan should exist");
    assert!(content.contains("验证 provider fallback 策略"));
    assert!(content.contains("生成可审计报告"));
    assert!(content.contains("Do not run `git reset --hard`"));
    assert!(content.contains("Do not delete files"));
}

#[test]
fn self_experiment_planner_rejects_missing_goal_or_budget() {
    let planner = SelfExperimentPlanner::new(temp_root("invalid"));

    let empty_goal = planner
        .create_plan(&ExperimentRequest {
            goal: " ".to_string(),
            success_criteria: "must pass".to_string(),
            time_budget_minutes: 1,
        })
        .expect_err("empty goal should fail");
    assert_eq!(empty_goal, "experiment_goal_required");

    let zero_budget = planner
        .create_plan(&ExperimentRequest {
            goal: "goal".to_string(),
            success_criteria: "must pass".to_string(),
            time_budget_minutes: 0,
        })
        .expect_err("zero budget should fail");
    assert_eq!(zero_budget, "experiment_time_budget_must_be_positive");
}

#[test]
fn self_experiment_planner_writes_completion_report_without_overwriting() {
    let root = temp_root("complete");
    let planner = SelfExperimentPlanner::new(&root);
    let receipt = planner
        .create_plan(&ExperimentRequest {
            goal: "验证安全实验报告".to_string(),
            success_criteria: "report.md 只追加一次".to_string(),
            time_budget_minutes: 10,
        })
        .expect("plan should be written");

    let report = planner
        .complete(&ExperimentCompleteRequest {
            experiment_id: receipt.experiment_id.clone(),
            outcome: ExperimentOutcome::Success,
            summary: "实验计划闭环已验证".to_string(),
            next_step: "后续再接沙箱执行".to_string(),
        })
        .expect("report should be written");

    assert_eq!(report.status, "completed");
    assert_eq!(report.outcome, "success");
    let content = fs::read_to_string(&report.report_path).expect("report should exist");
    assert!(content.contains("实验计划闭环已验证"));
    assert!(content.contains("No `git reset --hard` was performed"));

    let duplicate = planner
        .complete(&ExperimentCompleteRequest {
            experiment_id: receipt.experiment_id,
            outcome: ExperimentOutcome::Success,
            summary: "第二次写入".to_string(),
            next_step: "不应该覆盖".to_string(),
        })
        .expect_err("duplicate report should not overwrite");
    assert!(duplicate.contains("experiment_report_write_failed"));
}

#[test]
fn self_experiment_planner_lists_planned_and_completed_experiments() {
    let root = temp_root("list");
    let planner = SelfExperimentPlanner::new(&root);
    let planned = planner
        .create_plan(&ExperimentRequest {
            goal: "planned only".to_string(),
            success_criteria: "listed as planned".to_string(),
            time_budget_minutes: 5,
        })
        .expect("planned experiment should be created");
    let completed = planner
        .create_plan(&ExperimentRequest {
            goal: "completed".to_string(),
            success_criteria: "listed as completed".to_string(),
            time_budget_minutes: 5,
        })
        .expect("completed experiment should be created");
    planner
        .complete(&ExperimentCompleteRequest {
            experiment_id: completed.experiment_id.clone(),
            outcome: ExperimentOutcome::Inconclusive,
            summary: "done".to_string(),
            next_step: "none".to_string(),
        })
        .expect("completion should write report");

    let output = planner.list().expect("list should succeed");

    assert_eq!(output.count, 2);
    let planned_item = output
        .items
        .iter()
        .find(|item| item.experiment_id == planned.experiment_id)
        .expect("planned item should exist");
    assert_eq!(planned_item.status, "planned");
    assert!(planned_item.has_plan);
    assert!(!planned_item.has_report);

    let completed_item = output
        .items
        .iter()
        .find(|item| item.experiment_id == completed.experiment_id)
        .expect("completed item should exist");
    assert_eq!(completed_item.status, "completed");
    assert!(completed_item.has_plan);
    assert!(completed_item.has_report);
}

#[test]
fn self_experiment_planner_shows_plan_and_report_content() {
    let root = temp_root("show");
    let planner = SelfExperimentPlanner::new(&root);
    let receipt = planner
        .create_plan(&ExperimentRequest {
            goal: "show experiment".to_string(),
            success_criteria: "show returns markdown".to_string(),
            time_budget_minutes: 5,
        })
        .expect("experiment should be created");
    planner
        .complete(&ExperimentCompleteRequest {
            experiment_id: receipt.experiment_id.clone(),
            outcome: ExperimentOutcome::Success,
            summary: "show report summary".to_string(),
            next_step: "keep read-only".to_string(),
        })
        .expect("report should be written");

    let output = planner
        .show(&receipt.experiment_id)
        .expect("show should read artifacts");

    assert_eq!(output.status, "completed");
    assert!(output
        .plan_markdown
        .as_deref()
        .expect("plan markdown should exist")
        .contains("show experiment"));
    assert!(output
        .report_markdown
        .as_deref()
        .expect("report markdown should exist")
        .contains("show report summary"));
}
