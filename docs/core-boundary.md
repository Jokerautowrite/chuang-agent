# 创项目核心边界

## 原则

核心只保留一条稳定主链：

```text
input -> identity/memory -> context -> governance -> execution port -> report -> memory writeback
```

核心负责协议、状态、预算、治理和报告，不负责具体外部能力。

## Core

- `chuang_kernel`：回合生命周期和记忆写回。
- `agent_runtime`：召回、上下文打包、调用 responder 抽象。
- `memory_store` / `memory_recall` / `memory_admission` / `memory_policy`：记忆接口、召回、准入和预算。
- `context_engine`：上下文策略接口和默认确定性预算实现。
- `governance`：动作风险判定和审计。
- `runtime_report` / `subagent_report`：结构化结果和可审计报告。
- `common` / `lifecycle`：通用 ID、时间、生命周期状态。

## Adapter / Plugin

- provider：OpenAI-compatible、本地模型、未来任意模型后端。
- subagent：Codex/OpenClaw/Hermes/GenericAgent runner。
- actuator：桌面、浏览器、键鼠、剪贴板、ADB、微信/飞书等真实操作面。
- browser worker：浏览器外脑、网页专家、opencli/Chrome 驱动等外部工作流。
- control plane：systemd、桌面服务、Agent 进程管理。
- external channel：飞书、微信、HTTP、CLI、桌面 UI。
- evolver：技能提炼、SOP 固化、外脑同步。

这些模块可以进仓库，但不能反向成为 core 的硬依赖。

## 当前护栏

- core 不直接构造 `FakeResponder` 或 OpenAI-compatible adapter；由 CLI、测试或后续 plugin loader 注入。
- `runtime_config` 和 `main` 属于组合层，可以认识具体 adapter，但不要把具体实现传回 core。
- `slot_registry` 属于组合层，负责把配置映射为 slot；上层只依赖 slot wrapper 和 trait，不直接绑定具体实现类型。provider adapter 也在这里实例化，`runtime_config` 只描述配置。
- 新能力默认先落在 adapter/plugin，只有身份、记忆、上下文、治理、报告这类稳定语义才允许进入 core。

## 当前迁移状态

- `responder` 主文件保留 responder trait、provider adapter trait 和统一壳；fake/scripted 测试实现已拆到子模块。
- `provider_openai_compatible` 承载 OpenAI-compatible 具体 adapter；调用点直接引用 provider 模块。
- `subagent_spawner` 主文件保留协议类型、trait、slot 转发和共用校验；fake / queued 实现已拆到子模块。
- `control_plane` 主文件保留控制面协议、治理/审计辅助函数和共用校验；fake 实现已拆到子模块。真实 systemd/桌面控制必须单独作为 adapter。
- `actuator` 主文件保留人类级操作面协议；fake 实现已拆到子模块。真实桌面、浏览器、微信、ADB 控制必须单独作为 adapter。
- `skill_evolver` 主文件保留进化层事件、proposal、trait 和共用校验；noop 占位实现已拆到子模块。真实技能提炼/固化必须单独作为 evolver adapter。
- `memory_store` 主文件保留记忆记录、查询、命中、trait 和错误类型；in-memory 测试/开发实现已拆到子模块。SQLite、Hermes 双文件、未来向量/远程记忆都必须作为独立实现。
- `hermes_memory` 主文件保留 Hermes 双文件记忆配置、快照、条目、错误和 trait；真实文件读写实现已拆到子模块。
- `context_engine` 主文件保留 segment、budget、packed context、packer 算法、trait 和错误类型；deterministic 与 summary_compression 策略包装已拆到子模块。未来真实摘要压缩/优先级/对话树策略必须作为独立 engine。
- `governance` 主文件保留动作、风险决策、错误类型和 trait；static-rule 实现已拆到子模块。未来策略引擎/审批通道/组织规则必须作为独立治理实现。
- `slot_registry` 已引入 `ProviderSlot / GovernanceSlot / ActuatorSlot / EvolutionSlot / ControlPlaneSlot`，避免 `RuntimeSlots` 字段直接绑定具体实现类型。
- `browser_worker` 明确属于 adapter/plugin 能力线。它可以保留在仓库中继续试验 opencli/网页外脑能力，但 MVP 主入口、runtime、kernel、slot registry 不应直接依赖它。
