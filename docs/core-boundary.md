# 创项目核心边界

## 原则

核心只保留一条稳定主链：

```text
input -> identity/memory -> context -> governance -> execution port -> report -> memory writeback
```

核心负责协议、状态、预算、治理和报告，不负责具体外部能力。

### 调度台，不是全能打手（2026-07-18）

创是 **本地 Agent 操作系统 / 调度台**，不是「再做一个最强编码 Agent」。

- **要强的**：记忆本体、治理、编排派活、插槽可替换、可审计收口。
- **不求最强、直接调用业界最强 agent 的**：写代码（如 Codex）、通才搜索/对话（如 Grok）等工人活。
- **禁止**：在 Codex / Claude Code / Grok 的主业赛道上死磕「处处最强」；禁止把某一工人壳当成本体。

补模块时先问：这是在加强 **调度与边界**，还是在错误赛道重复造工人？后者不做。详见 `docs/blueprint-v1.md` §0.1。

### 派发与并行原则（2026-08-11）

- **按复杂度派子代理**：analyze（只读调研/证据收集）→ execute（有界实现/集成）→
  orchestrate（多段编排/拆活）。简单任务直接做，不派子代理；复杂任务必须拆，不单线程硬扛。
- **多子代理并行，能派多少派多少**：可拆分的独立单元并行执行（max_concurrency 到
  配置上限），不要串行化可以并行的事。父代理负责拆分、合并、最终验收，不把子代理当串行工人。
- 与治理不冲突：并行仍走 governance 审计、子代理只出报告/记忆提案、父代理做最终判定
  （rules/core.md 第 13/14/17 条）。

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
- genesis actuator：网页版 AI 查询插件，核心只依赖统一 ask/search port；AutoCLI、DeepSeek、userDataDir、CDP 真人浏览器都属于具体 adapter。
- browser worker：旧浏览器外脑实验线，当前冻结，不继续作为 MVP 推进方向。
- control plane：systemd、桌面服务、Agent 进程管理。
- external channel：飞书、微信、HTTP、CLI、桌面 UI。
- channel adapter：外部消息和 app-server/runtime 的薄转换层；Feishu/WeChat credential、webhook、ack、重试都属于插件，不进入 core。
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
- `control_plane` 主文件保留控制面协议、治理/审计辅助函数和共用校验；fake 与 command-backed 实现已拆到子模块。真实 systemd/桌面控制必须单独作为 adapter 或外部 command bridge。
- `actuator` 主文件保留人类级操作面协议；fake 与 command-backed 实现已拆到子模块。真实桌面、浏览器、微信、ADB 控制必须单独作为外部 command adapter，不写死进 core。
- `skill_evolver` 主文件保留进化层事件、proposal、trait 和共用校验；noop 占位实现与 dry-run proposal adapter 已拆到子模块。dry-run 只能生成带 `dry_run=true / writes_skills=false / requires_approval=true` 和 provenance 的候选，不得写 skill；真实技能提炼/固化必须单独作为 evolver adapter。
- `memory_store` 主文件保留记忆记录、查询、命中、trait 和错误类型；in-memory 测试/开发实现已拆到子模块。SQLite、Hermes 双文件、未来向量/远程记忆都必须作为独立实现。
- `hermes_memory` 主文件保留 Hermes 双文件记忆配置、快照、条目、错误和 trait；真实文件读写实现已拆到子模块。
- `context_engine` 主文件保留 segment、budget、packed context、packer 算法、trait 和错误类型；deterministic 与 summary_compression 策略包装已拆到子模块。未来真实摘要压缩/优先级/对话树策略必须作为独立 engine。
- `governance` 主文件保留动作、风险决策、错误类型和 trait；static-rule 实现已拆到子模块。未来策略引擎/审批通道/组织规则必须作为独立治理实现。
- `slot_registry` 已引入 `ProviderSlot / GovernanceSlot / ActuatorSlot / EvolutionSlot / ControlPlaneSlot`，避免 `RuntimeSlots` 字段直接绑定具体实现类型。
- `slot_registry` 也负责 `GenesisActuator` 的具体构造入口，CLI 只拿 `GenesisSlot` wrapper，不直接持有 AutoCLI 细节。
- `browser_worker` 明确属于 adapter/plugin 能力线。旧实现当前先冻结，不作为 MVP 推进方向；MVP 主入口、runtime、kernel、slot registry 不应直接依赖它。
- `Genesis Actuator` 是后续网页 AI 查询能力的新插件线：对核心只暴露 `ask/search` 语义，主通道 userDataDir、备用 CDP、登录态检测和修复策略都必须留在 adapter 内，并接受治理和审计约束。
