# 创项目规格 V2（收口版）

## 文档目标
本版用于把三个核心对象收口为**可实现、可测试、可审计**的工程输入：
1. `SubagentReport`
2. `MemoryAdmissionPolicy`
3. `ContextEngineLifecycle`

统一约定：
- 类型名：`PascalCase`
- 字段名：`snake_case`
- 枚举成员：`PascalCase`
- 所有对象优先输出**结构化日志**和**可验证状态**，而不是隐式行为

---

# 1. SubagentReport

## 1.1 目标
子代理执行结束后向主控提交一个**不可变的结构化执行报告**，用于：
- 判断任务是否完成
- 归档资源消耗与产物
- 支持失败审计、重放与后续调度

## 1.2 执行结果状态 vs 主控受理状态
这两者必须严格分离。

### 执行结果状态（子代理视角）
表示“子代理自己执行成了什么样”。

```rust
enum ExecutionStatus {
    Success,
    Failed,
    Cancelled,
    TimedOut,
    Rejected,
}
```

含义：
- `Success`：正常完成，退出码为 0
- `Failed`：执行失败，退出码非 0 或内部异常
- `Cancelled`：被主控或外部中止
- `TimedOut`：超时结束
- `Rejected`：尚未真正执行就被资源/策略拒绝

### 主控受理状态（主控视角）
表示“主控是否接受这份报告进入系统”。

> 注意：主控受理状态**不应写回 `SubagentReport` 本体**，否则会破坏“报告不可变快照”原则。应由主控单独生成关联记录。

```rust
struct ReportAdmission {
    report_id: String,
    admission_status: AdmissionStatus,
    reason: Option<String>,
    admitted_at: Timestamp,
}

enum AdmissionStatus {
    Accepted,
    RejectedInvalid,
    RejectedDuplicate,
    RejectedMalformed,
}
```

### 区分表
| 维度 | 执行结果状态 | 主控受理状态 |
|---|---|---|
| 生产者 | 子代理 | 主控 |
| 产生时机 | 执行结束时 | 报告到达主控后 |
| 是否可变 | 否 | 可追加，但不可回写原报告 |
| 语义 | 任务执行结果 | 报告入库/受理结果 |

## 1.3 核心数据结构
```rust
struct SubagentReport {
    report_id: String,
    task_id: String,
    agent_id: String,
    parent_agent_id: Option<String>,
    status: ExecutionStatus,
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

## 1.4 输入
- 子代理启动元数据：`task_id`、`agent_id`、父任务信息、工作目录
- 子代理退出信息：退出码、取消原因、超时信息
- 执行输出：`stdout` / `stderr` / 结构化日志
- 资源统计：CPU、内存、I/O、持续时间
- 产物清单：文件、diff、日志、计划等

## 1.5 输出
- 持久化的 `SubagentReport`
- 主控产生的 `ReportAdmission`
- 可选的 `replay_ref`（日志路径 / trace id / transcript id）

## 1.6 约束
- 报告写入后不可原地修改
- 报告体建议上限 1 MiB；超出时仅截断 preview，并保留 `replay_ref`
- preview 不得包含明文密钥、token、cookie
- `summary` 必须是主控可直接展示的结论，不允许只是日志拼接
- `finished_at >= started_at`

## 1.7 失败模式
- 子代理崩溃未上报 → 主控补写占位失败报告
- 日志过大 → `truncated=true`，完整内容走 `replay_ref`
- 产物路径失效 → 报告保留元数据，外部标记 dangling
- 脱敏失败 → 仅保留结构化元数据，不回传原始文本
- 重复 `report_id` → 原报告保留，新受理记录返回 `RejectedDuplicate`

## 1.8 最小验证方式
1. 成功执行：`status=Success`、`artifacts` 非空、`resource_usage.wall_ms > 0`
2. 异常退出：`status=Failed` 且 `stderr_preview` 含错误片段
3. 超大 stdout：`truncated=true`
4. 取消流程：`status=Cancelled` 且 `exit_code=None`
5. 重复受理：生成 `RejectedDuplicate`，原始报告不变

---

# 2. MemoryAdmissionPolicy

## 2.1 目标
在主控启动新任务或子代理前，判断是否允许其占用上下文/内存预算，防止超分配导致主控退化、抖动或 OOM。

## 2.2 核心数据结构
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

## 2.3 默认策略
默认：`mode = HardLimit`

### 原因
1. **稳定性优先**：宁可拒绝一次请求，也不要把主控拖进 OOM
2. **实现最简单**：不需要先做驱逐器、抢占关系和回收协议
3. **可观测性最好**：拒绝比“超配后慢性出血”更容易调优
4. **符合系统角色**：主控应保守，吞吐优化应放到显式配置阶段，而不是默认路径

### 升级条件
仅当以下条件同时满足，才允许显式切换到 `SoftLimitWithEviction`：
- 已定义优先级抢占规则
- 已实现可回收任务识别
- 已有稳定结构化决策日志
- 已通过压力测试证明不会把 `Critical` 任务饿死

## 2.4 输入
- `AdmissionRequest`
- 当前分配账本 `active_allocations`
- 全局预算与系统保留值
- 当前活跃 agent 数量

## 2.5 输出
- `AdmissionDecision`
- 一条结构化决策日志：请求值、剩余额度、决策原因、驱逐对象（如有）

## 2.6 约束
- `reserved_system_bytes` 不可被业务任务侵占
- 决策逻辑应是纯函数风格，相同输入得到相同输出
- 决策耗时目标 < 1ms
- `requested_bytes == 0` → `Deny(InvalidRequest)`
- `Critical` 任务不得被非 `Critical` 请求驱逐
- `max_agents` 超限时优先拒绝新请求，而不是强行挤占旧任务

## 2.7 失败模式
- 活动账本损坏 → 回退为保守模式：仅允许 `Critical`
- 实时内存读数缺失 → 用最近稳定快照，否则默认拒绝
- 驱逐候选为空但必须回收 → 返回 `Deny`
- 请求值异常（0/溢出/非法） → `InvalidRequest`

## 2.8 最小验证方式
1. `HardLimit`：预算 100，已分配 95，请求 10 → `Deny(BudgetExceeded)`
2. `SoftLimitWithEviction`：低优先级占满，高优请求到来 → `Degrade` 或驱逐低优先级
3. `requested_bytes = 0` → `Deny(InvalidRequest)`
4. 并发相同请求 → 决策一致、账本不损坏
5. 未配置模式时 → 默认 `HardLimit`

---

# 3. ContextEngineLifecycle

## 3.1 目标
管理上下文引擎的状态机，使启动、运行、checkpoint、暂停、恢复、停止和故障恢复全部具备**显式状态、显式边界、显式验证路径**。

## 3.2 核心状态与结构
```rust
enum ContextEngineState {
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

## 3.3 允许 / 禁止状态迁移表
| 当前状态 | 允许迁移到 | 禁止迁移示例 |
|---|---|---|
| `Uninitialized` | `Starting` | `Running`, `Paused`, `Stopped`, `Failed` |
| `Starting` | `Running`, `Failed` | `Pausing`, `Draining`, `Checkpointing`, `Restarting` |
| `Running` | `Pausing`, `Checkpointing`, `Draining`, `Stopped`, `Failed`, `Restarting` | `Starting`, `Uninitialized` |
| `Checkpointing` | `Running`, `Failed` | `Pausing`, `Draining`, `Stopped` |
| `Pausing` | `Paused`, `Failed` | `Running`, `Checkpointing`, `Draining` |
| `Paused` | `Running`, `Stopped`, `Failed`, `Restarting` | `Checkpointing`, `Draining` |
| `Draining` | `Stopped`, `Failed` | `Running`, `Paused`, `Checkpointing`, `Restarting` |
| `Restarting` | `Starting`, `Failed` | 其他所有状态 |
| `Stopped` | `Starting` | `Running`, `Paused`, `Draining` |
| `Failed` | `Restarting`, `Stopped` | `Running`, `Paused`, `Draining`, `Checkpointing` |

## 3.4 迁移规则补充
- `Checkpoint` 只能在 `Running` 下触发
- `Pause` 只能从 `Running` 进入 `Pausing`
- `Drain` 只能从 `Running` 进入 `Draining`
- `Restart` 只能从 `Running` / `Paused` / `Failed` 触发
- `Failed` 后不得“伪恢复”为 `Running`，必须先 `Restarting` 或 `Stopped`
- `Draining` 期间不得接受新的写入型请求

## 3.5 输入
- 外部控制命令：`Start / Pause / Resume / Checkpoint / Drain / Stop / Restart`
- 内部事件：初始化完成、checkpoint 完成、健康检查失败、I/O 超时
- 活跃请求变化：`pending_ops` 增减

## 3.6 输出
- 状态迁移事件 `LifecycleEvent`
- 控制命令确认结果：`accepted / rejected`
- 可选 checkpoint id

## 3.7 约束
- 非法跳转必须拒绝并记录错误事件
- `Checkpointing` 期间读写策略必须固定，推荐：允许读、阻塞写
- `Draining` 期间允许存量请求收尾，但拒绝新写入
- 健康检查连续失败达到阈值时，默认进入 `Failed`
- `Draining` 超时必须升级为 `Failed` 或强制 `Stopped`，不能无限挂起

## 3.8 失败模式
- 启动超时 → `Starting -> Failed`
- checkpoint 存储失败 → 保留旧 checkpoint，记录错误，推荐回到 `Running`
- 健康检查失败 → `Running -> Failed`
- draining 长时间不结束 → `Draining -> Failed`
- 非法控制命令 → 状态不变，返回拒绝结果

## 3.9 最小验证方式
1. `Start`：`Uninitialized -> Starting -> Running`
2. `Pause/Resume`：`Running -> Pausing -> Paused -> Running`
3. `Checkpoint`：`Running -> Checkpointing -> Running`，并生成 `last_checkpoint_id`
4. 启动超时：`Starting -> Failed`
5. `Draining` 期间发新写请求 → 被拒绝
6. 非法迁移：`Paused -> Checkpointing` → 命令被拒绝，状态不变
7. `Restart`：`Running -> Restarting -> Starting -> Running`

---

# 4. 全局设计结论
1. `SubagentReport` 必须保持不可变；主控受理状态单独建模。
2. `MemoryAdmissionPolicy` 默认采用 `HardLimit`，把主控稳定性放在吞吐前面。
3. `ContextEngineLifecycle` 必须显式列出迁移表，否则恢复逻辑不可验证。
4. 三个对象的默认策略都应偏保守，先把“拒绝/失败/回退”设计清楚，再追求复杂优化。
5. 所有关键决策都必须落结构化日志，便于回放、调优和责任追踪。
