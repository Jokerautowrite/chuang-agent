# DeepSeek DS-3：Context Engine 规格草案（收回原文）

来源：2026-04-30 通过 opencli + 可见 Chrome 向 DeepSeek 派发 DS-3 任务后收回。
注意：这是网页外脑产物的本地收档，不代表本地代码已实现或已验证。

## A. 结论

上下文引擎下一阶段（v0.2）的核心是提供**确定性 budget 管理 + 可插拔 trim/rank 策略**，与现有 `MemoryRecallPipeline` 解耦但衔接，通过 `RuntimeRequest::LoadContext` 和 `RuntimeResult::Context` 定义边界。第一版实现以**固定优先级 + 滑动窗口 trim**为主，后续可扩展为语义 rank。

## B. 最小数据结构

```rust
pub struct ContextSegment {
    pub id: String,
    pub source: SegmentSource,
    pub content: String,
    pub tokens: Option<u16>,
    pub priority: u8,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub last_accessed: chrono::DateTime<chrono::Utc>,
    pub metadata: std::collections::HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SegmentSource {
    Memory,
    Working,
    ToolResult,
    System,
}

pub struct ContextBudget {
    pub max_tokens: u16,
    pub reserve_system_tokens: u16,
    pub min_working_tokens: u16,
}

pub struct PackedContext {
    pub segments: Vec<ContextSegment>,
    pub total_tokens: u16,
    pub dropped_ids: Vec<String>,
    pub budget_exceeded: bool,
}
```

## C. 流程顺序

单次 context packing 执行顺序（不可颠倒）：

1. **Merge**
   - 合并 `MemoryRecallPipeline` 输出的 `Vec<ContextSegment>`（`source=Memory`）
   - 合并 `AgentRuntime` 持有的 Working 片段
   - 合并固定 System 片段
2. **Trim（预裁剪）**
   - 保护所有 `System` 与 `priority >= 240` 的片段
   - 对 `ToolResult` 按 `created_at` 保留最新 N 条（默认 5）
   - 对 `Memory` 按 `last_accessed` 保留最近 M 条（默认 20）
3. **Rank（排序）**
   - 主键：`priority` 降序
   - 次键：`last_accessed` 降序
   - 三键：`created_at` 降序
4. **Budget Merge**
   - 遍历排序后列表，累加 `tokens`
   - 超过 `max_tokens` 即停止，其余写入 `dropped_ids`
   - 若未保留任何 `Working` 且仍有空间，强制拉回优先级最高的 `Working`
5. **后校验**
   - 若 `reserve_system_tokens` 不足，报错，让调用方加 budget

## D. 边界设计

### 1. 与 `MemoryRecallPipeline` 的衔接点

- `MemoryRecallPipeline` 输出类型改为 `Vec<ContextSegment>`，而不是纯文本
- Pipeline 内部为每个 recall hit 补：
  - `id`
  - `source = SegmentSource::Memory`
  - `tokens`（可延迟估算）
  - `priority`（由 recall score 映射）
- `AgentRuntime` 在调用 `ContextPacker::pack()` 时，把 recall segments 当作 Memory 输入

### 2. 与 `RuntimeRequest / RuntimeResult` 的边界

- 建议新增显式 load-context 请求/结果边界
- 让 runtime 在“检索”和“最终 prompt 拼装”之间插入独立 context engine
- 让结果里保留 packed context 的摘要、token 用量和 dropped 片段信息，方便 trace 与后续调试

## E. 红测清单

> 这部分网页回收内容在本次 opencli 快照里没有完整抓全，下面是基于已收回结构和当前本地主线补出的最小红测建议，后续以本地实现为准。

1. `pack_rejects_when_system_reservation_exceeds_budget`
2. `pack_preserves_high_priority_system_segments`
3. `pack_trims_tool_results_to_latest_n`
4. `pack_restores_one_working_segment_when_all_dropped`
5. `pack_orders_by_priority_then_last_accessed_then_created_at`
6. `pack_records_dropped_ids_when_budget_runs_out`

## F. 最小 patch 方案

> 这部分网页回收内容同样未完整抓全，下面按当前本地代码结构收口成可落地 patch 顺序。

1. 新增 `src/context_engine.rs`
   - `ContextSegment`
   - `SegmentSource`
   - `ContextBudget`
   - `PackedContext`
   - `ContextPacker`
2. 新增 `tests/context_engine_tests.rs`
   - 先把上面的 5~6 条红测立住
3. 改 `src/memory_recall.rs`
   - 在 `RecallHit / RecallResult` 旁增加可转 `ContextSegment` 的输出
   - 先别删已有 `agent_input`，走兼容演进
4. 改 `src/agent_runtime.rs`
   - 在 recall 和 responder 之间插入 `ContextPacker`
   - 让 `RuntimeResult` 带最小 `context_summary / packed_token_count / dropped_segment_ids`
5. 跑最小验证
   - `cargo test --test context_engine_tests`
   - `cargo test --test memory_recall_tests`
   - `cargo test --test agent_runtime_tests`
   - 最后 `cargo test`
