# 研究笔记：有人把《毛选》给 Agent 学习 · 2026-07-18

## 1. 老爸观察（先落盘）

- 看到有人把**毛选**（及类似经典）喂给 Agent 学。
- 任务：先落盘 → 再去 Git 搜证据与做法 → 再判断创要不要用、怎么用。

## 2. Git 检索结论（2026-07-18）

现象**真实存在**，且已产品化成 **Claude Code / Codex Skill**，主流不是「整本《毛选》灌进 system」，而是：

```text
经典原文（磁盘/外挂知识库）
    → 蒸馏成「心智模型 / 决策启发式 / 对话协议」
    → 做成按需 skill
    → 分析问题时加载，不是每轮复读语录
```

### 2.1 代表性仓库

| 仓库 | 形态 | 备注 |
|------|------|------|
| [leezythu/maoxuan-skill](https://github.com/leezythu/maoxuan-skill) | Claude Code skill；~958★ | 7 心智模型 + 10 启发式；基于矛盾论/实践论/持久战等；**明确说不是复读语录**；可选外挂原文检索 |
| [kangarooking/mao-selected-works-skill](https://github.com/kangarooking/mao-selected-works-skill) | 1–5 卷 → 多个 skill | 与「书蒸馏成 skill 工具包」同族（cangjie-skill 索引） |
| [raycaoccc/mao-strategy](https://github.com/raycaoccc/mao-strategy) | 决策框架 skill | 标为 Claude Code strategic analysis |
| [zhangtianruiwork-droid/Maoxuan-Changzheng](https://github.com/zhangtianruiwork-droid/Maoxuan-Changzheng) | 「长征机」对话协议 | 调查前置、矛盾重构、持久战略、苏格拉底式反问；**先问再答** |
| [SamadhiFire/xinqingnian-skill](https://github.com/SamadhiFire/xinqingnian-skill) | 重蒸馏 skill | 自称 157 篇蒸馏 + 方法卡；曾更名避敏感 |
| [wwwaapplleecu-source/mao-skill](https://github.com/wwwaapplleecu-source/mao-skill) | /mao 命令式 | 含 learn path / 分析入口 |
| [DONGLUOJI/maoxuan-methodology](https://github.com/DONGLUOJI/maoxuan-methodology) | Codex skill | 明确给 Codex 用 |
| [M0rtzz/Selected-Works-of-MaoTseTung](https://github.com/M0rtzz/Selected-Works-of-MaoTseTung) 等 | **原文语料** | 选集/年谱/文集；给 RAG 用，不是 agent 规则 |

另有若干 fork / 产品经理版（如 atdy/maoxuan-product-agent）。

### 2.2 他们通常蒸馏什么（方法论，不是角色扮演）

常见映射：

| 经典概念 | Agent 用法 |
|----------|------------|
| 主要矛盾 / 次要矛盾 | 先锁真正瓶颈，别被现象带跑 |
| 实践论 / 没有调查就没有发言权 | 先调研再判断（对齐「问人前先搜」） |
| 论持久战 / 根据地 | 阶段策略、不在主战场硬刚 |
| 统一战线 | 内部分歧 vs 外部目标 |
| 星星之火 | 小场景做深再扩展 |
| 一分为二 | 优劣势拆解 |

高质量 skill 的自我定位：**认知框架 / 参谋**，不是「扮演教员复读」。

### 2.3 与创现有规范的重合

已有或接近的：

- **第一性原理 / 调查前置** ≈ 实践论 + grill 前 research  
- **奥卡姆** ≈ 抓主要矛盾、不铺摊子  
- **科斯派工** ≈ 力量与阶段（什么该集中自己干）  
- **对抗审查** ≈ 批评与自我批评的弱形式  

毛选 skill 多出来的是：**矛盾排序、阶段/持久、力量对比、统一战线式组织冲突**——偏战略参谋，不是编码纪律。

## 3. 对创的建议（奥卡姆）

| 做法 | 建议 |
|------|------|
| 整本毛选进 system | **不做**（上下文爆炸 + 引文幻觉 + 边界混乱） |
| 原文 RAG 知识库 | 可选 **D 磁盘**；需要时 file_read / knowledge，不常驻 |
| 蒸馏「矛盾/调查/阶段」短 skill | **可做 B 按需**；仅战略/复杂分析触发，编码日常不加载 |
| 复读语录角色扮演 | **不做**；与调度台、禁可选旁白冲突 |
| 直接依赖第三方 maoxuan-skill 上游 | 可参考结构；**条文自己重写**进 `assets/norm/`，免绑外部 |

**结论一句话**：Git 上已是「方法论 skill 化」主流；创若要用，只抄**蒸馏结构与少数可操作启发式**，按需加载；**不要把毛选当第二套 system prompt**。

## 4. 已落地（最小包 · 2026-07-18）

1. `assets/norm/skills/contradiction-analysis.md`（极短，自写，非语录）  
2. 窄触发：主要矛盾/取舍/战略/资源不够/团队扯皮/先做哪个…；日常编码不加载  
3. `skill-index` + `norm_layer` + `prompt-doctrine` 已挂  
4. **无** CLI、**无**常驻、**无**全文毛选
## 5. 检索命令备忘

```text
github: maoxuan-skill, mao-selected-works-skill, mao-strategy, Maoxuan-Changzheng
```

---

*本笔记仅作研究索引；不构成对任何第三方 skill 的背书或版权意见。*
