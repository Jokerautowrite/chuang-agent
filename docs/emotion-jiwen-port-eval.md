# 备选 1：jiwen（积温）移植评估 v1（2026-08-06）

> 源码：`github.com/ClaraShafiq/jiwen`（MIT，核心 jiwen.js ~500 行，29 项测试）

## 为什么选它做实验 1

| 维度 | 评估 |
|---|---|
| 资源 | 纯逻辑零依赖，本机无压力（不需要 GPU/大模型） |
| 可拔插 | 持久化/消息源/LLM 分析全部回调注入，**无状态存储，天然可拔插** |
| 情绪模型 | 五轴 = connection/pride/valence/arousal/immersion；valence/arousal 即 Russell 环状模型，与 Ombre-Brain 情感记忆天然兼容 |
| 主动性 | 阈值触发 contact / find_activity / observation，确定性（不靠骰子） |
| 成本分层 | 数学漂移（0 模型）→ 对话分析（轻量模型，只出 delta）→ 行动生成（大模型，触发时） |
| 协议 | MIT，可商用 |

## 接口摘要（JS 原版）

```
createJiwen(opts) -> engine
  opts: axes / rates / thresholds / persona
        onSave(state)      // 持久化回调（创用 Sqlite 接）
        onLoad() -> state  // 加载回调
        getLastMessage()   // 消息源（创的 turn/会话接）
engine.tick(minutes) -> [{action: contact|find_activity|observation, urgency}]
engine.applyDelta({pride, valence, arousal, connection, mood})  // 对话情绪注入
engine.getState() -> 五轴快照
engine.getPromptContext() / getStyleGuidance() -> 人话描述（塞 prompt）
engine.setActivity(type, label) / resetConnection() / checkThresholds()
```

## 三层成本模型（原设计，直接沿用）

| 层级 | 做什么 | 用什么 | 频率 |
|---|---|---|---|
| 数学漂移 | 五轴随时间变化 | 不需要模型 | 每 5 分钟 |
| 对话分析 | 读对话提取情绪 delta | 轻量模型（如现有 deepseek-v4-flash）或规则 | 有新对话 |
| 行动生成 | 生成开口内容/行为 | 大模型 | 阈值触发时 |

## 移植方案（贴合创的铁律）

1. **接口先行**：定义 `EmotionSlot` trait（Rust），先 Fake 后真实，契约测试。
2. **真实实现 A（规则版）**：把五轴数学直接翻译成 Rust 纯逻辑（无外部依赖），
   对话 delta 先用规则提取（关键词/情感词表）或留给后续接模型。→ 本机立即可跑。
3. **真实实现 B（模型版，可选）**：对话分析接现有 provider fallback 链（deepseek-v4-flash
   只出几个 delta 值），行动生成接大模型。
4. **持久化**：Sqlite 新表（emotion_state 时间序列 + 快照）。
5. **Context 注入**：`emotion-state` segment（每轮把 getPromptContext 结果注入模型上下文）。
6. **心跳**：app-server/channel 层定时 tick，触发 contact 时按 governance 规则决定是否主动发送。

## 风险

- 五轴参数多（30+），移植后需要针对「创的人格」校准（可用原版 simulate.js 跑参数轨迹）。
- 行动生成层（contact 说什么）依赖大模型质量，前期可以先只做状态 + prompt 注入，
  主动消息默认关闭（governance 把关）。
