# MVP Checkpoint 2026-05-01

## 结论

创项目最小 MVP 已经可以启动、运行、健康检查、写回记忆、派发并回收 fake 子代理报告。

当前闭环：

```text
input -> identity/memory -> context -> governed runtime -> report -> optional memory writeback
```

## 已验收命令

```bash
cargo test
cargo run --quiet -- status
cargo run --quiet -- doctor
cargo run --quiet -- run --input "MVP check run" --remember
cargo run --quiet -- run --input "MVP identity check" --remember-identity
cargo run --quiet -- subagent dispatch --task "MVP subagent check"
cargo run --quiet -- subagent list
cargo run --quiet -- subagent run-once --runner fake
cargo run --quiet -- subagent report --run-id cli-run-1
cargo run --quiet -- subagent collect --run-id cli-run-1 --json
cargo run --quiet -- status --json
cargo run --quiet -- doctor --json
```

## 当前能力

- `doctor`：安全健康检查，覆盖配置、身份记忆、slot 装配、隔离 fake runtime、隔离子代理队列 dispatch。
- `run`：默认经过治理层，治理结果进入 CLI 输出、runtime meta 和 report metadata。
- `summary_compression`：非默认轻量压缩策略，会压缩长 memory / tool result 段。
- `subagent queued_external`：文件队列 dispatch / list / report / collect 已闭环，fake runner 可模拟外部执行，`command` runner 可在显式 `--approve-exec` 后把外部进程输出收成 report。
- `config`：支持扁平配置、检查、脱敏展示、初始化。

## 下一阶段

- 真实 provider native HTTPS adapter；当前先通过 `--provider-transport curl` 显式接入系统 curl。
- 真实子代理 runner adapter 继续增强；当前已有显式审批的 `command` runner 最小接缝。
- 真实 control plane adapter。
- 桌面/飞书控制台读取 `doctor --json` 和 `status --json`。

## 保持边界

默认仍保持 fake / 安全模式。真实外部能力必须作为 adapter/plugin 接入，不反向污染核心主链。
