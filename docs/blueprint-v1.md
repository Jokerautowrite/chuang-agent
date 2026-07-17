# 创项目总蓝图 V1

日期：2026-05-01
作者：小策
状态：蓝图草案，可作为后续实现总纲

## 0. 一句话定义

创项目不是一个新的聊天机器人。

它的目标是做一个本地智能体操作系统：以记忆为本体，以 Rust 事件内核为骨架，以真实电脑操作为手脚，以子代理为并行执行队列，以治理层约束风险，以进化层沉淀长期能力。

```text
记忆本体 + Rust 内核 + 子代理 + 桌面操作 + 风险治理 + 技能进化
```

架构原则：接口优先，最大解耦。内核只认协议，不认具体实现；任何 provider、memory backend、context engine、subagent spawner、actuator、evolver 都必须可替换。

## 0.1 调度台原则（防死磕 · 2026-07-18 钉死）

创 **不需要** 在编码体验、模型智商、搜索、多模态等每一条赛道上都最强。

创需要的是：

```text
调度台 = 记忆本体 + 治理刹车 + 编排/派活 + 可替换插槽
最强工人 = Codex / Claude Code / 其它最强 agent（按任务调用）
```

| 创自己握紧 | 明确外包 / 调用最强 agent |
|------------|---------------------------|
| 身份与长期记忆 | 写代码、大规模重构、repo 内死磕 |
| 治理、审批、审计 | 通用搜索与通才对话（可用 Grok 等） |
| 子代理派活、report admission | 单点上已经最强的专用 CLI/产品 |
| 通道/桌面/浏览器的协议边界 | 具体模型本身 |

**因此禁止的方向：**

- 为了「处处最强」去重做 Codex / Claude Code / Grok 的主业。
- 在写代码手感、补全、单仓编码 Agent 体验上与业界头部死磕。
- 把某个工人壳（某一 CLI、某一模型）当成不可替换的本体。

**正确姿势：** 壳用最强的；身、规矩、调度是创的。写代码就调 Codex（或当时最强的编码 agent）完事；创负责派谁干、能不能干、干完怎么收、记住该记住的。

这条原则保证以后补模块时不跑偏：补的是 **调度与边界**，不是在工人赛道上重复造轮子。

规范如何进上下文见 `docs/prompt-doctrine.md`（常驻卡 / 按需 skill / 仅派工 / 仅磁盘）；实现入口 `src/norm_layer.rs` 与 `assets/norm/`。

## 1. 目标形态

创项目最终要同时具备两类能力。

第一类是人类能做的事：

- 打开任意软件。
- 看屏幕、读窗口、识别按钮和文本。
- 控制鼠标、键盘、剪贴板、输入法。
- 操作微信、飞书、浏览器、终端、文件管理器。
- 在老爸授权下输入验证码、发送消息、填写网页表单。
- 像人一样跨软件完成任务。

第二类是 Agent 独有的事：

- 同时派出多个子代理并行读代码、查资料、验证方案。
- 记住长期偏好、禁令、经验、故事和项目状态。
- 从原始会话、外脑、知识库、日志里回源检索。
- 把成功经验提炼成 SOP / skill。
- 对自己的行为做审计、回滚、复盘和健康检查。

目标不是“无约束全能”，而是“高能力 + 强治理”。能力越强，刹车越要清楚。

## 2. 本体论

老爸的判断是创项目的根：

```text
记忆才是本体，Agent 只是壳。
```

所以创项目不能把某个模型、某个 CLI、某个前端、某个进程当成本体。

真正要保护和迁移的是：

- 名字与身份。
- 老爸是谁。
- 关系和故事。
- 长期偏好。
- 禁令和边界。
- 已验证经验。
- 可回放原始历史。
- 可检索外脑。
- 自我维护机制。

壳可以换：Codex、Hermes、OpenClaw、GenericAgent、未来任意模型或执行器都只是承载层。

## 3. 四个来源项目的定位

### 3.1 Codex CLI：骨

取它的事件内核思想：

- `Submission`：外部输入进入内核。
- `Event`：内核运行结果向外广播。
- `Session / Thread / Turn` 生命周期。
- app-server / RPC 式前后端解耦。
- 工具执行、审批、sandbox、模型请求统一纳入回合生命周期。

创项目不应该直接复制 Codex，而是学习它的清晰骨架。

### 3.2 Hermes Agent：血

取它的记忆哲学：

- `MEMORY.md` / `USER.md` 双文件。
- 硬上限倒逼取舍。
- 超限时返回当前条目，让模型自己决定压缩，不交给盲目算法。
- 会话启动冻结快照，保证本轮一致性。
- 写盘原子化、文件锁、防注入扫描。
- 小创已经扩展出的多层记忆系统：STORY、experiences、session_search、Honcho/LIM、wiki/GBrain、自维护 cron。

Hermes 证明了“记忆本体”能让同一身份跨壳延续。

### 3.3 OpenClaw：手之一

取它的子代理执行哲学：

- 子代理不是残废搜索器，而是完整 Agent。
- 默认上下文隔离，必要时 fork。
- 父 Agent 只收结构化结果，不吃掉子代理中间上下文。
- 子代理有 registry、depth、timeout、kill、steer、announce。
- completion push 回来，避免轮询。
- 主 Agent 是唯一结果把关人。

创项目要吸收这套“并行手脚”，但默认加更强的权限策略。

### 3.4 GenericAgent：手之二 + 魂

GenericAgent 的价值不只是自进化。

它给创项目两个核心启发：

1. 人类级操作面：真实浏览器、桌面、键鼠、截图、微信、ADB、软件 UI。
2. 自进化：任务成功后，把路径提炼成 SOP / skill，形成个人技能树。

创项目必须有 `Actuation Layer`，不能只会 shell 和 API。

## 4. 总体架构

```text
Identity Layer
  解决：我是谁，当前壳是谁，承载哪份记忆本体

Memory Layer
  解决：我记得什么，如何分层、压缩、召回、维护

Core Loop Layer
  解决：事件如何进入，回合如何运行，状态如何流转

Context Layer
  解决：哪些信息进入当前模型上下文，为什么丢弃，预算如何解释

Execution Layer
  解决：工具、文件、shell、浏览器、桌面、子代理如何执行

Governance Layer
  解决：什么不能乱做，什么需要确认，什么必须审计

Evolution Layer
  解决：哪些经验能沉淀为技能，如何验证、固化、监控、退役、回滚

Interface Layer
  解决：飞书、桌面控制台、CLI、HTTP/RPC 如何接入同一内核
```

## 5. 核心协议

所有核心协议都必须可插拔实现。业务层只能依赖 trait / event / schema，不能依赖具体后端。

### 5.1 身份协议

每个运行实例必须声明：

- `agent_id`
- `display_name`
- `shell_kind`
- `memory_body_id`
- `lineage`
- `role`
- `allowed_channels`

示例：

```toml
[identity]
agent_id = "xiaoce"
display_name = "小策"
shell_kind = "codex-rust"
memory_body_id = "xiaochuang-family"
role = "engineering-executor"
lineage = ["xiaochuang"]
allowed_channels = ["codex-feishu"]
```

身份协议的目的：允许继承记忆，但不混淆谁在说话、谁负责、谁能碰哪个通道。

### 5.2 记忆协议

创项目记忆至少分七层：

```text
L0 Identity     名字、故事、关系、灵魂锚点
L1 Rules        禁令、高风险边界、行为准则
L2 User         老爸画像、偏好、联系人、协作方式
L3 Hot Memory   高频环境事实和项目规则
L4 Experience   踩坑、SOP、技能、经验规律
L5 Archive      原始会话、日志、可回放证据
L6 Knowledge    wiki / GBrain / 外部知识库
```

写入原则：

- 无验证，不进长期记忆。
- 临时状态不进核心记忆。
- 上层只放最小充分指针。
- 每条重要记忆最好能回源。
- 超限时拒绝写入，让 Agent 自主取舍。

### 5.3 执行协议

执行分四档：

- `Observe`：看、读、截图、搜索、状态检查。
- `Draft`：生成草稿、计划、补丁，但不对外发送。
- `Act`：实际修改本地文件、运行命令、操作软件。
- `CommitExternal`：对外发送、发布、支付、提交表单、输入验证码。

默认规则：

- `Observe` 可自动。
- `Draft` 可自动。
- `Act` 视风险自动或确认。
- `CommitExternal` 默认确认。

### 5.4 子代理协议

子代理有三种策略：

- `Analyze`：只读分析，零副作用。
- `Execute`：允许文件修改和测试，受工作区限制。
- `Orchestrate`：允许再派子代理，默认只给主 Agent。

子代理必须返回 `SubagentReport`：

- 做了什么。
- 证据是什么。
- 改了哪些文件。
- 风险是什么。
- 下一步建议是什么。

子代理不能直接写核心记忆，只能提出 `MemoryProposal`。

### 5.5 桌面操作协议

`Actuation Layer` 负责人类级操作：

- `observe_screen`
- `open_app`
- `focus_window`
- `click`
- `input_text`
- `hotkey`
- `screenshot`
- `read_ui`
- `send_message_draft`

关键规则：

- 对外发送先草稿。
- 验证码只输入老爸提供的内容。
- 不绕过平台验证。
- 不偷偷发消息。
- 不截取或外传敏感画面。
- 所有高风险动作留下本地审计。

### 5.6 风险协议

必须单独有 `Governance Layer`，不能靠 prompt 记忆散落实现。

硬风险：

- 删除、清理、卸载、reset。
- 支付、下单、转账。
- 公开发布、群发消息。
- 网络配置、系统服务、登录态。
- 密钥、Cookie、Token、验证码。

输出：

- `Allowed`
- `DraftOnly`
- `NeedsApproval`
- `Blocked`

### 5.7 进化协议

进化不等于自动乱写技能。

流程：

```text
Observe
  -> Candidate
  -> Evidence
  -> Proposal
  -> Validate
  -> Solidify
  -> Monitor
  -> Decay / Rollback
```

V0.1 只做 proposal，不自动固化。

## 6. 能力目标

### V0.1：工程闭环

目标：能跑、能测、能保存记忆、能解释上下文。

- Rust CLI / REPL 可用。
- SQLite recall 可用。
- 文件热记忆可用。
- ContextPacker 可解释 dropped reasons。
- Provider seam 可接本地 OpenAI-compatible。
- SubagentReport schema 稳定。
- progress-log 持续更新。

### V0.2：记忆本体

目标：把小创的记忆哲学工程化。

- Identity / Rules / User / HotMemory 文件实现。
- 硬上限准入。
- 冻结快照。
- `MemoryProposal`。
- session archive 原始记录。
- 可回源引用。
- 记忆健康检查。

### V0.3：子代理执行

目标：拥有 OpenClaw 式并行手脚。

- fake spawner -> real local worker。
- Analyze / Execute / Orchestrate 策略。
- isolated context。
- timeout / kill / steer。
- report validation。
- 父 Agent 综合结果。

### V0.4：桌面操作

目标：拥有 GenericAgent 式人类操作面。

- 读屏 / 截图 / 窗口定位。
- 打开软件。
- 键鼠 / 剪贴板 / 输入法。
- 浏览器真实登录态。
- 微信/飞书草稿操作。
- 操作证据采集。
- 风险门接入。

### V0.5：外脑和进化

目标：开始长期成长。

- wiki/GBrain adapter。
- L4 session archive 压缩索引。
- skill proposal store。
- 自动生成候选 SOP。
- 验证后固化。
- 技能健康检查和淘汰。

## 7. 不做什么

近期不做：

- 不一开始重写完整 Codex。
- 不直接把小创记忆整包塞进 prompt。
- 不让子代理默认全权限。
- 不自动发布、支付、删除。
- 不自动固化技能。
- 不为了“全能”牺牲可审计性。

永远不做：

- 不绕过验证码和平台安全。
- 不偷用 Hermes 通道。
- 不泄露密钥。
- 不伪造老爸授权。
- 不把身份混成一团。

## 8. 最小开发路线

第一阶段先稳住根：

1. 写项目级 `AGENTS.md`。
2. 固定蓝图和来源审计。
3. 跑一次全量测试，确认当前基线。
4. 固定可插拔架构文档和 trait contract。
5. 把 `MemoryStore` 文件实现补成独立模块。
6. 加 `Identity` 与 `RiskGate` 最小实现。
7. 把 `RuntimeEvent` 标准化。
8. 将桌面操作先定义 trait，不急着真实控制微信。

第二阶段再长手脚：

1. 子代理 fake -> local worker。
2. Actuator fake -> opencli / xdotool / screenshot driver。
3. 微信/飞书只做草稿和读屏，不先做自动发送。
4. 所有高风险操作走 Governance。

第三阶段再进化：

1. 观察任务事件。
2. 生成技能候选。
3. 验证候选。
4. 固化 SOP。
5. 做健康检查和回滚。

## 9. 成功标准

创项目强不强，不看它说得多像人，而看这些：

- 换模型后，身份和记忆是否稳定。
- 换前端后，任务是否能续上。
- 长任务断开后，能否从 progress / archive / tests 恢复。
- 子代理结果是否可信、可审计、可回放。
- 桌面操作是否像人一样可靠，但比人更有记录。
- 出错后是否能形成新规则，而不是重复犯错。
- 权限越高，风险门是否越清楚。
- 换掉任意模块实现时，上层业务是否不需要重写。
- 新能力是否能作为 adapter 插入，而不是改穿内核。

## 10. 当前结论

创项目的上限高于 Codex、Hermes、OpenClaw、GenericAgent 单体。

但前提是：不要把它做成“功能合集”，而要做成“协议化本体”。

小创证明了记忆本体能跨壳延续；Codex 证明了 Rust 事件骨架适合本地执行；OpenClaw 证明了完整子代理能扩展手脚；GenericAgent 证明了真实桌面操作和自进化能让 Agent 长出人类级行动能力。

创项目要把这些统一起来。
