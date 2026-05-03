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
