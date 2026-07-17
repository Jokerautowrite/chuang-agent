# Git 扫仓笔记：近期可写入创的薄素材 · 2026-07-18

> 原则：仓库可厚；上下文必须薄。只记**可蒸馏成半句/短 skill**的；整仓产品壳不进 always-on。

## 1. 高信号源（近几个月仍活跃）

| 源 | 形态 | 对创的价值 |
|----|------|------------|
| [DenisSergeevitch/agents-best-practices](https://github.com/DenisSergeevitch/agents-best-practices) | 中立 harness skill + 分片 references | **最高**。金句：模型提议，harness 校验/授权/执行/回执；失败时补工具/验收/权限，不只会改 prompt |
| [affaan-m/everything-claude-code · gan-style-harness](https://github.com/affaan-m/everything-claude-code/blob/main/skills/gan-style-harness/SKILL.md) | Generator–Evaluator 分离 | **高**。写码/交付质量环；与创 `adversarial-review` 互补（默认仍不每任务开） |
| [ratel-ai/ratel](https://github.com/ratel-ai/ratel) | 工具/skill 渐进披露 | **中高（架构）**。创 skill-index 已是同族；不必再抄 SDK |
| [ai-boost/awesome-harness-engineering](https://github.com/ai-boost/awesome-harness-engineering) | awesome 列表 | 索引用。关键洞见：关键规则勿只靠 chat 记忆，须进 system/磁盘（创已分层） |
| [huangjia2019/agent-design-patterns](https://github.com/huangjia2019/agent-design-patterns) | 28 模式坐标 | **D 磁盘**：选型字典，不灌上下文 |
| [LeoYeAI/skill-evolution-spec](https://github.com/LeoYeAI/skill-evolution-spec) | skill 生命周期 / Curator | 与闭环 skill「积分防抖」同向；全套自进化延后 |
| [first-fluke/oh-my-agent](https://github.com/first-fluke/oh-my-agent) | 多角色 skill 全家桶 | **不抄**：角色剧场厚，创是调度台不是角色 cosplay |
| [zeronesun/cybernetic-your-agent](https://github.com/zeronesun/cybernetic-your-agent) | 工程控制论四概念 | 已落 `closed-loop-control` |

## 2. 已与创重合（别再写厚）

| 社区说法 | 创已有 |
|----------|--------|
| progressive disclosure | skill-index + 按需 skill |
| compact 会丢规则 | doctrine/skill 常驻分层 |
| Murphy / verify | murphy-accept + verify-before-claim |
| multi-agent 派工 | coding-dispatch + coase + tasks[] |
| 闭环 / 测量 | closed-loop-control + 常驻半句 |

## 3. 建议写入（薄，本轮）

| 条目 | 放哪 | 理由 |
|------|------|------|
| 模型提议 / 运行时校验执行回执 | 常驻半句 | 调度台自我定位，和内核一致 |
| 失败先问缺哪块 harness | 并入闭环 skill 或半句 | agents-best-practices 核心 |
| Generator ≠ Evaluator | **B 按需 skill** | ECC/小乖都在用；窄触发，不默认 |

## 4. 明确不写

- oh-my-agent / 全角色人设包  
- Ratel 整 SDK  
- 28 模式全文 always-on  
- skill 自进化全 pipeline（等创 skill 库真有「腐烂」再 Curator）
