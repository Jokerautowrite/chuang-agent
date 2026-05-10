# Codex Architecture Audit V1

日期：2026-05-11
对象：OpenAI Codex 官方源码 `/tmp/openai-codex-audit`
版本：commit `76845d7`
定位：补全 Chuang 对 Codex 的代码级架构审计，用于和 `claude-rust` 一起校准落地细节。

## 结论

Chuang 最初方向没有问题：Slot / trait / event / governance / memory-body 仍然成立。当前效果不够好的原因不是蓝图错，而是执行内核、工具协议、治理执行面和状态追踪还不够像成熟 Agent runtime。

Codex 当前最值得吸收的不是 UI，而是五条工程骨架：

1. `protocol`：Submission Queue / Event Queue，把 turn、approval、tool、MCP、dynamic tool、interrupt 都变成统一协议事件。
2. `core::Session` / `tasks` / `TurnContext`：一条 thread 一次只跑一个 turn，turn context 明确携带 cwd、permissions、sandbox、model、tools、environment。
3. `ToolRegistry` + `ToolHandler`：工具不是散落的 enum 分支，而是可注册、可观测、可 hook、可标记 mutating / parallel 的执行单元。
4. `exec_policy` + `sandboxing` + `guardian`：审批、规则、sandbox transform、网络访问、MCP approval 和 patch approval 走同一条治理脊柱。
5. `state` + `rollout-trace`：thread metadata、agent jobs、spawn edges、tool dispatch、multi-agent edge 可持久化、可回放、可归约。

## 关键源码证据

| 能力线 | Codex 模块 | 可迁移价值 |
| --- | --- | --- |
| Core protocol | `codex-rs/protocol/src/protocol.rs` | `Submission { id, op, trace }` 与 `Event { id, msg }` 明确 SQ/EQ；`Op` 覆盖 turn、interrupt、approval、MCP elicitation、dynamic tool response、rollback、review、shell command。 |
| App-server API | `codex-rs/app-server-protocol/src/protocol/v2/{thread,turn}.rs` | `thread/start` 与 `turn/start` 可单独覆盖 cwd、approval、sandbox、permissions、model、environment、dynamic tools，适合 Chuang Feishu/CLI/HTTP 共享同一 runtime 面。 |
| Turn context | `codex-rs/core/src/session/turn_context.rs` | `TurnContext` 是工具执行的事实源：cwd、permission profile、network policy、tool config、dynamic tools、provider、model、environment 都集中在 turn 内，而不是散在脚本 env。 |
| Session lifecycle | `codex-rs/core/src/session/session.rs`、`core/src/tasks/mod.rs` | 一个 session 最多一个 running task；task trait 统一 regular/review/compact/user-shell；interrupt 有可见历史 marker 和 abort event。 |
| Tool registry | `codex-rs/core/src/tools/registry.rs` | `ToolHandler` 暴露 `spec()`、`supports_parallel_tool_calls()`、`is_mutating()`、pre/post hook payload、diff consumer、typed output；dispatch 统一埋点、hook、gate、telemetry、trace。 |
| Unified exec | `codex-rs/core/src/unified_exec/mod.rs` | 交互式进程执行把 approval、sandbox selection、PTY、输出限流、background process、stdin 统一在一个 orchestrator 里，不再让 shell 工具各自处理风险。 |
| Exec policy | `codex-rs/core/src/exec_policy.rs` | prefix rules、dangerous/safe command heuristics、approval policy conflict、allow/prompt/forbidden、规则 amendment 全部结构化。 |
| Sandbox | `codex-rs/sandboxing/src/manager.rs`、`protocol/src/permissions.rs` | permission profile 编译成 filesystem/network policy，再转平台 sandbox；保护 `.git`、`.agents`、`.codex` 这类元数据路径。 |
| Guardian | `codex-rs/core/src/guardian/approval_request.rs` | Shell、ExecCommand、Execve、ApplyPatch、NetworkAccess、McpToolCall、RequestPermissions 都有同构 approval action。 |
| MCP / dynamic tools | `core/src/mcp_tool_call.rs`、`core/src/tools/handlers/dynamic.rs` | MCP 工具有独立 begin/end event、approval meta、connector policy、elicitation；dynamic tool 通过 event 发给外部客户端并等待响应。 |
| Multi-agent | `core/src/agent/*`、`core/src/tools/handlers/multi_agents_v2/*` | root-scoped `AgentControl`、spawn depth、thread spawn edges、agent path/nickname/role、send/wait/close/list 工具已经成体系。 |
| State / trace | `codex-rs/state/*`、`codex-rs/rollout-trace/*` | thread metadata、agent jobs、dynamic tools、spawn edges 入 SQLite；rollout trace 可记录 thread、turn、tool dispatch、agent interaction edge。 |

## 对 Chuang 的 Slot 修正

| Chuang Slot | Codex 吸收点 | 对当前落地的修正 |
| --- | --- | --- |
| AgentLoop | `SessionTask` + `TurnContext` + SQ/EQ event | 不再让 Feishu 主链直接拼 prompt / 工具循环；先把 turn 运行收成事件驱动 state machine。 |
| Execution | `ToolRegistry`、`UnifiedExec`、dynamic tools | 把现有 atomic tools 从 enum 分支升级为 registry handler；每个工具必须声明 mutating、parallel、schema、pre/post hook、trace。 |
| Governance | `ExecPolicyManager`、`GuardianApprovalRequest`、permission profile | Chuang “普通本地无审批，高危询问”要落成 policy profile，不是 prompt 口头要求；高危与外部提交仍强制治理。 |
| Context | `TurnContext`、thread/turn overrides、environment selection | 将 cwd、desktop env、provider/model、tools、permissions 放入 turn context，避免 open-source 后写死本机环境。 |
| GroupCoordinator / SubagentSpawner | `AgentControl`、spawn depth、agent path、multi_agents_v2 tools | Chuang 子代理应从“脚本队列”升级到 root-scoped agent tree；报告仍走 Chuang ReportAdmission。 |
| Interface | app-server v2 thread/turn APIs | Feishu、CLI、console 都走同一套 thread/turn API，不各自做隐式状态。 |
| Provider | `codex-client` / model manager / turn model override | provider 仍保持 OpenAI-compatible，但要吸收 per-turn model/profile snapshot 和 stream error event。 |
| State / Memory | `state` thread metadata + rollout trace | Codex state 不是核心记忆本体，但适合做运行态 ledger；长期 memory 继续归 Chuang/Hermes 层。 |
| Skill / Plugin | `skills`、`core-plugins`、tool exposure | skill/plugin 的发现与注入可以参考 Codex，但执行仍要进入 Chuang ToolRegistry + Governance。 |

## Codex 与 Claude 的分工

Codex 更强的地方：

- 运行态骨架：thread、turn、interrupt、approval、event、state。
- 执行安全：permission profile、sandbox、exec policy、network policy、guardian approval。
- 可审计性：SQLite state、rollout trace、tool dispatch trace、multi-agent edge reducer。
- app-server 协议：多客户端共享同一内核。

Claude-rust 更强或更适合作为轻量参考的地方：

- `Tool` trait 元数据更接近 Chuang 需要的工具描述层。
- MCP stdio client / fake-first 接入更容易直接迁移。
- `QueryEngine` 工具回灌、overload retry、compaction trigger 适合优化模型工具循环。
- permission allow/deny pattern 与 skill scoped rules 适合补 Chuang policy UX。

## 不直接照搬

- 不把 Codex app-server 协议变成 Chuang 唯一内部协议；它是参考模型，Chuang 仍保留自己的 Slot traits。
- 不把 Codex 的 `danger-no-sandbox` 当默认开源模式；Chuang 默认应是“普通本地自动 + 高危治理 + 可配置审批模式”。
- 不让 dynamic tools / MCP / plugin 直接绕过 Chuang governance。
- 不把 Codex state 当长期记忆本体；它是运行 ledger，不是 Identity/Memory。
- 不让子代理直接写核心记忆或绕过 ReportAdmission。

## 第一批应落地的修正

1. 新增 `RuntimeEventLedger` 设计：把 turn/tool/approval/subagent/actuator 事件先统一成可序列化 ledger。
2. 新增 `ToolRegistrySlot`：吸收 Codex dispatch 生命周期 + Claude Tool metadata。
3. 新增 `PermissionProfileSlot`：把 Chuang 的 no-approval 默认行为表达成 profile，同时把删除/网络/服务/验证码/密钥/外部提交固定为高危。
4. 新增 `ExecutionTrace`：工具调用必须有 started/completed/failed、risk decision、evidence uri、output preview。
5. 新增 `SubagentTreeLedger`：记录 root/child thread、spawn edge、agent role、report admission、closed status。

