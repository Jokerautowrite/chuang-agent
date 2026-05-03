# Goal Mode Operating Plan

更新时间：2026-05-03

## 定位

这里记录当前 Codex 侧采用的目标驱动推进方式，以及后续迁移到 Chuang 的目标态。

当前阶段它是协作流程，不是 Chuang runtime 的内核模块。不要为了“goal mode”新增核心 slot，也不要打断现有主链：

```text
input -> identity/memory -> context -> runtime -> governance -> execution slot -> report -> memory
```

## 当前用法

每轮推进先固定四件事：

1. Goal：本轮只推进一个主线目标。
2. Acceptance：写清楚可验证结果。
3. Budget：限定本轮范围，避免扩散到子代理、飞书或外部智能体。
4. Checkpoint：结束前更新 `docs/progress-log.md` 和 `docs/handoff-current.md`。

当前默认 goal：

```text
补全 Chuang 主进程 Execution Slot，让主进程能稳定调用 GA 原子工具映射，并把治理、审计、结构化回传打通。
```

当前验收标准：

```text
cargo fmt --all
git diff --check
timeout 240s cargo test -q
```

## Codex 0.128 实战借鉴

2026-05-03 用 Codex 子代理按 goal-style 连续推进 Chuang 主线，验证出一套可迁移到 Chuang 的长期任务组织方式。当前 Codex 侧没有可直接稳定调用的显式 `goal` 子命令，因此本轮使用 `GOAL_SPEC` 文本契约驱动子代理；对 Chuang 来说，真正值得吸收的是执行组织模式，不是某个 CLI 命令名字。

这条也同步写入飞书架构终稿：**Codex 是 Chuang 的 Rust 骨架参考实现。后续本地执行、安全边界、审批、沙箱、验证、回传、goal-style 长任务推进和子代理组织方式，先审计 Codex Rust 源码与现有行为，再决定移植、裁剪或复用接口。少造轮子，多复制成熟实现。**

### 已验证有效的模式

1. 主进程只做目标拆分、边界定义、集成审核和最终提交。
2. 每个子代理必须拿到完整 `GOAL_SPEC`：目标、写入范围、禁止事项、验收命令、最终报告格式。
3. 子代理任务必须按文件和模块拆开，避免并行写同一核心文件。
4. 子代理不直接提交，主进程统一审 diff、跑格式检查、跑 smoke、跑全量测试，再按逻辑提交。
5. 每轮完成后关闭子代理，避免后台会话残留。
6. 私有 `config.toml`、飞书桥、Hermes、本机密钥和真实服务控制都不进入子代理写入范围。

本轮验证过的分工样例：

```text
Worker A: tool / GA atomic manifest
Worker B: governance / runtime observability
Worker C: identity / memory / context diagnostics
Worker D: provider / fallback diagnostics
Worker E: report / audit identity
Worker F: config / doctor / readiness
Worker G: app-server / channel protocol
Worker H: control / actuator command contract
```

### 迁移成 Chuang 能力时的最小设计

Chuang 后续可以在现有 `GoalSpec` 基础上增加一个轻量 `GoalRun` 概念，但仍不新增 core slot：

```text
GoalRun
  -> goal_spec
  -> worker_plan[]
  -> disjoint_write_scopes[]
  -> validation_plan
  -> integration_policy
  -> checkpoint_log
```

它应该落在现有主链外壳上：

```text
GoalSpec -> Governance -> Context -> Execution Slot -> Report -> Memory
```

子代理并行属于 Execution Slot 下游能力。主进程必须保留唯一集成权：

- 子代理只能产出 patch / report / validation result。
- 主进程负责合并、复验、提交、更新记忆。
- 失败子代理必须有结构化失败报告，不能悄悄消失。
- 大任务优先拆为 2 个子代理，稳定后再扩到 3-4 个。

### 不迁移的部分

- 不把 Codex 的当前实现细节硬编码进 Chuang。
- 不把 goal mode 做成无限后台循环。
- 不允许子代理绕过 governance、approval、audit。
- 不让子代理直接操作真实飞书、Hermes、密钥、本机服务或桌面控制。

## 少造轮子原则

后续实现顺序固定为：

1. 先找 Codex Rust 是否已有成熟实现。
2. 能移植就移植，能裁剪就裁剪，能按协议适配就适配。
3. 只有当 Codex 的实现与 Chuang 的记忆本体、可拔插边界或本机安全约束冲突时，才写新的实现。
4. 新抽象必须说明替换点和收益；不能为了“看起来架构化”多包一层。

## 近期优先级

1. 继续收紧 Execution Slot 的正式 action/request schema，减少纯字符串协议。
2. 继续让 app-server / channel / runtime report 共用同一套工具事件和工具报告结构。
3. 补治理策略配置和审计字段，不让主进程工具绕过审批边界。
4. 主进程稳定后再做子代理 runner 增强。
5. 子代理稳定后再做外部智能体和搜索能力。

## 迁移到 Chuang 的目标态

未来 Chuang 可以把 goal mode 做成 `Governance + Runtime + Memory` 的轻量能力：

```text
GoalSpec
  -> goal_id
  -> objective
  -> acceptance_checks
  -> budget
  -> allowed_slots
  -> checkpoint_policy
  -> final_report_policy
```

目标执行仍然走现有主链，不新增第十个 slot。Goal 只负责给 runtime 一个可审计的长期任务外壳：

- Governance 判断目标是否越权。
- Context 固定本轮目标和验收标准。
- Execution Slot 执行被允许的本地工具。
- Report 输出阶段性结果。
- Memory 写入目标进度和下一步。

当前最小落地：

```text
src/goal_mode.rs
tests/goal_mode_tests.rs
```

`GoalSpec` 目前只定义目标、验收、预算、允许 slot、checkpoint 策略和最终报告策略，并能渲染成 runtime extra context。它不执行命令、不绕过治理、不新增 slot。

## 禁止事项

- 不把 goal mode 做成绕过治理的后台执行器。
- 不自动删除、清理、reset 或卸载任何东西。
- 不复用 Codex/Hermes 飞书通道。
- 不把外部智能体调度提前塞进主进程主线。
