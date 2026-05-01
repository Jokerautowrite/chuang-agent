# 创项目 MVP 边界

## 当前 MVP 目标

先证明创项目内核能独立跑通一轮，而不是先追求完整桌面壳、飞书壳或真实服务控制。

当前 MVP 的核心闭环：

1. 读取长期记忆
2. 打包上下文
3. 经过治理层形成执行请求
4. 通过注入的 execution/provider port 执行
5. 生成可审计报告
6. 可选写回本轮 turn summary
7. 暴露核心状态

核心边界见 `docs/core-boundary.md`。provider、子代理、桌面/浏览器、控制面和外部通道都属于 adapter/plugin，不进入核心主干。

## 已具备能力

- `ChuangKernel::run_turn()`：主运行入口，统一串起 recall、context packing、responder 和 report。
- `ChuangKernel::remember_turn()`：显式写回普通 `turn_summary` 记忆。
- `cargo run -- run --input TEXT`：通过内核运行一轮。
- `cargo run -- run --input TEXT --remember`：运行后写回普通 SQLite turn summary。
- `cargo run -- run --input TEXT --remember-identity`：运行后追加写入 Hermes 风格 `MEMORY.md` 热记忆。
- `cargo run -- status`：查看 MVP 核心状态。
- `cargo run -- status --json`：给未来桌面壳和插件读取结构化状态。
- `cargo run -- doctor`：执行安全健康检查，校验配置、身份记忆、slot 装配、隔离 fake runtime smoke 和隔离子代理队列 smoke。
- `cargo run -- doctor --json`：输出结构化健康检查结果，给桌面控制台或插件读取。
- `cargo run -- status --config PATH`：读取简单 `config.toml`，CLI 参数仍可覆盖配置文件。
- `cargo run -- config init`：生成默认 `config.toml`；目标文件已存在时拒绝覆盖。
- `cargo run -- config check`：只校验配置和内核快照，不执行任务；未传 `--config` 时会自动读取当前目录 `config.toml`（如果存在）。
- `cargo run -- config show --json`：输出脱敏后的配置摘要，给桌面控制台或插件读取。
- `cargo run -- control ...`：保留 fake 控制面协议，用于后续接真实服务/Agent。
- `cargo run -- subagent dispatch --task TEXT`：把子代理任务写入文件队列 dispatch JSON，不启动外部 runner。
- `cargo run -- subagent report --run-id ID`：只读轮询子代理 report JSON。
- `cargo run -- subagent collect --run-id ID`：从 dispatch 恢复运行身份，经 queued slot 校验并回收 report。
- `cargo run -- subagent list`：只读查看 dispatch 队列和 report presence；同一队列目录可连续派发多个任务。
- `cargo run -- subagent run-once --runner fake`：用 fake runner 处理一个 pending dispatch 并写入模拟 report，不执行真实命令。
- `cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec`：显式审批后执行一个外部 runner 命令，把 dispatch JSON 写到 stdin，并把进程输出收成 report。
- `--context-max-tokens / --context-reserve-system-tokens / --context-min-working-tokens / --context-max-tool-results / --context-max-memory-segments`：可从 CLI 调整 context budget。
- `ContextEngine` trait + `deterministic_budget` 默认实现：上下文策略已具备可替换接口。
- `summary_compression` 非默认轻量压缩策略：会对长 memory / tool result 段做本地截断压缩，再交给同一预算 packer，用于验证配置切换面。
- `GenesisActuator` trait + `AutoCliGenesisActuator` 最小实现：主通道 userDataDir，备用 CDP，登录态失效时 fallback，并返回需审批的修复计划，不自动删除 profile。
- `cargo run -- genesis ask --prompt TEXT --approve-exec`：手动验证 Genesis 查询入口；真实外部程序执行必须显式审批。

## 当前明确不做

- 不自动删除任何记忆。
- 不自动压缩身份记忆。
- 不直接操作真实 systemd 服务。
- 不直接控制 Hermes / OpenClaw 进程。
- 不继续扩展旧 `BrowserWorker` 实验线；网页版 AI 查询能力后续改走 `Genesis Actuator` 插件线。
- 不把飞书作为核心依赖，飞书只作为未来插件入口。
- 不在核心层硬编码任何密钥、飞书凭证、Hermes 凭证或本机私有 token。
- 不把 API key 明文写进配置文件；真实 provider 使用 `api_key_env` 引用环境变量。

## 下一步优先级

1. 继续完善真实子代理 runner adapter 的协议约束；当前已有显式审批的 `command` runner 接缝，默认仍保持 fake/queued，不自动执行危险命令。
2. 继续增强 `summary_compression` 的压缩质量，但默认仍保持 `deterministic_budget`。
3. 把 fake control plane 替换为可插拔真实 adapter，但默认仍保持 fake。
4. 新开 `Genesis Actuator` 插件线，先做可审计的 AutoCLI 查询 port；旧 `BrowserWorker` 先冻结。
5. 最后再接桌面壳、飞书插件和服务控制 UI。

## 判定 MVP 可用的最低标准

- `cargo test` 全仓通过。
- `cargo run -- status` 能显示核心状态。
- `cargo run -- doctor` 能确认配置、slot 和隔离 runtime smoke 正常。
- `cargo run -- run --input TEXT` 能返回结构化响应。
- `cargo run -- status --config PATH` 能加载简单配置文件，且 CLI 参数可覆盖配置。
- `cargo run -- run --input TEXT --remember` 能写回记忆，并在下一轮被 recall。
- `cargo run -- run --input TEXT --remember-identity --identity-memory-root PATH` 能显式追加身份热记忆。
- `cargo run -- subagent dispatch --task TEXT --subagent-queue-root PATH` 能生成 dispatch JSON。
- 同一个 `--subagent-queue-root PATH` 下连续 dispatch 多个任务不会覆盖。
- `cargo run -- subagent list --subagent-queue-root PATH` 能列出 dispatch 数量和 report presence。
- `cargo run -- subagent run-once --subagent-queue-root PATH` 能把一个 pending dispatch 转成 fake report。
- `cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec --subagent-queue-root PATH` 能把外部 runner 进程输出转成 report，且缺少审批时拒绝执行。
- `cargo run -- subagent report --run-id ID --subagent-queue-root PATH` 能读取或轮询 report JSON。
- `cargo run -- subagent collect --run-id ID --subagent-queue-root PATH` 能经 dispatch 身份校验回收 report。
- 所有危险操作仍需显式审批或保持 fake。
