| response |
| --- |
| 已思考（用时 1 秒）

我们根据要求直接输出。

1. SubagentReport
目标

子代理执行后向创项目主控报告执行结果、资源消耗与产出摘要，用于审计、错误溯源与后续调度决策。

核心字段/状态
rust
复制
下载
struct SubagentReport {
    id: String,
    parent_task_id: String,
    status: ReportStatus,
    start_time: Timestamp,
    end_time: Timestamp,
    exit_code: i32,
    signal: Option<Signal>,
    stdout_preview: String,      // 前N字节/行
    stderr_preview: String,
    resource_usage: ResourceUsage,
    produced_artifacts: Vec<ArtifactRef>,
    replay_log_path: Option<String>,
}
enum ReportStatus { Pending, Accepted, Rejected, Failed }
struct ResourceUsage { cpu_ms: u64, mem_bytes: u64, io_read_bytes: u64, io_write_bytes: u64 }
struct ArtifactRef { path: String, checksum: String, size_bytes: u64 }
输入

子代理的完整执行上下文（启动参数、环境、工作目录）

子代理退出状态、stdout/stderr 捕获（截断或滚动）

系统资源记账数据

输出

持久化存储的 JSON/二进制报告文件路径

主控接收确认（报告 ID + 受理状态）

约束

报告大小上限 1 MiB（超出则截断并标记 truncated）

不得包含未脱敏的密钥/令牌（输出前自动过滤预设 pattern）

报告生成后不可变

失败模式

子代理崩溃未产生报告 → 主控超时后生成失败占位报告

报告写入磁盘无空间 → 标记 critical_failure 并降级到内存环形缓冲

过滤脱敏规则失效 → 回退到只允许安全字段

最小验证方式

注入退出码为 0 的子代理，检查 ProducerArtifacts 与 resource_usage 非空

注入 panic 子代理，验证 exit_code !=0 且 stderr_preview 包含错误片段

强制 stdout 超 1 MiB，确认 truncated 标记出现

2. MemoryAdmissionPolicy
目标

决定一个新的任务或子代理请求是否能获得内存资源准入，防止内存耗尽导致整体不稳定。

核心字段/状态
rust
复制
下载
struct MemoryAdmissionPolicy {
    total_memory_bytes: u64,
    reserved_bytes: u64,                // 系统/关键组件保留
    allocated_map: HashMap<AgentId, u64>,
    policy: PolicyType,
}
enum PolicyType {
    BestEffort,          // 允许超售，软限制
    HardLimit,           // 永不超售，超限拒绝
    SoftWithBackoff,     // 超限后降级子代理优先级
}
struct AdmissionRequest {
    agent_id: AgentId,
    required_bytes: u64,
    priority: Priority,
    can_swap_out: bool,
}
enum AdmissionDecision { Allow, Deny, Degrade { reduced_bytes: u64, swap_out_id: Option<AgentId> } }
输入

AdmissionRequest（请求者 ID、需求内存、优先级、是否可被换出）

当前内存分配视图 allocated_map

系统空闲内存（实时）

输出

AdmissionDecision：允许、拒绝、或降级（给更少内存并指定换出谁）

约束

保守值：reserved_bytes >= 5% total_memory_bytes

决策耗时 < 1ms（软实时）

对同一个 agent_id 在 1 秒内重复请求合并为一次

失败模式

空闲内存读数延迟或错误 → 使用最近已知有效值 + 失败时默认拒绝

请求者提供 required_bytes=0 → 视为查询，实际返回 Allow with 0

can_swap_out=false 但需要换出它 → 改为 Deny

最小验证方式

模拟 total=100, allocated=95, request=10 → HardLimit 返回 Deny

模拟 BestEffort 超售，请求通过，后续 OOM 检测触发策略降级

并发 10 个请求，检查决策延迟平均值 <1ms

3. ContextEngineLifecycle
目标

管理上下文引擎（负责存储/检索当前 agent 对话与执行上下文）的启动、运行状态迁移、关闭及故障恢复。

核心字段/状态
rust
复制
下载
enum CtxState { Uninitialized, Starting, Running, Pausing, Paused, Draining, Stopped, Failed }
struct ContextEngineLifecycle {
    state: CtxState,
    start_time: Option<Timestamp>,
    health_check_tick_secs: u64,
    pending_requests: usize,
    last_known_checkpoint: Option<CheckpointId>,
}
struct ControlCmd { cmd: CmdType, timeout_ms: u64 }
enum CmdType { Start, Stop, Pause, Resume, Checkpoint, Restart }
输入

外部控制命令 ControlCmd

定期健康检查结果（来自监控组件）

子代理对上下文的读写请求（作为活跃触发）

输出

状态变更事件（对外广播）

Checkpoint 成功时返回 CheckpointId

对每个控制命令的确认或错误

约束

状态迁移必须遵守有限状态机：不可从 Paused 跳转到 Draining

任何状态到 Failed 后，必须成功执行 Restart 或进入 Stopped

Checkpoint 期间读写请求排队，最大队列 5000，超限返回 503

失败模式

启动超时（>30s）→ 状态变为 Failed，并尝试自动重启一次

健康检查连续失败 3 次 → 主动触发 Pause，等待外部干预

Checkpoint 写入存储超时 → 保留上一个已知 checkpoint，状态保持 Running

最小验证方式

发送 Start → 状态从 Uninitialized → Starting → Running（模拟内部初始化完成）

在 Running 下发 Pause，验证读取请求被阻塞，状态变为 Paused

注入存储写入超时，触发 Checkpoint 失败，验证不改变 Running 状态且 last_known_checkpoint 不变

关键设计权衡

报告截断 vs 完整性：优先保证系统稳定性，截断报告而非耗尽磁盘。丢失尾部信息可接受，但关键退出码和错误前几行必须保留。

内存准入超售风险：最佳效率允许超售，但引入 OOM 风险。选择 HardLimit 作为默认安全策略，BestEffort 需显式启用。

上下文引擎状态机复杂度：增加 Pausing/Paused/Draining 以支持优雅升级，代价是实现和测试复杂度上升。优先保证 Running 到 Stopped 的可靠性。

健康检查与自动恢复粒度：自动重启仅一次，避免反复崩溃循环。更复杂的恢复交给上层编排。

报告脱敏的确定性：采用预先定义的 pattern 替换，而非机器学习检测，保证性能且无假阴性。

内存分配映射扩容：使用哈希表而非预分配数组，灵活性高但可能碎片化。小规模子代理（<1000）下可接受。

上下文 checkpoint 写入阻塞读：选择在读队列满时返回 503 而非阻塞调用者，确保系统不会因慢 I/O 而锁死。

决策权衡记录：每个模块的拒绝/降级动作必须输出结构化日志，便于事后分析策略是否过严。

本回答由 AI 生成，内容仅供参考，请仔细甄别。 |
