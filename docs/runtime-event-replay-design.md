# 运行时事件记录、追踪与重放核查流程设计

## 目标

一次任务执行从进入到结束的完整生命周期，关键运行时事件全程落账；
事后可按 thread / turn / call 三级追踪事件因果链，并可重放核查完整性与合规性。

## 现状基线（已存在，直接复用）

- `src/runtime_event_ledger.rs`：
  - `RuntimeEventKind`：19 类事件（生命周期、上下文、Provider、风险、记忆、技能、工具、审批、子代理、回合结果）。
  - `RuntimeEvent { schema_version, event_type, thread_id, turn_id, call_id, created_at, risk_decision, evidence_ref }`。
  - trait `RuntimeEventLedger { append, list, query_by_turn, query_by_call, summarize_turn }`。
  - `InMemoryRuntimeEventLedger`（热路径）/ `JsonlRuntimeEventLedger`（append-only JSONL 持久化）。
- `src/agent_runtime.rs::run_with_ledger`：主循环已埋点，产生 TurnStarted / ContextPacked / ProviderRequested / ProviderResponded / ToolStarted / ToolFinished / TurnCompleted / TurnFailed 等事件。

## 一次任务的标准事件流

```text
ThreadStarted
└─ TurnStarted
   ├─ ContextPacked                  # 上下文打包
   ├─ RiskClassified                 # 风险决策 + policy_ref
   ├─ ProviderRequested ─→ ProviderResponded
   ├─ MemoryProposed ─→ MemoryCommitted   （如触发记忆写入）
   ├─ SkillProposed  ─→ SkillSolidified   （如触发技能沉淀）
   ├─ 工具循环（可多次）:
   │    ToolStarted(call_id)
   │      └─ ApprovalRequested ─→ ApprovalResolved  （如触发审批）
   │    ToolFinished(call_id)
   ├─ SubagentSpawned ─→ SubagentReported       （如派工）
   └─ TurnCompleted / TurnFailed
```

## 记录（Record）

- 热路径写内存账本；同时 append 到 JSONL 文件（追加写、O(1)、崩溃后尾部可恢复）。
- 每条事件带 `created_at`（UTC RFC3339）与 thread/turn/call 三级归属。
- `risk_decision` 记录风险判定与 `policy_ref`；`evidence_ref` 指向工具输出或证据文件，供事后取证。

## 追踪（Trace）

- 按 `thread_id` 取全链路；按 `turn_id` 取单轮事件序列；按 `call_id` 配对单次工具调用。
- 配对完整性即追踪正确性的判据。

## 重放核查（Replay & Audit）

- 重放 = `JsonlRuntimeEventLedger::list()` 读回 JSONL → 按 `created_at` 排序 → 结构化输出事件流。
- 核查不变量：
  1. 每个 `ToolStarted` 有唯一 `call_id`，且必须配对 `ToolFinished`。
  2. `ApprovalRequested` 必须配对 `ApprovalResolved`。
  3. `ProviderRequested` 必须配对 `ProviderResponded`。
  4. `TurnStarted` 必须以 `TurnCompleted` 或 `TurnFailed` 结束。
  5. 全部事件 `schema_version` 一致。
- 产出：`RuntimeTurnSummary`（事件数 / 工具数 / 审批数 / 风险数 / 证据数）+ 可读重放报告。

## 落地步骤（增量、最小改动）

1. 保留现有埋点；将 `JsonlRuntimeEventLedger` 接入主流程（当前 chuang_kernel 用 InMemory）。
2. 增加只读核查命令：输入 `thread_id` → 输出事件流 + 不变量校验结果。
3. 验收（墨菲）：跑一次示例任务 → 校验 JSONL 可解析、配对完整、可重放。

## 验收标准

- [ ] 一次任务产生 ≥10 条事件，落盘 JSONL 逐行可解析。
- [ ] 所有 `call_id` 配对闭合，无悬挂 ToolStarted。
- [ ] 失败回合产生 `TurnFailed`，不出现幽灵成功。
- [ ] 重放输出与执行顺序一致（created_at 单调）。
