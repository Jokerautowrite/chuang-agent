# chuang-agent

创项目本地 Agent 内核 MVP。

## 当前目标

先打通一条稳定、可审计、可插拔的最小主链：

```text
input -> identity/memory -> context -> runtime -> governance -> report -> memory
```

核心只保留身份、记忆、上下文、治理和报告。provider、子代理、桌面/浏览器、控制面、飞书等外部能力走 slot / adapter / plugin。

## 当前状态

- `cargo run -- doctor`：安全健康检查，校验配置、身份记忆、slot 装配和隔离 runtime smoke。
- `cargo run -- status`：查看核心状态。
- `cargo run -- run --input TEXT`：跑一轮本地 runtime。
- `cargo run -- run --input TEXT --remember`：跑完后写回 SQLite turn summary。
- `cargo run -- subagent dispatch --task TEXT`：写入子代理 dispatch 文件队列。
- `cargo run -- subagent run-once --runner fake`：用 fake runner 处理一个 pending dispatch。
- `cargo run -- subagent run-once --runner command --runner-command PATH --approve-exec`：显式执行外部 runner，并把输出收成 report。
- `GenesisActuator`：新版网页 AI 查询插件线，旧 `BrowserWorker` 暂停推进。
- `cargo test`：全量回归。

当前 MVP 边界见 `docs/mvp-scope.md`，核心边界见 `docs/core-boundary.md`，长期进度见 `docs/progress-log.md`。

## 目录约定

- `src/`：Rust 实现。
- `docs/`：规格草案、架构说明、评审结论
- `tests/`：MVP 合同和回归测试。
- `context/`：协作上下文、提示词、窗口接续材料。
