# Codex + Claude Implementation Slices V1

日期：2026-05-11

依据：

- `docs/codex-claude-optimization-plan-v1.md`
- `docs/implementation-prep-v1.md`
- `docs/codex-architecture-audit-v1.md`
- `docs/claude-rust-slot-audit-v1.md`
- `docs/claude-rust-integration-plan-v1.md`

## 定位

本文把 Codex + Claude 优化计划里的 M1-M7 拆成可派发的实现工单和验收矩阵。它是 implementation prep 的后续执行拆解，不替代原设计文档。

本轮只写文档，不改 Rust 代码、不改脚本、不碰真实服务、不更新 `docs/progress-log.md`。

## 全局硬边界

- 不删除、不清理、不 reset、不 purge、不 uninstall；实现和测试都不得引入自动删除路径。
- 不运行或建议 `rm` / cleanup / reset / purge / uninstall 类命令作为默认验收步骤。
- 不泄露 secret。日志、event preview、receipt、测试 fixture、错误消息里只能出现变量名、`<set>`、`<missing>` 或脱敏摘要。
- 不绕过治理。所有工具、MCP、actuator、subagent、service-like 动作都必须经过 `PermissionProfile` / `RiskPolicy` / audit receipt。
- 普通本地动作默认 `allow_with_audit`：例如受限文件写入、`code_execute`、`open_app`、普通 click/input。
- 高危动作默认 `require_approval` 或 `deny`：外发消息、公开发布、支付、订单、账号动作、验证码、服务控制、网络变更、secret access、删除/清理/重置/卸载/purge。
- policy 与 prompt 冲突时，以 policy 为准，并返回结构化拒绝原因。
- fake-first。每个 slot 先有 fake/in-memory/fixture contract，再接真实 adapter。
- 子代理不得直接写 core memory，只能提交 report 或 memory proposal。

## 默认权限语义

| 动作族 | 默认决策 | 必要审计字段 |
| --- | --- | --- |
| read / list / status / observe / screenshot | `allow` | tool name、target、evidence ref、read_only=true |
| file_write / code_execute / open_app / click / input | `allow_with_audit` | permission profile、risk tags、cwd/sandbox、audit label、receipt id |
| external_send / public_post / payment / order / account_action / verification_code | `require_approval` | approval request、operator decision、scope、rollback condition |
| service_control / network_change / secret_access | `require_approval` 或 `deny` | denial/approval reason、secret redaction proof、audit receipt |
| delete / cleanup / reset / uninstall / purge | `require_explicit_target_approval` 或 `deny` | exact target list、explicit operator approval、destructive=true |

## 依赖顺序

```text
M1 RuntimeEventLedger
  -> M2 ToolRegistrySlot
  -> M3 PermissionProfileSlot
  -> M4 UnifiedExec + Actuator Orchestrator
  -> M5 MCP Fake Adapter
  -> M6 SubagentTreeLedger
  -> M7 Context + Compaction correction
```

M1-M3 是第一批。M4-M6 不应在 M1-M3 合同未稳定前接真实 adapter。M7 可先做 TurnContext snapshot 读面，但 compaction/error-correction 需要 M1 ledger 事件。

## 2026-05-12 Current Implementation Status

- M5 主链状态：fake MCP/list/call/risk/approval/elicitation 合同已进入 crate 与回归；runtime event ledger、tool protocol errors、approval/elicitation 计数和 channel/app-server turn observability 已可在 runtime_report/status/doctor/readiness/candidate/third-test 面复验。
- M6 主链状态：subagent tree/report admission/parent handoff/GoalRun checkpoint evidence 已进入 runtime report surface 与 GoalRun/status/doctor/app-server/goal show 文本面；latest checkpoint 的 created_at、completed workers、validation notes 可直接查询。
- M7 主链状态：TurnContext、context pack trace、compaction events、context_compaction_summary_json、tool protocol correction/typed failure 已进入 runtime report artifacts、observability、status/doctor/app-server/channel 和 high-level smoke gates。
- 当前剩余边界：真实 live worker、真实 desktop/browser/wiki/GBrain acceptance receipt 仍在 `docs/acceptance-next-matrix.md` 跟踪；这些不是 fake-first 合同缺口，不能把 local-ready/readiness 误标成 live-ready。
- 当前复验入口：`cargo test -q`、`sh scripts/chuang-candidate-verify.sh`、`sh scripts/chuang-third-test-smoke.sh` 已在 2026-05-12 对最新 M5/M6/M7 主链口径通过；具体 checkpoint 见 `docs/progress-log.md` 顶部记录。

## M1 RuntimeEventLedger

### M1.1 RuntimeEvent schema

- 写入范围：新增 `src/runtime_event_ledger.rs` 或 `src/runtime_event/mod.rs`；在 `src/lib.rs` re-export；新增 `tests/runtime_event_ledger_tests.rs`。
- 接口：`RuntimeEvent`、`RuntimeEventKind`、`RuntimeEventIds`、`RuntimeEvidenceRef`、`RiskDecisionSnapshot`、`RuntimeEventLedger` trait。
- 测试：事件 serde roundtrip；必填 `thread_id` / `turn_id` / `created_at`；unknown kind 拒绝；secret-like preview 拒绝或脱敏。
- 验收命令：`cargo fmt --all --check`；`cargo test -q --test runtime_event_ledger_tests`; `git diff --check`。
- 风险边界：不复用 `skill_evolver::RuntimeEvent` 作为 runtime ledger schema；skill event 可后续适配，但 ledger schema 必须覆盖 turn/tool/approval/subagent/provider/actuator。

### M1.2 In-memory ledger + JSONL fake

- 写入范围：`src/runtime_event/in_memory.rs`、`src/runtime_event/jsonl.rs`、`tests/runtime_event_ledger_tests.rs`。
- 接口：`append(event)`、`query_by_turn(thread_id, turn_id)`、`query_by_call(call_id)`、`summarize_turn(turn_id)`。
- 测试：append order 稳定；JSONL 一行一个事件；坏 JSON 返回结构化错误；query 不修改账本。
- 验收命令：`cargo test -q --test runtime_event_ledger_tests`; `cargo fmt --all --check`; `git diff --check`。
- 风险边界：JSONL ledger 只能 append；不提供自动删除、压缩、清理接口；错误消息不得打印 secret 或完整 env。

### M1.3 Turn/tool/provider event 接入

- 写入范围：`src/agent_runtime.rs`、`src/tool_runtime.rs`、`src/provider_openai_compatible.rs`、`src/runtime_report.rs`、相关 tests。
- 接口：turn_started、model_delta、tool_started、tool_finished、turn_completed、turn_failed、provider_failed。
- 测试：正常 turn 事件闭环；tool 协议错误进入 `tool_failed`；provider timeout/capacity/missing content 进入 typed failure event。
- 验收命令：`cargo test -q --test agent_runtime_tests --test tool_runtime_tests --test runtime_report_tests`; `cargo test -q --test openai_compatible_http_live_transport_local_tests`; `git diff --check`。
- 风险边界：provider request/response preview 必须限流和脱敏；不得把 API key、Authorization header、完整 prompt dump 写入 ledger。

### M1.4 Approval/subagent/actuator event 接入

- 写入范围：`src/governance.rs`、`src/live_adapter_gate.rs`、`src/actuator.rs`、`src/subagent_queue.rs`、`src/live_subagent_rehearsal.rs`。
- 接口：approval_requested、approval_resolved、subagent_spawned、subagent_reported、actuator_started、actuator_finished。
- 测试：approval missing 时有 event；subagent report accepted/rejected 都有 event；actuator dry-run/live receipt 都有 audit label。
- 验收命令：`cargo test -q --test governance_tests --test cli_subagent_live_preflight_tests --test actuator_tests --test live_runner_rehearsal_receipt_tests`。
- 风险边界：approval event 不等于 approval granted；actuator event 不等于真实执行成功。

## M2 ToolRegistrySlot

### M2.1 ToolDescriptor schema

- 写入范围：新增或扩展 `src/tool_registry_slot.rs`；桥接 `src/atomic_tool.rs`；新增 `tests/tool_registry_slot_tests.rs`。
- 接口：`ToolDescriptor { name, namespace, schema, read_only, mutating, destructive, external_commit, concurrent_safe, requires_approval, risk_tags }`。
- 测试：descriptor serde；required flags 完整；destructive 与 read_only 互斥；risk tags 稳定。
- 验收命令：`cargo test -q --test tool_registry_slot_tests --test atomic_tool_tests`; `cargo fmt --all --check`; `git diff --check`。
- 风险边界：governance 不能继续只靠工具名字猜风险；descriptor 是风险判断输入，不是最终授权。

### M2.2 ToolHandler registry contract

- 写入范围：`src/tool_registry_slot.rs`、`src/tool_runtime.rs`、`tests/tool_registry_slot_tests.rs`。
- 接口：`ToolHandler::descriptor()`、`precheck()`、`execute()`、`postprocess()`；`ToolRegistry::register()`、`get()`、`dispatch()`。
- 测试：fake read-only tool；fake mutating tool；unknown tool structured error；handler panic/invalid output 被包装为 typed failure。
- 验收命令：`cargo test -q --test tool_registry_slot_tests --test tool_runtime_tests`。
- 风险边界：registry dispatch 必须写 M1 ledger；handler 不能自行绕过 governance 或直接调用 actuator live path。

### M2.3 Existing atomic tools descriptor bridge

- 写入范围：`src/atomic_tool.rs`、`src/tool_runtime.rs`、`src/cli_output.rs`、`src/cli_doctor.rs`、`src/app_server.rs`。
- 接口：file_read、file_write、code_execute、list_dir、locate、screenshot、open_app、mouse、keyboard 的 descriptor。
- 测试：status/doctor/app-server health 输出 descriptor 摘要；GA tools 仍 9/9 mapped；read-only 与 mutating 标记正确。
- 验收命令：`cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests --test atomic_tool_tests`。
- 风险边界：`mouse` / `keyboard` 不得标成 read-only；`screenshot` / `locate` 不得被误标为 mutating。

### M2.4 Tool dispatch ledger integration

- 写入范围：`src/tool_runtime.rs`、`src/agent_runtime.rs`、`src/runtime_report.rs`。
- 接口：dispatch id / call id 统一；tool_started/tool_finished 事件与 runtime report 对齐。
- 测试：每个 tool call 有 call id；failed tool 有 typed reason；runtime report id 可回源 ledger。
- 验收命令：`cargo test -q --test agent_runtime_tests --test runtime_report_tests --test tool_runtime_tests`。
- 风险边界：tool output preview 限流；binary/large output 不写入完整 ledger。

## M3 PermissionProfileSlot

### M3.1 PermissionProfile schema + config

- 写入范围：新增 `src/permission_profile.rs` 或扩展 `src/governance.rs`；`src/runtime_config.rs`；`src/runtime_config_file.rs`；`tests/permission_profile_tests.rs`。
- 接口：`PermissionProfile`、`PermissionDecision`、`RiskPolicy`、`ActionClass`、`PolicySource`。
- 测试：`local_ga` 与 `safe_default` parse；unknown profile 拒绝；缺配置默认安全 profile；policy source 进入 status。
- 验收命令：`cargo test -q --test permission_profile_tests --test runtime_config_tests --test runtime_config_file_tests`。
- 风险边界：不得用 prompt 文本作为唯一 policy；配置错误不能 silent fallback 到更宽权限。

### M3.2 Risk evaluator

- 写入范围：`src/governance.rs`、`src/tool_registry_slot.rs`、`tests/governance_tests.rs`。
- 接口：`evaluate(descriptor, params, profile) -> PermissionDecision`。
- 测试：read/list/status allow；file_write/code_execute/open_app/click/input allow_with_audit；external/send/payment/verification require approval；secret/network/service deny or approval；destructive exact-target approval required.
- 验收命令：`cargo test -q --test governance_tests --test tool_registry_slot_tests`。
- 风险边界：普通本地动作不应无理由拒绝；高危动作不能因 prompt 说“可以”而放行。

### M3.3 Approval request + audit receipt

- 写入范围：`src/governance.rs`、`src/runtime_event_ledger.rs`、`src/runtime_report.rs`、CLI/status tests。
- 接口：`ApprovalRequest`、`ApprovalDecision`、`AuditReceipt`；approval_requested/resolved ledger events。
- 测试：approval missing blocks external commit；deny reason 可见；approval scope 限制 action/target；audit receipt 无 secret。
- 验收命令：`cargo test -q --test governance_tests --test runtime_report_tests --test kernel_status_tests`。
- 风险边界：approval granted 只对 exact scope 有效；不能复用旧 approval 放大到其他 target。

### M3.4 Policy status surface

- 写入范围：`src/kernel_status.rs`、`src/cli_output.rs`、`src/cli_doctor.rs`、`src/app_server.rs`、`src/cli_console.rs`。
- 接口：status/doctor/app-server health 输出 active profile、decision summary、high-risk defaults、prompt-policy conflict handling。
- 测试：status/doctor text + JSON；app-server diagnostic JSON；console snapshot。
- 验收命令：`cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests --test cli_console_tests`。
- 风险边界：status 只能显示 policy 元数据，不显示 secret、tokens、full env。

## M4 UnifiedExec + Actuator Orchestrator

### M4.1 ExecutionRequest / ExecutionResult

- 写入范围：新增或扩展 `src/unified_execution_slot.rs`；桥接 `src/tool_runtime.rs`、`src/actuator.rs`、`src/control_plane.rs`。
- 接口：`ExecutionRequest`、`ExecutionResult`、`ExecutionOutputPreview`、`ExecutionEnvironmentSnapshot`、`ExecutionFailureKind`。
- 测试：request serde；result preview 限流；cwd/env/sandbox/audit label 写入 receipt；secret env 脱敏。
- 验收命令：`cargo test -q --test unified_execution_slot_tests --test tool_runtime_tests --test control_actuator_contract_tests`。
- 风险边界：ExecutionResult 不得包含完整 secret env；不提供 destructive cleanup helper。

### M4.2 Shell/code_execute bridge

- 写入范围：`src/tool_runtime.rs`、`src/workspace_file_adapter.rs`、`src/path_utils.rs`、`tests/tool_runtime_tests.rs`。
- 接口：shell/code execution through UnifiedExec with ledger events.
- 测试：stdout/stderr truncation；nonzero exit typed failure；workspace path escape still rejected；audit label retained.
- 验收命令：`cargo test -q --test tool_runtime_tests workspace_file_adapter`; `cargo test -q --test tool_runtime_tests symlink_parent`; `cargo test -q --test agent_runtime_tests`。
- 风险边界：do not add shell cleanup/delete commands to tests; path safety must remain canonicalize-parent based.

### M4.3 Actuator/browser execution bridge

- 写入范围：`src/actuator.rs`、`src/actuator/command.rs`、`src/browser_read.rs`、`src/genesis_actuator.rs`。
- 接口：open_app/mouse/keyboard/screenshot/locate/browser_read all emit ExecutionResult-shaped receipts.
- 测试：observe/screenshot read_only; click/input allow_with_audit under local profile; browser read unavailable remains structured; no DOM claim without adapter.
- 验收命令：`cargo test -q --test actuator_tests --test genesis_actuator_tests --test browser_read_tests --test atomic_tool_tests`。
- 风险边界：desktop observation evidence cannot be relabeled as browser DOM/URL/title evidence.

### M4.4 Typed execution failure in tool loop

- 写入范围：`src/agent_runtime.rs`、`src/tool_loop_meta.rs`、`src/runtime_report.rs`。
- 接口：adapter_unavailable、permission_denied、protocol_error、timeout、nonzero_exit、invalid_output.
- 测试：actuator unavailable returns final typed receipt instead of tool_loop_exhausted; provider/tool errors remain distinct.
- 验收命令：`cargo test -q --test agent_runtime_tests --test runtime_report_tests --test actuator_tests`。
- 风险边界：typed failure is not success; final message must not claim action happened when adapter blocked it.

## M5 MCP Fake Adapter

### M5.1 Fake stdio MCP server/client

- 写入范围：新增或扩展 `src/mcp_fake_adapter.rs`; tests `tests/mcp_fake_adapter_tests.rs`。
- 接口：tools/list、tools/call、server stderr capture, timeout config.
- 测试：valid list/call; malformed JSON; timeout; stderr noise; missing tool; process exit.
- 验收命令：`cargo test -q --test mcp_fake_adapter_tests`。
- 风险边界：fake server only; no real network MCP; no secret in stderr preview.

### M5.2 MCP descriptor into ToolRegistry

- 写入范围：`src/mcp_fake_adapter.rs`、`src/tool_registry_slot.rs`、`tests/mcp_fake_adapter_tests.rs`、`tests/tool_registry_slot_tests.rs`.
- 接口：MCP tool -> `ToolDescriptor` with risk tags and approval requirement.
- 测试：read-only MCP allow; mutating MCP allow_with_audit; destructive/open-world MCP require approval; unknown schema rejected.
- 验收命令：`cargo test -q --test mcp_fake_adapter_tests --test tool_registry_slot_tests --test governance_tests`。
- 风险边界：MCP tools cannot bypass governance; descriptor cannot default destructive=false when server omits risk.

### M5.3 Approval + elicitation events

- 写入范围：`src/mcp_fake_adapter.rs`、`src/runtime_event_ledger.rs`、`src/governance.rs`。
- 接口：approval_required, elicitation_required, tool_result, tool_error events.
- 测试：approval-required MCP call blocks without approval; elicitation-required returns structured request; event previews redacted.
- 验收命令：`cargo test -q --test mcp_fake_adapter_tests --test runtime_event_ledger_tests --test governance_tests`。
- 风险边界：elicitation cannot ask for secrets unless policy explicitly approves secret access; default is deny/redact.

## M6 SubagentTreeLedger

### M6.1 Agent tree schema

- 写入范围：新增或扩展 `src/subagent_tree_ledger.rs`; `src/subagent_queue.rs`; tests `tests/subagent_tree_ledger_tests.rs`.
- 接口：`AgentNode`、`SpawnEdge`、`AgentRole`、`AgentTreeLedger`、depth/concurrency limits.
- 测试：root/child relation; max depth reject; max concurrent reject; nickname/status serialization.
- 验收命令：`cargo test -q --test subagent_tree_ledger_tests --test subagent_queue_tests`。
- 风险边界：agent tree is trace/state, not permission grant; child capabilities still go through governance.

### M6.2 Spawn/send/wait/close/list events

- 写入范围：`src/cli_subagent.rs`、`src/subagent_queue.rs`、`src/live_subagent_rehearsal.rs`、`src/runtime_event_ledger.rs`.
- 接口：subagent_spawned, subagent_message_sent, subagent_wait_started, subagent_closed, subagent_reported.
- 测试：queued dispatch creates spawn edge; run-loop updates status; close is explicit and non-destructive; list answers who/what/status/evidence.
- 验收命令：`cargo test -q --test cli_subagent_dispatch_tests --test subagent_queue_tests --test live_runner_readiness_view_tests`。
- 风险边界：close must not delete reports/logs; no automatic cleanup of queue roots.

### M6.3 ReportAdmission to parent context

- 写入范围：`src/subagent_report/*`、`src/subagent_queue.rs`、`src/context_engine.rs`、`src/agent_runtime.rs`.
- 接口：accepted report summary segment; rejected report blocked evidence; memory proposal only.
- 测试：accepted report enters parent context with provenance; rejected report does not; memory proposal not auto-written.
- 验收命令：`cargo test -q --test subagent_report_tests --test context_engine_tests --test agent_runtime_tests --test memory_maintenance_cli_tests`。
- 风险边界：subagent report cannot directly write core memory; parent must explicitly accept memory proposal.

## M7 Context and Compaction Correction

### M7.1 TurnContext snapshot

- 写入范围：新增 or extend `src/turn_context.rs`; bridge `src/agent_runtime.rs`, `src/context_engine.rs`, `src/runtime_config.rs`.
- 接口：workspace、env snapshot、model/provider、permissions、tools、memory snapshot、recent history、thread/turn ids.
- 测试：snapshot deterministic; secret env redacted; tools and permission profile stable under context packing; recent history preserved.
- 验收命令：`cargo test -q --test context_engine_tests --test agent_runtime_tests --test runtime_config_tests --test app_server_tests`。
- 风险边界：TurnContext is runtime input snapshot, not long-term memory; do not persist secret values.

### M7.2 Compaction event + deterministic trace

- 写入范围：`src/context_engine.rs`、`src/context_engine/summary_compression.rs`、`src/runtime_event_ledger.rs`.
- 接口：context_compaction_started, context_compaction_completed, context_segment_dropped events; pack trace remains deterministic.
- 测试：budget pressure preserves core/session/tool/history; compaction event contains reason and dropped segment ids; no full sensitive segment dump.
- 验收命令：`cargo test -q --test context_engine_tests --test agent_runtime_tests --test runtime_event_ledger_tests`。
- 风险边界：compaction must not silently drop governance/tool instructions before ordinary recall/memory.

### M7.3 Tool protocol correction context

- 写入范围：`src/agent_runtime.rs`、`src/tool_loop_meta.rs`、`src/capability_primer.rs`.
- 接口：minimal correction context after invalid tool JSON, trailing text, wrong tool name, or missing final.
- 测试：model gets concise correction after tool protocol error; no infinite loop; final typed failure when exhausted.
- 验收命令：`cargo test -q --test agent_runtime_tests --test runtime_report_tests --test atomic_tool_tests`。
- 风险边界：correction context guides protocol only; it must not relax permission policy or approve high-risk actions.

## Cross-slice Acceptance Matrix

| Matrix item | Required by | Acceptance command | Pass condition |
| --- | --- | --- | --- |
| Format and whitespace | all slices | `cargo fmt --all --check`; `git diff --check` | no formatting drift or whitespace errors |
| Ledger contract | M1, M4-M7 | `cargo test -q --test runtime_event_ledger_tests` | event schema, append/query, redaction, typed failures pass |
| Registry contract | M2, M5 | `cargo test -q --test tool_registry_slot_tests --test atomic_tool_tests` | descriptors complete; dispatch stable; risk flags correct |
| Governance policy | M3-M5 | `cargo test -q --test governance_tests --test kernel_status_tests --test cli_doctor_tests` | local actions `allow_with_audit`; high-risk approval/deny |
| Tool runtime | M2, M4, M7 | `cargo test -q --test tool_runtime_tests --test agent_runtime_tests --test runtime_report_tests` | tool events, previews, typed failures, protocol correction pass |
| Actuator/browser boundary | M4 | `cargo test -q --test actuator_tests --test browser_read_tests --test genesis_actuator_tests` | observe/read split preserved; live actions audited |
| MCP fake | M5 | `cargo test -q --test mcp_fake_adapter_tests --test tool_registry_slot_tests --test governance_tests` | fake MCP cannot bypass governance or leak secret previews |
| Subagent tree | M6 | `cargo test -q --test subagent_tree_ledger_tests --test subagent_queue_tests --test subagent_report_tests` | spawn tree, admission, report context handoff pass |
| Context snapshot/compaction | M7 | `cargo test -q --test context_engine_tests --test agent_runtime_tests --test app_server_tests` | TurnContext stable; compaction preserves boundaries |
| Status/doctor/app-server surface | M2-M4, M6-M7 | `cargo test -q --test cli_status_tests --test cli_doctor_tests --test app_server_tests --test cli_console_tests` | JSON/text surfaces expose profile, tools, gates, events without secrets |
| Local smoke after first batch | M1-M3 | `sh scripts/chuang-complete-local-smoke.sh` | local contract remains green; no real service required |
| Final candidate after integration | M1-M7 | `sh scripts/chuang-candidate-verify.sh` | candidate gate passes or reports explicit local blocker without live claims |

## Review checklist for each PR

Each implementation PR should answer these before merge:

1. Which slice id does this implement?
2. What exact files were changed?
3. Which new trait/schema/command boundary was added?
4. Which tests prove fake-first contract behavior?
5. Which tests prove high-risk actions still require approval or deny?
6. Which event/receipt proves local actions are audited?
7. Where are secret values redacted?
8. Does the change avoid deletion/cleanup/reset/purge/uninstall behavior?
9. Does the change avoid touching Hermes/Codex Feishu credentials or services?
10. Which follow-up slice is now unblocked?

## Link targets for主控

建议主控后续把本文链接到：

- `docs/codex-claude-optimization-plan-v1.md` 的“当前优先级”段落。
- `docs/implementation-prep-v1.md` 的开头或末尾，作为 M1-M7 的执行拆分。
- `docs/progress-log.md` 的下一条统一进度记录。

