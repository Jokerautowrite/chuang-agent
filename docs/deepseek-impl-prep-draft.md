| response |
| --- |
| 已思考（用时 6 秒）

我们根据spec-v3，输出三个文件：Rust trait草案，文件切分建议，测试清单。注意不要重复大段定义，要结构化、工程化，直接可用。

先梳理涉及的核心组件：

SubagentReport: schema_version, required/optional, serialization, 大小限制等。对应trait可能是序列化/验证相关。

MemoryAdmissionPolicy: 账本操作，预占、确认、回收、驱逐等。对应trait可能是BudgetManager。

ContextEngineLifecycle: command×state真值表，状态机。对应trait可能是LifecycleStateMachine。

输出格式明确：# 1. Rust trait草案 里面给出trait定义及方法签名，可能需要关联类型或结构。注意遵循spec的原子性、幂等性等。

2. 文件切分建议：建议目录结构，struct/enum放在哪些模块，包括序列化、策略、状态机等。
3. 测试清单：针对状态机与准入策略的测试清单，需要具体测试点。

注意：不要重复spec-v3大段定义，但可以引用。

开始输出。

1. Rust trait 草案
rust
复制
下载
// ========== 1.1 SubagentReport 相关的核心 trait ==========
// 模块: subagent_report::validation

/// 报告校验器：处理 schema 版本、必填字段、大小限制等。
pub trait ReportValidator {
    type Report: Serialize + DeserializeOwned;

    /// 校验报告整体（版本、必填字段、大小等）
    /// 返回 Ok(()) 表示可接受；Err(ReportRejectReason) 表示拒绝
    fn validate(&self, raw: &[u8]) -> Result<(), ReportRejectReason>;

    /// 校验特定字段的 optional 缺失语义（应用层可覆盖）
    fn apply_optional_defaults(&self, report: &mut Self::Report);
}

/// 报告拒绝原因（主控侧）
#[derive(Debug, PartialEq)]
pub enum ReportRejectReason {
    UnsupportedSchemaVersion { required: String, current: String },
    MissingRequiredField { field: &'static str },
    InvalidEnumFormat { field: &'static str, found: String },
    SizeLimitExceeded { limit_bytes: usize, actual: usize },
    TruncationFailed { after_truncate: usize },
}

/// 报告生成器（子代理侧）：负责截断、truncated 标记、大小限制。
pub trait ReportBuilder {
    fn build(self) -> SubagentReport;
    fn truncate_previews(self, max_total_bytes: usize) -> Self;
}

// ========== 1.2 MemoryAdmissionPolicy 相关 trait ==========
// 模块: memory_policy::admission

/// 预算管理器核心接口：预占、确认、回收、准入+驱逐的原子操作。
pub trait BudgetManager: Send + Sync {
    /// 预占（Pre-Reserve）：返回预占 token，不修改正式账本。
    /// 预占需带 TTL，调用者应在启动成功后调用 commit()。
    fn try_reserve(&mut self, request: &AdmissionRequest) -> Result<ReservationToken, DenyReason>;

    /// 启动成功确认：将预占转为正式分配。
    /// 若 token 无效或已过期，返回错误但不崩溃。
    fn commit(&mut self, token: ReservationToken) -> Result<AllocationId, CommitError>;

    /// 启动失败释放预占（或 TTL 到期自动释放）。
    fn release_reservation(&mut self, token: ReservationToken);

    /// 异常退出回收（幂等）。
    fn reclaim(&mut self, task_id: &TaskId, agent_id: &AgentId) -> Result<FreedBytes, ReclaimError>;

    /// 带驱逐的准入决策与原子执行（SoftLimitWithEviction 模式）。
    /// 要求：原子性检查所有 evict 候选 -> 一次性驱逐并提交新分配。
    fn admit_with_eviction(
        &mut self,
        request: &AdmissionRequest,
        evict_candidates: &[AllocationId],
    ) -> Result<AdmissionDecision, DenyReason>;

    /// 查询当前正式分配快照（用于驱逐候选排序）
    fn active_allocations(&self) -> Vec<ActiveAllocation>;

    /// 获取总预算与系统保留量（只读）
    fn budget_config(&self) -> BudgetConfig;
}

/// 预占 token：包含 TTL、请求量、唯一标识。
#[derive(Clone)]
pub struct ReservationToken {
    pub id: String,
    pub granted_bytes: u64,
    pub expires_at: Timestamp,
    // 内部不可克隆的状态（如预占锁）通过 Arc<Mutex> 封装，此处只暴露 ID
}

/// 准入请求（来自调度器）
pub struct AdmissionRequest {
    pub task_id: TaskId,
    pub agent_id: AgentId,
    pub requested_bytes: u64,
    pub priority: Priority,
    pub eviction_policy: EvictionPolicy, // SoftLimitWithEviction / HardLimit
}

/// 准入决策（成功返回）
pub enum AdmissionDecision {
    /// 直接授予，无需驱逐
    Granted { granted_bytes: u64, allocation_id: AllocationId },
    /// 降级授予 + 需要驱逐哪些 allocation
    Degrade { granted_bytes: u64, evicted: Vec<AllocationId> },
}

/// 拒绝原因
pub enum DenyReason {
    BudgetExceeded { available: u64, requested: u64 },
    EvictionCandidateInvalid { reason: String },
    AtomicCommitFailed,
    ReservationExpired,
}

/// 回收结果
pub struct FreedBytes(pub u64);

// ========== 1.3 ContextEngineLifecycle 相关 trait ==========
// 模块: lifecycle::state_machine

/// 生命周期状态机：封装命令处理、defer 队列、超时转换。
pub trait LifecycleStateMachine {
    type Command;
    type Error;

    /// 处理外部命令，依据真值表返回动作。
    /// 接收当前状态（内部不可变借用），返回 (next_state, effect)。
    fn handle_command(
        &mut self,
        command: Self::Command,
    ) -> Result<CommandEffect<Self::Command>, CommandRejectReason>;

    /// 获取当前状态（只读）
    fn current_state(&self) -> LifecycleState;

    /// 驱动 defer 队列：应周期性调用（如 100ms），检查超时或状态是否允许重试。
    /// 返回被重新激活的命令及新的 effect。
    fn drive_deferred(&mut self) -> Vec<CommandEffect<Self::Command>>;
}

/// 命令处理结果
pub enum CommandEffect<Cmd> {
    /// 立即接受，状态迁移
    Accepted { next_state: LifecycleState },
    /// 拒绝，状态不变
    Rejected { reason: String },
    /// 无操作（状态不变，但幂等接受）
    Noop,
    /// 暂缓，进入 defer 队列
    Deferred { command: Cmd, inserted_at: Timestamp },
}

/// 状态定义（与真值表一致）
#[derive(Debug, Clone, PartialEq)]
pub enum LifecycleState {
    Uninitialized,
    Starting,
    Running,
    Checkpointing,
    Pausing,
    Paused,
    Draining,
    Restarting,
    Stopped,
    Failed,
}

/// 命令定义（与真值表一致）
#[derive(Debug, Clone)]
pub enum LifecycleCommand {
    Start,
    Pause,
    Resume,
    Checkpoint,
    Drain,
    Stop,
    Restart,
}

/// 拒绝原因
pub enum CommandRejectReason {
    InvalidState { current: LifecycleState, expected_states: Vec<LifecycleState> },
    TimeoutDeferred { command: LifecycleCommand, elapsed_ms: u64 },
    ConcurrencyLocked,
}

// ========== 1.4 辅助 trait：可审计、幂等 ==========
// 模块: common::audit

/// 要求所有账本操作实现审计日志。
pub trait Auditable {
    fn audit_log(&self) -> AuditRecord;
}

pub struct AuditRecord {
    pub operation: String,
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub delta_bytes: i64,
    pub reason: String,
    pub timestamp: Timestamp,
}

/// 幂等操作标记：标识通过 (task_id, agent_id) 可去重。
pub trait IdempotentKey {
    fn idempotency_key(&self) -> String;
}
2. 文件切分建议
text
复制
下载
src/
├── lib.rs
├── subagent_report/
│   ├── mod.rs                 // 导出 report 结构、validator trait、builder
│   ├── schema.rs              // SubagentReport struct, ExecutionStatus, ResourceUsage
│   ├── validation.rs          // ReportValidator 实现 + 必填字段检查
│   ├── serialization.rs       // JSON/JSONL/Parquet 序列化规则（Timestamp, enum 字符串）
│   └── size_limit.rs          // 截断逻辑、1MiB 强制限制
│
├── memory_policy/
│   ├── mod.rs                 // 导出 BudgetManager trait
│   ├── budget.rs              // BudgetConfig, 账本结构（active_allocations, reserved_system_bytes）
│   ├── reservation.rs         // ReservationToken, 预占 TTL 管理（建议用 dashmap + 后台清理）
│   ├── commit_reclaim.rs      // commit / release_reservation / reclaim 实现
│   ├── eviction.rs            // admit_with_eviction 原子驱逐实现（锁区间或事务）
│   └── audit_log.rs           // 审计日志实现（结构化输出）
│
├── lifecycle/
│   ├── mod.rs                 // 导出状态机 trait 及主要类型
│   ├── state.rs               // LifecycleState, LifecycleCommand enum
│   ├── transition.rs          // 真值表实现：matches! 或 lookup table，处理 accept/reject/defer/noop
│   ├── defer_queue.rs         // 带超时的 defer 队列（BinaryHeap + 超时检测）
│   └── engine.rs              // ContextEngine 主体，持有状态机，处理并发命令锁
│
├── common/
│   ├── mod.rs
│   ├── id.rs                  // TaskId, AgentId, AllocationId (newtype)
│   ├── timestamp.rs           // RFC3339 毫秒精度，serde 实现
│   └── audit.rs               // Auditable trait 及 AuditRecord
│
└── tests/                     // 测试清单对应的集成测试
    ├── subagent_report_tests.rs
    ├── memory_policy_tests.rs
    └── lifecycle_tests.rs

struct / enum 分布细节（不重复完整定义，只说明归属）：

SubagentReport → subagent_report/schema.rs

ExecutionStatus (enum) → subagent_report/schema.rs

ResourceUsage → subagent_report/schema.rs

MemoryAdmissionPolicy (struct 实现 BudgetManager) → memory_policy/budget.rs

ReservationToken → memory_policy/reservation.rs

LifecycleState / LifecycleCommand → lifecycle/state.rs

CommandEffect → lifecycle/mod.rs 或 transition.rs

建议的代码组织规则：

每个模块内部的 mod.rs 只做重导出，具体实现放在子模块。

真值表用 const TRANSITION_TABLE: [[Action; 10]; 7] 静态查找表实现，避免大量 match。

defer 队列使用 VecDeque<DeferredEntry> 结合时间戳，外部由 engine 定期调用 drive_deferred。

3. 测试清单
3.1 状态机（ContextEngineLifecycle）测试
3.1.1 基本 accept / reject / noop

Start on Uninitialized → accept，状态变为 Starting

Start on Running → noop，状态不变

Pause on Running → accept，状态变为 Pausing（或直接 Paused，取决于实现，spec 未强制中间态，但可扩展）

Pause on Paused → noop

Checkpoint on Paused → reject

Resume on Paused → accept，状态变为 Running（或先 Starting 再 Running）

Stop on Stopped → noop

Restart on Failed → accept，状态变为 Restarting

3.1.2 defer 行为

Resume on Starting → defer，命令进入 defer 队列

状态转为 Running 后，驱动 defer 队列 → 之前 defer 的 Resume 被重新评估，应变为 accept 或 noop

Restart on Uninitialized → defer，超时 30s 未处理 → 自动转为 reject，命令从队列移除

defer 队列 FIFO 顺序：先进先出，且每个命令最多 defer 一次（或重试有限次）

3.1.3 并发与锁

同一时刻两个 Stop 命令并发到达 → 第一个 accept，第二个 reject（因为有命令在执行）

幂等命令（如 Stop on Stopped）允许 noop，不占用执行锁

3.1.4 Failed 恢复路径

Failed → Restart → accept 进入 Restarting

Failed → Start → reject（spec 未直接禁止，但表里 Start on Failed 为 defer，我们测试 defer 后若转为 Restarting 再评估）

Failed → Stop → accept 进入 Stopped

3.1.5 超时与清理

defer 队列中的命令超过 30s → 自动 reject，且从队列中删除

状态机在 Draining 时收到 Checkpoint → defer，直到 Draining 结束（进入 Stopped 或 Running）后重新评估，应 reject（因为 Checkpoint 只 accept on Running）

3.2 准入策略（MemoryAdmissionPolicy）测试
3.2.1 预占（Pre-Reserve）与确认（Commit）

可用内存充足：try_reserve(requested=100) → 返回 ReservationToken，账本无变化

预占成功后调用 commit → 正式分配出现在 active_allocations，预算减少 100

预占成功但启动失败（不调用 commit，而是调用 release_reservation） → 正式账本无记录，预算不变

TTL 超时（例如 5s 内未 commit） → 后台清理线程自动释放预占，后续 commit 失败（token 无效）

3.2.2 异常退出回收

任务运行中崩溃：调用 reclaim(task_id, agent_id) → 从 active_allocations 移除，释放 100 字节，返回 FreedBytes(100)

重复 reclaim 同一对 (task_id, agent_id) 两次 → 第二次返回 ReclaimError(AlreadyReclaimed) 或 Ok(0) 但无副作用

心跳超时触发回收：模拟 reclaim 应正确释放

3.2.3 驱逐（Eviction）原子性

成功场景：请求 granted_bytes=200，驱逐候选 [alloc1, alloc2]（各 100 字节）

admit_with_eviction 原子操作：先校验两个候选都存在且可驱逐 → 一次性删除 → 释放 200 → 新分配 200 → 返回 Degrade { granted_bytes: 200, evicted: [alloc1, alloc2] }

失败场景 1：候选 alloc1 在锁期间被其他线程回收（不存在） → 整个事务回滚，Deny(EvictionCandidateInvalid)

失败场景 2：候选存在但处于不可驱逐状态（例如 alloc1 被标记为 pinned） → 回滚，返回 Deny

失败场景 3：驱逐成功但新分配提交时预算被第三方耗尽（极端竞争） → 回滚驱逐，返回 Deny(BudgetExceeded)

3.2.4 预算溢出处理

请求 requested_bytes > available 且无驱逐候选 → Deny(BudgetExceeded)

HardLimit 模式（不允许驱逐） → 同上直接拒绝

3.2.5 审计与幂等

每次 commit / reclaim / admit_with_eviction 必须产生 AuditRecord，字段齐全（含 agent_id, task_id, delta_bytes, reason）

使用 idempotency key 重放同一个 commit 请求（例如网络重试） → 第二次应返回 CommitError(AlreadyCommitted)，不修改预算

3.3 SubagentReport 验证测试
3.3.1 schema 版本

schema_version = "1.0.0" → 验证通过

schema_version = "2.0.0" 且主控支持 1.x → RejectedMalformed(UnsupportedSchemaVersion)

缺少 schema_version 字段 → MissingRequiredField

3.3.2 必填字段

依次缺失每个 required 字段，验证拒绝并指出具体字段

所有 required 字段存在且非 null → 接受

3.3.3 可选字段缺失语义

缺失 parent_agent_id → 主控理解为无父代理，不报错

缺失 exit_code → 主控保持 None，序列化时省略 key

stdout_preview, stderr_preview, replay_ref 缺失同理

3.3.4 序列化约束

ExecutionStatus 枚举序列化为字符串（"Success" 而非 0） → 可反序列化

若收到整数枚举值 → RejectedMalformed(InvalidEnumFormat)

空 artifacts 序列化为 []，不写 null

Timestamp 格式符合 RFC3339 毫秒，示例 2026-04-30T10:30:00.123Z

3.3.5 大小限制与截断

生成大小恰好 1 MiB 的报告 → 接受

大小 1.2 MiB，优先截断 stdout_preview/stderr_preview 直到总长度 ≤ 1 MiB → truncated=true，其他字段完整

截断后仍然 > 1 MiB（如 artifacts 巨大） → 子代理不应生成，测试主控补写失败报告，ReportOversize 原因

以上测试清单可直接转为 #[test] 函数或集成测试场景。每个测试点建议独立可重复。

本回答由 AI 生成，内容仅供参考，请仔细甄别。 |
