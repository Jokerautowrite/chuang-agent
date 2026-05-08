# Goal Mode Operating Plan

更新时间：2026-05-08

## 定位

这里记录当前 Codex 侧采用的目标驱动推进方式，以及后续迁移到 Chuang 的目标态。

当前阶段它是协作流程，不是 Chuang runtime 的内核模块。不要为了“goal mode”新增核心 slot，也不要打断现有主链：

```text
input -> identity/memory -> context -> runtime -> governance -> execution slot -> report -> memory
```

## 当前用法

每轮推进先固定四件事：

1. Goal：本轮只推进一个主线目标。
2. Acceptance：写清楚可验证结果。
3. Budget：限定本轮范围，避免扩散到子代理、飞书或外部智能体。
4. Checkpoint：结束前更新 `docs/progress-log.md` 和 `docs/handoff-current.md`。
5. Continuation：结束时保留可恢复 checkpoint，下一轮优先从 `GoalRun` 续接，尽量不要求操作员重复说 `continue`。

当前默认 goal：

```text
补全 Chuang 主进程 Execution Slot，让主进程能稳定调用 GA 原子工具映射，并把治理、审计、结构化回传打通。
```

当前验收标准：

```text
cargo fmt --all
git diff --check
timeout 240s cargo test -q
```

## Codex 0.128 实战借鉴

2026-05-03 用 Codex 子代理按 goal-style 连续推进 Chuang 主线，验证出一套可迁移到 Chuang 的长期任务组织方式。当前 Codex 侧没有可直接稳定调用的显式 `goal` 子命令，因此本轮使用 `GOAL_SPEC` 文本契约驱动子代理；对 Chuang 来说，真正值得吸收的是执行组织模式，不是某个 CLI 命令名字。

这条也同步写入飞书架构终稿：**Codex 是 Chuang 的 Rust 骨架参考实现。后续本地执行、安全边界、审批、沙箱、验证、回传、goal-style 长任务推进和子代理组织方式，先审计 Codex Rust 源码与现有行为，再决定移植、裁剪或复用接口。少造轮子，多复制成熟实现。**

### 已验证有效的模式

1. 主进程只做目标拆分、边界定义、集成审核和最终提交。
2. 每个子代理必须拿到完整 `GOAL_SPEC`：目标、写入范围、禁止事项、验收命令、最终报告格式。
3. 子代理任务必须按文件和模块拆开，避免并行写同一核心文件。
4. 子代理不直接提交，主进程统一审 diff、跑格式检查、跑 smoke、跑全量测试，再按逻辑提交。
5. 每轮完成后关闭子代理，避免后台会话残留。
6. 私有 `config.toml`、飞书桥、Hermes、本机密钥和真实服务控制都不进入子代理写入范围。

本轮验证过的分工样例：

```text
Worker A: tool / GA atomic manifest
Worker B: governance / runtime observability
Worker C: identity / memory / context diagnostics
Worker D: provider / fallback diagnostics
Worker E: report / audit identity
Worker F: config / doctor / readiness
Worker G: app-server / channel protocol
Worker H: control / actuator command contract
```

### 迁移成 Chuang 能力时的最小设计

Chuang 后续可以在现有 `GoalSpec` 基础上增加一个轻量 `GoalRun` 概念，但仍不新增 core slot。它是 checkpointable 的 planning primitive：

```text
GoalRun
  -> goal_spec
  -> worker_plan[]
  -> disjoint_write_scopes[]
  -> validation_plan
  -> integration_policy
  -> checkpoint_log
  -> diagnostics
```

`goal_spec.budget.max_subtasks` 是并发 worker 的显式预算上限。它不是执行器本身，但它让 goal 计划可以声明“今天最多并行几条子任务”，并在 `GoalRun` 里做硬校验。

当前 continuation model 是 checkpoint-first：目标计划保留接续点，下一轮优先恢复 `GoalRun`，而不是把 `continue` 当成协议核心。
每次记录 checkpoint 时必须写上完成者和验证备注；没有这些证据的 checkpoint 会被拒绝落盘。

它应该落在现有主链外壳上：

```text
GoalSpec -> Governance -> Context -> Execution Slot -> Report -> Memory
```

子代理并行属于 Execution Slot 下游能力。主进程必须保留唯一集成权：

- 子代理只能产出 patch / report / validation result。
- 主进程负责合并、复验、提交、更新记忆。
- 失败子代理必须有结构化失败报告，不能悄悄消失。
- 大任务优先拆为 2 个子代理，稳定后再扩到 3-4 个。

### 不迁移的部分

- 不把 Codex 的当前实现细节硬编码进 Chuang。
- 不把 goal mode 做成无限后台循环。
- 不允许子代理绕过 governance、approval、audit。
- 不让子代理直接操作真实飞书、Hermes、密钥、本机服务或桌面控制。

## 少造轮子原则

后续实现顺序固定为：

1. 先找 Codex Rust 是否已有成熟实现。
2. 能移植就移植，能裁剪就裁剪，能按协议适配就适配。
3. 只有当 Codex 的实现与 Chuang 的记忆本体、可拔插边界或本机安全约束冲突时，才写新的实现。
4. 新抽象必须说明替换点和收益；不能为了“看起来架构化”多包一层。

## 近期优先级

1. 继续收紧 Execution Slot 的正式 action/request schema，减少纯字符串协议。
2. 继续让 app-server / channel / runtime report 共用同一套工具事件和工具报告结构。
3. 补治理策略配置和审计字段，不让主进程工具绕过审批边界。
4. 主进程稳定后再做子代理 runner 增强。
5. 子代理稳定后再做外部智能体和搜索能力。

## 迁移到 Chuang 的目标态

未来 Chuang 可以把 goal mode 做成 `Governance + Runtime + Memory` 的轻量能力：

```text
GoalSpec
  -> goal_id
  -> objective
  -> acceptance_checks
  -> budget
  -> allowed_slots
  -> checkpoint_policy
  -> final_report_policy
```

目标执行仍然走现有主链，不新增第十个 slot。Goal 只负责给 runtime 一个可审计的长期任务外壳：

- Governance 判断目标是否越权。
- Context 固定本轮目标和验收标准。
- Execution Slot 执行被允许的本地工具。
- Report 输出阶段性结果。
- Memory 写入目标进度和下一步。

当前最小落地：

```text
src/goal_mode.rs
src/goal_run.rs
tests/goal_mode_tests.rs
tests/goal_run_tests.rs
```

`GoalSpec` 目前定义目标、验收、预算、允许 slot、checkpoint 策略和最终报告策略，并能渲染成 runtime extra context。`GoalRun` 负责把目标计划、worker plan、写入范围、验证计划和 checkpoint log 保存成 JSON；`goal show` 在读取时也会重新校验这些结构，避免坏文件伪装成可恢复状态。现在 `status` / `doctor` / `console snapshot` / `app-server health` 也会把 goal policy 和最新 checkpoint 复盘证据显式暴露出来。

`goal step` 的定位必须保持很窄：它是 foreground bounded goal-scoped wrapper over `subagent run-loop`。换句话说，它只是在一个明确 `goal_id` 的 dispatch manifest / queued dispatch 范围内，按显式 `max-runs` 和 `max-concurrency` 跑一批 worker，然后返回本次 step receipt。它不是新的执行内核，也不是后台调度器。

`goal step` 的硬边界：

- 不做 daemon，不后台常驻，不自恢复循环。
- 不自动创建 checkpoint；checkpoint 仍必须由 `goal checkpoint` 显式写入。
- 不写 `docs/progress-log.md`、`docs/handoff-current.md` 或任何长期记忆。
- 不做 cleanup、delete、purge、reset、release-claim 或队列文件删除。
- 不连接 Feishu，不复用 Codex/Hermes 飞书桥，不控制 Hermes。
- 不扩大 runner 权限；底层 command runner 仍必须遵守 `subagent run-loop` 的 `--approve-exec`、capability routing、ReportAdmission 和 live-preflight 边界。
- 它只消费该 `goal_id` 的 dispatch manifest 里已经派发出去的 queued dispatch；跑完后最多只返回 step receipt 和 collect 证据，不会自己补写 checkpoint suggestion。

第二测试版补强的验收面：

- 每个 worker 必须有 `validation_checks`；CLI 自定义 `--worker` 默认继承全局 `--validation`。
- checkpoint 会校验 `completed_worker_ids` 不能为空且只能引用计划内 worker。
- `goal show` 文本和 JSON 会输出 `goal_run_diagnostics`：worker scope 完整性、worker validation 完整性、validation plan 完整性、latest checkpoint 完整性、last checkpoint id/summary，以及 `executes_automatically=false` / `bypasses_governance=false`。
- `GoalRun` checkpoint 会记录 RFC3339 `created_at`；旧 checkpoint 缺该字段仍可读取，但新写入和带字段的持久化记录必须是合法时间戳。
- `goal_run` 只读状态会显式暴露最新 checkpoint 的 `created_at`、`completed_worker_ids` 和 `validation_notes`，方便操作者直接看复盘证据，不必再打开 JSON 文件。
- `incomplete_reasons` 是结构化恢复提示：旧 checkpoint 缺完成者、缺验证备注、worker scope 不完整或 validation plan 不完整时，readiness/诊断面应直接暴露原因。
- checkpoint 必须带至少一个 `completed_worker_id` 和至少一条 `validation_note`。缺 checkpoint 仍只用于提示续接风险，不会触发任务执行。
- `goal collect` 只有在所有 dispatch 都有 report、report 身份与 manifest 对齐且 `status=Success` 时才会给出 `checkpoint_suggestion`；失败、缺失或身份不匹配的 report 只会保留在 `blocked_report_*` 里，并保持 `ready_to_checkpoint=false`。

当前 CLI 入口：

```text
cargo run -- goal plan --objective TEXT [--root PATH] [--goal-id ID] [--max-subtasks N] [--scope scope_id=path[,path...]] [--worker 'worker_id|scope_id[,scope_id...]|objective'] [--validation COMMAND]
cargo run -- goal show [--root PATH] [--goal-id ID]
cargo run -- goal dispatch [--root PATH] [--goal-id ID]
cargo run -- goal step --max-runs N --max-concurrency N [--root PATH] [--goal-id ID]
cargo run -- goal collect [--root PATH] [--goal-id ID]
cargo run -- goal checkpoint --summary TEXT --completed-worker-id ID --validation-note TEXT [--completed-worker-id ID ...] [--validation-note TEXT ...] [--root PATH] [--goal-id ID]
```

默认 root 是 `./context/goal-runs`，属于本地可恢复运行态，不进入 git。`goal dispatch` 只派发计划内 worker，`goal step` 只前台执行有界 worker 批次，`goal collect` 只读收集报告，`goal checkpoint` 才写 checkpoint；这些入口不绕过治理、不新增 slot、不写文档或记忆。

`goal checkpoint` 现在有两种显式写回方式：手动填 `--summary/--completed-worker-id/--validation-note`，或者用 `--from-collect --subagent-queue-root PATH` 从 collect receipt 生成 checkpoint suggestion 后再写回。两条路径都只是显式 checkpoint，不会自动落 `docs/progress-log.md` 或 `docs/handoff-current.md`。

## 6 Worker Operator Runbook

下一轮主控要一次派满 6 个 worker 时，先把任务拆成 6 个互不重叠的 scope，再用 `GoalRun` 记录计划。示例命令里的路径和 objective 必须按当轮任务替换；`--worker` 参数需要整体加引号，避免 shell 把 `|` 当管道。

```bash
GOAL_ID=mainline-95-operator
GOAL_ROOT=./context/goal-runs
QUEUE_ROOT=./context/subagent-queue

cargo run -- goal plan \
  --goal-id "$GOAL_ID" \
  --root "$GOAL_ROOT" \
  --max-subtasks 6 \
  --objective "push Chuang local operator usability toward 95 percent" \
  --scope goal-ux=docs/goal-mode-operating-plan.md,docs/multi-worker-orchestration.md \
  --scope status-ux=src/kernel_status.rs,src/cli_output.rs,tests/kernel_status_tests.rs,tests/cli_status_tests.rs \
  --scope doctor-console=src/cli_doctor.rs,src/cli_console.rs,tests/cli_doctor_tests.rs,tests/cli_console_tests.rs \
  --scope app-health=src/app_server.rs,tests/app_server_tests.rs \
  --scope provider-diag=src/provider_openai_compatible.rs,src/runtime_report.rs,tests/runtime_report_tests.rs \
  --scope smoke-docs=scripts/chuang-complete-local-smoke.sh,docs/handoff-current.md,docs/progress-log.md \
  --worker 'worker-goal-ux|goal-ux|document goal/subagent operator command order and evidence fields' \
  --worker 'worker-status-ux|status-ux|align status readiness wording for live worker availability' \
  --worker 'worker-doctor-console|doctor-console|align doctor and console blocked reason visibility' \
  --worker 'worker-app-health|app-health|expose health text/json runner evidence consistently' \
  --worker 'worker-provider-diag|provider-diag|harden provider diagnostics surfaced to runtime observability' \
  --worker 'worker-smoke-docs|smoke-docs|keep smoke and handoff evidence aligned after integration' \
  --validation 'cargo fmt --all --check' \
  --validation 'git diff --check' \
  --validation 'cargo test -q'
```

计划完成后按固定顺序执行，不跳步：

```bash
cargo run -- goal show --goal-id "$GOAL_ID" --root "$GOAL_ROOT"
cargo run -- goal dispatch --goal-id "$GOAL_ID" --root "$GOAL_ROOT" --subagent-queue-root "$QUEUE_ROOT"
cargo run -- goal step --goal-id "$GOAL_ID" --root "$GOAL_ROOT" --subagent-queue-root "$QUEUE_ROOT" --max-runs 6 --max-concurrency 6
cargo run -- goal collect --goal-id "$GOAL_ID" --root "$GOAL_ROOT" --subagent-queue-root "$QUEUE_ROOT"
cargo run -- goal checkpoint --goal-id "$GOAL_ID" --root "$GOAL_ROOT" --subagent-queue-root "$QUEUE_ROOT" --from-collect
cargo run -- goal show --goal-id "$GOAL_ID" --root "$GOAL_ROOT"
```

验收字段按阶段看：

- `goal show`：`goal_worker_scope_complete=true`、`goal_worker_validation_complete=true`、`goal_validation_plan_complete=true`，并确认 `goal_executes_automatically=false` / `goal_bypasses_governance=false`。
- `goal dispatch`：`goal_dispatch_ready=true`、`goal_dispatch_count=6`、`goal_dispatch_workers` 和 `goal_dispatch_run_ids` 都齐全，`goal_dispatch_manifest_path` 指向本轮 manifest。
- `goal step`：`goal_step_ran_count` 不超过 6，`goal_step_checkpoint_recorded=false`、`goal_step_writes_progress_log=false`、`goal_step_writes_handoff=false`。
- `goal collect`：只有 `goal_collect_ready_to_checkpoint=true` 且 `goal_collect_missing_run_ids=none`、`goal_collect_blocked_report_run_ids=none`、`goal_collect_blocked_report_reasons=none` 时，才进入 `goal checkpoint --from-collect`。
- `goal checkpoint --from-collect`：文本面应出现 `goal_checkpoint_source: collect`；随后 `goal show` 的 `goal_checkpoint_log_complete=true`，最新 checkpoint 带 completed workers 和 validation notes。

失败时先看 `goal collect`，不要猜。`goal_collect_missing_run_ids` 表示 worker 还没交报告；`goal_collect_blocked_report_run_ids` 表示已有报告但不能进 checkpoint；`goal_collect_blocked_report_reasons` 会说明是 failed report、identity mismatch 或其他 admission 问题。此时 `goal checkpoint --from-collect` 必须失败，主控应把 blocked evidence 贴给对应 worker 返工，不能手工绕过生成 checkpoint。

2026-05-08 增量状态：`goal plan -> goal dispatch -> goal step -> goal collect -> goal checkpoint --from-collect -> goal show` 的 happy path 已进入 `scripts/chuang-goal-mode-smoke.sh`，not-ready 负例已进入 `scripts/chuang-goal-mode-negative-smoke.sh`，并由 `scripts/chuang-complete-local-smoke.sh` 直接调用，当前不再把“先跑通 happy path”或“先补负例门禁”作为下一阶段目标。已锁定的门禁包括：

1. `goal checkpoint --from-collect` 在 collect receipt not-ready 时必须拒绝写回。
2. failed report / identity-mismatched report 必须只进入 `blocked_report_run_ids` 和 `blocked_report_reasons`，不能生成 checkpoint material。
3. `goal step` 必须持续保持 bounded / allowlist：只执行 manifest run id，拒绝无效 `max-runs` / `max-concurrency`，不扩大到后台调度。

## 禁止事项

- 不把 goal mode 做成绕过治理的后台执行器。
- 不把 `goal step` 做成 daemon、自动 checkpoint、自动文档/记忆写回、cleanup/delete 工具或 Feishu/Hermes 通道。
- 不自动删除、清理、reset 或卸载任何东西。
- 不复用 Codex/Hermes 飞书通道。
- 不把外部智能体调度提前塞进主进程主线。
