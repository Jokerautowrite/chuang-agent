| response |
| --- |
| 已思考（用时 41 秒）

我们收到任务：对V1审稿版做V2收口。要求：

统一三个对象的命名风格

为ContextEngineLifecycle补一张“允许/禁止状态迁移表”

为MemoryAdmissionPolicy明确默认策略，并解释为什么

为SubagentReport明确区分“执行结果状态”和“主控受理状态”

输出更接近正式实现输入文档的版本

注意：不要寒暄，直接输出V2文档正文。保持工程化、可落地、可测试。

首先观察V1中的命名风格：

SubagentReport：字段命名snake_case，结构体名称PascalCase，枚举PascalCase。

MemoryAdmissionPolicy：同理。

ContextEngineLifecycle：同理。基本统一。但是否需要调整？任务要求“统一三个对象的命名风格”，可能是指对象名后缀或命名模式？当前都是PascalCase + 描述性名词。可以保持一致，不需要改。可能还需要统一一些术语，比如"agent_id" vs "subagent_id"? 但SubagentReport里有agent_id和parent_agent_id，MemoryAdmissionPolicy里使用agent_id，ContextEngineLifecycle中无直接agent概念。可以保留。

重点是补充状态迁移表、默认策略、区分执行结果和受理状态。

对于SubagentReport：目前有ReportStatus枚举表示执行结果状态。但主控受理状态（即主控是否接受这个report？）没有明确。任务要求区分“执行结果状态”和“主控受理状态”。可能是指：子代理执行后产生报告，主控可能因为报告格式错误、签名无效、重复等拒绝受理。需要在报告中增加一个单独的受理状态字段，或者明确报告本身有一个accepted/rejected标志。根据V1中输出部分提到“一个可被主控消费的受理结果：accepted / rejected”，但未在结构体字段中体现。V2需要明确区分：执行结果状态（子代理自身的退出状态）和主控对报告的受理状态（例如报告是否被主控接受、是否有效）。可以添加一个字段admission_status: AdmissionStatus，或者保持报告不可变但由主控单独记录。但为了一致性，在SubagentReport结构中加入admission_status，可选，或者保持分离。任务要求“明确区分”，可能需要在设计上说明两者是不同的，并在字段中体现。更好的方式：在主控侧有一个ReportReceipt，或者在报告中增加accepted标志。但是注意报告是子代理生成的，主控受理状态是主控决定的，所以不应放在子代理生成的报告中（因为报告生成时主控尚未受理）。所以应该在主控消费报告时产生一个受理记录。但为了文档完整性，可以在SubagentReport章节中明确增加“主控受理状态”作为一个单独的概念，不一定作为结构体字段。但要求是“为SubagentReport明确区分”，建议在结构体内加入一个可选字段control_acceptance: Option<Acceptance>，或者通过外部关联记录。考虑到可测试性和实现，可以这样：SubagentReport本身只记录执行结果；主控在收到报告后会生成一个ReportAcceptanceRecord，包含受理状态。但为了简化，可以在SubagentReport中预留一个字段accepted_by_controller: Option<bool>，但实际写入时可能为空，由主控后续填充分？这违反了不可变性。所以更好的方案：不在报告中，而是主控返回一个受理结果对象。文档中可以在SubagentReport输出部分明确：子代理提交报告后，主控返回ReportAdmission，其中包含acceptance_status。要求“明确区分”只需要在文档中描述清楚两者的定义和关系，不一定必须放在同一个struct里。我们可以在章节中增加“执行结果状态 vs 主控受理状态”小节，并给出定义。为了满足任务，在SubagentReport的字段和说明中区分出来。

但任务要求“为 SubagentReport 明确区分‘执行结果状态’和‘主控受理状态’”，可能是当前V1中只有ReportStatus（执行结果），缺少主控受理决策。所以V2要补充明确定义，并在设计或字段中体现。可以新增一个独立的ReportAdmissionStatus枚举，并在报告中包含一个字段admission_status，但是由主控在受理后填充？这会破坏不可变快照。通常做法：主控收到报告后存储报告，并在另一个表或元数据中记录受理状态。为了清晰，我们可以在文档中定义：SubagentReport 只记录执行结果，主控受理状态是由主控决策生成的ReportAcceptance记录，两者分离。在文档中明确区分即可，不需要修改结构体。很多系统设计会这样做。我倾向于加一节“主控受理状态”描述，并给出示例结构体（独立的），说明与执行结果状态的差异。这样既满足明确区分，又保持报告的不可变性。

对于MemoryAdmissionPolicy：需要明确默认策略。当前有三种模式：HardLimit, SoftLimit, SoftLimitWithEviction。默认应该选择哪个？根据设计权衡第2条：默认偏保守，宁可拒绝也不要把主控拖进OOM。所以默认应选择HardLimit或SoftLimit但禁止抢占？HardLimit最保守，拒绝任何超出预算的请求。但可能过于严格。实际系统可能会选择SoftLimitWithEviction但只允许驱逐低优先级且可回收的任务？更保守的方式是HardLimit。从工程落地角度，默认使用HardLimit，并解释：在未配置或不确定抢占策略时，硬限制可最大程度避免OOM风险，保证主控稳定性。或者默认使用SoftLimit但禁止eviction（即只做软限制记录，但不自动驱逐）。但枚举中没有单独的SoftLimit no eviction? SoftLimit模式不会自动驱逐，只是允许超限但记录告警？需要定义。根据现有枚举：SoftLimit可能表示允许超限但不驱逐，可能导致OOM。所以更保守是HardLimit。我选择默认策略为HardLimit，并解释原因。

对于ContextEngineLifecycle：需要补一张允许/禁止状态迁移表。以表格形式列出状态转换。状态包括：Uninitialized, Starting, Running, Checkpointing, Pausing, Paused, Draining, Stopped, Failed。控制命令和内部事件触发迁移。表格应清晰标示哪些迁移允许，哪些禁止。

还需要统一命名风格：检查V1中是否有不一致？例如MemoryAdmissionPolicy中字段active_allocations类型为Vec<AllocationRecord>，而struct AllocationRecord中有priority，但AdmissionRequest中也有priority。一致。但注意命名：ContextEngineLifecycle中ControlCommand枚举包含Restart，但状态机中没有Restarting状态？有auto_restart_count，但没有显式的Restarting状态。可以增加Restarting状态。或者Restart命令映射到先Stop再Start。为了明确，可以增加Restarting状态。V1中未定义，但任务要求补迁移表，我们可以完善。为了保持合理，建议加上Restarting状态，并使Restart命令从任何可终止状态（Running, Paused, Failed）进入Restarting，然后到Starting/Running。但是这样违反简单性？根据典型生命周期，Restart相当于停止后再启动。我们可以定义Restart命令的行为：先触发Stop，然后自动Start，但作为一个原子命令。为了简化，可以不在状态机中加入Restarting，而是将Restart命令转换为Stop+Start序列。但是迁移表需要明确。我选择加入Restarting状态：从Running/Paused/Failed可以进入Restarting，然后Restarting完成后再进入Starting等。注意避免复杂化。为了提高可落地性，按照常见设计：Restart命令要求引擎先执行停止流程（可能经过Draining等），但作为外部控制命令，可以让引擎进入Restarting状态，在该状态下执行内部停止和启动。具体实现时可能复用。但为了文档完整性，我将在状态集合中加入Restarting，并定义迁移。

最终输出V2文档正文，包含三个章节，每个章节内结构清晰，加上必要的新增内容。

另外，总任务要求“输出一个更接近正式实现输入文档的版本”，意味着格式规范、语言简洁、去掉讨论性文字，保留定义、字段、约束、验证方式等。

现在开始撰写。

V2 收口版：SubagentReport / MemoryAdmissionPolicy / ContextEngineLifecycle
1. SubagentReport
1.1 目标

子代理执行后向主控提交不可变的结构化报告，用于：

判断任务是否完成

归档资源消耗与产物

支持失败审计、重放与后续调度

1.2 执行结果状态（子代理视角）

子代理自身的退出状态，记录在 status 字段中。

rust
复制
下载
enum ExecutionStatus {
    Success,      // 正常完成，exit_code = 0
    Failed,       // 执行失败，exit_code != 0 或内部错误
    Cancelled,    // 被主控或外部取消
    TimedOut,     // 超过分配时限
    Rejected,     // 子代理启动前被拒绝（资源/准入）
}
1.3 主控受理状态（主控视角）

主控收到报告后，独立产生的受理决策，不存储在子代理报告中，而是由主控生成关联记录。两者明确分离：

维度	执行结果状态 (ExecutionStatus)	主控受理状态 (AdmissionStatus)
生产者	子代理	主控
产生时机	子代理结束时	主控收到报告后
是否可变	否（报告不可变快照）	可更新（由主控决策流程控制）
典型取值	Success, Failed, …	Accepted, Rejected_Invalid, Rejected_Duplicate, Rejected_Malformed

主控受理状态示例结构（独立存储）：

rust
复制
下载
struct ReportAdmission {
    report_id: String,
    admission_status: AdmissionStatus,
    reason: Option<String>,
    admitted_at: Timestamp,
}

enum AdmissionStatus {
    Accepted,
    RejectedInvalid,      // 格式错误、缺失字段
    RejectedDuplicate,    // 重复 report_id
    RejectedMalformed,    // 校验失败（签名/哈希）
}
1.4 核心字段（子代理报告）
rust
复制
下载
struct SubagentReport {
    report_id: String,
    task_id: String,
    agent_id: String,
    parent_agent_id: Option<String>,
    status: ExecutionStatus,          // 仅记录执行结果
    started_at: Timestamp,
    finished_at: Timestamp,
    exit_code: Option<i32>,
    summary: String,                  // 主控可直接展示的结论
    stdout_preview: String,
    stderr_preview: String,
    resource_usage: ResourceUsage,
    artifacts: Vec<ArtifactRef>,
    truncated: bool,                  // preview 是否被截断
    replay_ref: Option<String>,       // 完整日志或 trace 引用
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
1.5 输入 / 输出

输入：子代理启动元数据、结束信号、输出流、资源统计、产物清单

输出：持久化的 SubagentReport + 主控产生的 ReportAdmission

1.6 约束

报告写入后不可原地修改

报告体上限 1 MiB，超出则截断 preview 并标记 truncated=true

preview 字段不得包含明文密钥 / token / cookie

summary 必须是简要结论，非原始日志拼接

1.7 失败模式

子代理崩溃未上报 → 主控补写占位报告，status=Failed

日志过大 → 截断 + replay_ref

产物路径失效 → 保留元数据，标记 dangling（通过外部系统记录）

脱敏失败 → 回退为仅保留结构化元数据

1.8 最小验证方式

成功执行：status=Success、artifacts 非空、resource_usage.wall_ms > 0

异常退出：status=Failed 且 stderr_preview 含错误片段

超大 stdout：truncated=true

取消流程：status=Cancelled 且 exit_code=None

主控受理重复报告：产生 RejectedDuplicate，原始报告保留不变

2. MemoryAdmissionPolicy
2.1 目标

在主控启动新任务或子代理前，判断是否允许其占用内存预算，避免超分配导致系统退化、抖动或 OOM。

2.2 核心数据结构
rust
复制
下载
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
    HardLimit,                // 硬限制：超出即拒绝
    SoftLimit,                // 软限制：允许超限，仅记录告警
    SoftLimitWithEviction,    // 软限制+驱逐：优先驱逐低优先级/可回收任务
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
2.3 默认策略

默认 mode = HardLimit。

原因：

保守优先：宁可拒绝一次请求，也不让主控进程进入不可控的 OOM 或抖动。

落地简单：不需要实现驱逐逻辑和优先级抢占，减少早期复杂性。

可观测性好：拒绝事件清晰，易于调优预算分配。

对于 Critical 任务，可以在外部（调度层）预留专用预算，而非依赖运行时抢占。

生产环境如需更高吞吐，可显式配置切换为 SoftLimitWithEviction，但必须同时配置可回收标记和优先级阈值。

2.4 输入 / 输出

输入：AdmissionRequest、当前活跃分配、剩余预算、max_agents

输出：AdmissionDecision + 结构化决策日志

2.5 约束

reserved_system_bytes 始终不可被业务任务侵占

决策函数应为纯函数（无副作用），相同输入得相同输出

决策耗时 < 1ms

requested_bytes == 0 → Deny(InvalidRequest)

Critical 任务只能被 Critical 或显式系统策略抢占（SoftLimitWithEviction 下也不得驱逐 Critical）

2.6 失败模式

活动分配账本损坏 → 回退为保守模式：仅允许 Critical 请求

实时内存读数缺失 → 使用最近稳定快照，若缺失则默认拒绝

驱逐候选为空但必须回收 → 返回 Deny

请求值异常 → InvalidRequest

2.7 最小验证方式

HardLimit：预算 100，已分配 95，请求 10 → Deny(BudgetExceeded)

SoftLimitWithEviction：低优先级占满，来 High 请求 → Degrade 并驱逐低优先级

requested_bytes = 0 → Deny(InvalidRequest)

并发相同请求 → 决策一致且无账本破坏

默认模式验证：未显式配置时 mode == HardLimit

3. ContextEngineLifecycle
3.1 目标

管理上下文引擎的状态机，使启动、运行、检查点、暂停、恢复、停止、故障恢复等行为可验证。

3.2 状态定义
rust
复制
下载
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
3.3 允许/禁止状态迁移表
当前状态	允许迁移到的状态	禁止迁移（示例）
Uninitialized	Starting	Running, Paused, Stopped, Failed
Starting	Running, Failed	Pausing, Draining, Checkpointing, Restarting
Running	Pausing, Checkpointing, Draining, Stopped, Failed, Restarting	Starting, Uninitialized
Checkpointing	Running, Failed	Pausing, Draining, Stopped
Pausing	Paused, Failed	Running, Checkpointing, Draining
Paused	Running, Stopped, Failed, Restarting	Checkpointing, Draining
Draining	Stopped, Failed	Running, Paused, Checkpointing, Restarting
Restarting	Starting, Failed	其他所有状态
Stopped	Starting (外部命令), Restarting	Running, Paused, Draining
Failed	Restarting, Stopped	Running, Paused, Draining, Checkpointing

迁移规则说明：

Restart 命令只能从 Running、Paused、Failed 进入 Restarting，不能从 Draining 或 Stopped 直接 restart。

Checkpoint 命令只能在 Running 时接受。

Draining 期间不接受新的写入型请求（由外部调用方保证）。

Failed 后必须显式 restart 或 stop，不能伪装回 Running。

3.4 输入 / 输出

输入：ControlCommand、内部事件（初始化完成、checkpoint 完成、健康检查失败等）、pending_ops 变化

输出：LifecycleEvent、命令接受/拒绝结果、可选 checkpoint id

3.5 约束

严格遵循迁移表，非法跳转必须拒绝并记录错误事件

Checkpointing 期间可阻塞写请求，但策略必须一致（例如仅阻塞写，允许读）

Draining 期间不接受新写入请求，正在进行的请求可完成

健康检查连续失败（配置阈值）→ Running -> Failed

Draining 超时（配置时长）→ Draining -> Failed

3.6 失败模式

启动超时 → Starting -> Failed

Checkpoint 存储失败 → 保留旧 checkpoint，记录错误事件，可回到 Running 或进入 Failed（策略固定，推荐回到 Running）

健康检查失败 → Running -> Failed 或 Running -> Paused（取决于配置，默认 -> Failed）

Draining 长时间不结束 → Draining -> Failed

3.7 最小验证方式

Start 流程：Uninitialized -> Starting -> Running

Pause/Resume：Running -> Pausing -> Paused -> Running

Checkpoint：Running -> Checkpointing -> Running，并生成 last_checkpoint_id

启动超时：Starting -> Failed

Draining 时发送新写请求 → 被拒绝（由调用方实现，引擎状态拒绝）

非法迁移：Paused -> Checkpointing → 命令被拒绝，状态不变

Restart 命令：Running -> Restarting -> Starting -> Running（若启动成功）

本回答由 AI 生成，内容仅供参考，请仔细甄别。 |
