# Context Engine 设计收口（小创版）

日期：2026-04-30
状态：设计收口，未实现

## 结论

DS-3 给的方向能用，但不能原样照抄进代码。

我这里收成一句话：**先在 runtime 和 recall 之间插一层独立 `ContextPacker`，第一版只做“结构化片段 + 确定性 budget + 可解释 dropped_ids”，不要一上来搞复杂语义排序。**

原因很直接：
- 现在本地主线已经有 `MemoryRecallPipeline -> AgentRuntime -> Responder`
- 缺的不是更多 recall，而是**把 recall / working / system 统一装箱**的中间层
- provider seam 已经有了，当前更该补的是 prompt 前的 context seam

---

## 当前代码现实边界

### 已有
- `src/memory_recall.rs`
  - `RecallRequest / RecallHit / RecallResult`
  - 当前输出还是 `summary + agent_input` 文本形态
- `src/agent_runtime.rs`
  - `RuntimeRequest { user_input, recall_limit, metadata }`
  - `RuntimeResult { prompt, response, recall_summary, recall_hit_count }`
- `src/responder.rs`
  - provider seam 已经抽出来了
  - responder 现在只吃一段最终 prompt

### 真缺口
- 没有统一的 `ContextSegment`
- 没有独立的 packing / trimming / budget merge 层
- dropped 了什么、为什么 dropped，目前 runtime 不可见
- recall 输出还太像“给模型直接吃的字符串”，不利于后续上下文治理

---

## 我认可的最小结构

### 1. 先加独立 context 模块
建议新增：`src/context_engine.rs`

第一版只放：
- `ContextSegment`
- `SegmentSource`
- `ContextBudget`
- `PackedContext`
- `ContextPacker`

### 2. segment source 先只保留 4 类
- `System`
- `Working`
- `Memory`
- `ToolResult`

够了。别再拆更多枚举，不然主线又散。

### 3. token 先允许延迟计算
`tokens: Option<u16>` 这个设计我认可。
因为现在本地还没有统一 tokenizer，第一版可以：
- 已知的就填
- 未知的走近似值或 lazy compute

但要补一个规则：
**packing 前必须把 `None` 补成可比较的实际数值**，不能一路带着 `None` 排序合并。

---

## 我收口后的执行顺序

固定为：

1. `collect`
2. `normalize_tokens`
3. `trim`
4. `rank`
5. `merge_under_budget`
6. `render_prompt`

这里我故意把 DeepSeek 说的 `Merge` 改成两段：
- 先 collect 原料
- 再 merge_under_budget

这样语义更清楚，也更适合写测试。

---

## 和现有主线怎么接

### 对 `memory_recall.rs`
不要硬改成只返回 `Vec<ContextSegment>`。

第一版更稳的做法：
- 保留 `RecallResult`
- 在里面新增一项：`segments: Vec<ContextSegment>`
- 原有 `summary / agent_input` 先别删

这样不会一下子把现有测试全打烂。

### 对 `agent_runtime.rs`
建议别发明什么 `RuntimeRequest::LoadContext` 枚举新层。

因为当前 `RuntimeRequest` 还是 struct，不是 command enum。
硬改边界会把现在最小 runtime 主线推倒重来。

更稳的 patch：
- `RuntimeRequest` 先加一个可选 `context_budget`
- `AgentRuntime::run()` 在 recall 后调用 `ContextPacker`
- `RuntimeResult` 新增：
  - `packed_context_preview`
  - `packed_token_count`
  - `dropped_segment_ids`

这就够第一版闭环了。

### 对 `responder.rs`
先不动接口。

原因：context engine 是 responder 之前的准备层，不该污染 provider seam。
只需要继续把最终拼好的 prompt 喂给 responder。

---

## 第一版必须立住的红测

1. `pack_rejects_when_system_budget_cannot_be_reserved`
2. `pack_keeps_system_segments_even_when_other_segments_are_dropped`
3. `pack_trims_tool_results_to_latest_n_before_rank`
4. `pack_orders_segments_by_priority_then_last_accessed_then_created_at`
5. `pack_restores_highest_priority_working_segment_when_budget_allows`
6. `runtime_exposes_dropped_segment_ids_after_context_pack`

如果只让我选最关键的 3 条，就是：
- system reservation 失败要报错
- dropped_ids 要可见
- working 至少保一条

---

## 最小实现顺序

1. `src/context_engine.rs`
2. `tests/context_engine_tests.rs`
3. `src/memory_recall.rs` 增加 `segments`
4. `tests/memory_recall_tests.rs` 补 recall -> segment 输出验证
5. `src/agent_runtime.rs` 接 `ContextPacker`
6. `tests/agent_runtime_tests.rs` / `tests/agent_runtime_sqlite_tests.rs` 补 packed context 可见性验证
7. 最后再决定要不要抽 `render_prompt()` 小函数

---

## 明确不做

第一版不做这些：
- 语义 embedding rank
- 自动摘要压缩
- 多级 memory lane
- provider 侧 token 精准计费联动
- BrowserWorker 上下文接入

这些都是真的，但现在做会抢主线。

---

## 下一步建议

直接开干 `context_engine`，别继续聊规格了。

最小落地方向：
- 先建 `src/context_engine.rs`
- 先立 5 条红测
- 再把 `agent_runtime` 接进去
