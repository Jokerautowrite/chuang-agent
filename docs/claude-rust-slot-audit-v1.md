# claude-rust Slot Audit V1

日期：2026-05-11
对象：`/home/user/projects/claude-rust`
定位：评估 `claude-rust` 对 Chuang 九大 Slot 的可移植价值。

补充：本文是 Claude 单源审计；Codex 代码级审计与双参考后的执行优先级见 `docs/codex-architecture-audit-v1.md` 和 `docs/codex-claude-optimization-plan-v1.md`。

## 结论

`claude-rust` 是 Chuang 当前最值得吸收的 Rust 参考实现之一，但不应整体替换 Chuang runtime。正确路径是按 Slot 做 adapter / contract test / fake-first 接入。

最高优先级不是 UI，而是三条能力线：

1. `Execution`：吸收 `Tool` trait、`ToolRegistry`、MCP 动态工具。
2. `AgentLoop`：吸收 `QueryEngine` 的流式事件、工具回灌、重试、并发安全工具执行和 compaction trigger。
3. `Governance`：吸收权限模式、allow/deny pattern、skill scoped allow rules，但保留 Chuang 的高危策略。

`GroupCoordinator` 的评价要降级：`claude-rust` 有可用的 `agent` / `explore` 子引擎工具和轻量 coordinator crate，但不是完整的群体协同系统。它适合作为 Chuang `SubagentSpawner` 的 in-process / read-only explorer adapter 原型，不适合直接当最终核二。

## Slot 映射

| Chuang Slot | claude-rust 模块 | 适配等级 | 迁移方案 |
| --- | --- | --- | --- |
| AgentLoop | `claude-rust-engine::QueryEngine` | 高 | 抽出 `AgentLoopAdapter` 设计；先移植事件模型、工具回灌、overload retry、parallel safe tool 执行策略，不直接替换现有 Feishu runtime。 |
| GroupCoordinator / SubagentSpawner | `claude-rust/src/infrastructure/agent_tool.rs`、`claude-rust-coordinator` | 中 | 先做 `ClaudeRustSubagentAdapter` 设计稿：`agent` = execute worker，`explore` = read-only worker；保留 Chuang `SubagentReport` / ReportAdmission，不让子代理直写 core memory。 |
| Execution | `claude-rust-types::Tool`、`claude-rust-tools::ToolRegistry`、MCP | 很高 | 新增 Chuang `ToolRegistrySlot` 方案，把现有 `ToolCall` enum 逐步桥接成 trait tool；MCP 必须先走 fake MCP server 合同测试，再接真实外部工具。 |
| Governance | `claude-rust-permission` | 高 | 吸收 `PermissionMode`、config allow/deny、pattern matcher、skill scoped allow；默认模式映射 Chuang 当前“普通本地动作直接执行，高危询问/拒绝”。不要采用 `Bypass` 绕过 Chuang Governance。 |
| Identity / Memory | `claude-rust-memory` | 低到中 | 其 JSON session repository 可作为 session persistence adapter 参考；不适合作为 Chuang 核心 memory backend。可借鉴 atomic write 和 project slug。 |
| Context | `claude-rust-compact`、`claude-rust-engine::compactor` | 中高 | 借鉴 4-stage compaction：auto threshold、micro tool-result trim、session memory extraction、provider summary；接入 Chuang `ContextEngine` 时必须保留 deterministic pack trace。 |
| Interface | `claude-rust-server`、`claude-rust-tui`、`claude-rust` | 中 | HTTP `/chat` / `/stream` 和 TUI 可作为备用 interface adapter；当前 Chuang 仍优先 Feishu，本阶段不切 UI 主线。 |
| Provider | `claude-rust-provider` | 中高 | 参考 streaming Provider trait、SSE parser、usage/thinking/model switch；实际 provider 仍保持 OpenAI-compatible 主线，避免引入 Claude OAuth 依赖作为默认。 |
| Skill / Plugin / Evolver | `claude-rust-skills`、`claude-rust-plugins`、commands frontmatter | 中 | 可吸收 skill `allowed-tools` 和 commands frontmatter 作为 Chuang SkillEvolver / plugin registry 输入格式；固化仍走 Chuang validation/report。 |

## 关键证据

- `claude-rust-engine/src/application/query_engine.rs`：已实现 provider streaming、tool-use stop reason、工具结果回灌、overload retry、hook、并发安全工具并行执行、context threshold compaction。
- `claude-rust-types/src/domain/tool.rs`：`Tool` trait 具备 discovery、permission、destructive/read-only/concurrent-safe、validation、path extraction、display summary 等元数据，明显强于 Chuang 当前 enum 工具协议。
- `claude-rust-tools/src/infrastructure/mcp_client.rs` 与 `mcp_tool.rs`：已有 stdio JSON-RPC MCP client、tools/list、tools/call 和动态工具注册雏形。
- `claude-rust-permission/src/infrastructure/config_aware_checker.rs`：权限模式、allow/deny pattern、skill allow rules 可直接服务 Chuang “默认非审批 + 高危审批”策略。
- `claude-rust/src/infrastructure/agent_tool.rs`：子代理通过嵌套 `QueryEngine` 实现，`explore` 是 read-only 且 concurrent-safe；这是 Chuang explorer worker 的好原型。
- `claude-rust-memory`：只提供 session JSON repository，不是多层记忆系统。
- `claude-rust-coordinator`：当前更多是 task/message bus scaffold，不足以称为完整 group coordinator。

## 第一阶段任务

1. 写 `docs/claude-rust-integration-plan-v1.md`，定义 adapter 方向和禁止直接替换的边界。
2. 新增 `ToolRegistrySlot` 设计：Chuang core 继续吃 `ToolCall`，同时预留 trait tool registry；先 fake registry，再桥接本地工具。
3. 新增 MCP fake contract：启动一个本地 fake MCP server，验证 list/call、超时、stderr 隔离、secret redaction、governance classification。
4. 新增 `ClaudeRustPermissionPolicy` 设计：把 `Normal/AutoAccept/Plan/Bypass` 映射为 Chuang `default_local_no_approval/high_risk_approval/read_only_plan`，禁止真正 bypass governance。
5. 新增 in-process explorer adapter spike：只读 `explore` 子代理返回 Chuang `SubagentReport`，不写核心记忆，不绕过 ReportAdmission。

## 不做

- 不把 `claude-rust-engine::QueryEngine` 直接替换 Feishu app-server 主链。
- 不引入 Claude OAuth / keychain 作为 Chuang 默认 provider。
- 不让 MCP 工具绕过 Chuang governance、allowlist 和审计。
- 不把 `claude-rust-memory` 当作 Chuang 核心记忆层。
- 不把 `Bypass` 模式作为开源默认能力。
