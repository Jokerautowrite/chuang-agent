# Experiment

experiment_id: provider-fallback-1777710884502618374
status: planned
time_budget_minutes: 20

## Goal

验证 provider fallback 策略

## Success Criteria

生成安全实验计划，不修改主工作区

## Fixed Safety Constraints

1. Do not run `git reset --hard`.
2. Do not delete files, directories, queues, reports, claims, memories, or credentials.
3. Do not purge, clean, uninstall, or destructively roll back state.
4. Keep all work outside the main branch unless 老爸 explicitly approves integration.
5. Produce an experiment report before any proposed integration.
6. Keep secrets out of logs, reports, fixtures, and chat output.

## Suggested Loop

1. Restate the target and success criteria.
2. Inspect only the files needed for the hypothesis.
3. Make the smallest isolated change or prototype.
4. Run the narrowest useful verification.
5. Write results, risks, and next recommendation.

## Result

Not run yet.
