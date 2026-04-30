# 创项目规格 V3（冻结稿）

> 本版只补 3 件事：
> 1. `SubagentReport` 的 schema / 必填字段 / 可选字段 / 序列化约定
> 2. `MemoryAdmissionPolicy` 的账本更新时机
> 3. `ContextEngineLifecycle` 的 command × state 真值表

---

# 1. SubagentReport 补充约定

## 1.1 Schema Version
每个 `SubagentReport` 必须携带：

```rust
schema_version: String // SemVer, 初始值 "1.0.0"
```

规则：
- 主控读取时必须校验**主版本号**
- 主版本不匹配 → `RejectedMalformed`
- 次版本升级可新增 optional 字段，但不得改变 required 字段语义
- 补丁版本仅允许修正文档或兼容性描述，不改变序列化结构

## 1.2 Required Fields
以下字段必须存在，且不可为 `null`：

| 字段 | 类型 |
|---|---|
| `schema_version` | `String` |
| `report_id` | `String` |
| `task_id` | `String` |
| `agent_id` | `String` |
| `status` | `ExecutionStatus` |
| `started_at` | `Timestamp` |
| `finished_at` | `Timestamp` |
| `summary` | `String` |
| `resource_usage` | `ResourceUsage` |
| `artifacts` | `Vec<ArtifactRef>` |
| `truncated` | `bool` |

## 1.3 Optional Fields
以下字段允许缺失；缺失时按约定语义处理：

| 字段 | 缺失语义 |
|---|---|
| `parent_agent_id` | 无父代理 |
| `exit_code` | 未提供或不适用（如 `Cancelled`） |
| `stdout_preview` | 无标准输出 |
| `stderr_preview` | 无标准错误 |
| `replay_ref` | 无可重放引用 |

## 1.4 Serialization Format
### 传输格式
- JSON（UTF-8）

### 持久化格式
- 默认：JSON Lines
- 可选：Parquet
- 要求：单条记录必须可反序列化为**同构 JSON 对象**

### 序列化规则
- `Timestamp`：RFC 3339，毫秒精度
  - 示例：`2026-04-30T10:30:00.123Z`
- `enum`：序列化为字符串字面量
  - 示例：`"Success"`，不能写成 `0`
- optional 字段缺失时：**直接省略 key**，不写 `null`
- `resource_usage` 缺失子字段时：按 `0` 填充，不允许结构不完整
- `artifacts` 为空时：序列化为 `[]`

## 1.5 大小限制
- 单份序列化报告 ≤ `1 MiB`
- 超限时：优先截断 `stdout_preview` / `stderr_preview`
- 截断后必须设置：

```rust
truncated = true
```

- 若截断后仍超限：
  - 子代理不得继续生成该报告
  - 由主控补写失败报告
  - 原因建议记录为：`ReportOversize`

## 1.6 最小验证点
1. 缺失 required 字段 → 主控拒绝受理
2. optional 字段省略 → 主控按默认缺失语义解释
3. `schema_version = 2.0.0` 且主控仅支持 `1.x` → `RejectedMalformed`
4. 枚举被序列化成整数 → 拒绝受理
5. 报告超 `1 MiB` → 截断或补写失败报告

---

# 2. MemoryAdmissionPolicy：账本更新时机

## 2.1 总原则
所有账本操作必须满足：
- 原子性：要么全部成功，要么全部回滚
- 幂等性：重复执行回收/确认时不产生重复副作用
- 可审计：每一步都有结构化日志

## 2.2 准入前预占（Pre-Reserve）
### 时机
收到 `AdmissionRequest` 后、真正启动任务前。

### 动作
1. 计算可用预算：

```text
available = total_budget_bytes
          - reserved_system_bytes
          - sum(active_allocations.allocated_bytes)
```

2. 若请求满足预算（或满足驱逐前提），创建**临时预占记录**
3. 临时预占记录不得立即进入 `active_allocations`

### 约束
- 预占成功不等于启动成功
- 预占记录必须有 TTL，默认建议 `5s`
- TTL 到期未确认启动成功 → 自动释放

### 失败处理
- 预占失败 → 直接 `Deny`
- 不得修改正式账本

## 2.3 启动成功确认（Commit on Start Success）
### 时机
任务/子代理进程**实际启动成功**后。

> 注意：不是 enqueue 成功，也不是线程创建请求成功，而是“已拿到可运行实体”。

### 动作
1. 将预占记录转为正式分配
2. 写入 `active_allocations`
3. 记录 `allocated_bytes = granted_bytes`

### 启动失败处理
若启动失败（如 fork 失败、worker 初始化失败）：
- 立即释放预占
- 不产生正式分配记录

## 2.4 异常退出回收（Reclaim on Exit）
### 触发条件
以下任一事件出现时执行回收：
- 任务状态变为 `Failed`
- 任务状态变为 `Cancelled`
- 任务状态变为 `TimedOut`
- 主控检测到进程消失
- 心跳超时 / 健康检查失败

### 动作
1. 从 `active_allocations` 删除对应记录
2. 释放 `allocated_bytes`
3. 写回结构化日志：
   - `agent_id`
   - `task_id`
   - `freed_bytes`
   - `reason`

### 幂等性要求
- 同一 `agent_id + task_id` 的重复回收必须无副作用

## 2.5 驱逐原子性（Atomic Eviction）
适用模式：`SoftLimitWithEviction`

### 时机
决策返回：

```rust
Degrade { granted_bytes, evict }
```

### 原子操作顺序
1. 校验全部 `evict` 候选都存在且可驱逐
2. 一次性移除全部候选
3. 计算总释放额度 `freed_bytes_sum`
4. 一次性提交新分配 `granted_bytes`

### 事务要求
整个过程必须在**单个事务或单个锁区间**内完成。

### 回滚要求
若以下任一条件失败：
- 候选记录不存在
- 候选在锁期间被并发修改
- 新分配提交失败

则：
- 回滚全部驱逐
- 回滚新分配
- 返回 `Deny(BudgetExceeded)`

## 2.6 时序摘要
```text
Request
  -> Pre-Reserve
    -> Launch Agent
      -> Start Success -> Commit Active Allocation
      -> Start Fail    -> Release Reserve

Running
  -> Abnormal Exit
    -> Reclaim
      -> Free Budget

Request(with eviction)
  -> Pre-Reserve
    -> Atomic Eviction + Commit
      -> success: new allocation active
      -> fail: full rollback
```

## 2.7 最小验证点
1. 预占成功但启动失败 → 正式账本无记录
2. 预占 TTL 超时 → 自动释放
3. 异常退出回收重复触发两次 → 只回收一次
4. 驱逐过程中任一候选失效 → 全部回滚
5. 启动成功后崩溃 → 正式账本记录被回收

---

# 3. ContextEngineLifecycle：Command × State 真值表

## 3.1 符号定义
| 符号 | 含义 |
|---|---|
| `accept` | 接受命令，开始状态迁移 |
| `reject` | 拒绝命令，状态不变 |
| `noop` | 命令无副作用，状态不变 |
| `defer` | 暂不处理，进入待处理队列，稍后重评估 |

## 3.2 真值表
| Command \ State | Uninitialized | Starting | Running | Checkpointing | Pausing | Paused | Draining | Restarting | Stopped | Failed |
|---|---|---|---|---|---|---|---|---|---|---|
| `Start` | accept | reject | noop | reject | reject | noop | reject | defer | accept | defer |
| `Pause` | reject | reject | accept | reject | reject | noop | reject | reject | noop | reject |
| `Resume` | reject | defer | noop | reject | defer | accept | reject | reject | noop | reject |
| `Checkpoint` | reject | reject | accept | reject | reject | reject | defer | reject | noop | reject |
| `Drain` | reject | reject | accept | defer | reject | noop | reject | reject | noop | reject |
| `Stop` | accept | defer | accept | defer | accept | accept | accept | defer | noop | accept |
| `Restart` | defer | reject | accept | reject | reject | accept | reject | reject | accept | accept |

## 3.3 解释规则
### `defer`
- `defer` 必须进入待处理队列
- 状态离开当前值后，必须重新评估该命令
- `defer` 超过 `30s` 未处理 → 自动转 `reject`

### 并发命令
- 同一时刻只允许一个命令进入执行态
- 其他命令默认 `reject`
- 明确幂等命令可走 `noop`，例如：
  - `Stop` on `Stopped`
  - `Pause` on `Paused`

### `Failed` 恢复路径
- `Failed` 不允许直接回 `Running`
- 必须走：

```text
Failed -> Restarting -> Starting -> Running
```

或：

```text
Failed -> Stopped
```

## 3.4 最小验证点
1. `Start` on `Uninitialized` → `accept`
2. `Checkpoint` on `Paused` → `reject`
3. `Resume` on `Paused` → `accept`
4. `Restart` on `Failed` → `accept`
5. `Drain` on `Running` → `accept`
6. `Start` on `Running` → `noop`
7. `Resume` on `Starting` → `defer`，超时后转 `reject`

---

# 4. 冻结结论
本 V3 版完成了实现前最缺的三块：
1. `SubagentReport` 的 schema 契约固定
2. `MemoryAdmissionPolicy` 的账本更新时机固定
3. `ContextEngineLifecycle` 的命令-状态行为表固定

这版已经可以直接作为：
- Rust trait / struct / enum 草案输入
- 序列化层定义输入
- 状态机测试输入
- 准入策略测试输入
