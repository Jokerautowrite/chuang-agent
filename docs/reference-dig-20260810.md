# 参考资料深挖报告（2026-08-10）

> 背景：老爸发起「深挖所有参考资料」，目标是把本机跟 harness / 自进化 / Agent OS 相关的参考资产挖透，找对创可落地的方向。
> 方法：全部本机文件，未联网；源码优先于文档；重点对照创现有能力（goal_mode / skill_evolver / benchmark / app-server）。

---

## 0. 一句话结论

**创缺的不是方法论，是"评测门禁 + 自驱外环 + 图片不进会话"这三块工程化闭环。**
本机参考资产里，penguin-harness 给了进化闭环的完整实现，Claude Code 源码分析给了运行时工程细节，
PI 对比给了 durable harness 方向，本机 methodology skills 给了可复用协议——全部已落盘，可按优先级移植。

---

## 1. 参考资产总清单（本机位置）

| # | 资产 | 位置 | 状态 |
|---|------|------|------|
| 1 | penguin-harness 源码（TS monorepo） | `~/project/penguin-harness/packages/core/src/` | ✅ 全文深读 |
| 2 | penguin 自进化 4 skills | `~/project/penguin-harness/packages/skills/skills/` | ✅ 全文深读 |
| 3 | penguin 官方文档（self-improvement/goal-mode） | `~/project/penguin-harness/docs/content/*.zh.md` | ✅ 对照源码 |
| 4 | 本机 methodology skills（.claude/.agents 两套） | `~/.claude/skills/` `~/.agents/skills/` | ✅ 读 8 个关键 |
| 5 | Claude Code 18 章源码分析（catyans） | `/data/06-资料库/claude-code-source/claude-code-source-analysis/` | ✅ 重点章深读 |
| 6 | 小创 4/1 Claude Code 笔记 | `/data/06-资料库/记忆研究/专题研究/claude-code-analysis.md` | ✅ 全文 |
| 7 | PI（earendil-works/pi）对比与移植清单 | `~/projects/chuang-agent/docs/pi-*.md` | ✅ 已有，交叉核对 |
| 8 | PI×Penguin×创 融合路线 | `~/projects/chuang-agent/docs/pi-penguin-chuang-integration-roadmap-2026-08-06.md` | ✅ 已有 |
| 9 | 创 harness 接入方案 | `~/projects/chuang-agent/docs/harness-integration-plan.md` | ✅ 已有（8-08） |
| 10 | Penguin 方法论拆解 → 创 | `~/agent-hub/reports/penguin-methodology-chuang-optimize-20260806.md` | ✅ 已有 |
| 11 | Penguin 自进化实验存档 + Case 工厂规格 | `~/projects/penguin-harness-evolve/` | ✅ 深读 |
| 12 | agentic-os（多 Agent 编排平台） | `~/projects/agentic-os/` | ✅ 结构+宪法 |
| 13 | memory-qa-layer（记忆问答层） | `~/projects/memory-qa-layer/` | ✅ 架构+流水线 |
| 14 | Apex 超级进化系列（10 篇） | `/data/06-资料库/apex-超级进化系列/` | ✅ 全读 |
| 15 | 昨日资产盘点（跟 OpenCode 一起做的） | `~/agent-hub/reports/asset-archive-20260809.md` | ✅ |
| 16 | 本机全面盘点报告 | `~/桌面/工具/本机全面盘点报告-2026-07-15.md` | ✅ 结构核对 |
| 17 | 小创记忆源清单 | `~/project/蒸馏小创/SOURCE-INVENTORY.md` | ✅ |
| 18 | multiagent（终端多 Agent 群聊） | `~/projects/multiagent/` | ✅ 定位核对 |

---

## 2. 深挖提炼（按对创的价值排序）

### 2.1 进化闭环（penguin 4 skills + 实验存档）——创最该补的第一块

penguin 的闭环设计（已从源码逐行验证）：

```
Benchmark 冻结 → Formal Baseline → Optimizer 诊断 → 有界 Candidate
  → Evaluator 并行评（同一冻结矩阵）→ 分数严格提升才接受，否则回滚快照
```

关键机制（每一条都是可抄的硬规则）：
1. **statement/rubric 隔离**：Target 只看题面，评分标准 0600 私有 → 防作弊。
2. **无基线不优化**：没有完整 Formal Baseline 不许动 Agent State → 防自嗨。
3. **分数严格提升才接受**：不满足就回滚 `snapshots/v<version>.tar.gz`（排除 vault）→ 防退化。
4. **Pilot 校准先预测**：派发前预测两个策略的分差，避免无效改题。
5. **Evaluator 纯协议输出**：只回 YAML，格式不合规由 Evaluator 重发，不重跑 Target。
6. **拒绝的 candidate 也是证据**：进候选池不算分，但留档供下次诊断。
7. **scoreboard 只信任写入的聚合值**，不重算；0..100 固定刻度。

创现状对照（来自 harness-integration-plan）：
- 已有 goal_mode / skill_evolver / benchmark 碎片，**缺串联**；
- skill_evolver 有 proposal/validation/approval，但**没有评分门禁**（改技能不跑分就上）。

### 2.2 goal 模式（penguin goal-loop / goal-file / goal-prompts）——创 goal_mode 的升级蓝本

| penguin 设计 | 创可抄点 |
|---|---|
| GOAL.yaml 只有 objective（系统写）+ status（模型唯一可写，只允许 complete/blocked） | 收敛控制通道，防止模型乱写 |
| 解析失败归一化为 blocked（控制通道坏了就停，不空转） | fail-closed |
| 每轮 `[goal]` 块重复完整协议（跨压缩自洽） | 压缩后不丢协议 |
| 终止只认 4 来源：goal status / token 预算 / 截断轮 / maxRounds(100) | 显式终止矩阵 |
| 幻影轮防护：abort 落轮间不重发、截断轮不重发 | 防重复烧钱 |
| blocked 审计：同一阻塞条件**连续 3 轮**才允许 blocked | 防过早放弃（正是 update_goal 的 3 轮规则） |
| 预算耗尽有 wrap-up 收尾轮（不许标 complete） | 收尾不留悬案 |
| token 会计 = 增量非缓存 input + output（含 subagent） | 成本归属 |

### 2.3 上下文压缩状态机（penguin context-engine + Claude Code 5 策略）——创 context 层的合并参考

penguin（1707 行状态机）：
- **没有可重试的 attempt 进入历史**（流消费完且验证才 commit）→ 重发安全；
- failed 也重试（分类只是提示不是裁决，只有 auth 终止）；指数退避 + skipReconnectWait；
- 压缩 trigger = context length 或 sessionTurns；MAX_SUMMARY_REJECTIONS=5；
- 压缩请求保持工具集不变 → 保 prefix cache。

Claude Code 5 策略级联（02 章 + 09 章）：
- Snip（丢旧工具输出）→ Micro（单条内去冗余）→ Collapse（多轮相似工具合并）→ Auto（90% 阈值全量压缩）→ Session Memory（与记忆去重）；
- **熔断器**：连续 3 次 autocompact 失败停止重试（全球每天浪费 25 万次无效调用）；
- **递归保护**：session_memory 和 compact 查询源禁止触发 autocompact；
- 压缩前先 strip images（避免压缩 API 超长）；
- Post-compact 分级保留：文件 5 个 / skill 截尾 5000 token / skill 预算 25000。

### 2.4 图片处理（penguin session.ts）——**昨天 vision 兜底的镜像正解**

关键发现（重要）：penguin 的 `modelHasVision` 决定的是**处理方式而非是否处理**——
- 无视觉模型：`foldInputImages` → 图片落 scratchpad，以 `[attached image: <路径>]` 文本路径引用；
- 有视觉模型：`read_image` 工具按需读；
- goal 模式图片每轮都作为文本路径注入，**与模型是否支持视觉无关**；
- `selectBuiltinToolsForModel` 按 `forModel: vision|text-only` 过滤（read_image vs describe_image）。

这正是老爸说的「图片存起来、读完告诉你、不要每轮带进会话」——**penguin 已有完整实现，创直接抄语义即可**：
图片落 scratchpad + 路径引用 + 按需 describe/read，而不是每轮把图塞给模型。

### 2.5 错误恢复（Claude Code 02 章七级级联 + withRetry）——创 provider 层的升级蓝本

```
L1 流式→非流式回退 → L2 上下文折叠 → L3 响应式压缩（每轮只试一次，hasAttemptedReactiveCompact）
→ L4 输出上限升 64K → L5 多轮续接（≤3 次）→ L6 stop hook → L7 token 预算续接
```
- 每种恢复**只尝试一次**（守卫标志防重入）；从轻到重；无交叉影响。
- withRetry：base 500ms / max 32s / jitter 0.25 / 只重试 [429,529,ECONNRESET,ETIMEDOUT]；
  连续 529 切备用模型；429/529 可切快速模型。
- 空闲看门狗：45s 警告 / 90s kill；被动停滞检测 30s。
- 扣留机制：可恢复错误先扣留不显示，全部恢复失败才释放给 UI（用户看不到即将修复的错误闪烁）。

### 2.6 权限系统（Claude Code 07 章 + 小创笔记）——创治理的精细化参考

- 三层规则 allow/deny/ask，每层按 source 区分来源（alwaysAllow / alwaysDeny / alwaysAsk）。
- **输入级** readonly/concurrency/destructive 判断（不是工具级）——`isReadOnly(input)`。
- ResolveOnce 双标志（claimed 同步 + delivered 异步）防权限竞态。
- 分类器宽限期 200ms：用户按键优先于自动批准。
- 三路处理器：Interactive / Swarm Worker / Coordinator（不同执行上下文不同决策路径）。
- 7 层纵深防御：配置规则 → 自动分类器 → Hook → 对话框 → 拒绝追踪 → 工具级验证 → 投机分类器。

### 2.7 记忆系统（Claude Code 11 章 + memory-qa-layer + agentic-os brain）——创记忆的参考

Claude Code 记忆设计：
- 4 类型：user / feedback / project / reference；feedback 用 **Why + How to apply** 结构；
- MEMORY.md 上限 200 行 / 25KB（每次调用都注入，必须精简）；
- 召回用 **Sonnet 侧查询**（manifest 清单 + 小模型选相关文件）而非嵌入搜索——深层语义关联，能处理模糊查询；
- 记忆提取 fire-and-forget（不阻塞轮次）；`shouldSaveAsMemory` 过滤：与 CLAUDE.md 重复/临时/可推导一律不存。

memory-qa-layer（本机已全完成 M1-M3）：
- M1 深读 → M2 索引（SQLite + bge-m3 向量）→ M3 问答（多路召回 + 带引用回答）；
- 护栏：原文只读、衍生层不进注入上下文、情感原文只指向不替代、缺口不编造、回答必带引用；
- 每日 06:30 systemd timer 增量更新 + 月度蒸馏 + bench50 回归。

agentic-os brain（宪法）：
- 每任务前 8 步：读 brain → 查 recent-decisions → 看 skill eval 分 → 记审计 → 执行 → 更新 learnings → 审计 → git commit。

### 2.8 工具系统工程（Claude Code 03/12 章 + 小创笔记）——创工具面的补课

- Tool 是纯 type 接口（无继承链）；ToolUseContext 一次传完上下文。
- **deferred tools + ToolSearch 按需加载**：控制 context 的关键机制（工具多了以后全塞 schema 太浪费）。
- maxResultSizeChars 超阈值自动持久化到磁盘，给模型预览 + 路径。
- interruptBehavior：cancel（丢弃）vs block（等待）——用户打断语义。
- Skill 文件提取 O_EXCL + O_NOFOLLOW 防 TOCTOU/符号链接攻击；目录 0700 + 文件 0700。
- Skill 元数据（name/description/whenToUse/allowedTools/model/hooks）自动注入 prompt，不注册。

### 2.9 自进化理念（Apex 超级进化 + evolver skill）——思想参考，别照抄

Apex 系列（10 篇）：
- 轨迹指纹 HashPool 固化（成功轨迹→哈希池→跨会话复用）→ 对应 penguin trace/scoreboard；
- Select-Read-Act 三段式检索闭环 + SkillBank 演进（成功/失败自动归纳新技能、淘汰低效）→ 对应 skill_evolver + 评分门禁；
- 公式多是口号式（基尼/熵/随机森林/海马体 SWR 类比），**工程价值低，只取"轨迹沉淀 + 技能库演进 + 模型路由"三个概念**；
- 量子通道路由：多 LLM 分类路由（高端推理/代码/低端）→ 对应创 provider slot + 模型目录（pi-port-checklist Phase 1-3）。

evolver skill（.agents）：GEP 基因/胶囊/events.jsonl 审计进化，EVOLVE_ALLOW_SELF_MODIFY=false 默认关自改。

### 2.10 methodology skills（8 个，本机可直接复用协议）

| Skill | 一句话 | 接创方式 |
|---|---|---|
| agent-harness-loop | observe→decide→act→record→reflect + bilevel 外环（重复失败≥2 改规则，一次一处/auto-revert） | 创缺的自驱外环核心 |
| verifier-first-loop | 开 loop 前 30 秒写 Done/证据/pass 检查；「自述完成」不算绿 | goal validate 升级为验收先行 |
| eval-outer-loop | 外部信号类型表 + page-human 闸门 + swear-meter | benchmark 验收量化 |
| failure-surface-first | partial≠total；阶段分叉；四层失败面（人话/短摘要/深证据） | failover 语义 |
| session-handoff-continuity | 最小 HANDOFF 七块 + 磁盘状态优先 | session archive |
| context-budget-thrift | Pocket/Desk/Bookshelf 三级加载；section 优先于全文 | context budget |
| proactive-infra-triage | 402/401/ENXIO 不算完整学；基建失败先修 harness | 自驱选题 |
| proactive-topic-roi | 选题 ROI，避免 meta 空循环 | 自驱选题 |

---

## 3. 对创的落地建议（按性价比排序）

### P0（直接开工，不碰主链，低风险）
1. **图片不进会话**（镜像 penguin session.ts 语义）：
   - 图片落 scratchpad，消息里只留 `[attached image: <路径>]`；
   - 按模型能力注入工具：有视觉 → read_image；无视觉 → describe_image 按需描述；
   - 这是昨天 vision 兜底的正式升级方向（mimo 每轮描述 → 按需描述）。
2. **模型目录静态表**（pi-port-checklist Phase 1 已有方案）：
   - `src/model_catalog.rs`：provider/model/supports_tools/max_input/reasoning_supported/aliases；
   - config 未命中目录报结构化错误，不 silent fallback。

### P1（中成本，需确认）
3. **Benchmark 闭环**（pi-penguin roadmap Phase A 已有方案）：
   - `src/benchmark/` + `data/benchmarks/<id>/`，statement/rubric 隔离（rubric 0600）；
   - 复用 subagent run-once 做 Evaluator；第一个能力建议记忆召回（已有 memory_recall + SQLite archive）。
4. **skill_evolver 接评分门禁**：提案先跑分，分数严格提升才 upsert；未达标进候选池；变更前自动快照。
5. **goal_mode 升级**（抄 penguin goal-file/goal-prompts）：
   - 收敛控制通道（objective 系统写 / status 模型只写 complete|blocked）；
   - blocked 审计连续 3 轮；预算耗尽 wrap-up 轮；幻影轮防护。
6. **provider 错误恢复级联**（抄 Claude Code 7 级）：
   - 每种恢复只试一次；429/529/ECONNRESET/ETIMEDOUT 重试（500ms→32s jitter 0.25）；
   - 连续 529 切备用模型；空闲看门狗 45s/90s。

### P2（长线，需立项）
7. **durable harness**（pi-port-checklist Phase 4-5）：thread 树 + lane_id + turn_operation_log + EffectBoundary 确定性步进测试。
8. **deferred tools**：工具 schema 按需加载，控制 context。
9. **Sonnet 式记忆召回**：manifest + 小模型侧查询（对比现在嵌入召回，处理模糊查询更强）。
10. **上下文 5 策略级联 + 熔断器**：Snip→Micro→Collapse→Auto→Session Memory；连续 3 次失败熔断；压缩前 strip images。

---

## 4. 明确不抄的（防止跑偏）

- 不把创改成 coding agent CLI（调度台原则）。
- 不引入 TS/Bun 重写 Rust 内核。
- 不外包治理给容器/OpenShell（创的治理必须内置）。
- 不照抄 Apex 的公式口号；只取"轨迹沉淀/技能库演进/模型路由"三个概念。
- 不照抄 agentic-os 的 Python FastAPI 架构（Rust 是创的骨架优势，Python 只做粘合）。

---

## 5. 交叉验证结论（哪些方向已被多次印证）

| 方向 | 印证来源 |
|---|---|
| 评测门禁才允许进化 | penguin（分数严格提升）+ evolver skill + Apex（演化评分）+ PI roadmap Phase C |
| 图片不进会话 | penguin session.ts + 老爸昨天指示 + 本机 8-07 经验 |
| 失败面分层 | failure-surface-first + Claude Code 7 级级联 + penguin 分类非裁决 |
| 快照回滚 | penguin snapshots + evolver + PI roadmap + agent-hub backups |
| 控制通道收敛 | penguin goal-file + 本机 update_goal 规则 + Claude Code 权限三路 |
| 压缩必须带熔断 | Claude Code（3 次熔断）+ penguin（MAX_SUMMARY_REJECTIONS=5）|

---

## 6. 遗留/待确认

- [ ] 老爸确认 P0 图片方案是否直接开工（改 chuang 主链的 image handling）
- [ ] 第一个 Benchmark 能力：记忆召回（建议）还是治理拦截？
- [ ] Evaluator 默认模型：DeepSeek 免费优先还是 gpt-5.6-luna？
- [ ] `~/桌面/工具/apex/` 已归档到 `/data/06-资料库/apex-超级进化系列/`（asset-archive-20260809），后续引用用新路径
