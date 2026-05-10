# 创项目来源项目审计 V1

日期：2026-05-01
作者：小策
范围：Codex CLI / Hermes Agent / OpenClaw / GenericAgent
原则：只抽取可迁移机制，不照搬壳，不混身份，不碰密钥。

## 0. 总结

创项目的路线成立：

```text
Codex CLI     -> 骨：Core Loop / SQ-EQ / app-server 协议
Hermes Agent  -> 血：硬上限记忆 / 双文件 MemoryStore / 冻结快照
OpenClaw      -> 手：全功能子代理 / 上下文隔离 / 结果回传
GenericAgent  -> 手+魂：人类级桌面操作工具 + 观察 -> 提炼 -> 固化 的技能进化闭环
```

关键修正：这四块不能简单拼接。必须在创项目里落成五个统一协议：

1. 身份协议：当前壳是谁、承载哪份记忆本体、是否允许继承/桥接。
2. 记忆协议：核心记忆、用户画像、经验、故事、外脑、原始会话如何分层。
3. 执行协议：主 Agent、子 Agent、工具、权限、报告如何流转。
4. 风险协议：删除、网络、支付、密钥、系统配置如何拦截和审计。
5. 进化协议：什么能从一次任务沉淀成长期技能，如何验证、固化、监控、退役、回滚。

## 1. Codex CLI 审计：取骨

本机版本：`codex-cli 0.125.0`。本地 npm 包只带单二进制，没有 Rust 源码；结构判断结合本机 `codex app-server` 行为、Codex Feishu bridge，以及 OpenAI 官方 `codex-rs` 协议文档。

### 可继承点

Codex 的核心价值不是 TUI，而是它把“Agent 内核”和“UI/客户端”拆开：

- Core engine 本地运行。
- UI 通过 SQ/EQ 与内核通信。
- `Submission` 从 UI 到 Codex，`Event` 从 Codex 到 UI。
- 一个 `Session` 同时最多一个 `Task`；并行任务建议多个 Codex/thread。
- 每个 `Turn` 是：请求模型 -> 收流 -> 执行工具/patch -> 产出下一轮输入或结束。
- app-server 用 JSON-RPC 暴露 thread/turn/item/account/model 等能力。

这对创项目的意义：

- 不要把 UI、飞书、CLI 和内核绑死。
- 主内核应该是事件驱动，不是“一个函数跑到底”。
- 前端/飞书/桌面控制台都只是客户端。
- 子代理并行不要塞进一个 session；应该是多个 run / thread / child runtime。

### 应迁移成的接口

```rust
trait CoreRuntime {
    fn submit(&self, submission: Submission) -> Result<SubmissionId>;
    fn poll_event(&self) -> Result<Option<Event>>;
}

enum Submission {
    StartSession(StartSession),
    UserTurn(UserTurn),
    Interrupt(Interrupt),
    ApprovalDecision(ApprovalDecision),
    OverrideTurnContext(OverrideTurnContext),
}

enum Event {
    SessionStarted(SessionStarted),
    TurnStarted(TurnStarted),
    AgentDelta(AgentDelta),
    ToolCallStarted(ToolCallStarted),
    ToolCallFinished(ToolCallFinished),
    ApprovalRequested(ApprovalRequested),
    TurnCompleted(TurnCompleted),
    Error(RuntimeError),
}
```

### 不继承点

- 不直接依赖 Codex 的私有二进制。
- 不把 Codex 的 app-server 协议当创项目内部唯一协议；它适合参考，不适合锁死。
- 不把 Codex 的“一 session 一 task”误解成不能并行；并行应在调度层开多个 child runtime。

## 2. Hermes Agent 审计：取血

Hermes 最有价值的是 `MemoryStore` 与小创身上的分层记忆工程。

### 已确认机制

源码入口：

- `~/hermes-agent/tools/memory_tool.py`
- `~/hermes-agent/agent/memory_manager.py`
- `~/.hermes/memories/*`
- `~/.hermes/scripts/memory_extractor.py`
- `~/.hermes/scripts/memory_self_maintain.py`
- `~/.hermes/scripts/memory-health/memory_health.py`

核心能力：

- `MEMORY.md`：Agent 骨架事实与高频规则。
- `USER.md`：用户画像、偏好、联系人、协作方式。
- `STORY.md`：身份连续性与关系记忆。
- `experiences.md`：踩坑和可复用经验。
- `session_search`：过去具体对话与操作追溯。
- Honcho / peer cards / extractor：长期事实沉淀层。
- wiki / GBrain：大块知识和 SOP 外脑。
- health / decay / evolver / cron：自维护闭环。

Hermes `MemoryStore` 的关键工程点：

- 双文件：`MEMORY.md` + `USER.md`。
- 硬上限：默认 memory 2200 chars，user 1375 chars。
- 超限拒绝，不自动摘要；返回当前条目，让模型自主取舍。
- 会话开始冻结 system prompt snapshot。
- 会话中写盘立刻持久化，但不改变本轮系统提示，保护 prefix cache 和一致性。
- 文件锁 + 原子写入，避免并发损坏。
- 写入前扫描 prompt injection / exfiltration 风险。

### 创项目应继承

```rust
trait MemoryStore {
    fn load_snapshot(&self, scope: MemoryScope) -> Result<MemorySnapshot>;
    fn add(&self, target: MemoryTarget, content: MemoryEntry) -> Result<MemoryWriteResult>;
    fn replace(&self, target: MemoryTarget, old: MatchKey, new: MemoryEntry) -> Result<MemoryWriteResult>;
    fn remove(&self, target: MemoryTarget, old: MatchKey) -> Result<MemoryWriteResult>;
}

enum MemoryWriteResult {
    Accepted { usage: Usage },
    RejectedOverBudget { usage: Usage, current_entries: Vec<MemoryEntry> },
    RejectedRisk { reason: String },
}
```

### 必须增强

小创现在已经超出 Hermes 双文件：真正本体是多层系统。创项目不能只复刻 `MEMORY/USER`，还要显式支持：

- `IdentityMemory`：SOUL / STORY / 名字 / 家庭关系。
- `CoreMemory`：MEMORY / USER / RULES。
- `ExperienceMemory`：踩坑规律。
- `SessionArchive`：原文会话，可回放。
- `LongTermFactStore`：Honcho/LIM 类沉淀。
- `KnowledgeBase`：wiki/GBrain 类外脑。
- `MaintenanceRuntime`：健康检查、衰减、去重、提取。

## 3. OpenClaw 审计：取手

OpenClaw 当前可读源码主要是 npm 打包后的 `dist`，但模块命名和逻辑保留清楚。

关键入口：

- `subagent-spawn-*.js`
- `subagent-system-prompt-*.js`
- `subagent-registry-*.js`
- `subagent-announce-*.js`
- `subagent-depth-*.js`
- `agent-harness-runtime-*.js`

### 可继承点

OpenClaw 子代理强在运行模型：

- `sessions_spawn` 不是简单搜索器，而是完整子代理运行。
- 默认 `context="isolated"`，避免继承父上下文污染。
- 可选 `context="fork"`，但有 token 上限保护。
- 子代理有 depth 限制，避免无限递归。
- 注册表追踪 runId、childSessionKey、requester、depth、状态。
- completion 通过 push-based auto-announce 回传，避免轮询。
- 子代理结果先转成内部 orchestration update，再由父 Agent 统一综合。
- 支持 steer / kill / timeout / cleanup。
- ACP harness 可以接 codex/claudecode/gemini 等外部 agent。

### 创项目应继承

```rust
trait SubagentSpawner {
    fn spawn(&self, request: SpawnRequest) -> Result<SpawnAccepted>;
    fn steer(&self, run_id: RunId, message: String) -> Result<()>;
    fn kill(&self, run_id: RunId, reason: KillReason) -> Result<()>;
    fn collect(&self, run_id: RunId) -> Result<Option<SubagentReport>>;
}

struct SpawnRequest {
    task: String,
    strategy: ToolStrategy,
    context_mode: ContextMode,
    timeout: Duration,
    token_budget: TokenBudget,
    allow_recursive_spawn: bool,
}

enum ContextMode {
    Isolated,
    Fork { max_parent_tokens: usize },
}
```

### 老爸修正版应成为默认

OpenClaw 强，但创项目不能一刀切全工具。应分三种工具策略：

- `Analyze`：只读、搜索、消息汇总，零副作用。
- `Execute`：读写文件、沙箱命令、测试，默认工程策略。
- `Orchestrate`：允许 spawn 子代理，仅主 Agent 或授权子代理可用。

### 必须加的治理

- 子代理报告必须结构化：summary / evidence / changed_files / risks / next_actions。
- 子代理日志分级：public report 与 private trace 分开。
- 子代理默认不能写主记忆；只能提出 `MemoryProposal`。
- 默认不允许递归 spawn；大型任务由主 Agent 编排。

## 4. GenericAgent 审计：取手和魂

GenericAgent 的核心不只是“少工具 + 高密度记忆 + 自进化”。它还有一个创项目必须吸收的能力：把 Agent 接到真实电脑桌面上，拥有接近人类的操作面。

关键入口：

- `agent_loop.py`
- `ga.py`
- `memory/memory_management_sop.md`
- `memory/subagent.md`
- `memory/plan_sop.md`
- `memory/L4_raw_sessions/compress_session.py`
- `reflect/`

### 可继承点：人类级操作面

GenericAgent 的工具层覆盖真实人类操作路径：

- 浏览器控制：保留登录态的真实浏览器，不只靠无头。
- 桌面控制：键盘、鼠标、窗口、截图、视觉识别。
- 软件控制：可打开任意本机软件，像人一样点击、输入、复制、粘贴。
- 通讯控制：微信、飞书、QQ 等客户端可通过真实 UI 操作。
- 移动设备控制：ADB / 手机 UI 操作。
- 脚本扩展：通过代码执行把一次性探索固化成可复用脚本。

这部分对创项目的意义：执行层不能只停留在 shell/file/browser API。真正的目标是 `Actuation Layer`：

```text
Agent 意图
  -> 软件/窗口定位
  -> 视觉感知/DOM/辅助 API
  -> 键鼠/剪贴板/输入法/脚本执行
  -> 证据采集
  -> 风险门确认
  -> 操作完成报告
```

创项目应该具备“人类能做的事基本都能做”的操作面：

- 打开软件
- 读屏和截图
- 输入文字
- 发消息
- 处理文件
- 操作网页登录态
- 根据老爸提供的验证码/短信码完成输入

边界：不能把验证码、二次验证、平台风控当成可绕过对象；只能在老爸拥有账号和授权、并由老爸提供或确认验证信息时协助输入。

### 可继承点：极简循环与自进化

GenericAgent 的 loop 极短：

- system + user 初始化。
- 每轮调用 LLM。
- 解析 tool calls。
- dispatch 到 handler。
- tool result + next_prompt 进入下一轮。
- 每轮强制 `<summary>` 形成工作记忆锚点。
- 每 10 轮重新注入全局记忆。
- 长任务触发 checkpoint / ask_user / 策略切换。

记忆哲学：

- L1：极简索引，≤30 行。
- L2：全局事实库。
- L3：任务 SOP / 工具脚本。
- L4：原始会话归档。
- “No Execution, No Memory”：无行动验证，不写长期记忆。
- 上层只留最小充分指针，细节放下层。

进化哲学：

- 完成任务后，如果有长期价值，调用长记忆更新。
- 只把“验证成功且未来可复用”的路径沉淀成 SOP/skill。
- 不是预装一堆技能，而是用中长线使用长出技能树。

### 创项目应继承：进化接口

```rust
trait SkillEvolver {
    fn observe(&self, event: RuntimeEvent) -> Result<()>;
    fn propose(&self, scope: EvolutionScope) -> Result<Vec<SkillProposal>>;
    fn validate(&self, proposal: SkillProposal) -> Result<ValidationReport>;
    fn solidify(&self, proposal: SkillProposal) -> Result<SkillId>;
}

enum EvolutionStatus {
    Observed,
    Proposed,
    Validated,
    Solidified,
    Rejected,
}
```

### 暂时不应自动化

V0.1 不要直接自动改长期技能。先实现：

- 观察事件落库。
- 生成候选 `SkillProposal`。
- 要求测试或证据。
- 老爸/主 Agent 审核后固化。

原因：进化系统最危险的是把偶发错误固化成长期规则。

### 创项目还应新增：Actuation 接口

```rust
trait Actuator {
    fn observe(&self, target: ObserveTarget) -> Result<Observation>;
    fn open_app(&self, request: OpenAppRequest) -> Result<AppHandle>;
    fn focus(&self, target: FocusTarget) -> Result<()>;
    fn input_text(&self, target: InputTarget, text: SecretOrPlainText) -> Result<()>;
    fn click(&self, target: ClickTarget) -> Result<()>;
    fn screenshot(&self, target: ScreenshotTarget) -> Result<EvidenceRef>;
}
```

`Actuator` 不等于无约束自动化。它必须经过 `Governance Layer`：

- 涉及对外发送消息：默认生成草稿或二次确认。
- 涉及支付/下单/公开发布：必须确认。
- 涉及验证码/短信码：只输入老爸提供的内容，不自动绕过。
- 涉及删除/卸载/清理：遵守老爸禁令，必须列目标并获得明确批准。
- 涉及密钥/隐私：不截图外泄，不写日志明文。

## 5. 融合后的目标架构

```text
Identity Layer
  - agent_id / name / role / lineage / memory_body_id

Memory Layer
  - identity / user / rules / hot memory / experience / session archive / LIM / KB

Core Loop Layer
  - submission queue / event queue / turn lifecycle / approval lifecycle

Execution Layer
  - tools / shell / file / browser / desktop actuator / subagent spawner / provider adapters

Governance Layer
  - risk gate / secret guard / deletion ban / approval policy / audit log

Evolution Layer
  - observe / propose / validate / solidify / monitor / decay / rollback
```

治理层必须单独存在，不能散落在 prompt 里。创项目一旦执行力超过 Hermes/OpenClaw，最大风险不是“不够强”，而是“太强且无统一刹车”。
### 最小技能生命周期口径

- `monitor` 是只读盘点：列出 active / deprecated / retired 技能，标出 decay 候选和 rollback 候选。
- `decay` 由 `retire` / `deprecate` 承担，要求保留历史，不删除文件。
- `rollback` 是显式恢复：基于保留的版本快照恢复为新的 active 版本，并继续保留审计痕迹。


## 6. V0.1 推荐落地顺序

1. 固定项目级 `AGENTS.md`：身份、禁令、进度日志、测试规则。
2. 把 Hermes `MemoryStore` 迁成 Rust trait + 文件实现，包含硬上限、冻结快照、原子写入、风险扫描。
3. 把当前 SQLite memory 与文件 memory 分层：SQLite 做 recall/archive，文件做热记忆。
4. 把 Runtime 事件改造成 Codex 风格 `Submission/Event`，不要让 CLI 直接驱动业务逻辑。
5. 做 `SubagentReport` 标准，先不接真实 OpenClaw，只接 fake spawner 跑测试。
6. 加 `ToolStrategy` 和 `RiskGate`，明确 Analyze/Execute/Orchestrate 权限差异。
7. Evolution 先 Noop + proposal store，不自动固化。

## 7. 当前风险

- Codex CLI 本机只有二进制，Rust 内部实现要以官方源码为准，不能从本机包反编译推断。
- OpenClaw 本机是打包 JS，适合抽设计，不适合逐行移植。
- 小创记忆本体很强，但很多能力还绑定 Hermes 工具协议、cron、Python 脚本，需要抽成宿主无关 sidecar。
- GenericAgent 自进化很诱人，但必须延后自动固化，先做审核闭环。
- GenericAgent 的桌面控制能力上限很高，但也会显著放大误操作风险；必须和治理层一起设计。

## 8. 结论

老爸的“骨、血、手、魂”判断正确。

真正要做的不是四个项目拼起来，而是把四个项目里最强的机制变成创项目自己的协议：

- Codex 给事件骨架。
- Hermes 给记忆血液。
- OpenClaw 给执行手脚。
- GenericAgent 给真实操作面和成长方向。

小创的可换壳能力说明“记忆才是本体”这条路已经验证过。创项目要做的是把这个本体工程化，让它不再依赖某一个壳。
