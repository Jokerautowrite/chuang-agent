# Chuang Agent Harness 接入方案（2026-08-08）

> 目的：把本机已有的 agent 方法论资产（skills）接入 chuang，补齐"自驱 harness 环"。
> 结论：chuang 已内置 harness 骨架（goal_mode + skill_evolver + benchmark），差 3 步接成完整自驱环。

## 一、本机方法论资产盘点（可接入 chuang）

### `.claude/skills`（小k 母库 · harness 方法论）

| Skill | 核心内容 | 接 chuang 方式 |
| --- | --- | --- |
| agent-harness-loop | observe→decide→act→record→reflect + **bilevel 外环**（重复失败改规则） | chuang 缺的"自驱 loop"核心 |
| verifier-first-loop | 先写 verifier 再开 loop；空 session stub 不算完整学 | 接 goal validate（验收先行） |
| eval-outer-loop | 量化验收 + 人类摩擦检测 | 接 benchmark |
| failure-surface-first | 部分失败≠全败；admission forks | 接 failover 语义 |
| session-handoff-continuity | 跨会话不丢状态；磁盘状态优先 | 接 session archive（已有基础） |
| context-budget-thrift | 省上下文；section 优先于全文 | chuang context budget 可强化 |
| proactive-infra-triage | 基建失败（401/402/ENXIO）优先修 harness | 自驱任务选题参考 |
| proactive-topic-roi | 选题 ROI，避免 meta 空循环 | 自驱任务选题参考 |

### `.agents/skills`（另一套体系）

| Skill | 核心内容 | 接 chuang 方式 |
| --- | --- | --- |
| long-running-app-harness | planner→contract→build→evaluator 完整循环 | **最接近"标准 harness"** |
| agent-builder | 核心哲学：capabilities + 简单 loop | 架构理念 |
| evolver | 自我进化引擎（auto-log 分析/自修复） | 与 skill_evolver 异曲同工 |
| cognitive-memory | core/episodic/semantic/vault 记忆分层 | 记忆体系借鉴 |
| code-review | 全面审查清单（安全/正确性/性能/可维护） | 审计方法论 |
| agents-team | orchestrator/worker/reviewer 多代理协作 | 子代理编排参考 |

## 二、chuang 已内置的 harness 骨架

```
goal_mode        → 目标 + 预算 + checkpoint 策略
goal_dispatch    → 目标分派
goal_run         → 目标执行（含 checkpoint 记录）
skill_evolver    → 进化：proposal → validation → approval
benchmark        → 评估（benchmark_evaluator）
```

已有 = planner + build + eval + evolve 的**碎片**，缺串联。

## 三、离完整自驱 harness 差 3 步

1. **串成自驱环**：goal_mode 偏"单轮目标执行"，缺"observe→reflect→外环改规则"的多轮循环
2. **验收先行**：goal 的 validate 是配置校验，还没做到 verifier-first（先写可量化验收再跑）
3. **进化闭环**：skill_evolver 有 proposal/validation/approval，但没接"重复失败→自动改规则"的外环触发

## 四、建议接入顺序

| 阶段 | 动作 | 对应资产 |
| --- | --- | --- |
| P0 | 把 agent-harness-loop 的 bilevel 外环概念落成 chuang 配置/规则 | agent-harness-loop |
| P0 | goal 模式接 verifier-first（验收先行） | verifier-first-loop |
| P1 | skill_evolver 接"重复失败→外环"触发 | evolver / agent-harness-loop |
| P1 | benchmark 接 eval-outer-loop（量化验收） | eval-outer-loop |
| P2 | session 持久化强化（事件级审计落库） | session-handoff-continuity |
| P2 | cognitive-memory 分层借鉴 | cognitive-memory |

## 五、配套文档（本机，已落盘）

- 审计留档：`docs/security-audit-20260808.md`（2026-08-08 全面审计结果）
- 本方案：`docs/harness-integration-plan.md`（本文）
