# chuang-agent

创项目本地 Agent 内核 MVP。

## 当前目标

先打通一条稳定、可审计、可插拔的最小主链：

```text
input -> identity/memory -> context -> runtime -> governance -> report -> memory
```

核心只保留身份、记忆、上下文、治理和报告。provider、子代理、桌面/浏览器、控制面、飞书等外部能力走 slot / adapter / plugin。

**调度台原则（重要）**：创不是要处处最强，而是要 **调动最强的 agent 打工**。写代码等工人活默认调用 Codex（或当时最强编码 agent），不在创内核里对标 Claude Code / Codex / Grok 死磕体验；创握紧记忆、治理、编排与插槽。详见 `docs/blueprint-v1.md` §0.1。

## 当前状态

- `chuang`：首选终端入口，直接启动本地交互 REPL；真实 TTY 分层显示「你 / 工作进展 / 小创最终答复」（编号+状态图标，答复在前）。运行中底部有实时状态行：`⏱总时长 · 阶段计时 · 模型 · 超时剩余 · 当前步骤`（约 200ms 刷新）；结束摘要含耗时/模型/思考/执行分段。默认不堆 report id 等内部诊断。`!补充` 可注入当前任务；最近 8 轮自动续聊，`/history` 可查。`/trace`：进行中多模型轮次+更长折叠，结束附技术汇总；`/notrace` 默认；`/verbose` 全量 runtime dump。仍是 stdout transcript，不打印隐藏思维链。
- `chuang ask "TEXT"`：用终端主线跑一次真实本地 runtime。
- `chuang status --config config.toml --json`：查看当前终端主线状态。
- `chuang mainchain-accept`：运行真实标准主链总验收，屏幕只显示阶段 OK/FAIL，详细日志写入 `/tmp/chuang-mainchain-acceptance-*`；会先跑 20 项矩阵和基础合同，再调用真实 provider 完成终端端到端验收和自然语言任务验收，成功时输出 `chuang_mainchain_acceptance_ok`。
- `chuang natural-accept`：单独运行真实自然语言任务验收，会让 Chuang 自己看 git、读日志、修测试失败、生成报告，成功时输出 `chuang_real_natural_acceptance_ok`。
- `chuang accept`：运行终端版完整验收，覆盖入口、真实 provider 工具循环、记忆、子任务和 goal 流，成功时输出 `chuang_terminal_acceptance_ok`。
- `chuang field-accept`（别名 `chuang field`）：**本机 15 项快速验收**（terra/RTK/规范/goal 硬预算/浏览器自动拉起/doctor 可见性）。成功输出 `chuang_field_accept_10_ok`。可 `SKIP_LIVE=1` / `SKIP_BROWSER=1`。
- `chuang browser status|start|stop`：managed headless Chrome（CDP）。`browser_read`/`browser_navigate` 默认也会自动拉起（`CHUANG_HEADLESS_AUTOSTART=0` 关闭）。
- `cargo run -- doctor`：安全健康检查，校验配置、身份记忆、slot 装配和隔离 runtime smoke；摘要含 `browser_cdp` / `tool_shell_rtk_rewrite` 与 `field_accept_next` 提示。
- `cargo run -- status`：查看核心状态（含 `browser_cdp` 与 RTK 开关）。
- `cargo run -- run --config config.toml --input TEXT`：按项目配置跑一轮本地 runtime。
- `cargo run -- run --input TEXT --remember`：跑完后写回 SQLite turn summary。
- `./scripts/launch-chuang-agent-repl.sh`：底层本地交互 REPL 启动脚本；真实 TTY 会显示增强终端外壳，管道模式仍保持简洁输出。真实对话会优先读取仓库外 `CHUANG_PROVIDER_ENV_FILE`（默认 `~/.config/chuang-agent/provider.env`）里的 `CHUANG_PROXY_API_KEY`，只验证链路可用可用 `CHUANG_REPL_STUB=1 ./scripts/launch-chuang-agent-repl.sh` 或 `chuang stub`。
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
- `sh scripts/chuang-mainchain-acceptance.sh`：真实标准主链总验收入口，包含 20 项矩阵、tool runtime 合同、CLI smoke、真实 provider 终端验收和真实自然语言任务验收，成功时输出 `chuang_mainchain_acceptance_ok`。
- `sh scripts/chuang-real-natural-acceptance.sh`：真实自然语言任务验收入口，使用临时 git/python 工作区和真实 provider，成功时输出 `chuang_real_natural_acceptance_ok`。
- `sh scripts/chuang-terminal-acceptance.sh`：终端版主线验收入口；使用临时隔离目录，真实 provider 只用于工具循环，成功时输出 `chuang_terminal_acceptance_ok`。
- `sh scripts/chuang-live-readonly-preflight.sh`：只读 live preflight 主入口，`scripts/chuang-live-readiness-preflight.sh` 仅作兼容别名/旧入口提示；串起 chmod/syntax check、provider fallback smoke、Feishu live preflight smoke、subagent live preflight、watchdog once、console snapshot 和 complete-local smoke，最终输出 `live_readiness_preflight_ok`。
- `GenesisActuator`：新版网页 AI 查询插件线，旧 `BrowserWorker` 暂停推进。
- `cargo test`：全量回归。

当前终端版主线以 `chuang` 为入口；Feishu 只作为后续插件/通道入口，不再作为终端可用性的阻塞项。当前 MVP 边界见 `docs/mvp-scope.md`，就绪状态见 `docs/mvp-readiness-2026-05-02.md`，核心边界见 `docs/core-boundary.md`，provider fallback 诊断见 `docs/provider-fallback-diagnostics.md`，app-server 服务说明见 `docs/app-server-service.md`，channel adapter 协议见 `docs/channel-adapter-protocol.md`，子代理 runner 协议见 `docs/subagent-runner-protocol.md`，新飞书通道检查清单见 `docs/feishu-dedicated-channel-checklist.md`，command 控制面协议见 `docs/control-command-protocol.md`，command 操作面协议见 `docs/actuator-command-protocol.md`，真实控制适配器安全计划见 `docs/real-control-adapter-safety-plan.md`，长期进度见 `docs/progress-log.md`。

## 目录约定

- `src/`：Rust 实现。
- `identity/`：创的最小身份启动层，包含 `SOUL.md`、`STORY.md`、`FIRST_WAKE.md` 和 `agents.toml`。
- `rules/`：治理层 Markdown 规则，当前核心规则为 `rules/core.md`。
- `plugins/`：插件/adapter 注册表，当前示例为 `plugins/registry.example.json`。
- `experiments/`：自我实验计划输出目录，默认只追加计划文件。
- `docs/`：规格草案、架构说明、评审结论
- `tests/`：MVP 合同和回归测试。
- `context/`：协作上下文、提示词、窗口接续材料。
