# 任务执行可追踪流程设计（Trace & Replay）

> 一次任务 = 一条 trace。关键运行时事件全量落盘（记录）、按因果链关联（追踪）、事后可重放核查（验证）。设计本身闭环：每个动作都有事件，每个事件都能被重放验证。

## 1. 验收标准

- 任意一次任务执行后，`chuang trace show <trace_id>` 能还原完整因果链：事件顺序、每个动作的入出摘要、耗时、断点（start 无 end、派工无回执、验收无证据）。
- `chuang trace replay <trace_id>` 能自动给出 PASS/FAIL 结论，并把不一致项（缺事件、断链、摘要不匹配）列成清单。
- FAIL 时，从事件日志即可定位具体事件与上下文，不靠印象复述。

## 2. 事件模型（统一信封）

每条事件是单行 JSON，追加写入，历史不可修改。

| 字段 | 说明 |
|---|---|
| schema_version | 事件结构版本，重放器兼容依据 |
| event_id | UUIDv7，全局唯一 |
| ts_ns | 单调纳秒时间戳 |
| trace_id | 一次任务一条 |
| span_id / parent_span_id | 因果树节点与父节点；无父则根 span |
| seq | trace 内全局单调递增序号，重放顺序依据 |
| kind | 事件类型（见下） |
| actor | agent / subagent / tool / human / runtime |
| status | started / ok / failed / approved / rejected |
| input_digest / output_digest | SHA-256 摘要，重放比对用 |
| payload_path | 大输出（diff/截图/日志）外置引用 |
| error | 失败时：kind + reason + context |
| duration_ms | 该 span 耗时（end 事件） |

事件类型清单：

| kind | 触发点 | 备注 |
|---|---|---|
| task_init | 任务接收 | intent、user_input_digest、channel |
| task_plan | 规划完成 | steps[]、expected_checks[] |
| tool_call_start / tool_call_end | 每次原子工具调用 | tool、action、args_digest、result_digest、duration_ms |
| subagent_dispatch / subagent_report | 派工/回收 | task、policy、report_ref、verdict |
| policy_decision | 权限判定/询问/拒绝 | decision、reason、rule_ref |
| human_approval | 人工闸门 | approver、decision、reason |
| checkpoint | 闭环验收点 | measure、evidence_ref（必达） |
| task_complete / task_fail | 任务结束 | summary、trace_stats（事件数/时长/断点数） |

## 3. 存储布局

```
data/runtime-events/
  <trace_id>.jsonl              # 追加式事件流（事实源）
  <trace_id>.replay.json        # 最近一次重放核查报告
  <trace_id>/assets/<seq>.out   # 大输出外置（事件内只有 digest + 引用）
```

- sqlite 索引表 runtime_events：按 trace_id/ts/kind 查询；索引可由 JSONL 随时重建（JSONL 是事实源，索引只是视图）。
- secret 值一律不进事件体；需对比时只存 digest。

## 4. 追踪（Track）

- `chuang trace list`：任务清单（trace_id、时间、状态、事件数）。
- `chuang trace show <trace_id>`：树形因果链；标断点（start 无 end / 派工无回执 / checkpoint 无证据 / failed 无 error）。
- `chuang trace tail [trace_id]`：实时跟随事件流。
- 排查约定：工具回执异常先 trace show 找断点，再定位具体事件。

## 5. 重放核查（Replay）

默认先跑 audit：

1. **audit（结构核查，不重执行）**
   - seq 严格单调递增，无空洞；
   - 每个 *_start 有对应 *_end；
   - 每个 subagent_dispatch 有 subagent_report；
   - 每个 checkpoint 有 evidence_ref 且引用的证据文件存在；
   - task_complete 前存在至少一个 checkpoint；
   - 所有 failed 事件带 error；
   - 不存在「声称完成但无验收事件」。
   - 输出：PASS / FAIL + 缺失与断裂清单。
2. **deterministic（确定性重放）**
   - 按 seq 重放 tool_call（只读类优先），比对 output_digest；
   - 不一致处标记回归点：事件 id + 预期/实际摘要。

重放报告写回 `<trace_id>.replay.json`，可留档比对。

## 6. 任务执行流程

```
task_init（接收：输入摘要+意图+通道）
   ↓
task_plan（规划：步骤 + 预期验收点）
   ↓
执行循环（每步一个 span）：
  tool_call_start → 执行 → tool_call_end
  或 policy_decision（权限/询问/拒绝）
  或 subagent_dispatch → subagent_report（派工回执）
  或 human_approval（人工闸门 approved/rejected）
   ↓
checkpoint（闭环验收：measure + evidence_ref，必达）
   ↓
task_complete / task_fail（summary + trace_stats）
   ↓
事后：chuang trace show 呈现因果链；chuang trace replay 出 PASS/FAIL。
```

## 7. 约束

- 事件日志只追加、不修改、不删除；业务回滚不得抹除日志。
- 大输出不嵌入事件体，走 payload_path + digest。
- 密钥/验证码/真实凭据只存 digest 或直接不落盘。
- 重放核查不替代验收；replay 是「claim ↔ evidence」证据链的一部分。

## 8. 示例事件

```json
{"schema_version":1,"event_id":"01JXK3...","ts_ns":1726200000000000000,"seq":7,
 "trace_id":"t_9f3a","span_id":"s_7","parent_span_id":"s_4",
 "kind":"tool_call_end","actor":"agent","tool":"code_execute","status":"ok",
 "input_digest":"sha256:...","output_digest":"sha256:...",
 "payload_path":"data/runtime-events/t_9f3a/assets/0007.out","duration_ms":842}
```

## 9. 落地顺序

1. 第一刀：事件信封 + JSONL 写入器 + trace show（并入 runtime 主链，命令执行即产事件）。
2. 第二刀：replay audit（结构核查）。
3. 第三刀：deterministic 重放 + digest 比对。
