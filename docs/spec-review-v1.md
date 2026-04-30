# 1. SubagentReport

## 目标
子代理执行后向主控提交一个不可变的结构化报告，用于：
- 判断任务是否完成
- 归档资源消耗与产物
- 支持失败审计、重放与后续调度

## 核心字段/状态
```rust
struct SubagentReport {
    report_id: String,
    task_id: String,
    agent_id: String,
    parent_agent_id: Option<String>,
    status: ReportStatus,
    started_at: Timestamp,
    finished_at: Timestamp,
    exit_code: Option<i32>,
    summary: String,
    stdout_preview: String,
    stderr_preview: String,
    resource_usage: ResourceUsage,
    artifacts: Vec<ArtifactRef>,
    truncated: bool,
    replay_ref: Option<String>,
}

enum ReportStatus {
    Success,
    Failed,
    Cancelled,
    TimedOut,
    Rejected,
}

struct ResourceUsage {
    cpu_ms: u64,
    wall_ms: u64,
    peak_mem_bytes: u64,
    read_bytes: u64,
    write_bytes: u64,
}

struct ArtifactRef {
    path: String,
    kind: ArtifactKind,
    checksum: Option<String>,
    size_bytes: u64,
}

enum ArtifactKind {
    File,
    Diff,
    Log,
    Plan,
    Other,
}
```

## 输入
- 子代理启动元数据：task_id、agent_id、父任务信息、工作目录
- 子代理结束信号：退出码、取消原因、超时信息
- 执行输出：stdout / stderr / structured logs
- 资源统计：CPU、内存、I/O、持续时间
- 产物清单：文件、diff、日志、计划等

## 输出
- 一个持久化的 `SubagentReport`
- 一个可被主控消费的受理结果：`accepted / rejected`
- 一个可选的 replay 引用（日志路径或 trace id）

## 约束
- 报告必须是**不可变快照**，写入后不得原地修改
- 报告体应有上限，例如 1 MiB；超出则截断并标记 `truncated=true`
- 预览字段不得携带明文密钥、token、cookie
- `summary` 必须是主控可直接展示给上层的简要结论，而不是原始日志拼接

## 失败模式
- 子代理崩溃，未主动上报 → 主控补写占位失败报告
- 日志过大 → 截断预览并保留 replay_ref
- 产物路径失效 → 报告保留 artifact 元数据，但标记为 dangling
- 脱敏失败 → 回退为仅保留结构化元数据，不暴露原始文本

## 最小验证方式
1. 构造一个成功子代理，验证：`status=Success`、`artifacts` 非空、`resource_usage.wall_ms > 0`
2. 构造一个 panic/异常退出子代理，验证：`status=Failed` 且 `stderr_preview` 有错误片段
3. 构造超大 stdout，验证：`truncated=true`
4. 构造取消流程，验证：`status=Cancelled` 且 `exit_code=None`

---

# 2. MemoryAdmissionPolicy

## 目标
在主控准备启动新任务或子代理前，判断是否允许其占用上下文/内存预算，避免系统因超分配而退化、抖动或 OOM。

## 核心字段/状态
```rust
struct MemoryAdmissionPolicy {
    total_budget_bytes: u64,
    reserved_system_bytes: u64,
    active_allocations: Vec<AllocationRecord>,
    mode: AdmissionMode,
    max_agents: u32,
}

struct AllocationRecord {
    agent_id: String,
    task_id: String,
    allocated_bytes: u64,
    priority: Priority,
    reclaimable: bool,
}

enum AdmissionMode {
    HardLimit,
    SoftLimit,
    SoftLimitWithEviction,
}

enum Priority {
    Critical,
    High,
    Normal,
    Low,
}

struct AdmissionRequest {
    agent_id: String,
    task_id: String,
    requested_bytes: u64,
    priority: Priority,
    reclaimable: bool,
}

enum AdmissionDecision {
    Allow { granted_bytes: u64 },
    Degrade { granted_bytes: u64, evict: Vec<String> },
    Deny { reason: DenyReason },
}

enum DenyReason {
    BudgetExceeded,
    AgentLimitExceeded,
    SystemReserveViolation,
    InvalidRequest,
}
```

## 输入
- `AdmissionRequest`
- 当前活动分配视图
- 全局剩余预算与系统保留值
- 当前活跃 agent 数量

## 输出
- `AdmissionDecision`
- 一条结构化决策日志，说明允许/降级/拒绝原因

## 约束
- `reserved_system_bytes` 必须始终保留，不可被业务任务侵占
- 决策必须是纯函数风格：相同输入得到相同输出
- 决策耗时应短，目标 < 1ms
- `requested_bytes == 0` 视为非法请求，而不是自动放行
- `Critical` 任务只能被 `Critical` 或显式系统策略抢占

## 失败模式
- 活动分配账本损坏 → 回退为保守模式：仅允许 Critical
- 实时可用内存读数缺失 → 使用最近一次稳定快照，否则默认拒绝
- eviction 候选为空但必须回收 → 返回 `Deny`
- 请求值异常（负数/溢出/0） → 返回 `InvalidRequest`

## 最小验证方式
1. `HardLimit` 下：预算 100，已分配 95，请求 10 → `Deny(BudgetExceeded)`
2. `SoftLimitWithEviction` 下：低优先级占满，来一个 `High` 请求 → `Degrade` 或驱逐低优先级
3. 构造 `requested_bytes = 0` → `Deny(InvalidRequest)`
4. 并发 10 个相同请求，验证决策一致且无账本破坏

---

# 3. ContextEngineLifecycle

## 目标
管理上下文引擎的状态机，使其在启动、运行、检查点、暂停、恢复、停止和故障恢复时都具有可验证的行为边界。

## 核心字段/状态
```rust
enum ContextEngineState {
    Uninitialized,
    Starting,
    Running,
    Checkpointing,
    Pausing,
    Paused,
    Draining,
    Stopped,
    Failed,
}

struct ContextEngineLifecycle {
    state: ContextEngineState,
    started_at: Option<Timestamp>,
    last_checkpoint_id: Option<String>,
    pending_ops: u32,
    last_error: Option<String>,
    auto_restart_count: u8,
}

enum ControlCommand {
    Start,
    Pause,
    Resume,
    Checkpoint,
    Drain,
    Stop,
    Restart,
}

struct LifecycleEvent {
    from: ContextEngineState,
    to: ContextEngineState,
    reason: String,
    at: Timestamp,
}
```

## 输入
- 外部控制命令：`Start / Pause / Resume / Checkpoint / Drain / Stop / Restart`
- 引擎内部事件：初始化完成、checkpoint 完成、健康检查失败、I/O 超时
- 活跃请求变化：pending_ops 增减

## 输出
- 状态迁移事件 `LifecycleEvent`
- 对控制命令的确认结果：accepted / rejected
- 可选的 checkpoint id

## 约束
- 必须遵守有限状态机，禁止非法跳转
  - 例如 `Paused -> Draining` 非法，必须先 `Resume -> Running -> Drain`
- `Checkpointing` 期间可选择阻塞写请求，但策略必须一致
- `Failed` 后只能进入 `Restarting/Starting` 或 `Stopped` 路径，不能直接伪装回 `Running`
- `Draining` 期间不得接受新写入型上下文请求

## 失败模式
- 启动超时 → `Starting -> Failed`
- checkpoint 存储失败 → 保留旧 checkpoint，写入错误事件，可回到 `Running` 或进入 `Failed`，但规则必须固定
- 健康检查连续失败 → `Running -> Failed` 或 `Running -> Paused`，不能模糊处理
- `Draining` 长时间不结束 → 升级为 `Failed` 或强制 `Stopped`

## 最小验证方式
1. `Start` 流程：`Uninitialized -> Starting -> Running`
2. `Pause/Resume` 流程：`Running -> Pausing -> Paused -> Running`
3. `Checkpoint` 流程：`Running -> Checkpointing -> Running`，并产出 `last_checkpoint_id`
4. 启动超时测试：`Starting -> Failed`
5. `Draining` 时发送新写请求，验证被拒绝

---

# 关键设计权衡
1. `SubagentReport` 选择“不可变快照”而不是可追加日志对象，优先保证审计一致性。
2. `MemoryAdmissionPolicy` 默认应偏保守，宁可拒绝也不要把主控拖进 OOM。
3. `ContextEngineLifecycle` 要显式建模 `Checkpointing / Draining / Failed`，否则边界行为会被隐藏到代码分支里。
4. 报告中的原始输出只保留 preview，完整日志交给 replay_ref，避免报告本身膨胀。
5. `SoftLimitWithEviction` 很强，但必须配合优先级和可回收标记，否则会产生不可解释的抢占。
6. 生命周期状态越细，测试负担越大；但如果状态过粗，会让恢复逻辑不可验证。
7. 三个对象都应优先输出结构化日志，方便后续做回放、统计与策略调优。
8. 默认先把“能稳定拒绝/降级/失败”设计清楚，再追求高吞吐与复杂自恢复。
