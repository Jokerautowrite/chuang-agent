# Template / Rule Asset Gap Scan - 2026-06-28

## 结论

本次只读扫描发现：Agent Hub 已有 `core/`、`runbooks/`、`policies/` 基础规则，也已有大量公共 skill 候选；当前主要缺口不是“再搬 skill”，而是把 Chuang 文档和 `$HOME/reports` 中反复出现的 **通用模板、规则、报告样板** 提炼成 Agent Hub 公共资产。

本报告没有复制文件、没有修改 inventory、没有创建软链接、没有删除、没有 commit/push。

## 扫描范围

- `$HOME/.codex/memories/skills`
- `$HOME/.codex/skills`
- `$CHUANG_AGENT_ROOT/docs`
- `$HOME/reports`
- `$HOME/agent-hub/core`
- `$HOME/agent-hub/runbooks`
- `$HOME/agent-hub/templates`
- `$HOME/agent-hub/policies`

## 已有 Agent Hub 覆盖

### 已覆盖较好

| 类别 | 已有位置 | 评价 |
| --- | --- | --- |
| 共同守则 | `core/constitution.md` | 已覆盖共享层根原则。 |
| 文件/环境/密钥边界 | `core/filesystem.md`, `core/environment.md`, `core/secret-policy.md` | 已覆盖“只记录路径、不泄露 secret”的基础规则。 |
| 只读与治理策略 | `policies/readonly.yaml`, `policies/governance.yaml`, `policies/ops.yaml` | 已有基础 policy，但缺少更细的报告/脱敏模板。 |
| 子代理派发 | `runbooks/dispatch-subagent.md`, `templates/subagent-dispatch-packet.md` | 已有最小派发包模板。 |
| 公共 skill 管理 | `core/shared-assets.md`, `runbooks/shared-assets.md`, `inventory/skills.yaml` | 已有公共/私有/缓存/敏感状态边界。 |
| Doctor / review / import | `runbooks/doctor.md`, `runbooks/review.md`, `runbooks/import-review.md` | 已有低风险只读工作流。 |

### 主要缺口

- `templates/` 只有 `subagent-dispatch-packet.md`，缺少 handoff、acceptance、live receipt、doctor/report、overnight summary 等通用报告样板。
- `core/` 缺少从 Chuang 抽象出的“可插拔运行时 / memory architecture / protocol contract / prompt doctrine”公共原则。
- `runbooks/` 缺少 tool-health、cron failure digest、correction miner、live acceptance、local failover 等可复用操作手册。
- `policies/` 缺少 redaction/data-classification/evolution-runner/prompting 等更细规则。

## 建议复制或提炼到 Agent Hub 的公共资产

### A. 报告与交接模板

| 来源 | 公共价值 | 建议目标目录 | 建议动作 |
| --- | --- | --- | --- |
| `$CHUANG_AGENT_ROOT/docs/handoff-current.md` | 多轮工程交接格式可复用。 | `templates/handoff.md` | 不直接复制全文；提炼为“当前状态 / 已改资源 / 验证 / 风险 / 下一步”模板。 |
| `$CHUANG_AGENT_ROOT/docs/global-real-live-receipt-handoff-2026-05-30.md` | real-live receipt 收口格式可复用。 | `templates/live-receipt-handoff.md` | 提炼为 live receipt 交接模板。 |
| `$CHUANG_AGENT_ROOT/docs/live-receipt-collection.md` | readiness / receipt / acceptance 概念边界清晰。 | `runbooks/live-receipt-collection.md`, `templates/live-receipt.md` | 抽象为公共验收收集手册和模板。 |
| `$CHUANG_AGENT_ROOT/docs/acceptance-next-matrix.md` | acceptance matrix 适合多项目复用。 | `templates/acceptance-matrix.md` | 提炼矩阵字段，不复制 Chuang 状态。 |
| `$CHUANG_AGENT_ROOT/docs/third-test-candidate.md` | 分层验收入口样板可复用。 | `templates/test-candidate-entry.md` | 提炼 local-ready / manual-live-check 结构。 |
| `$HOME/reports/jarvis-evolution-overnight/hourly-*.md` | hourly 增量摘要格式可复用。 | `templates/hourly-progress-summary.md` | 提炼为夜间/长跑任务增量报告模板。 |
| `$HOME/reports/jarvis-evolution-overnight/final-report.md` | 最终汇总报告结构可复用。 | `templates/final-synthesis-report.md` | 提炼为 Top N / 风险 / 下一步汇总模板。 |
| `$HOME/agent-hub/reports/doctor-*.md` | Agent Hub 自己已有 doctor report 样板。 | `templates/doctor-report.md` | 从已有报告提炼，不再依赖历史报告复制。 |
| `$HOME/agent-hub/reports/review-*.md` | review proposal 样板已稳定。 | `templates/review-proposal.md` | 提炼 source reports / summary / candidates / yaml snippet。 |

### B. 规则与核心原则

| 来源 | 公共价值 | 建议目标目录 | 建议动作 |
| --- | --- | --- | --- |
| `$CHUANG_AGENT_ROOT/docs/pluggable-architecture-v1.md` | provider/memory/context/subagent/actuator/governance/evolver 插槽原则适合 Agent Hub。 | `core/pluggable-runtime.md` | 提炼接口优先、无 silent fallback、contract test 等原则。 |
| `$CHUANG_AGENT_ROOT/docs/core-boundary.md` | Core / Adapter / Plugin 边界可复用。 | `core/runtime-boundary.md` | 提炼为公共边界，不复制项目状态。 |
| `$CHUANG_AGENT_ROOT/docs/memory-architecture-layering.md` | 记忆分层与 shared sediment 理念高度契合 Agent Hub。 | `core/memory-architecture.md` | 提炼五层分工、迁移顺序、禁止误解。 |
| `$CHUANG_AGENT_ROOT/docs/memory-maintenance-loop.md` | 记忆维护闭环可复用。 | `runbooks/memory-maintenance.md` | 提炼维护对象、约束、下一步。 |
| `$CHUANG_AGENT_ROOT/docs/prompt-doctrine-2026-06-20.md` | prompt 运行时规则可复用。 | `core/prompt-doctrine.md` 或 `policies/prompting.yaml` | 提炼提示词原则，避免复制 Chuang 专属路径。 |
| `$HOME/reports/jarvis-evolution-overnight/behavior-session-mining.md` | 用户纠错沉淀方法可复用。 | `policies/behavior-learning.yaml`, `runbooks/correction-mining.md` | 只提炼分类法和脱敏要求；不复制私人偏好原文。 |
| `$HOME/reports/jarvis-evolution-overnight/correction-miner-spec.md` | correction miner 的事件 schema / 脱敏规则可复用。 | `runbooks/correction-miner.md`, `templates/correction-event.md`, `policies/redaction.yaml` | 强烈建议提炼，注意不带历史会话内容。 |
| `$HOME/reports/jarvis-evolution-overnight/apex-dao-runner-spec.md` | evolution runner 候选评分和轨迹模板可复用。 | `runbooks/evolution-runner.md`, `templates/evolution-candidate.md`, `policies/evolution.yaml` | 提炼 runner I/O、Dao 评分字段和 guardrails。 |

### C. 操作手册 / Runbook

| 来源 | 公共价值 | 建议目标目录 | 建议动作 |
| --- | --- | --- | --- |
| `$HOME/reports/jarvis-evolution-overnight/tool-health-spec.md` | 工具健康检查 schema 可直接公共化。 | `runbooks/tool-health.md`, `templates/tool-health-report.md` | 提炼输入、输出、状态定义、blocked 条件。 |
| `$HOME/reports/jarvis-evolution-overnight/cron-failure-digest-spec.md` | cron 失败摘要和静默成功规则可复用。 | `runbooks/cron-failure-digest.md`, `templates/cron-failure-digest.md` | 提炼脱敏和失败摘要格式。 |
| `$HOME/reports/jarvis-evolution-overnight/README.md` | overnight pack 的安全边界和命令结构可复用。 | `runbooks/overnight-runner.md` | 提炼为长跑任务安全手册。 |
| `$CHUANG_AGENT_ROOT/docs/live-operator-test-runbook.md` | live operator test 的只读门禁可复用。 | `runbooks/live-operator-test.md` | 抽象一键只读检查、人工测试顺序、停止条件。 |
| `$CHUANG_AGENT_ROOT/docs/terminal-goal-watchdog-sop.md` | goal/watchdog 收口流程可复用。 | `runbooks/goal-watchdog.md` | 提炼查看进度、收口、暂停恢复、安全规则。 |
| `$CHUANG_AGENT_ROOT/docs/local-physical-machine-failover-runbook-2026-06-01.md` | 本机接管/故障转移框架有公共价值。 | `runbooks/local-host-failover.md` | 只提炼检查项和架构选项；不要复制具体资产/IP/域名。 |
| `$CHUANG_AGENT_ROOT/docs/app-server-service.md` | service template 可复用。 | `runbooks/app-server-service.md`, `templates/systemd-service.md` | 提炼 health check 和 service template。 |
| `$HOME/.codex/memories/skills/feishu-bridge-readonly-triage/SKILL.md` | Feishu bridge 只读排障流程可复用。 | `runbooks/feishu-bridge-readonly-triage.md` | 不复制 skill；提炼通用 systemd/journal/network 检查流程。 |
| `$HOME/.codex/memories/skills/feishu-bridge-optional-dependency-recovery/SKILL.md` | optional dependency recovery 流程可复用。 | `runbooks/codex-optional-dependency-recovery.md` | 仅作为工作站 runbook，保留私有路径为占位符。 |

### D. 协议 / Contract 样板

| 来源 | 公共价值 | 建议目标目录 | 建议动作 |
| --- | --- | --- | --- |
| `$CHUANG_AGENT_ROOT/docs/subagent-runner-protocol.md` | 子代理派发、claim、timeout、collection 规则可复用。 | `core/protocols/subagent-runner.md`, `templates/subagent-report.md` | 与现有 `templates/subagent-dispatch-packet.md` 配套。 |
| `$CHUANG_AGENT_ROOT/docs/channel-adapter-protocol.md` | channel adapter 的输入输出边界可复用。 | `core/protocols/channel-adapter.md` | 提炼 inbound/app-server/output，不复制 Feishu 专属状态。 |
| `$CHUANG_AGENT_ROOT/docs/control-command-protocol.md` | control plane list/apply schema 可复用。 | `core/protocols/control-command.md` | 提炼 config/list/apply/output schema。 |
| `$CHUANG_AGENT_ROOT/docs/actuator-command-protocol.md` | actuator 命令和 allowlist 概念可复用。 | `core/protocols/actuator-command.md`, `policies/actuator-risk.yaml` | 提炼命令协议和风险边界。 |
| `$CHUANG_AGENT_ROOT/docs/execution-slot-tool-protocol.md` | execution/tool/report/app-server 字段可复用。 | `core/protocols/execution-slot-tool.md` | 适合做公共接口参考。 |
| `$CHUANG_AGENT_ROOT/docs/browser-read-contract.md` | browser read 边界可复用。 | `core/protocols/browser-read.md` | 提炼只读边界。 |
| `$CHUANG_AGENT_ROOT/docs/knowledge-read-contract.md` | knowledge read contract 可复用。 | `core/protocols/knowledge-read.md` | 提炼只读知识读取边界。 |
| `$CHUANG_AGENT_ROOT/docs/provider-fallback-diagnostics.md` | provider fallback 诊断字段可复用。 | `runbooks/provider-fallback-diagnostics.md`, `templates/provider-diagnostic-report.md` | 提炼 runtime fields 和 acceptance examples。 |
| `$CHUANG_AGENT_ROOT/docs/real-control-adapter-safety-plan.md` | 真实控制适配器安全边界可复用。 | `policies/real-control-adapter.yaml`, `runbooks/real-control-adapter.md` | 提炼 allowlist、apply rules、preflight audit。 |

### E. Skill 相关资产

| 来源 | 当前状态 | 建议目标目录 | 建议动作 |
| --- | --- | --- | --- |
| `$HOME/.codex/skills/.system/imagegen` | 已在 `inventory/skills.yaml` 登记为 `public_candidate`，Agent Hub 已有 `skills/image-media/imagegen`。 | 无新增复制缺口。 | 可另提炼 `runbooks/image-generation-governance.md`，记录生成后本地落盘、不可覆盖、透明背景降级需确认等规则。 |
| `$HOME/.codex/skills/.system/openai-docs` | 已登记并已有 Agent Hub skill。 | 无新增复制缺口。 | 可另提炼 `runbooks/official-docs-verification.md`，记录官方文档优先级。 |
| `$HOME/.codex/skills/.system/plugin-creator` | 已登记并已有 Agent Hub skill。 | 无新增复制缺口。 | 暂不再复制。 |
| `$HOME/.codex/skills/.system/skill-creator` | 已登记并已有 Agent Hub skill。 | 无新增复制缺口。 | 暂不再复制。 |
| `$HOME/.codex/skills/.system/skill-installer` | 已登记并已有 Agent Hub skill。 | 无新增复制缺口。 | 暂不再复制。 |
| `$HOME/.codex/skills/subscription-upgrade-image` | 已登记并已有 Agent Hub skill。 | 无新增复制缺口。 | 保留 API key / secret path 外置原则。 |
| `$HOME/.codex/skills/thirdparty-image2` | 已登记并已有 Agent Hub skill。 | 无新增复制缺口。 | 不复制 secret；可提炼 Image2 endpoint 规则为 runbook。 |

## 建议保持私有 / 不复制项

| 来源 | 原因 | 建议处理 |
| --- | --- | --- |
| `$HOME/.codex/memories/skills/*` 原始 skill 全文 | 属于 Xiaoce/Codex 记忆技能，含本机 Feishu bridge 路径和运行假设。 | 不直接复制；只提炼通用 runbook。 |
| `$CHUANG_AGENT_ROOT/docs/progress-log.md` | Chuang 项目历史进度，不是公共模板。 | 保持项目内；可抽象 progress-log 模板。 |
| `$CHUANG_AGENT_ROOT/docs/handoff-current.md` 原文 | 含 Chuang 长期项目状态。 | 不复制原文；提炼模板。 |
| `$CHUANG_AGENT_ROOT/docs/blueprint-v1.md` 原文 | 是 Chuang 项目蓝图，公共价值在原则，不在全文。 | 保持项目内；提炼 architecture pattern。 |
| `$CHUANG_AGENT_ROOT/docs/spec-v2.md`, `spec-v3.md`, `implementation-prep-v1.md` 原文 | Chuang 内部规格，容易把项目状态误当公共规则。 | 保持项目内；只抽象 schema/contract 模板。 |
| `$CHUANG_AGENT_ROOT/docs/sub2-*.md` | Sub2 业务/生产操作文档，可能含业务状态、路径、订阅规则。 | 不复制；只提炼部署/升级 runbook 骨架。 |
| `$CHUANG_AGENT_ROOT/docs/vultr-*.md` | 具体主机、域名、迁移状态、运行资产。 | 不复制；只提炼 migration/failover 模板。 |
| `$HOME/reports/agent-hub-inventory-plan-20260609.md` | 已被 Agent Hub 当前 core/inventory/runbook 吸收，且含历史状态。 | 不复制；作为 provenance 保留在原位置。 |
| `$HOME/reports/jarvis-evolution-overnight/behavior-session-mining.md` 原文 | 含用户偏好和行为纠错历史，隐私/身份浓度高。 | 不复制原文；只提炼匿名分类和脱敏规则。 |
| `$HOME/reports/jarvis-evolution-overnight/project-assets.md` | 项目资产机会图，含本机项目状态。 | 不复制；可作为 inventory scan 思路参考。 |
| `$HOME/reports/jarvis-evolution-overnight/hourly-*.md` 原文 | 具体一次夜间任务过程。 | 不复制原文；提炼 hourly 模板。 |
| `$HOME/reports/sub2-revenue/*.xlsx` | 业务营收/成本/利润数据。 | 绝对不复制到 Agent Hub。 |
| `skills/imported-local-*`, `skills/organized-local-*`, `plugins/*` runtime/cache 派生物 | 已有筛选与候选流程，不能把缓存当规范来源。 | 继续按 `core/shared-assets.md`：扫描、登记、确认后再动。 |

## 建议新建目标目录

当前 `core/` 直接堆规则文档还可用，但协议类资产会越来越多。建议未来如果执行复制/提炼，先创建这些目录：

- `core/protocols/`：放 channel、subagent、actuator、execution、browser/knowledge read contract。
- `templates/reports/`：放 handoff、doctor、review、acceptance、live receipt、hourly/final summary。
- `templates/schemas/`：放 correction event、tool-health JSON、evolution candidate、subagent report。
- `runbooks/ops/`：放 tool-health、cron digest、local failover、service template、provider diagnostics。
- `policies/data/`：放 redaction、data classification、private/public asset policy。

如果暂时不想扩目录，也可以先平铺到现有 `core/`、`runbooks/`、`templates/`、`policies/`。

## 建议优先级

1. `templates/handoff.md`, `templates/live-receipt.md`, `templates/acceptance-matrix.md`：最能减少交接和验收重复。
2. `runbooks/tool-health.md`, `runbooks/cron-failure-digest.md`, `runbooks/correction-miner.md`：来自 `$HOME/reports/jarvis-evolution-overnight`，可施工性强。
3. `core/memory-architecture.md`, `core/pluggable-runtime.md`, `core/prompt-doctrine.md`：把 Chuang 的公共原则沉淀进 Agent Hub。
4. `core/protocols/subagent-runner.md`, `core/protocols/actuator-command.md`, `core/protocols/channel-adapter.md`：补齐跨 agent 协议参考。
5. `policies/redaction.yaml`, `policies/evolution.yaml`, `policies/real-control-adapter.yaml`：补齐高风险自动化边界。

## 本轮未执行动作

- 未复制任何候选文件。
- 未修改 `inventory/skills.yaml` 或其他 inventory。
- 未创建软链接。
- 未删除、清理、移动任何文件。
- 未修改 Codex 自己运行配置。
- 未 commit/push。
