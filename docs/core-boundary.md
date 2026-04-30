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
- control plane：systemd、桌面服务、Agent 进程管理。
- external channel：飞书、微信、HTTP、CLI、桌面 UI。
- evolver：技能提炼、SOP 固化、外脑同步。

这些模块可以进仓库，但不能反向成为 core 的硬依赖。

## 当前护栏

- core 不直接构造 `FakeResponder` 或 OpenAI-compatible adapter；由 CLI、测试或后续 plugin loader 注入。
- `runtime_config` 和 `main` 属于组合层，可以认识具体 adapter，但不要把具体实现传回 core。
- 新能力默认先落在 adapter/plugin，只有身份、记忆、上下文、治理、报告这类稳定语义才允许进入 core。

## 当前迁移状态

- `responder` 保留 responder trait、provider adapter trait、fake/scripted 测试实现。
- `provider_openai_compatible` 承载 OpenAI-compatible 具体 adapter；调用点直接引用 provider 模块。
- `subagent_spawner` 主文件保留协议类型、trait、slot 转发和共用校验；fake / queued 实现已拆到子模块。
- `control_plane` 当前 fake 实现仍在同一文件。真实 systemd/桌面控制必须单独作为 adapter。
