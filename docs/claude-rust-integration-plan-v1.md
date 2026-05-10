# claude-rust Integration Plan V1

日期：2026-05-11
依据：`docs/claude-rust-slot-audit-v1.md`

## 目标

把 `claude-rust` 的成熟模块吸收到 Chuang Slot 体系中，但保持 Chuang 的核心边界：

- core runtime 依赖 trait / event / report，不依赖具体外部实现。
- governance 永远在执行链路上。
- 子代理只回传 `SubagentReport` / memory proposal，不直写核心记忆。
- 普通本地能力默认执行，高危操作才询问或拒绝。

## 里程碑

### M1：Execution / ToolRegistrySlot

新增 `ToolRegistrySlot` 设计与 fake contract，不接真实 MCP。

交付：

- `ToolDescriptor`：name、schema、permission_level、read_only、destructive、concurrent_safe、path。
- `RegisteredTool` trait 草案，对齐 `claude-rust-types::Tool`，但字段名使用 Chuang 语义。
- Fake registry contract tests。
- 现有 `ToolCall` enum 到 `ToolDescriptor` 的只读映射。

验收：

- status/doctor 能显示 trait registry readiness。
- 现有 `tool_runtime_tests` 不回退。
- governance 能按 descriptor 分类 read-only / local action / destructive。

### M2：MCP Fake Adapter

先接 fake MCP server，不接真实外部 MCP。

交付：

- `McpToolAdapter` contract doc。
- fake stdio MCP server script。
- list/call/timeout/malformed-json/stderr-noise tests。
- secret redaction tests。

验收：

- MCP tool 只能经 Chuang `ExecutionSlot` 触发。
- MCP dangerous tool 会进入高危治理，不允许绕过 allowlist。

### M3：Permission Policy Adapter

吸收 `claude-rust-permission` 的模式和 pattern matcher，但不引入 bypass。

映射：

| claude-rust | Chuang |
| --- | --- |
| Normal | high_risk_approval |
| AutoAccept | default_local_no_approval |
| Plan | read_only_plan |
| Bypass | forbidden / diagnostics only |

验收：

- 普通 open/click/input 不询问。
- delete/reset/uninstall/payment/verification/service/network/secret 询问或拒绝。
- deny rule 优先级高于 allow rule。

### M4：AgentLoop Adapter Spike

不替换主链，只做 sidecar spike。

交付：

- `ClaudeRustLoopAdapter` 设计文档。
- 映射 `EngineEvent` 到 Chuang runtime event。
- overload retry / tool result self-correction / parallel safe tool execution 对照测试。

验收：

- 可以用 fake provider 和 fake tools 重放一轮 tool-use loop。
- 不触碰 Feishu app-server 主链。

### M5：Read-only Explorer Subagent

借鉴 `agent_tool.rs` 的 nested engine，但输出 Chuang 标准报告。

交付：

- `explorer` adapter spike。
- read-only tool policy。
- `SubagentReport` 转换。
- ReportAdmission 验证。

验收：

- explorer 不能写文件、不能执行危险工具、不能写 core memory。
- report 被 Chuang admission 接受后才能进入 collect。

## 当前优先级

先做 M1 + M2。原因：

- Execution 是 Chuang 后续 MCP、browser、skills、subagents 的共同底座。
- Tool trait / registry 对现有 enum 工具协议是增量增强，不会扰动 Feishu live 主链。
- MCP fake contract 能尽早锁住外部工具边界。

## 暂缓

- TUI / HTTP server 替换。
- Claude OAuth provider 默认接入。
- `claude-rust-memory` 作为核心 memory backend。
- 真实 MCP 外部工具市场接入。
