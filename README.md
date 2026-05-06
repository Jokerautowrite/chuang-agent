# chuang-agent

创项目本地 Agent 内核 MVP。

## 当前目标

先打通一条稳定、可审计、可插拔的最小主链：

```text
input -> identity/memory -> context -> runtime -> governance -> report -> memory
```

核心只保留身份、记忆、上下文、治理和报告。provider、子代理、桌面/浏览器、控制面、飞书等外部能力走 slot / adapter / plugin。

## 当前状态

- `cargo run -- doctor`：安全健康检查，校验配置、身份记忆、slot 装配和隔离 runtime smoke。
- `cargo run -- status`：查看核心状态。
- `cargo run -- run --config config.toml --input TEXT`：按项目配置跑一轮本地 runtime。
- `cargo run -- run --input TEXT --remember`：跑完后写回 SQLite turn summary。
- `./scripts/launch-chuang-agent-repl.sh`：启动本地交互 REPL；默认只显示对话正文。真实对话会优先读取仓库外 `CHUANG_PROVIDER_ENV_FILE`（默认 `~/.config/chuang-agent/provider.env`）里的 `CODEX_PPTOKEN_API_KEY`，需要调试诊断时可直接加 `--verbose`；只验证链路可用可用 `CHUANG_REPL_STUB=1 ./scripts/launch-chuang-agent-repl.sh`。
- `cargo run -- memory identity show|append|write-user|write-memory`：管理 Hermes 风格 `USER.md / MEMORY.md`，覆盖写入必须显式 `--approve-overwrite`。
- `--provider-transport stub|http|native|curl`：OpenAI-compatible provider 的四种接入形态。
- `fallback_provider = "openai_compatible"`：可在配置里显式启用备用 provider；未配置时不会 silent fallback。
- `config.example-provider-fallback.toml` / `sh scripts/chuang-provider-fallback-smoke.sh`：provider fallback 操作员配置示例和本地 fixture 验证入口；secret 只通过 `api_key_env` 指向环境变量。
- `cargo run -- app-server`：JSON-RPC 式应用入口，当前会读取 workspace `config.toml` 并写会话记忆；后续新飞书机器人应接这里或独立 channel adapter。
- `cargo run -- app-server health --workspace-root PATH --json`：只读健康检查，验证 workspace runtime 配置，不发起模型请求。
- `cargo run -- control list|apply --config PATH`：通过 command 控制面管理服务/Agent；未配置 command adapter 时只做本地协议检查。
- `config.example-control.toml`：安全 command 控制面示例，只验证协议，不控制真实服务。
- `cargo run -- subagent dispatch --task TEXT`：写入子代理 dispatch 文件队列。
- `cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec`：显式执行外部 runner，并把输出收成 report。
- `cargo run -- subagent run-loop --runner command --runner-command PATH --approve-exec --max-runs N --max-concurrency 1 --capability rust`：按队列批量处理 pending dispatch，并声明 worker 能力。
- `scripts/chuang-subagent-runner-example.sh`：安全子代理 runner 示例，读取 dispatch stdin 并输出标准 `SubagentReport`。
- `cargo run -- genesis ask --prompt TEXT --approve-exec`：显式执行 Genesis 网页 AI 查询插件入口。
- `cargo run -- genesis ask --prompt TEXT --dry-run`：只查看 Genesis 主/备通道命令，不执行外部程序。
- `cargo run -- experiment plan --goal TEXT --success TEXT`：生成安全自我实验计划，不执行、不回滚、不删除。
- `cargo run -- experiment complete --experiment-id ID --outcome success|failure|inconclusive --summary TEXT --next TEXT`：为实验追加不可覆盖的结果报告。
- `cargo run -- experiment list`：只读查看实验计划和报告状态。
- `cargo run -- experiment show --experiment-id ID`：只读查看某个实验的计划和报告内容。
- `cargo run -- channel simulate --workspace-root PATH --message-id ID --sender-id ID --text TEXT`：本地演练外部消息通道，不接真实飞书。
- Chuang Feishu bridge 本地命令：`/new` 作为开新窗口/新上下文入口，提示新开飞书聊天/话题/线程并保持不进入 Agent runtime；`/help` 显示桥命令。
- `cargo run -- console snapshot --json`：给未来桌面/工具/服务控制台读取只读状态、插件摘要、control unit 列表和插件清单。
- `cargo run -- plugin list|check --registry plugins/registry.example.json`：查看和校验插件/adapter 注册表，不执行插件。
- `sh scripts/chuang-mvp-smoke.sh`：安全端到端验收脚本，使用临时目录和 stub provider，不触碰真实服务。
- `sh scripts/chuang-second-test-smoke.sh`：第二测试版本验收入口，复用同一安全 smoke，但输出 `second_test_smoke_ok`。
- `sh scripts/chuang-complete-local-smoke.sh`：完整本地可用闭环验收入口，串起第二测试 smoke、watchdog 一次性只读检查、本地诊断读面和飞书本地命令 smoke，最终输出 `complete_local_smoke_ok`。
- `GenesisActuator`：新版网页 AI 查询插件线，旧 `BrowserWorker` 暂停推进。
- `cargo test`：全量回归。

当前 MVP 边界见 `docs/mvp-scope.md`，就绪状态见 `docs/mvp-readiness-2026-05-02.md`，核心边界见 `docs/core-boundary.md`，provider fallback 诊断见 `docs/provider-fallback-diagnostics.md`，app-server 服务说明见 `docs/app-server-service.md`，channel adapter 协议见 `docs/channel-adapter-protocol.md`，子代理 runner 协议见 `docs/subagent-runner-protocol.md`，新飞书通道检查清单见 `docs/feishu-dedicated-channel-checklist.md`，command 控制面协议见 `docs/control-command-protocol.md`，command 操作面协议见 `docs/actuator-command-protocol.md`，真实控制适配器安全计划见 `docs/real-control-adapter-safety-plan.md`，长期进度见 `docs/progress-log.md`。

## 目录约定

- `src/`：Rust 实现。
- `identity/`：创的最小身份启动层，包含 `SOUL.md`、`STORY.md`、`FIRST_WAKE.md` 和 `agents.toml`。
- `rules/`：治理层 Markdown 规则，当前核心规则为 `rules/core.md`。
- `plugins/`：插件/adapter 注册表，当前示例为 `plugins/registry.example.json`。
- `experiments/`：自我实验计划输出目录，默认只追加计划文件。
- `docs/`：规格草案、架构说明、评审结论
- `tests/`：MVP 合同和回归测试。
- `context/`：协作上下文、提示词、窗口接续材料。
