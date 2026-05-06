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

当前主线验收顺序固定为：先看 `status --json` 的只读 readiness，再跑 `doctor --json` 的安全健康检查，然后用 `scripts/chuang-mvp-smoke.sh` 串起 goal/session/channel/subagent/control/experiment 的端到端冒烟。smoke 不连接真实飞书、不控制真实服务、不读取真实密钥，只使用临时目录、stub provider 和安全示例 adapter。

## 已具备能力

- `ChuangKernel::run_turn()`：主运行入口，统一串起 recall、context packing、responder 和 report。
- `ChuangKernel::remember_turn()`：显式写回普通 `turn_summary` 记忆。
- `cargo run -- run --input TEXT`：通过内核运行一轮。
- `./scripts/launch-chuang-agent-repl.sh`：启动本地交互 REPL，默认读取项目根 `config.toml`，并优先从仓库外 `CHUANG_PROVIDER_ENV_FILE`（默认 `~/.config/chuang-agent/provider.env`）加载 `CODEX_PPTOKEN_API_KEY`；缺失时只给出提示，不回落到 fake。设置 `CHUANG_REPL_STUB=1` 时只走 stub 链路验证。
- `cargo run -- run --goal TEXT --input TEXT`：把长期目标作为额外 context segment 注入 runtime；不改写原始 `user_input`，不新增 slot，不绕过治理。
- `cargo run -- goal plan --objective TEXT [--root PATH] [--goal-id ID]`：创建本地 `GoalRun` 计划 JSON，用于 checkpoint-first continuation；只记录目标计划，不执行命令。
- `cargo run -- goal checkpoint --summary TEXT --completed-worker-id ID --validation-note TEXT [--completed-worker-id ID ...] [--validation-note TEXT ...] [--root PATH] [--goal-id ID]` 和 `cargo run -- goal show ...`：追加/读取 `GoalRun` checkpoint，用于下一轮恢复目标状态；不自动续跑、不调度子代理。
- `cargo run -- run --input TEXT --remember`：运行后写回普通 SQLite turn summary。
- `cargo run -- run --input TEXT --session-id ID --remember-session`：写入带 session 范围的 turn summary；后续同 session recall 会带隔离诊断，不跨 session 召回。
- `cargo run -- memory session search --query TEXT [--session-id ID] [--limit N]`：只读检索历史 `turn_summary`；带 `--session-id` 时按会话隔离过滤。
- `cargo run -- memory lim extract --query TEXT [--session-id ID] [--limit N]`：只读生成 LIM 候选经验，带 provenance，不自动写回。
- `cargo run -- memory maintenance report --query TEXT [--session-id ID] [--limit N]`：只读生成维护报告，复用 session search 和 LIM extract，输出健康状态、候选和建议，不自动写回。
- `cargo run -- memory maintenance apply --query TEXT [--session-id ID] [--limit N] [--candidate-id ID] [--approve-writeback]`：在人工确认后把 LIM 候选写回 `experiences.md`，默认幂等跳过重复候选，不自动维护。
- `cargo run -- memory knowledge search --root PATH --query TEXT [--limit N]`：只读检索本地 markdown/text 外脑目录，输出 provenance hit，不连接真实 wiki/GBrain，不写核心记忆，不注入 runtime。
- `cargo run -- run --input TEXT --remember-identity`：运行后追加写入 Hermes 风格 `MEMORY.md` 热记忆。
- `cargo run -- run --input TEXT --remember-experience`：显式把本轮结果按 provenance 写入 `experiences.md`，用于内部经验层沉淀；默认运行不自动写经验。
- `cargo run -- memory identity show`：只读展示当前 `USER.md / MEMORY.md` 全文、字符数和硬上限。
- `data/hermes-memory/experiences.md`：内部经验层的 MVP contract；`status/config show/doctor/memory identity show` 可诊断它的路径和存在性，`--remember-experience` 或 `append-experience` 可显式写入，但当前不默认自动写入、不注入 prompt。
- `cargo run -- memory identity append --id ID --content TEXT`：显式追加一条 `MEMORY.md` 热记忆。
- `cargo run -- memory identity append-experience --id ID --content TEXT`：显式追加一条带来源的经验层条目。
- `cargo run -- memory identity write-user|write-memory --content TEXT --approve-overwrite`：显式覆盖写入压缩后的 `USER.md` 或 `MEMORY.md`，用于完成“超限拒绝后由模型/老爸决定保留内容”的闭环。
- `identity/SOUL.md`、`identity/STORY.md`、`identity/FIRST_WAKE.md`、`identity/agents.toml`：最小身份启动层，启动时作为冻结 identity context 注入。
- `rules/core.md`：治理层 Markdown 规则；slot 构建时加载，治理决策 reason 会带规则指纹，便于追溯。
- `--provider-transport stub|http|native|curl`：OpenAI-compatible provider 的当前接入形态；`native` 已支持 `https://` 目标。
- 显式 provider fallback：配置 `fallback_*` 字段后，组合层可在主 provider 结构化失败时切备用 provider；未配置时不会 silent fallback。
- `cargo run -- app-server`：JSON-RPC 式应用入口，会读取 workspace `config.toml`，并用 thread id 写会话记忆；后续新飞书机器人可接这个入口或独立 channel adapter。
- `cargo run -- app-server health --workspace-root PATH --json`：只读健康检查 workspace runtime 配置，不发起模型请求。
- `cargo run -- channel simulate --workspace-root PATH --message-id ID --sender-id ID --text TEXT`：本地演练外部消息通道，读取 workspace 配置并返回 `ChannelOutboundMessage`。
- `cargo run -- channel simulate ... --goal TEXT`：本地演练通道目标上下文注入；真实飞书桥是否传 goal 仍由独立 channel adapter 决定。
- `cargo run -- plugin list|check --registry PATH`：读取插件注册表，统一展示 channel、runner、control、actuator、genesis adapter，不执行插件。
- `cargo run -- status`：查看 MVP 核心状态。
- `cargo run -- status --json`：给未来桌面壳和插件读取结构化状态，包含 execution slot、GA 原子工具 manifest/schema、mapped/interface-only 原子工具名单、goal mode、goal_run readiness、identity bootstrap presence、provider request timeout、只读 `plugin_registry` 摘要。
- `status --json` / `doctor --json` 的 `project_readiness`：按主链模块给出 `ready / partial / deferred / blocked` 和下一步动作。当前正常状态是 `ready`，不是“全部真实外部服务都已接通”。
- `status --json` / `doctor --json` 的 `release_readiness`：给出当前测试版本交付的顶层结论。当前正常状态是 `second_test_version_ready`，表示第二测试版本围绕 readiness、smoke、goal/run 续接和 subagent protocol 可回归验收；真实外部服务验证仍按 adapter 边界后置。
- `status` / `doctor` / `config check|show` 会输出 `placeholder_warnings`，明确标出仍是占位的 adapter，避免把 fake 测试实现误认为真实能力；项目根配置当前应显示 `placeholder_warnings: none`。
- `status --json` / `doctor --json` 的 `memory_readiness`：按内部记忆、历史会话、LIM、外脑知识库、自动维护闭环给出 `ready / partial / deferred / blocked` 和下一步动作。当前五层本地第二测试版边界为 `ready`，但这不代表真实 wiki/GBrain 已接通，也不代表自动维护会自行写长期记忆。
- `status --json` / `doctor --json` 的 `channel_readiness`：按 app-server、channel simulate、Chuang 专用飞书桥、Codex/Hermes 隔离、rich messages 拆分状态。它只确认边界和脚本存在性，不代表真实飞书连接在线。
- `status --json` / `doctor --json` 的 `subagent_readiness`：按 dispatch queue、report collect、command runner、multi-worker orchestration、external-AI downstream 拆分状态，并显式区分 `local_contract_ready` 与 `live_adapter_ready`。当前 `queued_external` 里本地协议合同可验收，但真实外部 worker/live adapter 仍未接入，协议层也不是自动执行器。
- `data/skills/external_agent_dispatch_sop.md`：外部 AI 分身调度 Skill contract，定义平台选择、任务翻译、质量评级、追问上限、记忆写回和审计边界；它不是真实浏览器/HTTP adapter，不新增 core slot。
- `data/skills/unified_identity_engine_adapter.md`：外部 AI 的 lower adapter contract，定义平台/session 复用、结构化输入输出、失败类和审计边界；它仍不是实际登录态执行器。
- `cargo run -- doctor`：执行安全健康检查，校验配置、身份记忆、slot 装配、actuator observe、control list、隔离 fake runtime smoke 和隔离子代理队列 smoke。
- `cargo run -- doctor --json`：输出结构化健康检查结果，包含 atomic tools、goal mode、goal_run readiness、plugin registry 等只读验收项，并内嵌当前 status 快照，给桌面控制台或插件读取。
- `cargo run -- console snapshot --json`：只读聚合 `status`、插件注册表摘要、control unit 列表和插件清单，作为未来桌面/工具/服务控制台的数据源；不执行 control apply，不启动服务。
- `cargo run -- status --config PATH`：读取简单 `config.toml`，CLI 参数仍可覆盖配置文件。
- `cargo run -- config init`：生成默认 `config.toml`；目标文件已存在时拒绝覆盖。
- `cargo run -- config check`：只校验配置和内核快照，不执行任务；未传 `--config` 时会自动读取当前目录 `config.toml`（如果存在）。
- `cargo run -- config show --json`：输出脱敏后的配置摘要，给桌面控制台或插件读取。
- `cargo run -- control ...`：项目根配置走 command-backed 安全示例 adapter；默认空配置仍可走 fake 控制面并在状态里标为占位。
- `config.example-control.toml` + `scripts/chuang-control-adapter-example.sh`：安全 command 控制面示例，只验证协议，不控制真实服务。
- `actuator = "command"` + `scripts/chuang-actuator-adapter-example.sh`：安全 command 操作面示例，只验证协议，不控制真实桌面。
- `cargo run -- subagent dispatch --task TEXT [--requires-capability NAME]`：把子代理任务写入文件队列 dispatch JSON，可声明所需 worker 能力，不启动外部 runner。
- `cargo run -- subagent report --run-id ID`：只读轮询子代理 report JSON。
- `cargo run -- subagent collect --run-id ID`：从 dispatch 恢复运行身份，经 queued slot 校验并回收 report。
- `cargo run -- subagent list`：只读查看 dispatch 队列、claim/stale claim 和 report presence；同一队列目录可连续派发多个任务。
- `cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec`：显式审批后执行一个外部 runner 命令，把 dispatch JSON 写到 stdin，并把进程输出收成 report。
- `cargo run -- subagent run-loop --runner command --runner-command PATH --approve-exec --max-runs N [--capability NAME]`：按队列批量处理匹配 worker 能力的 pending dispatch，并保留 claim/report 证据；超出 dispatch `idle_timeout_ms` 的 stale claim 可被重领。
- `--context-max-tokens / --context-reserve-system-tokens / --context-min-working-tokens / --context-max-tool-results / --context-max-memory-segments`：可从 CLI 调整 context budget。
- `ContextEngine` trait + `deterministic_budget` 默认实现：上下文策略已具备可替换接口。
- `summary_compression` 非默认轻量压缩策略：会对长 memory / tool result 段做本地截断压缩，再交给同一预算 packer，用于验证配置切换面。
- `GenesisActuator` trait + `AutoCliGenesisActuator` 最小实现：主通道 userDataDir，备用 CDP，登录态失效时 fallback，并返回需审批的修复计划，不自动删除 profile。
- `cargo run -- genesis ask --prompt TEXT --approve-exec`：手动验证 Genesis 查询入口；真实外部程序执行必须显式审批，并输出治理决策与审计状态。
- `cargo run -- genesis ask --prompt TEXT --dry-run`：只渲染 Genesis 主/备通道 AutoCLI 命令规格，不执行外部程序。
- `cargo run -- external-ai dispatch --platform NAME --task TEXT --context TEXT --dry-run [--session-hint ID] [--timeout-ms N]`：本地生成统一身份引擎 dispatch 请求、审计 id 和结构化结果；不连接外部平台、不写记忆、不接真实浏览器/HTTP adapter。
- `cargo run -- experiment plan --goal TEXT --success TEXT`：生成固定时间预算的安全实验计划，只写 `experiment.md`，不执行、不删除、不回滚。
- `cargo run -- experiment complete --experiment-id ID --outcome success|failure|inconclusive --summary TEXT --next TEXT`：追加实验结果 `report.md`，已存在时拒绝覆盖。
- `cargo run -- experiment list`：只读列出实验状态，显示是否已有计划和报告。
- `cargo run -- experiment show --experiment-id ID`：只读查看某个实验的计划和报告内容，不修改文件。
- `sh scripts/chuang-mvp-smoke.sh`：安全端到端验收脚本，使用临时目录、stub provider 和示例 command control，不触碰真实服务。
- `sh scripts/chuang-second-test-smoke.sh`：第二测试版本验收入口，复用同一安全 smoke，并输出 `second_test_smoke_ok`。

## 当前明确不做

- 不自动删除任何记忆。
- 不自动压缩身份记忆。
- 不把长期记忆误写成只有三层；wiki/GBrain 外脑和 LIM 长期沉淀是正式目标层，只是 MVP 阶段先不硬搬完整 GBrain/PGLite 内核。
- 默认不直接操作真实 systemd 服务；真实控制必须通过显式配置的 adapter/command bridge，并经过治理审批。
- 不直接控制 Hermes / OpenClaw 进程。
- 不继续扩展旧 `BrowserWorker` 实验线；网页版 AI 查询能力后续改走 `Genesis Actuator` 插件线。
- 不把飞书作为核心依赖，飞书只作为未来插件入口。
- 不在核心层硬编码任何密钥、飞书凭证、Hermes 凭证或本机私有 token。
- 不把 API key 明文写进配置文件；真实 provider 使用 `api_key_env` 引用环境变量。
- 不把 AutoResearch 的 `git reset --hard` 模式照搬进创项目；实验模块只能追加计划/报告，不能破坏主工作区。
- 不把 goal mode 当成常驻后台执行器；当前它只是结构化目标上下文，加上可恢复的计划/checkpoint 记录。
- 不把插件注册表、GA interface-only 原子工具或安全示例 adapter 宣称为真实外部能力。

## 下一步优先级

1. 继续完善真实子代理 runner adapter 的协议约束；当前已有显式审批的 `command` runner 接缝，项目根默认使用 queued external，不自动执行危险命令。
2. 继续增强 `summary_compression` 的压缩质量，但默认仍保持 `deterministic_budget`。
3. 继续增强 command control plane / command actuator 的真实脚本协议；真实脚本必须先有 allowlist 和验收。
4. 新开 `Genesis Actuator` 插件线，先做可审计的 AutoCLI 查询 port；旧 `BrowserWorker` 先冻结。
5. 最后再接桌面壳、飞书插件和服务控制 UI。

## 判定 MVP 可用的最低标准

- `cargo test` 全仓通过。
- `cargo run -- status` 能显示核心状态。
- `cargo run -- doctor` 能确认配置、slot、actuator smoke、control smoke 和隔离 runtime smoke 正常。
- `cargo run -- run --input TEXT` 能返回结构化响应。
- `cargo run -- status --config PATH` 能加载简单配置文件，且 CLI 参数可覆盖配置。
- `cargo run -- status --config PATH` 能显示 identity bootstrap 文件路径和字符数。
- `cargo run -- run --config PATH --input TEXT` 的治理元数据能显示当前规则指纹。
- `app-server` 通过 workspace `config.toml` 启动时不能回落到 `fake-responder`，必须走配置里的 provider。
- `channel simulate` 通过 workspace `config.toml` 启动时不能回落到 `fake-responder`，必须走配置里的 provider。
- `cargo run -- run --input TEXT --remember` 能写回记忆，并在下一轮被 recall。
- `cargo run -- memory session search --query TEXT --session-id ID` 能只读检索指定会话，不跨 session 返回。
- `cargo run -- memory lim extract --query TEXT --session-id ID` 能生成 dry-run 候选，不修改 `experiences.md`。
- `cargo run -- memory maintenance report --query TEXT --session-id ID` 能生成 dry-run 维护报告，不修改 `MEMORY.md` 或 `experiences.md`。
- `cargo run -- run --input TEXT --remember-identity --identity-memory-root PATH` 能显式追加身份热记忆。
- `cargo run -- run --input TEXT --remember-experience --identity-memory-root PATH` 能显式追加带 provenance 的经验层记忆。
- `cargo run -- subagent dispatch --task TEXT --subagent-queue-root PATH` 能生成 dispatch JSON。
- 同一个 `--subagent-queue-root PATH` 下连续 dispatch 多个任务不会覆盖。
- `cargo run -- subagent list --subagent-queue-root PATH` 能列出 dispatch 数量和 report presence。
- `cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec --subagent-queue-root PATH` 能把外部 runner 进程输出转成 report，且缺少审批时拒绝执行。
- `cargo run -- subagent report --run-id ID --subagent-queue-root PATH` 能读取或轮询 report JSON。
- `cargo run -- subagent collect --run-id ID --subagent-queue-root PATH` 能经 dispatch 身份校验回收 report。
- `cargo run -- experiment plan --goal TEXT --success TEXT --root PATH` 能生成带安全约束的 `experiment.md`。
- `cargo run -- experiment complete --experiment-id ID --outcome inconclusive --summary TEXT --next TEXT --root PATH` 能生成不可覆盖的 `report.md`。
- `cargo run -- experiment list --root PATH` 能只读列出 planned/completed 状态。
- `cargo run -- experiment show --experiment-id ID --root PATH` 能只读展示 `experiment.md` 和可选 `report.md`。
- `sh scripts/chuang-mvp-smoke.sh` 能串起 status JSON、doctor JSON、goal runtime context、session memory diagnostics、channel simulate goal input、subagent queue、command control example 和 experiment show。
- `sh scripts/chuang-mvp-smoke.sh` 能断言 status/doctor 的 readiness 字段：GA mapped/interface-only 工具名单、identity bootstrap presence、provider request timeout、goal_run readiness、plugin registry 和 placeholder warning。
- `sh scripts/chuang-mvp-smoke.sh` 能断言项目级 readiness：主链和 execution tools 必须 ready，channel 和 external AI 可保持 partial/deferred，但不能被误报成已完成真实能力。
- `sh scripts/chuang-mvp-smoke.sh` 能断言通道 readiness：Chuang 专用桥保持 partial，Codex/Hermes 隔离保持 ready，rich messages 保持 deferred。
- `sh scripts/chuang-mvp-smoke.sh` 能断言子代理 readiness：dispatch/report 维持 ready，command runner 维持显式审批态，multi-worker 和 external AI 保持 partial。
- `sh scripts/chuang-mvp-smoke.sh` 能断言子代理 readiness：dispatch/report 维持协议态，command runner 维持显式审批态，multi-worker 和 external AI 保持 partial。
- `sh scripts/chuang-mvp-smoke.sh` 能验证 `GoalRun` plan/checkpoint/show 的 checkpoint-first 记录闭环，但不期待它自动执行任务。
- `sh scripts/chuang-second-test-smoke.sh` 能复用完整 smoke 并固定第二测试版本输出标记，方便后续把第二版验收和旧 MVP 入口区分开。
- 所有危险操作仍需显式审批或保持 fake。
- command 控制面示例配置能 `list` 和 `apply --approve` 跑通，但不会触碰真实服务。
- 对话 provider、子代理、actuator、control plane 的项目根配置已去 fake；安全示例 adapter 不能伪装成真实桌面/服务控制。
