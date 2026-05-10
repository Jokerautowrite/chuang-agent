# Codex + Claude Optimization Plan V1

日期：2026-05-11
依据：
- `docs/codex-architecture-audit-v1.md`
- `docs/claude-rust-slot-audit-v1.md`
- `docs/claude-rust-integration-plan-v1.md`

## 目标

保留 Chuang 最初方向：本地 Agent OS、记忆为本体、Slot 可插拔、治理必经、子代理并行。

本计划只修正“落地细节不够强”的部分：用 Codex 做运行骨架，用 Claude-rust 做工具/MCP/模型工具循环细节，把 Chuang 从脚本拼接推进到可审计 runtime。

## 总体取舍

| 领域 | 主参考 | 辅参考 | Chuang 决策 |
| --- | --- | --- | --- |
| AgentLoop | Codex | Claude `QueryEngine` | 以 SQ/EQ、SessionTask、TurnContext 建 Chuang 主循环；吸收 Claude tool result correction / retry。 |
| Execution | Codex + Claude | - | Codex 提供 dispatch/trace/hook/gate；Claude 提供 Tool descriptor / ToolRegistry / MCP trait 形态。 |
| Governance | Codex | Claude permission pattern | Codex permission profile + exec policy + guardian 为主；Claude allow/deny pattern 用于配置 UX。 |
| MCP / dynamic tools | Claude | Codex | fake-first 用 Claude stdio MCP 思路；Codex 的 MCP approval/elicitation/event 模型必须补上。 |
| Context | Codex | Claude compaction | TurnContext 作为事实源；Chuang ContextEngine 继续保留 deterministic pack trace。 |
| Subagents | Codex | Claude nested explorer | Codex `AgentControl` / agent tree / spawn edge 为主；Claude nested explorer 可做只读 sidecar adapter。 |
| State / trace | Codex | - | 引入 runtime ledger / trace reducer；不替代核心 memory。 |
| Interface | Codex app-server | Claude server/TUI | Feishu/CLI/HTTP 共用 thread/turn API；当前不切 UI 主线。 |
| Skill / plugin | Codex + Claude | - | skill/plugin 只负责 discovery/instructions；执行统一进 ToolRegistry + Governance。 |

## 新九大 Slot 优化图

```text
InterfaceSlot
  Feishu / CLI / HTTP / Console
  -> 只提交 ThreadCommand / TurnCommand，不直接执行工具

AgentLoopSlot
  SQ/EQ event loop
  Session / Thread / Turn state machine
  model stream -> tool dispatch -> tool result -> final

ContextSlot
  TurnContext = cwd + model + provider + permissions + tools + env + memory snapshot
  ContextEngine = collect / rank / pack / render / trace

ExecutionSlot
  ToolRegistrySlot
  UnifiedExecSlot
  DesktopActuatorSlot
  BrowserReadSlot
  McpToolAdapter

GovernanceSlot
  PermissionProfile
  RiskPolicy
  ExecPolicy
  ApprovalRequest
  AuditReceipt

SubagentSlot
  AgentTree
  SpawnEdge
  AgentRole
  ReportAdmission
  MemoryProposal only

StateTraceSlot
  RuntimeEventLedger
  ToolDispatchTrace
  SubagentEdgeTrace
  Replay/diagnostic reducer

MemorySlot
  Identity/User/Rules/Experience/Archive/Knowledge
  runtime ledger 可回源，但不等于核心记忆

EvolutionSlot
  从 verified trace/report 提炼 SOP/skill
  skill 执行仍进 ToolRegistry + Governance
```

## 近期重排

### M1：RuntimeEventLedger

先补 Chuang 当前最弱的共用脊柱：事件账本。

交付：

- `RuntimeEvent` schema 草案：thread_started、turn_started、model_delta、tool_started、tool_finished、approval_requested、approval_resolved、subagent_spawned、subagent_reported、turn_completed、turn_failed。
- 每个事件带 `thread_id`、`turn_id`、`call_id`、`risk_decision`、`evidence_ref`、`created_at`。
- fake in-memory ledger + JSONL ledger contract tests。

验收：

- Feishu 一轮任务可输出完整 turn/tool/approval 事件摘要。
- 工具协议错误、actuator 失败、provider 失败都进入同一错误事件面。

### M2：ToolRegistrySlot

把现有工具从 enum 执行分支升级为 registry handler。

交付：

- `ToolDescriptor`：name、namespace、schema、read_only、mutating、destructive、external_commit、concurrent_safe、requires_approval、risk_tags。
- `ToolHandler`：descriptor、precheck、execute、postprocess。
- 现有 `file_read/file_write/code_execute/list_dir/locate/screenshot/open_app/mouse/keyboard` 映射为 descriptors。
- dispatch 统一写 `RuntimeEventLedger`。

验收：

- governance 不再靠工具名字符串猜风险，而是读取 descriptor + action params。
- mutating 工具有 gate；read-only 工具可并行；destructive 永远进入高危策略。

### M3：PermissionProfileSlot

把“默认完整能力，无需审批，只有高危询问”落成配置，而不是提示词。

默认 profile：

```text
local_ga
  read/list/status/observe/screenshot: allow
  file_write/code_execute/open_app/click/input: allow with audit
  external_send/public_post/payment/order/verification_code: require approval
  delete/cleanup/reset/uninstall/purge: require explicit target approval
  service_control/network_change/secret_access: require approval or deny by default
```

开源 profile：

```text
safe_default
  read/list/status: allow
  write/exec/desktop: approval or project trust
  destructive/external/secret/network/service: approval/deny
```

验收：

- Chuang 不再因为普通打开 Chrome、点击、输入而拒绝。
- 删除、清理、重置、卸载仍不自动执行。
- policy 与 prompt 不一致时，以 policy 为准并返回结构化原因。

### M4：UnifiedExec + Actuator Orchestrator

把 shell、code_execute、desktop actuator、browser read 的执行前后流程统一。

交付：

- `ExecutionRequest` / `ExecutionResult` 公共结构。
- stdout/stderr/output preview 限流。
- started/completed/failed events。
- sandbox / env / cwd / adapter availability 全部进入 receipt。

验收：

- `open_app` 这类动作即使 adapter 输出多行，也不会破坏工具协议。
- actuator 失败不再变成 tool loop exhausted，而是 typed execution failure。

### M5：MCP Fake Adapter

吸收 Claude MCP 的易迁移实现，同时补 Codex 的 approval/event/elicitation。

交付：

- fake stdio MCP server。
- tools/list、tools/call、malformed json、timeout、stderr-noise、approval-required、elicitation-required tests。
- MCP 工具 descriptor 进入 ToolRegistry。

验收：

- MCP 工具不能绕过 Governance。
- destructive/open-world MCP 工具默认要求 approval。
- secret 不进入 log、event preview、receipt。

### M6：SubagentTreeLedger

把子代理从“队列任务”升级成可追踪 agent tree。

交付：

- root/child thread relation。
- spawn depth / max concurrent / role / nickname / status。
- send/wait/close/list/report event。
- `SubagentReport` admission 后才能进入父 agent context。

验收：

- 多子代理并行时能回答：谁在跑、做什么、改了哪些文件、是否完成、证据在哪里。
- 子代理不能直接写 core memory，只能提出 memory proposal。

### M7：Context 与 Compaction 修正

Codex 提供 turn context 事实源，Claude 提供 compaction trigger 思路，Chuang 保留 deterministic trace。

交付：

- TurnContext snapshot 固定：workspace、env、model、provider、permissions、tools、memory snapshot、recent history。
- compaction 事件入 ledger。
- model 工具协议失败时，下一轮注入最小纠错 context。

验收：

- 不再靠口头 prompt 让模型记住工具协议。
- 上下文压缩后仍保留能力、边界、最近工具错误和当前任务目标。

## 当前优先级

先做 M1-M3，再做 M4-M6。

原因：

- Chuang 当前真实能力问题多发生在“工具能不能被正确允许、执行、回执、追踪”，不是缺更多工具。
- 没有 ledger 和 registry，MCP、子代理、桌面动作都会继续散。
- 治理要默认放开普通本地动作，但必须用 policy 表达，不能只靠 prompt。

## 文档状态

- `docs/claude-rust-slot-audit-v1.md` 仍保留，作为 Claude 单源审计。
- `docs/claude-rust-integration-plan-v1.md` 的 M1/M2 仍有效，但现在纳入本计划：ToolRegistrySlot 必须同时吸收 Codex dispatch/trace 与 Claude descriptor/MCP。
- 本文档是新的执行优先级来源。

