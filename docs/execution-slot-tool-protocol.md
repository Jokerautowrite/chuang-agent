# Execution Slot Tool Protocol

更新时间：2026-05-03

## 定位

Execution Slot 是主进程的本地工具骨架。

当前实现是 `generic_agent_mvp`：

- 工具骨架来自 GenericAgent 9 原子工具。
- 执行安全链路复用 Chuang 的治理、审计、workspace 边界和结构化 report。
- 真实桌面控制、浏览器控制、外部智能体调度暂不直接混进主线。

## GA 9 原子工具

当前 manifest 固定为：

```text
mouse
keyboard
screenshot
locate
file_read
file_write
code_execute
wait
human_suspend
```

当前可执行映射：

```text
file_read    -> tool_runtime.read_file
file_write   -> tool_runtime.write_file
code_execute -> tool_runtime.shell_exec
```

当前接口态映射：

```text
mouse        -> actuator.click
keyboard     -> actuator.input_text
screenshot   -> actuator.screenshot
locate       -> actuator.observe
wait         -> not executable yet
human_suspend -> not executable yet
```

`list_dir` 是辅助工具，不属于 GA 9 原子工具。

状态面和 doctor 现在会把原子工具拆成两组名单：

- `mapped_atomic_tool_names`: `file_read`, `file_write`, `code_execute`
- `interface_only_atomic_tool_names`: `mouse`, `keyboard`, `screenshot`, `locate`, `wait`, `human_suspend`

这样可以直接区分“当前可执行映射”与“仅接口登记”的桌面能力，不再只靠 `status` 字段人工判断。

## 模型调用协议

模型优先输出 `ACTION`：

```text
ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"file_read","path":"src/main.rs"}}
ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"file_write","path":"notes/out.txt","content":"hello"}}
ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"code_execute","command":"cargo test -q","cwd":"."}}
ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"list_dir","path":"."}}
ACTION: {"schema_version":1,"type":"final","answer":"最终答复"}
```

`schema_version` 当前为 1；缺省时按 v1 兼容处理，高于当前支持版本会返回 `unsupported_action_schema_version` 协议错误。

当前代码中的 action schema 常量：

```text
ToolActionEnvelope::schema_version() = 1
ToolActionEnvelope::schema_fields()
ToolActionEnvelope::call_schema_fields()
```

协议回归要求：

- `schema_version` 缺省时只兼容当前 v1，高于当前版本必须回灌 `unsupported_action_schema_version`。
- `ACTION` 前缀缺失、JSON 错误、字段缺失、空 final answer 都必须变成 `protocol_error`，不得被当作普通最终回复。
- `schema_fields()` / `call_schema_fields()` 的顺序和内容是对控制台、通道、doctor 的契约，改字段时必须同步升级测试和文档。

兼容旧工具名：

```text
read_file
write_file
shell_exec
```

暂不接受桌面接口态工具作为 `ACTION` 直接调用。

格式错误的 `ACTION` / `TOOL_CALL` 不会被当作最终回复；主进程会把 `protocol_error` 回灌给模型，要求它修正为正式 `ACTION` JSON 或输出 `FINAL:`。
首轮普通文本仍可作为直接答复；一旦进入工具往返，后续普通文本会被视为 `plain_text_response` 协议错误，继续回灌给模型。

## 治理与审计

`ExecutionSlot` 会先构造 `ProposedAction`，再交给治理层分类。

`code_execute` 的 shell 风险分类使用 `ShellRiskRules`，默认规则覆盖删除/清理、服务变更、网络调用、密钥访问四类。配置文件可覆盖这些模式：

```toml
[tool_loop.risk]
delete_or_cleanup = " rm , git reset --hard"
service_change = " systemctl , service "
network_change = " curl , wget , ssh "
secret_access = " .env, token, secret, password"
```

配置只改变风险分类；真正是否执行仍由治理层决定。

原子工具 action id：

```text
tool:file_read
tool:file_write
tool:code_execute
```

辅助工具 action id：

```text
tool:list_dir
```

审计 operation：

```text
tool.file_read
tool.file_write
tool.code_execute
tool.list_dir
```

被治理拒绝时追加 `.rejected`。

主进程工具循环不会因为可预期的治理拒绝直接中断；`NeedsApproval / DraftOnly / Blocked` 会以 `ok=false`、`failure_class=governance_rejected` 的 `ToolExecutionRecord` 回灌给模型，模型应解释原因并收口，而不是继续强行执行。

治理拒绝回灌要求：

- `NeedsApproval`、`DraftOnly`、`Blocked` 都不能执行真实工具动作。
- 回灌记录必须保留 `atomic_tool_name`、结构化 `decision` 和 `failure_class=governance_rejected`。
- 审计 operation 必须使用 GA 原子工具名并追加 `.rejected`，例如 `tool.code_execute.rejected`。

## Report 字段

当前代码中的 schema 常量：

```text
ToolLoopReport::schema_version() = 6
ToolLoopReport::schema_fields()
ToolLoopReport::call_schema_fields()
```

每次工具调用写入 `ToolExecutionRecord`：

```text
tool_name          兼容旧协议名，如 read_file/write_file/shell_exec/list_dir
atomic_tool_name   GA 原子工具名；list_dir 为 none
ok                 是否执行成功
decision           治理结果
duration_ms        执行耗时
retryable          是否建议重试
target_path/resolved_path
cwd/command
entries             list_dir 的结构化 name/kind 列表
output_bytes/output_lines
stderr_bytes/stderr_lines
output/stdout/stderr
exit_code
changed_files
write_before_bytes/write_after_bytes/write_changed
write_operation (enum: created/modified/unchanged)
write_diff_preview/write_diff_truncated
failure_class
output_redacted/stdout_redacted/stderr_redacted
*_truncated
call               原始结构化调用
```

`write_diff_preview` 是有界预览，不是完整补丁；命中 `.env`、token、secret、password、private key 等疑似敏感路径或内容时只返回脱敏占位。
`read_file` 和 `code_execute` 的文本输出命中疑似密钥路径或内容时也会返回脱敏占位，并通过 `*_redacted` 字段标记；`*_bytes / *_lines` 仍描述原始输出规模。
`write_operation` 只允许 `created / modified / unchanged`，上层不要再从 `write_before_bytes / write_changed` 反推写入语义。

通道层输出应保留 `atomicTool` 字段，和 runtime meta 的 `tool_report_json` 对齐。

## App Server / Channel 字段

`app-server turn/start` 的 `turn/completed` 事件和最终 response 都应输出：

```text
toolCallCount
toolProtocolErrorCount
toolTrace
toolReport
toolCalls
toolProtocolErrors
toolEvents
```

`toolCalls[]` 内每个调用应包含：

```text
tool
atomicTool
ok
summary
decision
durationMs
retryable
targetPath/resolvedPath
cwd/command
entries
outputBytes/outputLines
stderrBytes/stderrLines
output/stdout/stderr
exitCode
changedFiles
writeBeforeBytes/writeAfterBytes/writeChanged
writeOperation
writeDiffPreview/writeDiffTruncated
failureClass
outputRedacted/stdoutRedacted/stderrRedacted
outputTruncated/stdoutTruncated/stderrTruncated
call
```

`channel simulate --json` 应输出同源字段：

```text
tool_call_count
tool_protocol_error_count
tool_trace
tool_report
tool_calls
tool_protocol_errors
tool_events
```

`toolEvents[]` 是结构化事件流，`tool_call` 事件至少包含：

```text
round
kind
tool_name
atomic_tool_name
decision
ok
failure_class
duration_ms
retryable
summary
```

`protocol_error` 事件包含 `protocol_error_code / protocol_error_message`。`summary` 只给人看，上层判断优先用结构化字段。

飞书真实回复文本不强行携带工具详情；工具详情留给桥日志、控制台或调试 JSON。

## 当前边界

- 不做真实桌面控制。
- 不开放 `mouse/keyboard/screenshot/locate` 为可执行 `ACTION`。
- 不新增第十个 Slot。
- 子代理和外部智能体仍在下游阶段，不抢主进程 Execution Slot 主线。
- BrowserWorker 旧线冻结；搜索/网页 AI 走 GenesisActuator 或统一身份引擎插件线。
