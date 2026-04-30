# DeepSeek 并行任务拆分（2026-04-30）

## 结论
适合继续外包给 DeepSeek 网页版的，是**独立、推理重、不会阻塞本地主线**的三块；不适合外包的是需要直接改本地代码并立刻跑测收口的部分。

## 可以派给 DeepSeek 网页版的任务

### DS-1：真实 Browser Driver 设计收口
目标：把 `RealBrowserDriver / BrowserProviderDriver / ProviderBackedRealBrowserDriver` 三层边界再压实。

要求它输出：
1. 最小 trait 设计修订建议
2. 命令序列是否还缺步骤
3. 错误分类建议（登录失效 / DOM缺失 / 等待超时 / 输出抓取为空）
4. 哪些字段应进入 `DispatchReceipt` / `WorkerOutput` / transcript
5. 一份不改大方向的最小 patch 方案

限制：
- 不能把 BrowserWorker 升格为主线
- 不能引入大而全框架
- 必须保持 driver-first 边界

### DS-2：BrowserWorker service 接真实 driver 的集成测试清单
目标：给 `BrowserWorkerDemoService` 到真实 driver 的测试矩阵。

要求它输出：
1. happy path
2. page not ready
3. prompt submit success but no assistant turn
4. assistant turn exists but capture empty
5. reconnect / retry 是否需要第一版支持
6. 哪些测试该单测，哪些该 integration test

限制：
- 先测 opencli 风格 workflow，不扩桌面自动化大全
- 不要写假的“已验证”，只给测试设计和判定标准

### DS-3：上下文引擎下一阶段规格草案
目标：围绕主线，提前产出 context engine 的下一版最小规格，而不是继续碰 BrowserWorker 细节。

要求它输出：
1. context packing 最小数据结构
2. trimming / ranking / budget merge 顺序
3. 和 `MemoryRecallPipeline` 的衔接点
4. 和 `RuntimeRequest/RuntimeResult` 的边界
5. 第一版最少 5 条红测建议

限制：
- 必须服务三大主线之一“上下文管理”
- 不准发散到产品层 UI

## 不该派给 DeepSeek 网页版的部分

### 1. 本地 CLI / runtime 真收口
这些要直接在本地写、跑、测：
- `src/main.rs`
- `agent_runtime`
- `responder`
- `memory_store_sqlite`

### 2. 任何需要马上以 cargo test 为准的改动
因为网页外脑给的是草案，不是本地事实。

### 3. 关键架构最终裁决
最终结构收口仍由小创本地完成。

## 当前小创本地继续推进的方向
1. 把 CLI 从一次性 `run --input` 推到 REPL
2. 给 runtime 补 provider seam，为接真模型做准备
3. 把 recall/context 主线往真正的 context engine 再推进半步

## 推荐派发顺序
1. 先派 **DS-3（context engine 规格）** —— 最贴主线
2. 再派 **DS-1（真实 driver 设计收口）** —— 保持 BrowserWorker 不跑偏
3. 最后派 **DS-2（真实 driver 测试矩阵）** —— 给后续落地做验证框架
