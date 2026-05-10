# 创项目当前情况介绍

日期：2026-05-02

## 一句话结论

创项目已经从“设计蓝图”推进到“最小可运行内核 MVP”：核心主链能启动、能读取身份和记忆、能调用真实 OpenAI-compatible provider、能写会话记忆、能通过文件队列派发子代理任务，并且已经补上第一版身份启动层。

当前还没有正式接入新的飞书机器人。后续飞书应作为独立插件接入，不能复用 Hermes 或 Codex 现有飞书通道。

## 当前核心链路

```text
input
-> identity / memory
-> context
-> governance
-> execution slot
-> report
-> memory writeback
```

核心只保留稳定语义：

- Identity：创是谁，启动时读哪些身份文件
- Memory：普通会话记忆、身份热记忆、后续多层长期记忆
- Context：把身份、记忆、输入和工具结果打包进模型上下文
- Governance：执行前做风险判断和审计
- Report：每轮运行和子代理结果都要可审计

provider、飞书、桌面控制、浏览器控制、服务控制、子代理 runner 都走 adapter / plugin，不写死进核心。

## 当前已经具备的能力

### 1. Provider 已经去 fake

当前项目根配置使用真实 OpenAI-compatible provider：

```text
provider: openai_compatible
provider_id: local-openai-compatible
model: gpt-5.5
transport: native
```

密钥通过环境变量引用，不写进配置文件。

### 2. 身份启动层已落地

新增了 `identity/` 目录：

```text
identity/
  SOUL.md
  STORY.md
  FIRST_WAKE.md
  agents.toml
```

含义：

- `SOUL.md`：创的核心身份锚点
- `STORY.md`：创的来历、项目背景、和其他 Agent 的关系
- `FIRST_WAKE.md`：每次启动时优先读取的启动规则
- `agents.toml`：小创、小承、小云、创等身份边界注册表

启动时这些文件会作为冻结 identity snapshot 注入上下文。当前状态已能显示：

```text
identity_bootstrap_chars: soul=296 story=377 first_wake=480 agents=942
```

### 3. Hermes 风格双文件记忆已接入

当前身份热记忆路径：

```text
data/hermes-memory/
  USER.md
  MEMORY.md
```

当前文件存在，但内容还是空的。它们用于后续沉淀用户画像和热记忆。

默认限制：

```text
USER.md   1375 chars
MEMORY.md 2200 chars
```

超限时应拒绝写入，让模型自己决定保留和压缩，不自动吞掉信息。

### 4. 会话记忆 MVP 已有

普通会话记忆使用 SQLite：

```text
data/chuang-agent.db
```

CLI 已支持：

```bash
cargo run -- run --input TEXT --remember
cargo run -- run --session-id ID --remember-session
```

`app-server` 当前也会用 thread id 做 session id 写会话记忆，但它还没有正式接飞书。

### 5. 子代理文件队列已可用

当前配置：

```text
subagent: queued_external
subagent_queue_root: ./data/subagent-queue
```

已支持：

```bash
cargo run -- subagent dispatch --task TEXT
cargo run -- subagent list
cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec
cargo run -- subagent run-loop --runner command --runner-command PATH --approve-exec --max-runs N
cargo run -- subagent report --run-id ID
cargo run -- subagent collect --run-id ID
```

子代理 runner 可以返回完整 `SubagentReport` JSON。主控会校验 task、agent、parent 身份，身份不一致不会误收。

### 6. 配置和状态可见性增强

`status / doctor / config check / config show` 会明确显示占位模块。当前项目根已经切到 command actuator 和 command control 的安全示例 adapter，实测应为 `placeholder_warnings: none`。

注意：这表示协议和装配已经正式化，不表示已经能真实控制桌面或系统服务。当前示例 adapter 只返回确定性 JSON，不触碰真实桌面、浏览器或服务进程。

## 当前主要配置文件

### config.toml

项目实际运行配置。

核心内容：

```toml
db_path = "./data/chuang-agent.db"
recall_limit = 5
identity_memory_root = "./data/hermes-memory"

identity_root = "./identity"
soul_path = "./identity/SOUL.md"
story_path = "./identity/STORY.md"
first_wake_path = "./identity/FIRST_WAKE.md"
agents_registry_path = "./identity/agents.toml"

provider = "openai_compatible"
provider_id = "local-openai-compatible"
base_url = "http://127.0.0.1:8317/v1"
model = "gpt-5.5"
api_key_env = "CODEX_LIUSU_API_KEY"
transport = "native"

subagent = "queued_external"
subagent_queue_root = "./data/subagent-queue"

actuator = "command"
actuator_program = "sh"
actuator_args = "./scripts/chuang-actuator-adapter-example.sh --json"
actuator_timeout_ms = 30000

control = "command"
program = "sh"
list_args = "./scripts/chuang-control-adapter-example.sh list --json"
apply_args = "./scripts/chuang-control-adapter-example.sh apply --json"
control_timeout_ms = 30000

context_engine = "deterministic_budget"
context_max_tokens = 272000
context_reserve_system_tokens = 32
context_min_working_tokens = 1
context_max_tool_results = 5
context_max_memory_segments = 5
```

### config.example.toml

配置模板。给新环境复制使用，不放真实密钥。

### AGENTS.md

项目级工作规则，定义工程约束、风险规则、记忆规则和进度记录规则。

### docs/handoff-current.md

给 `/new` 后接续用的当前交接文件。

### docs/progress-log.md

长期进度日志。每次有架构或实现变化都应更新。

## 当前文件结构

```text
chuang-agent/
  AGENTS.md
  README.md
  Cargo.toml
  Cargo.lock
  config.toml
  config.example.toml

  identity/
    SOUL.md
    STORY.md
    FIRST_WAKE.md
    agents.toml

  data/
    chuang-agent.db
    hermes-memory/
      USER.md
      MEMORY.md

  src/
    main.rs
    lib.rs
    runtime_config.rs
    runtime_config_file.rs
    chuang_kernel.rs
    agent_runtime.rs
    cli_runtime.rs
    cli_args.rs
    cli_output.rs
    cli_config.rs
    cli_control.rs
    cli_subagent.rs
    cli_genesis.rs
    cli_doctor.rs
    slot_registry.rs
    provider_openai_compatible.rs
    context_engine.rs
    governance.rs
    hermes_memory.rs
    memory_store.rs
    memory_store_sqlite.rs
    memory_recall.rs
    memory_admission.rs
    subagent_spawner.rs
    subagent_queue.rs
    control_plane.rs
    control_workflow.rs
    genesis_actuator.rs
    actuator.rs
    skill_evolver.rs

  docs/
    core-boundary.md
    mvp-scope.md
    mvp-checkpoint-2026-05-01.md
    progress-log.md
    handoff-current.md
    blueprint-v1.md
    pluggable-architecture-v1.md
    source-project-audit-v1.md
    control-command-protocol.md

  tests/
    *_tests.rs

  scripts/
    chuang-codex.sh
    launch-chuang-agent-repl.sh
```

## 模块说明

### src/main.rs

薄入口，只做顶层命令分发。

### src/chuang_kernel.rs

核心主链。负责一轮 turn 的运行、身份上下文注入、治理、报告和记忆写回入口。

### src/agent_runtime.rs

模型运行时，把 prompt、recall、context pack 和 provider 输出串起来。

### src/runtime_config.rs

配置结构定义和校验。这里只描述配置，不构造具体 provider adapter。

### src/runtime_config_file.rs

简单 TOML 解析。当前刻意保持格式简单，方便维护。

### src/slot_registry.rs

把配置转换成可运行 slot。具体 provider、control、subagent adapter 在这里装配，避免污染核心。

### src/provider_openai_compatible.rs

OpenAI-compatible provider adapter。支持 `stub/http/native/curl`，当前默认走 `native`。

### src/subagent_queue.rs / src/subagent_spawner.rs

子代理协议和文件队列。支持 dispatch、report、claim、release。

### src/genesis_actuator.rs

新的网页版 AI 查询插件线。旧 `BrowserWorker` 暂停推进，后续网页 AI 搜索走 Genesis。

### src/control_plane.rs / src/control_workflow.rs

服务和 Agent 控制面协议。当前项目根配置使用 command 安全示例 adapter，真实服务控制仍需要单独 allowlist 脚本。

## 当前缺口

### 1. 新飞书机器人未接入

这是下一阶段重点，但必须独立接入：

- 不复用 Hermes 飞书机器人
- 不复用 Codex 当前飞书桥
- 新建 Chuang 专属机器人和 session 绑定
- app-server 或插件入口要接到当前真实 provider 会话链路

### 2. 控制面还没接真实服务脚本

当前 `control = "command"` 使用的是安全示例 adapter。后续要替换成真实 allowlist 脚本：

```toml
control = "command"
program = "/path/to/chuang-control-adapter"
list_args = "list --json"
apply_args = "apply --json"
control_timeout_ms = 30000
```

真实控制必须保留治理审批和审计记录。

### 3. Actuator 还没接真实桌面脚本

当前 `actuator=command` 使用的是安全示例 adapter。后续桌面、浏览器、微信、验证码输入等能力都应该通过 actuator/plugin 接入。

### 4. 长期记忆还只是第一层

当前已有：

- `identity/` 启动身份层
- `data/hermes-memory/USER.md`
- `data/hermes-memory/MEMORY.md`
- SQLite 会话记忆

后续还需要补：

- RULES.md
- EXPERIENCE.md
- session archive
- knowledge base
- memory maintenance / health / decay / extractor

### 5. 子代理 runner 还需要增强

当前文件队列和 command runner 已通，但还需要：

- worker 能力声明
- 并发上限
- claim 自动过期
- runner 心跳
- 失败重试策略
- 多模型子代理配置

### 6. Provider fallback 还没做

当前 provider 已有错误分类：

- `provider_retryable`
- `provider_error_class`
- `provider_timeout_ms`

下一步应该基于这些字段做：

- 自动重试
- fallback provider
- 可诊断失败报告

## 后续规划

### 第一阶段：让创稳定说话

目标：新的飞书机器人能和创正常对话，且不会再出现 fake responder 文案。

任务：

1. 新建 Chuang 专属飞书机器人。
2. 新建独立 Feishu bridge/plugin，不碰 Hermes 和 Codex 现有通道。
3. 将飞书输入接到 `app-server` 或独立 channel adapter。
4. 每个 thread 使用独立 `session_id`。
5. 默认写 `--remember-session`。
6. 输出里保留 provider、model、governance、memory 状态，便于诊断。

验收：

```text
飞书消息 -> 创 app-server -> provider gpt-5.5 -> 回复 -> session memory 写入
```

### 第二阶段：让创稳定记住

目标：从“能写会话记忆”升级到“有分层长期记忆”。

任务：

1. 扩展 `identity/` 和 `data/hermes-memory/` 的职责边界。
2. 增加 RULES / EXPERIENCE / STORY 的正式配置。
3. 写入时区分 session、hot memory、identity memory、experience。
4. 超限时返回当前条目，让模型自主压缩。
5. 增加记忆审计和来源字段。

### 第三阶段：真实子代理 runner

目标：创能派出多个真实子代理处理复杂任务。

任务：

1. 做 `chuang-codex-runner` 或类似 runner。
2. runner 读取 dispatch JSON。
3. runner 输出标准 `SubagentReport`。
4. 支持模型配置、工具策略、超时、最大输出。
5. 支持并行 worker 和 claim 过期。

### 第四阶段：真实控制面

目标：通过控制台或飞书管理本机服务和 Agent。

任务：

1. 接 command-backed control adapter。
2. 管理服务 start / stop / restart。
3. 管理 Agent start / stop / restart / change-model。
4. 所有危险操作必须治理审批。
5. 状态展示给桌面控制台或飞书。

### 第五阶段：Genesis / 桌面 / 浏览器能力

目标：让创具备接近人类的本机操作能力。

任务：

1. Genesis Actuator 接 AutoCLI。
2. 主通道 userDataDir。
3. 备用通道 CDP 真人浏览器。
4. 登录态失效检测和可审计修复计划。
5. 后续扩展桌面、浏览器、微信、验证码输入等能力。

原则：

- 不自动删除 profile。
- 不绕过验证码。
- 验证码只能在老爸提供或批准时输入。
- 真实外发消息、账号操作、支付、订单等必须审批。

### 第六阶段：技能进化

目标：从执行任务升级到沉淀技能。

任务：

1. 观察任务执行。
2. 提炼可复用 SOP。
3. 固化为 skill。
4. 允许老爸审核和编辑。
5. 逐步形成创自己的工具和经验库。

## 当前风险和注意事项

1. 不要把飞书接到现有 Codex 或 Hermes 通道。
2. 不要把安全 command control 示例当成真实控制面。
3. 不要把安全 command actuator 示例当成真实桌面操作。
4. 不要把密钥写入 `config.toml`、文档、日志或回复。
5. 不要删除队列、claim、report、记忆文件，除非老爸明确批准具体目标。
6. BrowserWorker 旧线先冻结，不要继续沿旧线扩展。
7. 复杂能力必须走插件槽位，不要反向污染核心主链。

## 下一步建议

最推荐的下一步：

1. 接新的 Chuang 飞书机器人。
2. 确认飞书对话走真实 provider。
3. 确认 session memory 能按 thread 写入和 recall。
4. 然后做真实子代理 runner。
5. 再做真实 control command adapter。

短期不要优先做大而全的桌面控制。先保证创能稳定说话、稳定记住、稳定派发子代理。
