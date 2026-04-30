| response |
| --- |
| 已思考（用时 3 秒）

我们按照任务要求输出V3冻结稿，只做三件事，且不重复V2大段内容。需要：

SubagentReport增加schema version / required fields / optional fields / serialization format约定。

MemoryAdmissionPolicy明确账本更新时机（准入前预占、启动成功确认、异常退出回收、驱逐原子性）。

ContextEngineLifecycle输出command × state真值表，标明accept/reject/noop/defer。

直接输出文档正文，中文，结构化，工程化。

创项目规格 V3（冻结稿）
1. SubagentReport 补充约定
1.1 Schema Version

每个 SubagentReport 必须携带 schema_version: SemVer，初始版本 "1.0.0"。

主控读取时校验主版本号，主版本不匹配必须拒绝受理并记录 RejectedMalformed。

1.2 Required Fields（必须存在，不可为 null）
字段	类型
report_id	String
task_id	String
agent_id	String
status	ExecutionStatus
started_at	Timestamp
finished_at	Timestamp
summary	String
resource_usage	ResourceUsage
artifacts	Vec<ArtifactRef>
truncated	bool
1.3 Optional Fields（可为空或缺失）
字段	默认缺失语义
parent_agent_id	无父代理
exit_code	未提供或不适用（如 Cancelled）
stdout_preview	无标准输出
stderr_preview	无标准错误
replay_ref	无可重放日志引用
1.4 Serialization Format

传输格式：JSON（UTF-8）

持久化格式：JSON Lines 或 Parquet（按需），但单条记录必须可反序列为同构 JSON。

序列化规则：

Timestamp：RFC 3339 毫秒精度，"2026-04-30T10:30:00.123Z"

Enum：序列化为字符串字面量（"Success" 而非 0）

可选字段缺失时直接省略 key，不写 null

resource_usage 所有字段为 u64，缺失则写 0

artifacts 空列表序列化为 []

大小限制：

单份序列化报告 ≤ 1 MiB

超出时截断 stdout_preview/stderr_preview 至总 payload ≤ 1 MiB，设置 truncated=true

若截断后仍超限，拒绝生成报告 → 主控补写失败报告（status=Failed，原因 ReportOversize）

2. MemoryAdmissionPolicy 账本更新时机

所有账本操作必须满足原子性：要么全部更新，要么全部回滚，不允许部分提交。

2.1 准入前预占（Reserve Before Admission）

时机：收到 AdmissionRequest，执行决策逻辑之前。

动作：

计算剩余预算：available = total_budget_bytes - reserved_system_bytes - sum(active_allocations.allocated_bytes)

若 requested_bytes ≤ available（或满足驱逐条件），先创建临时预占记录（status=Reserved，不进入 active_allocations）。

失败处理：预占失败 → 直接返回 Deny，不修改账本。

2.2 启动成功确认（Commit on Start Success）

时机：AdmissionDecision = Allow 且子代理/任务进程实际启动成功（非 enqueue，非 pending）。

动作：

将预占记录转为正式分配：移入 active_allocations，标记 allocated_bytes = granted_bytes

若启动失败（如 fork 失败），立即释放预占，不产生正式记录。

约束：预占记录超时未确认（默认 5s）则自动释放，主控可重试请求。

2.3 异常退出回收（Reclaim on Abnormal Exit）

时机：

子代理/任务状态变为 Failed, Cancelled, TimedOut, Rejected（执行后）

主控检测到进程消失、心跳超时、健康检查失败

动作：

从 active_allocations 中删除对应 AllocationRecord

释放 allocated_bytes 回全局预算

记录回收事件日志（含 agent_id, freed_bytes, reason）

幂等性：同一 agent_id 重复回收无副作用。

2.4 驱逐原子性（Atomic Eviction）

适用模式：SoftLimitWithEviction

时机：决策返回 Degrade { granted_bytes, evict: Vec<String> } 时

原子性要求：

必须一次性从 active_allocations 中移除所有 evict 列表中的 agent

计算释放字节总和 freed_bytes_sum

再为新请求添加正式分配 granted_bytes

整个操作在单个事务或锁区间内完成，不允许部分驱逐

失败回滚：若驱逐任何一个 agent 失败（如记录不存在、被其他事务锁定），回滚全部驱逐和新分配，返回 Deny(BudgetExceeded)

日志：驱逐前后必须输出结构化日志，包含被驱逐 agent_id、优先级、释放字节、新分配字节

2.5 账本更新时序图（关键路径）
text
复制
下载
Request → Pre‑Reserve → Launch Agent → Start Success → Commit
                                  ↓
                              Start Fail → Release Pre‑Reserve
                                  
Running → Abnormal Exit → Reclaim → Free Budget

Request（带驱逐）→ Pre‑Reserve → Atomic Eviction + Commit（全成功或全失败）
3. ContextEngineLifecycle 命令×状态真值表
3.1 真值表说明
符号	含义
accept	命令被接受，状态迁移开始（进入中间态如 Starting/Pausing，最终到达目标态）
reject	命令被拒绝，状态不变，记录错误日志
noop	命令无操作，状态不变，无副作用（可能记录 info）
defer	当前状态暂时无法处理，放入待处理队列，稍后自动重试或等到状态变更后再处理
3.2 Command × State 真值表
Command ↓ \ State →	Uninit	Starting	Running	Checkpointing	Pausing	Paused	Draining	Restarting	Stopped	Failed
Start	accept	reject	noop	reject	reject	noop	reject	defer	accept	defer
Pause	reject	reject	accept	reject	reject	noop	reject	reject	noop	reject
Resume	reject	defer	noop	reject	defer	accept	reject	reject	noop	reject
Checkpoint	reject	reject	accept	reject	reject	reject	defer	reject	noop	reject
Drain	reject	reject	accept	defer	reject	noop	reject	reject	noop	reject
Stop	accept	defer	accept	defer	accept	accept	accept	defer	noop	accept
Restart	defer	reject	accept	reject	reject	accept	reject	reject	accept	accept
3.3 真值表解读约束

defer 必须实现队列：每个命令 defer 后应在状态离开当前值后重新评估（如 Failed 下的 Start 或 Restart 可先 defer 再立即处理，但明确不进入中间态）

defer 超时：任何 defer 超过 30s 未处理，转为 reject 并记录错误

并发命令：同一时刻只处理一个命令；其余到达的直接 reject（BusyReject），除非命令定义支持幂等（如 Stop 从 Stopped 视为 noop）

Failed 的恢复路径：仅 Restart 或 Start（若 Restart 不可用时）可离开 Failed，两者都先进入 Restarting → Starting，禁止直接到 Running |
