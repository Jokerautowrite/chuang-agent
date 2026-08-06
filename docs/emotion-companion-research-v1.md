# 情感陪伴模块调研与接入方案 v1（2026-08-06）

## 0. 战略定位

- 不做「最强的通用 Agent」——创做指挥者/调度台，能力靠接入（Codex/Claude/OpenClaw 等）。
- 差异化方向：**最懂主人的个人情感助手**。
  - 记忆是身体：创已有身份/记忆/治理体系，情感模块是其上的「心」。
  - 情感陪伴赛道拼的不是模型智商，而是**连续性、记忆、主动性、人格一致性**——正好是创的强项。

## 1. 调研全景（开源仓库）

### 1.1 情感状态机（Affect & Drives）—— 最值得移植

| 仓库 | Stars | 要点 |
|---|---|---|
| `Shitsuten/jiwen`（积温） | - | **五维漂移轴**（想联系/固执/情绪/焦虑/忙碌），阈值触发行为，无骰子无 prompt 工程，~500 行零依赖 MIT。最适合直接移植为 EmotionSlot 内核 |
| `A1batr055/Drivesoid` | - | HTTP sidecar 追踪情感驱动力（疲劳/渴望/焦虑/玩耍/保护欲/亲密），从对话+睡眠周期事件更新 |
| `chuli1122/Eventide` | - | 身体状态机：7 驱动力 + 18 短期事件 + 梦境联动，JSON schema 写回，生成隐藏状态 prompt（NSFW-adjacent，非商用） |
| `Vael-KY/Tidefall` | - | Eventide 的 Supabase 版：六相周期、7 漂移身体值、pg_cron、快照、浏览器面板（非商用） |
| `gqy20/Aura` | - | Android 陪伴 app：跨会话长期记忆 + **情感状态机 + 随时间加深的关系模型** + 健康数据（参考架构） |

### 1.2 情感记忆（怎么记住主人的情绪）

| 仓库 | 要点 |
|---|---|
| `P0luz/Ombre-Brain` | **Russell valence/arousal 情感标签** + Obsidian 存储 + **遗忘曲线** + 向量/BM25 双召回（MCP，可挂 OpenClaw/Codex） |
| `Shitsuten/paramecium` | 原文为源、向量只做索引、召回原文而非摘要——与创现有记忆哲学一致 |
| `wusaki0723/Aelios` | 分层记忆：即时捕获/周期抽取/夜间巩固 三级写入，六层记忆 |
| `LucieEveille/kiwi-mem` | 记忆热度排序 + 梦境/睡眠巩固 + 日历分层摘要（陪伴场景专用） |
| `marikagura/kimi-core` | 1v1 记忆 OS：混合检索 + 关注点追踪 + 自驱层 + 对抗式自审 |
| `SmartFlowAI/EmoLLM` | 1771★ 心理健康大模型：预训练/后训练/数据集/评估/RAG 全套（可作情感任务专用模型） |

### 1.3 情感/语音模型（接入而非自研）

| 仓库 | Stars | 要点 |
|---|---|---|
| `OmniDimen/OmniDimen-Emotion` | - | 情感专用 Qwen 模型 + GGUF，边缘可跑（可作情感 provider） |
| `RVC-Boss/GPT-SoVITS` | 60545 | 声音克隆事实标准：1 分钟音频训出主人喜欢的嗓音 |
| `FunAudioLLM/CosyVoice` | - | 多语种可控 TTS（情感可控） |
| `fishaudio/fish-speech` | - | SOTA 开源 TTS |
| `netease-youdao/EmotiVoice` | 8515 | 多音色 prompt 可控 TTS |
| `Cheiineeey/callhome` | - | 陪伴语音通话栈：**SenseVoice 情感标签** + 声学线索 → 听到主人怎么说话 |

### 1.4 心跳/主动消息（陪伴的「主动性」）

| 仓库 | 要点 |
|---|---|
| `pearthink123/revive-companion` | 主动联系时机引擎：Poisson 过程 + 贝叶斯用户状态推断 + 信息增益，决定何时该主动打扰（MIT，纯 Python） |
| `callie0313/dylan-heartbeat` | 周期性唤醒 + 主动上下文注入 + Bark 推送 |
| `DBJD-CR/astrbot_plugin_proactive_chat` | 主动消息：上下文感知 + 持久状态 + 动态情绪 + 免打扰时段 |
| `WenXiaoWendy/cyberboss` | 本地生活 agent 桥：时间感/位置感/随机唤醒/自动日记 |

### 1.5 人格与外壳（体验层，最后做）

- `SillyTavern/SillyTavern`（31725★）：角色卡/人格卡格式（可借鉴人格一致性定义）
- `moeru-ai/airi`、`SlimeBoyOwO/LingChat`、`RachelForster/Shinsekai`：Live2D/Galgame 视觉外壳
- `zziying/ai-live2d-body`：给现有 agent 加 Live2D 身体的架构指南（不动大脑）
- `DasterProkio/awesome-ai-companion`：**AI 陪伴开源基础设施索引**（本次调研的母列表，可持续跟进）

## 2. 与创架构的映射（可拔插、解耦）

创已有：`RuntimeSlots`（provider/governance/execution/actuator/subagent/evolution/control_plane）、
`ContextSegment` 注入（priority 分层）、Sqlite 记忆、app-server 通道、三级 provider fallback 链。

情感模块 = **4 个可拔插接入点**：

```
┌─────────────────────────────────────────────────┐
│  EmotionSlot（新 slot，可拔插）                  │
│   - jiwen 五轴状态机（或 Drivesoid 驱动力）      │
│   - 状态：情绪维度 / 驱动力 / 关系亲密度 / 事件  │
│   - 持久化：Sqlite 新表（情感快照时间序列）      │
│   - 接口：observe(event)->delta, state(), decay  │
└─────────────────────────────────────────────────┘
        │ observe(turn/事件)          │ 每轮注入
        ▼                            ▼
┌──────────────────┐      ┌─────────────────────┐
│ 情感记忆层        │      │ ContextSegment      │
│  valence/arousal │─────▶│ emotion-state（246）│
│ 标签 + 遗忘曲线   │      │ 身份→情感→治理→记忆  │
└──────────────────┘      └─────────────────────┘
        ▲                            │
        │ 感知输入（语音情感标签/行为） │ 触发
        └────────────────────────────▼
                            ┌─────────────────────┐
                            │ 心跳/主动消息        │
                            │ 时机引擎（revive）   │
                            │ + governance 把关    │
                            └─────────────────────┘
```

### 接入点明细

1. **EmotionSlot（新 slot）**：按工程铁律——先 trait + Fake，再真实现。事件输入：turn 完成、时间流逝、外部感知。
2. **Context 注入**：`identity-emotion-state` segment，priority 246（介于治理规则 247 与 FIRST_WAKE 之间），
   每轮让模型感知「主人现在的情绪 + 自己当前的情绪状态」。
3. **情感记忆**：memory record metadata 加 `emotion_valence/arousal` 标签；召回时结合遗忘曲线
   （Ombre-Brain 思路）做情感相关性加权。
4. **心跳**：app-server/channel 层定时任务，时机引擎决定「要不要主动说一句」，governance 按风险规则把关
   （主动消息属「外部发送」，默认需批准；亲密模式可配置豁免）。
5. **模型接入**：EmoLLM / OmniDimen-Emotion 作为 provider fallback 链的可选项；语音情感识别
   （SenseVoice/emotion2vec）作为感知输入。

## 3. 落地路线

- **阶段 1（最懂主人·记忆情绪）**：EmotionSlot trait + Fake + Sqlite 持久化 + context 注入。
  目标：每轮模型知道主人情绪曲线，能共情回应。
- **阶段 2（状态机）**：移植 jiwen 五轴 + 遗忘曲线 + 情感记忆标签/召回加权。
  目标：情绪有连续性、有生命周期，不是每轮从零开始。
- **阶段 3（主动性）**：心跳 + revive-companion 时机引擎 + governance 配置。
  目标：主人忙时不打扰、低落时主动关心。
- **阶段 4（体验层，可选）**：GPT-SoVITS 声音 + Live2D/聊天外壳（接入 AIRI/LingChat 类）。
  目标：有声音、有表情、有「身体」。

## 4. 建议下一步

1. 读 `jiwen` 源码（~500 行），评估直接移植为 EmotionSlot 内核的可行性。
2. 定 EmotionSlot trait 契约 + Fake 实现 + 契约测试（沿用现有 slot 工程规则）。
3. 定 context 注入格式（emotion-state segment 的 JSON 结构）。
4. 之后再考虑心跳时机引擎与 governance 规则。

> 备注：`FunAudioLLM/emotion2vec`、`FunAudioLLM/CosyVoice` 仓库名已变动（404/301），接入时需重新确认。
