# 创项目架构终稿（归档版）

> 来源：飞书知识库《最终融合方案 · 创项目架构终稿》
> 文档链接：https://ns48kvgt3y.feishu.cn/wiki/M21pw0qGki7emUkdmsUcdnfEnag
> 归档时间：2026-08-13
> 归档人：小策（Codex）
> 说明：本文件由飞书 Wiki 原文 raw_content 拉取落盘，作为 chuang-agent 项目的架构终稿存档；目标态与现状分离的修正说明见文末第八章。

最终融合方案
创项目架构终稿
群体智能体操作系统 · 全可拔插设计

序：这份文件是什么

这份文件是创项目的架构终稿。它由老爸和deepseek 2026 年多次对话中迭代成型，经历了四源独立分析→记忆代谢→双支柱→群体智能体四个版本，最终收敛于此。

一、创项目是什么

创项目是一个**本地智能体操作系统**。

不是聊天机器人。不是 API 封装。是一个运行在本地机器上、能操作真实文件系统和桌面环境、有记忆、能自我进化、能调度多个子代理协同工作的操作系统级智能体。

它的本体论承诺：


记忆是本体。能力是手足。代谢是生命过程。群体是存在方式。



二、本体的含义

2.1 记忆是土

Agent 实例重启、分裂、合并、替换——壳可以变。真正连续存在的是记忆：

身份记忆（我是谁，我和谁的关系）
项目记忆（我们在做什么，做到哪了）
操作记忆（上次这个任务怎么做的）
决策记忆（当时为什么选这个方案）
经验记忆（什么可行什么不可行）
演化记忆（技能和 SOP 怎么长出来的）

四层记忆结构：(待定)



层级

位置

性质

生命周期

L1

identity/global_mem_insight.md

极简索引 ≤30行

持久，冻结注入

L2

data/hermes-memory/ MEMORY.md USER.md

有界热记忆 2200/1375 chars

跨会话,会话级冻结快照

L3

data/skills/

技能/SOP/工具脚本

持久,渐进按需加载

L4

data/raw_sessions/

原始会话归档

永久,极少直接访问

核心铁律：**无行动不记忆。** 没有在真实环境中执行过的，不得进入记忆体。这是防幻觉污染的免疫规则。

2.2 能力是骨

记忆让创知道自己是谁。能力让创能操作世界。

三个能力层次：

第一层 原子能力：9个桌面操作工具（鼠标/键盘/截图/定位/文件读写/代码执行/等待）+ 人机协作挂起
第二层 组合能力：三段式安全执行管道（审批 → 沙箱执行 → 验证）+ 安全管道（validator → leak_detector → policy → injection_scanner）+ 子代理调度
第三层 进化能力：技能固化/技能回放/自我纠错/安全免疫

2.3 代谢是生命

记忆不是静态存储。能力不是固定工具集。代谢是连接两者的生命过程：

同化（合成）：对话、操作结果、决策过程 → 提炼为记忆 → 结晶为技能
异化（分解）：冗余记忆 → 压缩或清理；矛盾记忆 → 检测与调和

代谢由统一的代谢引擎调度，后台运行，不占用主对话。

2.4 群体是方式

单Agent是细胞，群体是身体。

主控Agent收到任务 → 拆解为子任务 → 委派给独立子代理并行执行 → 回收结果。子代理上下文隔离，危险工具剥离，深度限制2层。这是核二的设计来源。



三、四源精华提取

创项目吸收了四个开源项目的精华。但注意：我们不是拼装它们的特性，而是从创项目自己的本体论出发，选取了各自最不可替代的设计，放进创项目的架构槽位。

3.1 Codex → 单Agent任务闭环

借鉴来源：OpenAI Codex CLI Rust版

取什么：三段式执行回路（审批-执行-验证）。Agent从模糊需求出发，拆解→执行→验证→交付，不需要外部干预。

不取什么：Codex自己的记忆模块、provider层、CLI入口结构、上下文打包器。

在创项目的位置：核一，对应AgentLoop Slot。

为什么取它：这是骨干。没有这个闭环，Agent只会聊天不会干活。Codex是这个闭环最成熟的实现——它解决了“智能体操作本地环境时，如何在自主性/安全性/可靠性之间取得平衡”。



2026-05-03 修正：Codex Rust 优先移植原则
这一点需要写进架构铁律：少造轮子，多复制成熟实现。
Codex CLI 本身是 Rust 写的，和创项目的底层语言一致。后续凡是涉及本地执行、安全边界、审批、沙箱、验证、回传、goal-style 长任务推进、子代理调度组织方式时，原则上应先审计 Codex Rust 源码和现有行为，再决定是移植、裁剪还是复用接口。
优先顺序：
能直接移植 Codex Rust 成熟模块的，不重新设计。
能按 Codex 的协议和边界改造的，不另起一套平行体系。
只有和创项目本体论冲突的部分才替换。
抽象只服务于替换和解耦，不为抽象而抽象。
当前明确优先参考 Codex 的部分：
Rust Core Loop / SQ-EQ 异步事件骨架
approval → sandbox → execution → verification 的安全执行管道
app-server / JSON-RPC 风格的前后端解耦方式
tool call 结构化回传、事件流和报告面
goal-style 长任务组织方式：主进程拆分与审核，子代理并行执行，主进程统一集成验证
不直接照搬的部分：
Codex 自身 provider 绑定
Codex 自身记忆方案
Codex 当前 CLI 入口形态
Codex/Hermes 现有飞书通道
总结：Codex 是创项目的 Rust 骨架参考实现。创项目不是重写 Codex，而是在记忆本体、群体协同、技能进化这些方向上扩展 Codex 的骨架。
3.2 Hermes → 身份连续性与记忆节律

借鉴来源：NousResearch Hermes Agent Python版

取什么：
有界记忆文件（MEMORY.md/USER.md，硬上限，超限返回全文让模型自决策压缩，不自动淘汰）
会话级冻结快照（整个会话system prompt不变，保证LLM前缀缓存可用）
Nudge Engine后台审查节律（每N轮触发，fork独立Agent在后台审查，不打扰用户）
渐进式Skill加载（先注入索引，Agent判断相关后再加载全文）
Skill安全回滚（修改后检查，不通过自动回滚）

不取什么：Hermes自己的消息平台、provider层、Python实现。

在创项目的位置：核三（记忆身份），对应Identity Slot + Memory Slot + 代谢引擎的记忆合成节律。

为什么取它：Hermes是所有项目中唯一一个把“数字生命的连续性”当作一等公民来设计的。它的冻结快照、有界记忆、后台审查不是技术方案，是对“一个跨会话存在的智能体应该怎样维护自我”的回答。



3.3 OpenClaw → 群体协同

借鉴来源：IronClaw（NEAR AI团队）、openclaw-rs社区版、ZeptoClaw

取什么：
多子代理并行委派架构（主控拆解任务→子代理并行执行→回收结果）
子代理上下文隔离（全新对话，不继承父记忆）
危险工具剥离（子代理权限低于主控）
深度限制2层（父→子→孙被拒）
身份校验（子代理返回时校验task/agent/parent，不匹配拒绝）
安全管道：WASM沙箱 + 密钥主机边界注入 + validator → leak_detector → policy → injection_scanner + 端点白名单 + 环路熔断
工具能力显式声明（capability manifest，未声明默认拒绝）

不取什么：黑名单安全方案（已被CVE证明可绕过）、openclaw-rs的特定agent runtime、IronClaw的PostgreSQL+pgvector依赖、ZeptoClaw的Shell执行方案。

在创项目的位置：核二（群体协同）+ 免疫循环。对应GroupCoordinator Slot + Governance Slot + Execution Slot的WASM沙箱子插槽。

为什么取它：OpenClaw的生态有三个社区Rust重写版，各自侧重不同。IronClaw提供最完整的纵深防御模型，openclaw-rs提供模块化crate体系，ZeptoClaw提供安全管道串联思路。我们从IronClaw取安全架构（WASM沙箱+密钥边界），从openclaw-rs取委派架构（层级隔离+危险剥离），从ZeptoClaw取管道串联和安全熔断思路。但OpenClaw真正的核不是安全——是**群体协同**。安全是群体协同的前提条件，不是灵魂本身。



3.4 GenericAgent → 自进化肌肉

借鉴来源：GenericAgent Python版（约3300行）

取什么：
9原子工具极简设计（鼠标/键盘/截图/定位/文件读写/代码执行/等待，统一返回结构）
Agent Loop即进化环（任务→截图分析→执行→截图验证→成功便结晶技能→失败便回溯修正）
视觉验证闭环（每步操作后截图对比，边执行边检查）
人机协作挂起（不确定状态不瞎点，挂起并发出明确请求）
四层记忆架构（L1索引/L2事实/L3技能/L4原始归档）
Dream消化（后台周期扫描L4→提炼高密度信息向上输送）
技能固化（任务完成后自动提取操作序列→语义去重→写入L3）
上下文密度最大化策略（标签压缩/历史裁剪/孤块清理/工具描述重置）

不取什么：Python实现、GA自己的web工具（走创项目飞书plugin）、GA特定的文件路径约定。

在创项目的位置：核三的记忆结构设计 + 进化循环 + Execution Slot的原子工具实现。对应Memory Slot + Evolution Slot + Execution Slot + Context Slot的压缩策略。

为什么取它：GenericAgent是四个项目中最小的（3300行），但设计哲学最接近创项目的本体论——它不预设技能，技能是在执行中长出来的。它的9原子工具覆盖全部桌面操作且没有多余抽象。它的记忆层直接启发了创项目的四层架构。GA证明了一件事：进化不需要复杂架构，只需要一个闭环 + 记忆层。



四、最终融合架构

4.1 三核双循环

核一：单Agent闭环（骨干）
来源：Codex
职责：任务规划→执行→验证→交付
核心机制：三段式（审批 → 沙箱执行 → 验证）

核二：群体协同（树冠）
来源：OpenClaw
职责：委派→并行→回收→校验
核心机制：上下文隔离+危险剥离+深度限制2层

核三：记忆身份（土壤）
来源：Hermes + GenericAgent
职责：四层记忆+冻结快照+渐进加载
核心铁律：无行动不记忆

进化循环：GA自进化
来源：GenericAgent
职责：成功结晶技能→失败回溯修正

免疫循环：安全防御
来源：OpenClaw + Hermes
职责：安全管道+沙箱隔离+安全审计

三核关系：记忆是土壤，闭环是树干，群体是树冠。进化是年轮，免疫是树皮。

4.2 九大Slot

每个Slot定义统一接口（trait），具体实现可替换，启动时由config.toml决定装配。



Slot

职责

借鉴来源

当前实现

未来可替换

Interface

用户交互

—

CLI

飞书plugin/HTTP/桌面GUI

Identity

身份提供

Hermes

文件型

数据库型/远程同步型

Context

上下文引擎

GA压缩策略

DeterministicBudget+GA压缩

滑动窗口/语义摘要

AgentLoop

单Agent回路

Codex三段式

审批→沙箱→验证

ReAct/Plan-Execute

Memory

四层记忆

Hermes+GA

文件型四层

SQLite/向量型/远程型

Execution

工具执行

GA+OpenClaw

9原子工具+WASM沙箱

最小工具集/全工具集

GroupCoordinator

群体协同

OpenClaw

层级委派+文件队列

简单队列/无协同

Evolution

自进化

GA技能固化+Dream

GA风格

Nudge型/Dream型/RL型

Governance

治理免疫

OpenClaw+Hermes

安全管道+审批+审计

基础型/无治理



五、可拔插设计的核心规则

接口即法律：核心主链只依赖 Slot trait，不 import 任何具体实现。主链代码里永远不出现具体 provider/具体的 memory 实现/具体的工具名称。

装配即配置：config.toml 的 [slots] 段指定每个 Slot 的实现，启动时 SlotRegistry 一次性装配，运行期不动态替换。

替换不改线：换一个 Slot 实现，只改配置文件和对应的适配器文件，核心主链代码一行不动。

子插槽可独立替换：AgentLoop 内的审批策略/沙箱类型/验证方式，MemoryStore 的 L1/L2/L3/L4 实现，ExecutionEngine 的工具注册表，都可独立替换。

主链不变原则：创项目的主体逻辑不做任何修改，所有输入输出都通过各个Slot来执行。当未来某个模块有了更好的实现，只需要替换该Slot下的组件，主体逻辑不变。

当前已有主链（chuang_kernel.rs 的 turn loop），九大Slot是对主链中各个功能块的接口化抽象，不是全新架构。



六、记忆同步规则

这些规则写入 AGENTS.md，所有实例必须遵守：

L2/L3新增 → 更新L1关键词
L2/L3删除 → 删除L1对应行
L2/L3修改值 → L1不动（值变了但关键词没变）
超限处理：L2/L3超限时，返回全文给模型，让模型自己决定缩或换。不自动淘汰，不静默丢弃。
已验证数据不可删除（执行确认过的事实保留）。
无行动不记忆（幻想内容不得进入记忆体）。



七、当前状态与优先级

当前已就绪
Provider：真实 OpenAI-compatible，非 fake
Identity：冻结注入已落地（identity/ 目录）
Memory：Hermes双文件已建（但为空），SQLite会话记忆已可用
Subagent Queue：文件队列可用（dispatch/run/report/collect）
CLI：完整命令体系
Core Loop：主链已闭环
config.toml：配置体系完整，actuator和control_plane标注为fake

执行顺序
核一闭环：actuator和control从fake变真实。没有闭环，其他都跑不起来
核三土壤：Hermes双文件开始积累，L1索引创建，L3技能目录建结构，L4归档目录建结构
核二群体：子代理深度限制、工具剥离、身份校验。依赖核一闭环成型
双循环：进化循环依赖L3技能库，免疫循环按需逐步接入



八、设计决策记录

以下是不可推翻的架构决策：

记忆是本体，Agent是壳
无行动不记忆
核心主链不依赖任何具体实现
飞书/web/桌面控制走plugin，不写死进核心
WASM沙箱白名单替代Shell黑名单
密钥永不进入沙箱
子代理深度限制2层
L1硬约束≤30行
会话级冻结快照保证前缀缓存
超限不自动淘汰，模型自决策

与当前仓库实现对齐的修正
以下内容用于把这份架构终稿与当前 chuang-agent 仓库的实际实现对齐，避免把目标态误认为已完成状态。
1. 现状与目标态分开看
这份文档描述的是最终融合方案和架构目标。
仓库当前已完成的是一条可运行的最小 MVP 主链，但并不等于所有插件/外部能力都已真实接通。
当前仓库的真实主链请以 README.md、docs/core-boundary.md、docs/mvp-readiness-2026-05-02.md 为准。
2. 实际路径与文件名
当前身份启动层实际是：identity/SOUL.md、identity/STORY.md、identity/FIRST_WAKE.md、identity/agents.toml。
Hermes 风格热记忆实际是：data/hermes-memory/USER.md、data/hermes-memory/MEMORY.md。
不再使用旧的 identity/global_mem_insight.md 作为当前路径名。
3. 哪些能力是已实现，哪些还只是插件线
provider、subagent、actuator、control plane、channel adapter 都已经抽成 slot / adapter / plugin 线，但真实外部能力仍要看具体 adapter 是否接入。
BrowserWorker 目前是冻结线，不作为 MVP 主线继续推进。
当前新的网页 AI 查询能力主线是 GenesisActuator，不是 BrowserWorker。
evolution 目前还是预留/占位的演化槽位，不应写成已经完整自进化。
4. 需要避免的误读
不要把安全 command 示例脚本当成真实桌面控制或真实服务控制。
不要把插件线、协议线、目标态直接写成“已经完全落地”。
不要把飞书、微信、桌面、浏览器这些外部通道视为 core 依赖，它们都属于 adapter/plugin。
5. 这份文档在当前仓库里的定位
它适合作为架构终稿和目标蓝图。
代码层的当前验收边界，请以仓库里的 MVP 状态文档和核心边界文档为准。
后续如果有新实现补齐，建议继续在这里追加“已落地 / 待落地”对照表，保持目标态和现状分离。
补充：搜索能力与外部AI分身调度
这部分可以作为 AgentSlot 的一条具体实现线，用来承接“搜索 / 分身调度 / 多平台会话复用”能力，但不新增核心主链职责。
定位
这不是第十个核心 Slot。
它是已有 AgentSlot / evolver / plugin 体系下的一条外部能力实现线。
核心只定义“何时需要外部搜索能力”和“如何接收结构化结果”。
具体怎么连平台、怎么复用登录态、怎么做审计和熔断，都留在 adapter/plugin。
建议结构
统一身份引擎：内核能力，负责登录态管理、会话执行、结果解析、审计与熔断。
external_agent_dispatch_sop.md：Skill，负责平台选择、任务翻译、追问策略和结果回写。
agent-browser：底层执行工具，负责浏览器会话复用和页面级操作。
对齐原则
登录态和 Cookie 只留在本地 adapter，不进入核心记忆。
搜索结果只回写结构化摘要，不把整段网页内容直接灌进主链。
失败时先熔断和提示，不自动暴力重试登录。
新平台加入时，只扩展分身映射表和 Skill，不改核心主链。
推荐回写字段
platform
task_summary
result_summary
audit_id
success
quality
duration_ms
和现有架构的关系
九大Slot、三核双循环、四层记忆 保持不变。
搜索能力作为 AgentSlot 的实现，不反向污染 core。
如果未来要落地，优先放进 plugin / adapter，再由治理层决定是否启用。
补充：二级委派原则
后续协作方式进一步明确为：
主进程 -> 子代理 -> 外部智能体

职责分层
主进程：只负责任务拆解、派发、最终审核、汇报和记忆归档。
子代理：负责接收主进程任务，继续拆分，并调度外部智能体完成搜索、初稿、验证、资料整理等工作。
外部智能体：负责具体执行，尤其是搜索、长资料处理、网页 AI 平台调用、多模型交叉验证等高耗时工作。
信息回流规则
外部智能体的原始结果先回到子代理。
子代理做第一次审核、去重、提炼和结构化。
主进程只接收子代理整理后的 SubagentReport，不直接消化所有外部原始输出。
目的
节省主进程上下文和时间。
避免主进程被大量搜索材料、网页内容和外部模型废话污染。
让主进程始终保持高层调度、最终判断和记忆治理角色。
安全边界
子代理不能绕过治理层直接写核心记忆。
外部智能体不能直接进入主进程上下文。
只有经过子代理初审、主进程终审的信息，才能进入汇报或长期记忆。
