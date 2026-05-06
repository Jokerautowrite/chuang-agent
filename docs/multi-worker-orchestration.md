# Multi-Worker Orchestration

更新时间：2026-05-05

## 目标

把多个子代理的计划、范围和验收拆清楚，并提供受控的本地多 worker 批处理入口。

## 当前边界

```text
GoalRun planning + durable queue + bounded local run-loop
```

`GoalRun` 继续只负责记录目标、worker plan、scope 和 checkpoint，不自动执行。

运行层的最小并行入口是：

```bash
cargo run -- subagent run-loop --max-concurrency 2 --max-runs 2
```

`--max-concurrency` 支持 `1..8`。每个 worker 仍通过文件队列 claim dispatch，按 capability 匹配任务，执行后写标准 `SubagentReport`，并由主控生成 `ReportAdmission`。

## 约束

- GoalRun 不自动调度 worker；实际执行必须显式调用 `subagent run-loop`。
- scope 必须先定义，不能互相重叠。
- worker 之间只通过计划和报告协作，不共享临时状态。
- command runner 仍必须显式传 `--approve-exec`。
- live external worker pool 仍是后续 audited adapter 边界，本地 run-loop 不连接真实外部平台。
- 真实外部 worker runner 启用前先跑只读 `subagent live-preflight`，确认 live gate、runner allowlist、capability routing、ReportAdmission 证据和 forbidden capability rejection 都可见；该命令不启动真实 worker。

## 下一步

1. 把 GoalRun 的 worker plan 继续用作唯一计划入口。
2. 继续收紧真实 runner 的 allowlist、身份校验和 capability routing。
3. 后续再把 live worker pool 接成 audited adapter，而不是塞进核心主链。
