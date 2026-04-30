# 协作进度日志

## 2026-04-30

### 已完成
- 建立 Rust crate：`/home/user/projects/chuang-agent`
- 落下基础目录：`common / subagent_report / memory_policy / lifecycle / tests`
- 写入 V3 对应 types/traits skeleton
- 生命周期真值表已先实现 6 条最小规则并通过测试：
  - `Start × Uninitialized -> Accepted(Starting)`
  - `Start × Running -> Noop`
  - `Resume × Starting -> Deferred`
  - `Checkpoint × Paused -> Rejected`
  - `Restart × Failed -> Accepted(Restarting)`
  - `Drain × Running -> Accepted(Draining)`
- 当前 `cargo test` 全量通过

### 当前分工
- 小创：主线收口、验证、落盘进度
- 小承（DeepSeek）：并行补 BrowserWorker 能力线最小闭环（session / transcript / coordinator / adapter / error / hash）

### 待做
1. 收小承后续产出并继续合并到本地文档/代码
2. 继续把 BrowserWorker 作为并行能力线推进，不抢三大核心主线
3. BrowserWorker 下一轮可补：adapter trait 真正对接 browser provider、`apply_task`/`apply_receipt` 也进一步统一到完整状态机、placeholder 时间戳/快照引用换成真实来源

### 最新进展
- lifecycle 真值表已扩到 14 条测试并全通过
- lifecycle engine 最小实现已落地，`handle_command / drive_deferred / current_state` 可用
- `lifecycle_engine_tests.rs` 新增 3 条并全部通过
- memory_policy 的 `try_reserve / commit / release_reservation / reclaim` 最小闭环已落地
- `ReservationToken::is_expired_at()` 已落地，TTL判断接口已补上
- `EvictionPlan::candidate_count()` 已落地，驱逐占位结构更完整
- `memory_policy_tests.rs` 已扩到 14 条并全部通过
- memory_policy 的 `admit_with_eviction()` 最小原子驱逐闭环已落地：先校验候选，再批量驱逐，再写入新 allocation
- memory_policy 的 `expire_reservations_at()` 已落地：可按时间批量释放过期 reservation
- SubagentReport validator/builder 最小闭环已落地
- `subagent_report_tests.rs` 已扩到 8 条并全部通过
- BrowserWorker MVP 模块骨架已落下：`types / session / transcript / coordinator / adapters::deepseek_web`
- BrowserWorker session 最小闭环已落地：`new / apply_task / apply_receipt / apply_output`
- BrowserWorker transcript 最小闭环已落地：`BrowserTranscript::new / start_record / complete_record`
- BrowserWorker coordinator 最小闭环已落地：`enqueue / attach_receipt / attach_output`
- BrowserWorker adapter trait 最小闭环已落地：`BrowserWorkerAdapter + adapter_session / adapter_ensure_expert_mode / adapter_mark_ready`
- BrowserWorker 错误返回最小闭环已落地：`BrowserWorkerError` + coordinator/session 关键路径 `Result` 化
- BrowserWorker 真实稳定 hash 已落地：`src/browser_worker/hash.rs`，当前使用 FNV-1a 64-bit 十六进制输出
- BrowserWorker adapter 已新增最小 dispatch/read 抽象：`submit_task / read_output`
- `DeepSeekWebAdapter` 已接上 dispatch/read 占位实现，并能真实驱动 session 状态从 `Ready -> Dispatching -> WaitingResponse -> Completed`
- BrowserWorker 新增/更新测试：
  - `browser_worker_adapter_trait_tests.rs` 扩到 5 条
  - `browser_worker_coordinator_tests.rs` 扩到 8 条
  - session 单测扩到 7 条
- 小创刚重新实测：当前仓库 `cargo test` **全绿**

### 约束
- 进度必须持续写入本文件，避免 new 后丢失上下文
- 最终以本地代码和测试为准，不以网页对话停留状态为准
- BrowserWorker 是并行能力线，不能反客为主抢掉长期记忆/多子代理/上下文管理三大主线
