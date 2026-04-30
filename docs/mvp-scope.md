# 创项目 MVP 边界

## 当前 MVP 目标

先证明创项目内核能独立跑通一轮，而不是先追求完整桌面壳、飞书壳或真实服务控制。

当前 MVP 的核心闭环：

1. 读取长期记忆
2. 打包上下文
3. 调用 responder
4. 生成可审计报告
5. 可选写回本轮 turn summary
6. 暴露核心状态

## 已具备能力

- `ChuangKernel::run_turn()`：主运行入口，统一串起 recall、context packing、responder 和 report。
- `ChuangKernel::remember_turn()`：显式写回普通 `turn_summary` 记忆。
- `cargo run -- run --input TEXT`：通过内核运行一轮。
- `cargo run -- run --input TEXT --remember`：运行后写回普通 SQLite turn summary。
- `cargo run -- run --input TEXT --remember-identity`：运行后追加写入 Hermes 风格 `MEMORY.md` 热记忆。
- `cargo run -- status`：查看 MVP 核心状态。
- `cargo run -- status --json`：给未来桌面壳和插件读取结构化状态。
- `cargo run -- status --config PATH`：读取简单 `config.toml`，CLI 参数仍可覆盖配置文件。
- `cargo run -- config check`：只校验配置和内核快照，不执行任务；未传 `--config` 时会自动读取当前目录 `config.toml`（如果存在）。
- `cargo run -- config show --json`：输出脱敏后的配置摘要，给桌面控制台或插件读取。
- `cargo run -- control ...`：保留 fake 控制面协议，用于后续接真实服务/Agent。
- `cargo run -- subagent dispatch --task TEXT`：把子代理任务写入文件队列 dispatch JSON，不启动外部 runner。
- `cargo run -- subagent report --run-id ID`：只读轮询子代理 report JSON。
- `cargo run -- subagent list`：只读查看 dispatch 队列和 report presence；同一队列目录可连续派发多个任务。
- `cargo run -- subagent run-once --runner fake`：用 fake runner 处理一个 pending dispatch 并写入模拟 report，不执行真实命令。
- `--context-max-tokens / --context-reserve-system-tokens / --context-min-working-tokens / --context-max-tool-results / --context-max-memory-segments`：可从 CLI 调整 context budget。
- `ContextEngine` trait + `deterministic_budget` 默认实现：上下文策略已具备可替换接口。

## 当前明确不做

- 不自动删除任何记忆。
- 不自动压缩身份记忆。
- 不直接操作真实 systemd 服务。
- 不直接控制 Hermes / OpenClaw 进程。
- 不把飞书作为核心依赖，飞书只作为未来插件入口。
- 不在核心层硬编码任何密钥、飞书凭证、Hermes 凭证或本机私有 token。
- 不把 API key 明文写进配置文件；真实 provider 使用 `api_key_env` 引用环境变量。

## 下一步优先级

1. 把子代理文件队列接到真实 runner adapter，但默认仍保持 fake/queued，不自动执行危险命令。
2. 给 context engine 增加第二个非默认策略占位，例如 summary_compression，但默认仍保持 deterministic_budget。
3. 把 fake control plane 替换为可插拔真实 adapter，但默认仍保持 fake。
4. 最后再接桌面壳、飞书插件和服务控制 UI。

## 判定 MVP 可用的最低标准

- `cargo test` 全仓通过。
- `cargo run -- status` 能显示核心状态。
- `cargo run -- run --input TEXT` 能返回结构化响应。
- `cargo run -- status --config PATH` 能加载简单配置文件，且 CLI 参数可覆盖配置。
- `cargo run -- run --input TEXT --remember` 能写回记忆，并在下一轮被 recall。
- `cargo run -- run --input TEXT --remember-identity --identity-memory-root PATH` 能显式追加身份热记忆。
- `cargo run -- subagent dispatch --task TEXT --subagent-queue-root PATH` 能生成 dispatch JSON。
- 同一个 `--subagent-queue-root PATH` 下连续 dispatch 多个任务不会覆盖。
- `cargo run -- subagent list --subagent-queue-root PATH` 能列出 dispatch 数量和 report presence。
- `cargo run -- subagent run-once --subagent-queue-root PATH` 能把一个 pending dispatch 转成 fake report。
- `cargo run -- subagent report --run-id ID --subagent-queue-root PATH` 能读取或轮询 report JSON。
- 所有危险操作仍需显式审批或保持 fake。
