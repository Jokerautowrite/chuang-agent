# 网页 Agent 浏览器接管与并行协作实现方案

## 结论
把“浏览器接管网页端 Agent 并把它当外脑 worker 调度”做成 `chuang-agent` 的内建能力，技术上可行，而且今天已经验证了最关键的一步：**本地主控可以边写代码边通过浏览器驱动另一侧网页 Agent协作**。

这条能力后续不只服务 DeepSeek，也适用于 ChatGPT 网页、Claude 网页、以及其他任何有稳定输入框/输出区的网页 Agent。

---

## 一、目标定义

### 1.1 目标
在 `chuang-agent` 中新增一条能力链路：

- 主控 Agent 负责：
  - 任务拆分
  - 上下文裁剪
  - 选择网页 Agent
  - 驱动浏览器发消息
  - 读取网页输出
  - 校验结果
  - 收口整合

- 网页端 Agent 负责：
  - 补规格
  - 生成草案
  - 并行推理
  - 输出候选实现/方案/测试点

### 1.2 本质
这不是“做一个浏览器自动化脚本”，而是：

**把网页端对话式 Agent 抽象成一个可调度的远程 worker。**

也就是：
- 入口：prompt/task
- 通道：browser session
- 输出：message transcript / structured response
- 主控：本地 `chuang-agent`
- 编排方式：像子代理一样调度，但底层 transport 是浏览器 DOM，不是 API

---

## 二、今天已验证的关键事实

### 2.1 已验证成功的部分
1. 本地可以稳定维护 `chuang-agent` 项目目录与进度日志
2. 本地可以边实现 Rust 骨架边跑 `cargo test`
3. 可以把 DeepSeek 协作上下文落到本地文件：
   - `context/deepseek-handoff.txt`
   - `context/deepseek-next-prompt.txt`
4. 可以把协作过程文档化：
   - `docs/deepseek-spec-draft.md`
   - `docs/deepseek-spec-v2-draft.md`
   - `docs/deepseek-spec-v3-draft.md`
   - `docs/deepseek-impl-prep-draft.md`
5. 已形成“主控本地收口、网页 Agent 并行补料”的工作方式

### 2.2 暴露出的真实问题
今天也暴露了三个关键坑：

#### 坑1：只写本地提示词，不等于真的发给网页 Agent
- 本地写入 prompt 文件只是中间态
- 如果没有真实驱动浏览器输入框并提交，对方根本没开始干活

#### 坑2：DeepSeek 默认会落在快速模式
- 快速模式不适合做重规格/重实现协作
- 默认必须切到**专家模式**后再发任务

#### 坑3：网页对话是易失态，必须落盘
- 浏览器窗口停住、刷新、断链、new 会导致上下文丢失
- 所以必须把：
  - 当前任务
  - 已发提示
  - 已收结果
  - 本地判断
  持续写回本地项目文件

---

## 三、能力设计：把网页 Agent 抽象成 BrowserWorker

### 3.1 核心抽象
建议在 `chuang-agent` 内部引入一个新概念：

```text
BrowserWorker
```

它代表一个“通过浏览器会话承载的外部 Agent worker”。

### 3.2 统一接口
建议抽象出统一接口：

```rust
trait ExternalWorker {
    fn send_task(&mut self, task: WorkerTask) -> Result<DispatchReceipt, WorkerError>;
    fn read_output(&mut self) -> Result<WorkerOutput, WorkerError>;
    fn is_busy(&self) -> bool;
    fn sync_state(&mut self) -> Result<WorkerState, WorkerError>;
}
```

然后实现一种具体 worker：

```rust
BrowserWorker<DeepSeekWeb>
BrowserWorker<ChatGPTWeb>
BrowserWorker<ClaudeWeb>
```

也就是说，未来网页 Agent 只是 provider 不同，主控逻辑是一套。

---

## 四、模块拆分建议

### 4.1 新增模块建议
```text
src/
├── browser_worker/
│   ├── mod.rs
│   ├── types.rs
│   ├── session.rs
│   ├── prompt_queue.rs
│   ├── transcript.rs
│   ├── adapters/
│   │   ├── deepseek_web.rs
│   │   ├── chatgpt_web.rs
│   │   └── claude_web.rs
│   └── coordinator.rs
```

### 4.2 模块职责

#### `types.rs`
定义通用类型：
- `WorkerTask`
- `DispatchReceipt`
- `WorkerOutput`
- `WorkerState`
- `BrowserMode`
- `ProviderKind`

#### `session.rs`
负责维护浏览器会话状态：
- 当前 provider
- 当前页面 URL
- 是否已登录
- 是否在专家模式
- 最近一次成功发送时间
- 最近一次成功抓取时间

#### `prompt_queue.rs`
管理待发消息与重试：
- prompt enqueue
- 待发/已发/失败/需重发
- 避免“我以为发了，其实没发”

#### `transcript.rs`
把网页侧对话输出抽成结构化 transcript：
- 用户输入
- assistant 输出
- message_id
- provider
- timestamp
- snapshot ref

#### `adapters/deepseek_web.rs`
封装 DeepSeek 网页专属行为：
- 找输入框
- 切专家模式
- 判断是否思考中
- 抓最后一条回复
- 处理页面停住/验证码/未登录

#### `coordinator.rs`
给主控层用的编排器：
- 发任务
- 轮询结果
- 超时
- 失败重试
- 输出交给主控校验

---

## 五、BrowserWorker 状态机建议

### 5.1 状态定义
```text
Uninitialized
Ready
SwitchingMode
Dispatching
WaitingResponse
ReadingResponse
Completed
Failed
Blocked
```

### 5.2 关键状态含义
- `Ready`：可发新任务
- `SwitchingMode`：正在从默认模式切专家模式
- `Dispatching`：正在输入并提交 prompt
- `WaitingResponse`：网页端正在生成
- `ReadingResponse`：正在抓输出
- `Blocked`：遇到登录、验证码、页面异常、DOM变化

### 5.3 必须记录的状态字段
```rust
struct BrowserWorkerSession {
    worker_id: String,
    provider: ProviderKind,
    mode: BrowserMode,
    page_url: String,
    logged_in: bool,
    last_prompt: Option<String>,
    last_prompt_hash: Option<String>,
    last_output_hash: Option<String>,
    last_dispatch_at: Option<String>,
    last_read_at: Option<String>,
    state: WorkerState,
}
```

---

## 六、发送链路设计

### 6.1 发送前必须做的事
每次发任务前必须先校验：
1. 当前网页是否正确
2. 当前账号是否已登录
3. 当前 provider 是否正确
4. 当前是否在专家模式
5. 输入框是否可编辑
6. 是否仍在上一轮生成中

### 6.2 标准发送流程
```text
prepare_task
  -> sync browser state
  -> ensure provider page alive
  -> ensure mode = expert
  -> focus input box
  -> paste prompt
  -> submit
  -> record dispatch receipt
  -> poll until response completes
  -> read final output
  -> persist transcript
```

### 6.3 DispatchReceipt 建议
```rust
struct DispatchReceipt {
    task_id: String,
    worker_id: String,
    provider: ProviderKind,
    submitted_at: String,
    prompt_hash: String,
    mode: BrowserMode,
    status: DispatchStatus,
}
```

---

## 七、专家模式保障机制

### 7.1 为什么要单独做
今天已经证明：**如果不显式切专家模式，默认就可能落到快速模式。**
这会直接降低产出质量。

### 7.2 设计要求
`DeepSeekWebAdapter` 必须提供：

```rust
fn ensure_expert_mode(&mut self) -> Result<(), WorkerError>
```

### 7.3 行为要求
- 读取当前模式标签
- 若不是专家模式，点击切换
- 切完后再次校验
- 校验失败则阻断发送
- 不允许“未确认已切成功”就继续发 prompt

### 7.4 日志要求
日志里必须留下：
- 切换前模式
- 切换动作时间
- 切换后模式
- 是否校验通过

---

## 八、输出抓取与结构化

### 8.1 不要只拿肉眼可见文本
建议输出结构化对象：

```rust
struct WorkerOutput {
    worker_id: String,
    provider: ProviderKind,
    task_id: String,
    content: String,
    raw_snapshot_ref: Option<String>,
    completed_at: String,
    finish_reason: WorkerFinishReason,
}
```

### 8.2 需要区分的结束态
- `Completed`
- `StoppedEarly`
- `Blocked`
- `NetworkError`
- `ManualInterruption`

### 8.3 Transcript 建议落盘位置
```text
context/browser-worker-transcripts/
```

每一轮至少存：
- prompt.txt
- output.md
- meta.json

这样 new 后仍能恢复，而不用把整段网页内容全塞上下文。

---

## 九、与 chuang-agent 现有能力的结合点

### 9.1 和 SubagentReport 的关系
网页 Agent worker 可以产出一份轻量版 `SubagentReport`：
- `task_id`
- `agent_id = browser-worker/deepseek`
- `summary`
- `artifacts`
- `stdout_preview = output excerpt`
- `truncated`

这样浏览器 worker 和本地子代理在主控层可以统一归档。

### 9.2 和 MemoryAdmissionPolicy 的关系
网页 worker 虽然不占本地大模型推理 token，但会占：
- 主控上下文预算
- 浏览器轮询时间
- transcript 存储

所以后面可以把它抽象为另一类资源配额：
- browser slot
- transcript bytes
- pending task count

### 9.3 和 Lifecycle 的关系
网页 worker 也能挂进状态机：
- `Start`：打开/接管网页 worker
- `Pause`：暂停发新任务
- `Resume`：恢复派发
- `Drain`：只等现有任务结束
- `Stop`：断开浏览器协作链路

---

## 十、最小可行版本（MVP）

### 10.1 第一阶段目标
先只支持：
- 单 provider：DeepSeek 网页
- 单 worker
- 单窗口
- 串行任务派发
- 专家模式强校验
- 本地 transcript 落盘

### 10.2 MVP 必须具备的能力
1. 读取本地 prompt 文件
2. 驱动网页输入并提交
3. 确认已进入专家模式
4. 轮询直到输出完成
5. 抓最后一条回复
6. 落盘 `prompt/output/meta`
7. 给主控返回结构化结果

### 10.3 MVP 暂时不做
- 多网页 provider 统一适配
- 自动验证码处理
- 多窗口并发
- 自动恢复跨浏览器崩溃
- DOM 大规模自适应修复

---

## 十一、第二阶段演进

### 11.1 多 worker 并行
支持：
- DeepSeek 网页 worker
- ChatGPT 网页 worker
- Claude 网页 worker

主控可同时派 2~3 个任务，最后收口比对。

### 11.2 结果交叉验证
同一任务发给两个网页 Agent：
- A 出规格草案
- B 做反审
- 主控整合

### 11.3 Prompt 模板系统
为不同任务类型保存模板：
- 规格补全
- Rust trait 草案
- 测试清单
- code review
- 反例攻击

---

## 十二、失败模式与防翻车设计

### 12.1 假发送
症状：本地以为发了，网页其实没提交。

防护：
- 发送后必须读取对话流是否出现本轮 prompt
- 无回显则视为发送失败

### 12.2 假完成
症状：网页还在生成，但主控误判为已完成。

防护：
- 轮询“生成中/停止生成”相关DOM标记
- 最后一次文本快照稳定后再收取

### 12.3 模式漂移
症状：默认又回到快速模式。

防护：
- 每轮发任务前都重新校验 mode
- 不要假设上次切过，这次还在

### 12.4 上下文丢失
症状：窗口刷新/new 后不知道干到哪。

防护：
- 本地 `progress-log.md`
- 本地 transcript 目录
- 每轮 prompt 文件落盘

### 12.5 DOM 变更
症状：网页按钮或输入框选择器失效。

防护：
- provider adapter 单独封装
- DOM 变了只改 adapter，不动编排层

---

## 十三、落地到当前项目的实施顺序

### Phase 1：文档冻结
1. 把本方案文档落到项目 docs
2. 在 `README.md` 补 browser worker 目标
3. 在 `progress-log.md` 记录 browser worker 作为正式能力线

### Phase 2：类型层
新增：
- `browser_worker/types.rs`
- `browser_worker/session.rs`
- `browser_worker/transcript.rs`

### Phase 3：DeepSeek adapter
新增：
- `browser_worker/adapters/deepseek_web.rs`

先只定义：
- `ensure_expert_mode`
- `send_prompt`
- `read_last_output`
- `wait_until_complete`

### Phase 4：主控编排
新增：
- `browser_worker/coordinator.rs`

### Phase 5：接进现有主控
让 `chuang-agent` 可以像调子代理一样调一个 `BrowserWorker`。

---

## 十四、建议的数据文件约定

```text
context/
├── deepseek-handoff.txt
├── deepseek-next-prompt.txt
└── browser-worker-transcripts/
    ├── 2026-04-30-task-001/
    │   ├── prompt.txt
    │   ├── output.md
    │   └── meta.json
    └── ...
```

`meta.json` 建议字段：
```json
{
  "task_id": "task-001",
  "provider": "deepseek_web",
  "mode": "expert",
  "submitted_at": "2026-04-30T14:30:00+08:00",
  "completed_at": "2026-04-30T14:31:12+08:00",
  "status": "completed",
  "prompt_hash": "...",
  "output_hash": "..."
}
```

---

## 十五、最终判断

### 15.1 这件事为什么重要
它意味着：

**以后小创不只会自己干活，还能接管网页端 Agent 给自己打工。**

主控不再只是单核执行，而是：
- 本地实现
- 网页外脑补料
- 多 Agent 交叉验证
- 小创统一收口

### 15.2 对创项目的价值
对 `chuang-agent` 来说，这条能力非常像未来的第四根支柱：
1. 长期记忆
2. 子代理调度
3. 上下文管理
4. **浏览器接管型外部 Agent 调度**

这不是边角料，是正经内核能力。

### 15.3 建议
建议把它正式命名为：

```text
BrowserWorker / WebAgentWorker / BrowserAgentBridge
```

我倾向：

```text
BrowserWorker
```

短、准、可扩展。
