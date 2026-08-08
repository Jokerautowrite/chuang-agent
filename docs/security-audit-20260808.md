# Chuang Agent 全面审计报告（2026-08-08）

> 审计日期：2026-08-08 · 审计范围：安全 / 性能 / 测试 / 审计追溯
> 结论：工程质量高，无高危漏洞；本次修复 4 处，确认 2 处取舍。
> 关联 commit：`c977a30`、`7adaba8`、`65126f2`、`c4d82e7`

## 一、本次修复（4 项，已全部推送）

| Commit | 修复 | 说明 |
| --- | --- | --- |
| `c977a30` | SQLite 连接复用 + WAL + busy_timeout | `memory_store_sqlite.rs` 此前每次操作重开连接；改单连接 + WAL + busy_timeout=5s |
| `7adaba8` | app_server missing_content 测试 mock 处理自动重试 | 200+空 content 触发自动重试，mock 只 accept 一次导致重试连接被拒 |
| `65126f2` | browser 测试隔离 CDP state dir | 脚本 fallback 读真实 state 文件，测试无法模拟"无 CDP" |
| `c4d82e7` | **审计记录持久化 + trait 覆盖 bug** | 审计只存内存 Vec + StaticRuleGovernance.audit_records 是 inherent 方法，经泛型调用走 trait 默认空实现（读出来恒空） |

## 二、确认安全（审计通过）

| 维度 | 结论 |
| --- | --- |
| 治理规则 | 16 条宪法；Delete/Payment/Secret/提权默认 NeedsApproval |
| unrestricted 模式 | 全部放行但每次工具调用都记 AuditRecord（保留审计） |
| 路径逃逸 | canonicalize + starts_with + symlink 防绕过 |
| SQL 注入 | 全参数化（params!） |
| 子代理 | allowlist 精确匹配 + capability 路由 + approve 前置三层防护 |
| TLS | 自定义 CA + native roots，`with_no_client_auth` 正确 |
| CDP | 硬编码 127.0.0.1，无 SSRF |
| actuator | allowlist 严格拒绝非白名单应用 |
| 依赖 | hyper 1.9 / rustls 0.22 / rusqlite 0.31 / tokio 1.52，有 Cargo.lock |
| 故障恢复 | 3 次重试 + 退避 + fallback 链；工具执行后不重试（保护副作用） |
| 并发 | Arc<Mutex> 使用克制，无嵌套锁风险 |

## 三、确认的取舍（合理，维持现状）

1. **事件账本纯内存**：`InMemoryRuntimeEventLedger` 单次运行内可重放；turn 级已持久化（session_turn_archive），事件级不落库。结论可追溯，过程细节不持久。
2. **大文件未拆**：`cli_runtime.rs` 9251 行 / `main.rs` 3483 行。运行无影响，迭代成本可控时不拆。

## 四、代码健康度

- 源码 76,524 行（93 模块）；测试 51,852 行（68% 比例）
- 正式代码 unwrap/expect 仅 17 处（测试区大量使用是正常的）
- 完整测试套件全绿（lib 114 + 集成 200+）
- 无 TODO/FIXME/unimplemented 残留

## 五、审计后状态

- 测试：lib 114 全过，完整套件全绿
- 待关注：审计记录现已随 turn meta 持久化，但完整事件级审计链仍依赖 turn 存档，如需事件级落库可后续扩展
