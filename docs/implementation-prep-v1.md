# 创项目实现准备材料 V1

## 目标
基于 `spec-v3.md`，补齐实现前最需要的三类材料：
1. Rust trait 草案
2. struct / enum 文件切分建议
3. 状态机与准入策略测试清单

---

# 1. Rust trait 草案

## 1.1 SubagentReport 相关 trait
```rust
pub trait ReportValidator {
    type Report;

    /// 校验 schema 版本、必填字段、大小限制、枚举序列化格式
    fn validate(&self, raw: &[u8]) -> Result<(), ReportRejectReason>;

    /// 对 optional 字段应用缺失语义
    fn apply_optional_defaults(&self, report: &mut Self::Report);
}

pub trait ReportBuilder {
    /// 生成最终不可变报告
    fn build(self) -> SubagentReport;

    /// 截断 preview，保证总大小不超过限制
    fn truncate_previews(self, max_total_bytes: usize) -> Self;
}

#[derive(Debug, PartialEq)]
pub enum ReportRejectReason {
    UnsupportedSchemaVersion { required: String, current: String },
    MissingRequiredField { field: &'static str },
    InvalidEnumFormat { field: &'static str, found: String },
    SizeLimitExceeded { limit_bytes: usize, actual: usize },
    TruncationFailed { after_truncate: usize },
}
```

### 设计要点
- `ReportValidator` 在主控侧
- `ReportBuilder` 在子代理侧
- “执行结果状态”与“主控受理状态”保持分离，不在同一个 trait 里混用

## 1.2 MemoryAdmissionPolicy 相关 trait
```rust
pub trait BudgetManager: Send + Sync {
    /// 预占：返回 ReservationToken，不写正式账本
    fn try_reserve(&mut self, request: &AdmissionRequest) -> Result<ReservationToken, DenyReason>;

    /// 启动成功后确认：预占转正式分配
    fn commit(&mut self, token: ReservationToken) -> Result<AllocationId, CommitError>;

    /// 启动失败或超时释放预占
    fn release_reservation(&mut self, token: ReservationToken);

    /// 异常退出回收，要求幂等
    fn reclaim(&mut self, task_id: &TaskId, agent_id: &AgentId) -> Result<FreedBytes, ReclaimError>;

    /// 驱逐 + 新分配原子执行
    fn admit_with_eviction(
        &mut self,
        request: &AdmissionRequest,
        evict_candidates: &[AllocationId],
    ) -> Result<AdmissionDecision, DenyReason>;

    /// 查询正式分配快照
    fn active_allocations(&self) -> Vec<ActiveAllocation>;

    /// 查询预算配置
    fn budget_config(&self) -> BudgetConfig;
}

#[derive(Clone)]
pub struct ReservationToken {
    pub id: String,
    pub granted_bytes: u64,
    pub expires_at: Timestamp,
}

pub struct FreedBytes(pub u64);
```

### 设计要点
- `try_reserve` / `commit` / `reclaim` 是账本的最小闭环
- `admit_with_eviction` 专门承载 `SoftLimitWithEviction` 的原子操作
- `reclaim` 必须幂等

## 1.3 ContextEngineLifecycle 相关 trait
```rust
pub trait LifecycleStateMachine {
    type Command;

    /// 根据 command × state 真值表处理命令
    fn handle_command(
        &mut self,
        command: Self::Command,
    ) -> Result<CommandEffect<Self::Command>, CommandRejectReason>;

    /// 获取当前状态
    fn current_state(&self) -> LifecycleState;

    /// 驱动 defer 队列
    fn drive_deferred(&mut self) -> Vec<CommandEffect<Self::Command>>;
}

pub enum CommandEffect<Cmd> {
    Accepted { next_state: LifecycleState },
    Rejected { reason: String },
    Noop,
    Deferred { command: Cmd, inserted_at: Timestamp },
}

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

pub enum CommandRejectReason {
    InvalidState { current: LifecycleState, expected_states: Vec<LifecycleState> },
    TimeoutDeferred { command: LifecycleCommand, elapsed_ms: u64 },
    ConcurrencyLocked,
}
```

### 设计要点
- `handle_command` 是状态机唯一入口
- `drive_deferred` 专门处理 `defer`
- `Accepted / Rejected / Noop / Deferred` 要和 spec-v3 真值表一一对应

## 1.4 通用辅助 trait
```rust
pub trait Auditable {
    fn audit_log(&self) -> AuditRecord;
}

pub trait IdempotentKey {
    fn idempotency_key(&self) -> String;
}

pub struct AuditRecord {
    pub operation: String,
    pub agent_id: AgentId,
    pub task_id: TaskId,
    pub delta_bytes: i64,
    pub reason: String,
    pub timestamp: Timestamp,
}
```

---

# 2. 文件切分建议

## 2.1 建议目录结构
```text
src/
├── lib.rs
├── subagent_report/
│   ├── mod.rs
│   ├── schema.rs
│   ├── validation.rs
│   ├── serialization.rs
│   └── size_limit.rs
│
├── memory_policy/
│   ├── mod.rs
│   ├── budget.rs
│   ├── reservation.rs
│   ├── commit_reclaim.rs
│   ├── eviction.rs
│   └── audit_log.rs
│
├── lifecycle/
│   ├── mod.rs
│   ├── state.rs
│   ├── transition.rs
│   ├── defer_queue.rs
│   └── engine.rs
│
├── common/
│   ├── mod.rs
│   ├── id.rs
│   ├── timestamp.rs
│   └── audit.rs
│
└── tests/
    ├── subagent_report_tests.rs
    ├── memory_policy_tests.rs
    └── lifecycle_tests.rs
```

## 2.2 struct / enum 归属建议
### `subagent_report/`
- `SubagentReport` → `schema.rs`
- `ExecutionStatus` → `schema.rs`
- `ResourceUsage` → `schema.rs`
- `ArtifactRef` / `ArtifactKind` → `schema.rs`
- `ReportRejectReason` → `validation.rs`

### `memory_policy/`
- `MemoryAdmissionPolicy` → `budget.rs`
- `AllocationRecord` → `budget.rs`
- `ReservationToken` → `reservation.rs`
- `AdmissionRequest` / `AdmissionDecision` / `DenyReason` → `budget.rs`
- 驱逐事务逻辑 → `eviction.rs`
- commit / reclaim 闭环 → `commit_reclaim.rs`

### `lifecycle/`
- `ContextEngineState` / `LifecycleState` → `state.rs`
- `LifecycleCommand` → `state.rs`
- command × state 真值表实现 → `transition.rs`
- defer 队列 → `defer_queue.rs`
- 外层引擎包装与并发锁 → `engine.rs`

### `common/`
- `TaskId` / `AgentId` / `AllocationId` → `id.rs`
- `Timestamp` serde 规则 → `timestamp.rs`
- `Auditable` / `AuditRecord` → `audit.rs`

## 2.3 代码组织规则
1. `mod.rs` 只做 re-export，不堆实现
2. 真值表优先用静态表或集中查找函数，不要到处散 `match`
3. `defer_queue` 独立，避免把超时/重试逻辑塞进主状态机函数
4. 账本操作与审计日志分层：核心逻辑负责返回事件，日志模块负责落盘/输出
5. schema / validation / serialization 分开，避免一个文件又定义结构又做校验又做存储格式

---

# 3. 测试清单

## 3.1 ContextEngineLifecycle 测试
### 基本 accept / reject / noop
- `Start` on `Uninitialized` → `accept`
- `Start` on `Running` → `noop`
- `Pause` on `Running` → `accept`
- `Pause` on `Paused` → `noop`
- `Checkpoint` on `Paused` → `reject`
- `Resume` on `Paused` → `accept`
- `Stop` on `Stopped` → `noop`
- `Restart` on `Failed` → `accept`

### defer 行为
- `Resume` on `Starting` → `defer`
- 状态切到 `Running` 后重驱动 → 之前的 `Resume` 变 `accept` 或 `noop`
- `defer` 超过 30s → 自动转 `reject`

### 并发与锁
- 同时两个命令进入 → 一个执行，另一个 `reject` 或 `noop`
- 幂等命令不应抢占执行锁

### Failed 恢复路径
- `Failed -> Restarting -> Starting -> Running`
- `Failed -> Stop -> Stopped`
- 不允许 `Failed -> Running`

### Draining / Checkpoint 约束
- `Drain` on `Running` → `accept`
- `Checkpoint` on `Draining` → `defer` 或 `reject`，必须与 spec 保持一致
- `Draining` 时新写请求被拒绝

## 3.2 MemoryAdmissionPolicy 测试
### 预占与确认
- 可用预算足够：`try_reserve` 成功，正式账本不变
- `commit` 成功后，正式账本新增分配
- 预占成功但启动失败：释放预占，正式账本不变
- 预占 TTL 超时：自动释放，后续 `commit` 失败

### 异常退出回收
- 任务崩溃：`reclaim` 释放预算
- 重复 `reclaim`：无副作用
- 心跳超时触发回收：正确释放

### 驱逐原子性
- 驱逐候选全部有效：驱逐 + 新分配一起成功
- 候选失效：全部回滚
- 候选不可驱逐：全部回滚
- 新分配提交失败：驱逐也回滚

### 预算与默认策略
- `HardLimit` 下预算不足直接拒绝
- `SoftLimitWithEviction` 下高优请求可驱逐低优先级
- 未配置模式时默认 `HardLimit`

### 审计与幂等
- 每次 commit / reclaim / eviction 都产出 `AuditRecord`
- 同一个幂等 key 重放时不重复扣账

## 3.3 SubagentReport 测试
### schema 版本
- `schema_version = 1.0.0` → 通过
- 主版本不匹配 → `RejectedMalformed`
- 缺失 `schema_version` → `MissingRequiredField`

### 必填字段
- 逐个删 required 字段，验证拒绝原因准确
- required 全齐且非 null → 接受

### optional 缺失语义
- 缺失 `parent_agent_id` → 解释为无父代理
- 缺失 `exit_code` → 解释为不适用/未提供
- 缺失 preview / replay_ref → 不报错

### 序列化约束
- 枚举必须是字符串字面量
- `Timestamp` 必须符合 RFC3339 毫秒精度
- `artifacts` 空列表序列化为 `[]`

### 大小限制与截断
- 刚好 1 MiB → 接受
- 超限但可截断 preview → `truncated=true`
- 截断后仍超限 → 主控补写失败报告

## 3.4 Skill 生命周期测试
### monitor / decay / rollback
- `skill solidify` 更新已有 canonical skill 时必须保留 `Previous Version Snapshot`，为后续回滚提供证据面。
- `skill retire` / `skill deprecate` 只能原位标记状态，不能删除文件，且要保留可恢复内容。
- `skill monitor` 应只读输出 active / deprecated / retired 数量，以及 decay 候选和 rollback 候选。
- `skill rollback` 应基于保留的快照恢复为新的 active 版本，并保留 rollback 记录。

---

# 4. 收口判断
这三份材料已经足够支撑下一步：
1. 开始写 Rust 类型定义
2. 开始写状态机与准入策略测试骨架
3. 开始做最小目录初始化

也就是说，`spec-v3.md` 负责“定义边界”，这份文档负责“把边界翻成可落代码结构”。
