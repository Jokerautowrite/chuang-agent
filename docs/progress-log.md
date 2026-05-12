# 协作进度日志

# 2026-05-12 third-test 复验覆盖 provider receipt 边界门禁
- 本轮在 `1d04c49` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 复验确认 provider receipt 边界字段已进入 candidate/third-test 的 live operator receipt 模板断言；GoalRun checkpoint count 到 102，checkpoint log complete 为 true，provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 candidate/third-test 锁住 provider receipt 只读边界
- 本轮把 provider receipt 新增的 `does_not_call_provider` 与 `does_not_read_provider_readiness` 继续抬到候选门禁：`scripts/chuang-candidate-verify.sh` 和 `scripts/chuang-third-test-smoke.sh` 的 live operator receipt 模板断言现在会检查 provider evidence 保留这两个字段。
- `tests/cli_smoke_tests.rs` 与 `tests/live_operator_scripts_tests.rs` 已锁住 wrapper 不会丢掉这些断言；`sh scripts/chuang-candidate-verify.sh` 复验通过，仍只读 receipt 模板、不连接 provider、不打印 secret。验证已通过 `cargo test -q --test cli_smoke_tests --test live_operator_scripts_tests`、`sh scripts/chuang-candidate-verify.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 live receipt 文档同步 provider 只读边界
- 本轮同步 `docs/live-receipt-collection.md` 的 provider evidence 字段表，把 `does_not_call_provider` 与 `does_not_read_provider_readiness` 纳入 provider receipt/operator receipt collector 文档口径，和刚落地的 `chuang-provider-live-receipt.sh`、operator receipt template、collector JSON 输出保持一致。
- 本轮只改文档，不运行真实 provider、不读取 secret、不触碰 Hermes；用于避免后续人工 overlay 仍只填 provider ref 而漏掉“模板不发请求、不读 readiness”的边界字段。验证已通过文档 diff 审计与 `git diff --check`。

# 2026-05-12 operator receipt provider evidence 同步只读边界
- 本轮继续沿 M5/M7 live receipt 主链对齐 provider evidence：`scripts/chuang-live-operator-receipt.sh` 与 `scripts/chuang-live-operator-receipt-collect.sh` 的 provider service evidence 现在也带出 `does_not_call_provider=true`、`does_not_read_provider_readiness=true`，和独立 `chuang-provider-live-receipt.sh --json` 同口径。
- `tests/live_operator_scripts_tests.rs` 与 `tests/live_operator_receipt_collect_tests.rs` 已锁住模板与 collector 合并后保留这两个边界字段；仍然只生成/合并本地 receipt，不连接 provider、不读取 secret。验证已通过 `cargo test -q --test live_operator_scripts_tests --test live_operator_receipt_collect_tests`、`cargo test -q --test provider_live_receipt_tests --test live_operator_receipt_collect_tests --test live_operator_scripts_tests`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 provider live receipt JSON 边界字段补齐
- 本轮继续沿 M5/M7 live receipt 状态面补一致性：`scripts/chuang-provider-live-receipt.sh --json` 现在和 help/text 输出一样显式带出 `does_not_call_provider=true` 与 `does_not_read_provider_readiness=true`，避免 JSON 消费方只看到 `connects_real_provider=false` 而漏掉 provider receipt 模板不读 readiness、不发请求的边界。
- `tests/provider_live_receipt_tests.rs` 已锁住新增字段；本轮仍只生成本地只读 receipt 模板，不连接 provider、不读取 secret、不打印 secret。验证已通过 `cargo test -q --test provider_live_receipt_tests`、`cargo test -q --test provider_live_receipt_tests --test live_operator_receipt_collect_tests --test live_operator_scripts_tests --test feishu_live_receipt_tests --test live_runner_rehearsal_receipt_tests`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 channel JSON 锁住 tool surface callable meta
- 本轮继续沿 M7 channel 输出面补 JSON 回归：`tests/cli_channel_tests.rs` 的基础 `channel simulate --json` 用例现在除 `tool_surface.available/governed` 与 callable tools 结构体外，也锁住 `runtime_observability.tool_surface_callable_tools` 包含 `file_read`，让 channel JSON 和 app-server turn/completed 事件面使用同一套 runtime observability tool surface 摘要。
- 该回归不改变运行逻辑、不打印 raw tool trace/payload，也不连接真实 Feishu 或 provider。验证已通过 `cargo test -q --test cli_channel_tests cli_channel_simulate_runs_workspace_config_without_fake_responder`、`cargo test -q --test cli_channel_tests --test app_server_tests --test runtime_report_tests`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 third-test 复验覆盖 GoalRun checkpoint 时间摘要
- 本轮在 `6bb88d7` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 复验确认 candidate 与 third-test 都已打印 latest GoalRun checkpoint 时间：`candidate_project_goal_run_last_checkpoint_created_at=...` 与 `project_goal_run_last_checkpoint_created_at=...`，当前 project checkpoint count 为 96，checkpoint log complete 为 true；provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 candidate/third-test GoalRun checkpoint 时间摘要
- 本轮继续把 M6/M7 GoalRun 状态证据往候选门禁抬：`scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 的 goal run status 摘要现在会打印 latest checkpoint 的 `created_at`，和 checkpoint id、完整性、worker count、validation note count 同屏展示。
- `tests/cli_smoke_tests.rs` 与 `tests/live_operator_scripts_tests.rs` 已锁住 candidate/third-test wrapper 保留该字段；candidate 复验已确认实际输出 `candidate_project_goal_run_last_checkpoint_created_at=...`。验证已通过 `cargo test -q --test cli_smoke_tests --test live_operator_scripts_tests`、`sh scripts/chuang-candidate-verify.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 GoalRun status 文本面补 checkpoint 时间
- 本轮继续沿 M6/M7 goal 状态可观测性补人读状态面：`scripts/chuang-goal-run-status.sh` 文本输出现在除 project GoalRun checkpoint id/summary、worker count、validation note count 外，也打印 `project_goal_run_last_checkpoint_created_at`，方便人工确认当前 status 面对应哪一次 checkpoint。
- JSON 面已有 `last_checkpoint_created_at`，本轮只补文本面和回归，不启动 worker、不修改运行态服务、不打印 validation note 原文。验证已通过 `cargo test -q --test cli_smoke_tests goal_run_status_script_reads_watchdog_and_overnight_status_without_actions`、`cargo test -q --test cli_smoke_tests`、`bash scripts/chuang-goal-run-status.sh` 抽样、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 app-server completed 事件面锁住 tool surface 摘要
- 本轮继续沿 M7 app-server/channel 输出面补回归：`tests/app_server_tests.rs` 的 `app_server_turn_uses_workspace_provider_config` 现在在 `turn/completed` 事件里不仅确认 `toolSurface.available/governed`，还锁住 `toolSurface.callable_tools` 包含 `file_read`/`list_dir`，并确认 `runtimeObservability.tool_surface_available`、`tool_surface_governed` 和 `tool_surface_callable_tools` 同步透出。
- 这样 app-server 事件订阅面和 `turn/start` 响应面、channel simulate JSON/文本面保持同一套 tool surface 可观测口径，不需要解析 raw trace，也不打印 payload 或 secret。验证已通过 `cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config`、`cargo test -q --test app_server_tests --test cli_channel_tests --test runtime_report_tests`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 live-gaps 防混矩阵回归加固
- 本轮继续沿 M5/M6/M7 live readiness 主链补高层测试：`tests/live_operator_scripts_tests.rs` 的 `live_gaps_check_uses_provider_env_file_when_available` 现在不只确认 provider env 脱敏为 `<set>`，还锁住 `live-gaps --json` 的 `check_name`、summary、marker、只读边界、local contract/preflight/real-live 三段矩阵，以及 `live_worker_adapter_pending`、`live_runner_gate_disabled`、`manual_operator_live_receipt_missing`、`real_external_services_not_verified` 四个 gap id。
- 这保证 candidate/third-test 依赖的 live-gaps 状态面持续表达“local/preflight ready 不等于 real live ready”，且不连接 Feishu/provider、不启动 worker、不启 live gate、不打印 secret。验证已通过 `cargo test -q --test live_operator_scripts_tests live_gaps_check_uses_provider_env_file_when_available`、`cargo test -q --test live_operator_scripts_tests --test live_operator_receipt_collect_tests --test live_runner_readiness_view_tests`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 third-test 复验覆盖 channel tool surface 文本面
- 本轮在 `293c75d` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、complete-local、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 复验确认最新 channel tool surface 文本面改动未破坏第三测试链路：candidate/third-test 仍显示 `runtime_report_surface=11/26`、`policy_tool_status=9/12`、`project_goal_run_checkpoint_count=91`、`project_goal_run_checkpoint_log_complete=true`；provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 candidate 复验覆盖 channel tool surface 文本面
- 本轮在 `d88a32f` 后从干净工作树跑通 `sh scripts/chuang-candidate-verify.sh`，继承 complete-local、goal-mode 正负 smoke、live runner rehearsal、live gaps、live runner readiness、operator checklist/receipt 和 goal run status 摘要；最终输出 `chuang_candidate_verify_ok`。
- 复验确认 channel tool surface 文本面改动未破坏候选链路：candidate 仍显示 `runtime_report_surface=11/26`、`policy_tool_status=9/12`、`project_goal_run_checkpoint_count=90`、`project_goal_run_checkpoint_log_complete=true`；provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 channel simulate 文本面透出 tool surface 摘要
- 本轮继续沿 M5/M7 channel 输出面补人读摘要：`channel simulate` 非 JSON 输出现在会打印 `tool_surface_available`、`tool_surface_governed` 与 `tool_surface_callable_tools`，让通道本地演练文本面能直接确认工具面存在且受治理约束。
- JSON 面保持原结构不变，仍保留完整 `tool_surface` / `runtime_observability` / `tool_events`；文本面只输出工具名列表和布尔摘要，不打印 raw tool trace 或协议 payload。验证已通过 `cargo test -q --test cli_channel_tests`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 M6 goal/subagent admission 矩阵复验
- 本轮在最新 app-server/channel 文本面收口后，复跑 M6 goal/subagent admission 宽矩阵：`cargo test -q --test goal_dispatch_tests --test cli_goal_tests --test subagent_tree_events_tests --test subagent_tree_ledger_tests` 全部通过。
- 复验覆盖 goal collect/step/show 的 handoff query summary、report admission refs/reason codes、subagent children admission refs，以及 legacy JSON 兼容；`cargo fmt --all --check`、`git diff --check` 同步通过，工作树保持干净。

# 2026-05-12 third-test 复验覆盖 policy/channel 文本面
- 本轮在 `54c282e` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、complete-local、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 复验确认最新 M5/M7 文本面改动未破坏候选链路：candidate/third-test 仍显示 `runtime_report_surface=11/26`、`policy_tool_status=9/12`、`project_goal_run_checkpoint_count=87`、`project_goal_run_checkpoint_log_complete=true`；provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 channel simulate 文本面透出 unified execution 摘要
- 本轮继续沿 M7 channel 输出面补文本可观测性：`channel simulate` 非 JSON 输出现在除 `tool_call_count`、协议错误计数和稳定 error codes 外，也打印 `tool_unified_execution_status` 与 `tool_unified_execution_failure_count`，让本地通道演练文本面能直接看到统一工具执行状态。
- JSON 面不变，仍保留完整 `runtime_observability`、`tool_events`、`tool_protocol_errors` 与结构化 provider meta；文本面继续不打印 raw `ACTION` payload。验证已通过 `cargo test -q --test cli_channel_tests`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 app-server health 文本面并入 policy tool status
- 本轮继续沿 M5 governance/tool descriptor 状态面补 app-server 文本口径：`app-server health` 非 JSON 输出现在和 `status` / `doctor` 一样打印 `policy_tool_status` 摘要，包含 active profile、normal local action 默认决策、高风险边界、GA descriptor 映射数和 missing 数。
- JSON 面保持原结构不变，仍保留完整 `policy_tool_status.ga_tool_descriptors` 风险字段；新增回归锁住 app-server health 文本面包含 `active_profile=local_ga`、`high_risk_boundary=external_send=require_approval` 和 `ga_tool_descriptors=9/12 missing=0`。验证已通过 `cargo test -q --test app_server_tests`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 Feishu process summary 不再输出 raw tool_trace
- 本轮继续沿 M7 Feishu 通道输出面收紧正文边界：`scripts/chuang-feishu-turn-summary.js` 的过程摘要不再展示兼容 `tool_trace` 原文，即使工具调用成功且协议错误为 0，也只显示工具调用数、统一执行状态、失败数、协议错误计数和 provider finish 摘要。
- `scripts/chuang-feishu-turn-summary-smoke.js` 新增无协议错误但 `providerMeta.tool_trace` 含 base_url/api_key 长度摘要的场景，锁住 Feishu 文本不输出 `工具轨迹`、`trace transport=` 或 `api_key=len:...`；结构化 app-server/channel JSON 仍保留 tool trace 供本地审计。验证已通过 `node scripts/chuang-feishu-turn-summary-smoke.js`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 third-test 复验覆盖 GoalRun evidence counts
- 本轮在 `78f15f3` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、complete-local、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 复验确认 M6 新增高层字段已进入 candidate 与 third-test：`project_goal_run_checkpoint_count=83`、`project_goal_run_checkpoint_log_complete=true`、latest completed worker count 为 1、validation note count 为 3；provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 GoalRun status checkpoint evidence 计数进入候选门禁
- 本轮继续沿 M6/M7 goal 状态可观测性补高层日志：`scripts/chuang-goal-run-status.sh` 文本面现在除 checkpoint count/latest id/summary 外，也打印 latest completed worker count 与 validation note count，便于人工确认本轮 checkpoint 有完成者和验证证据。
- `scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 同步断言 `last_completed_worker_ids` / `last_validation_notes` 为结构化列表，并只打印 checkpoint 完整性、worker 数和 note 数，不扩散 validation note 原文。验证已通过 `cargo test -q --test cli_smoke_tests`、`cargo test -q --test live_operator_scripts_tests`、`bash scripts/chuang-goal-run-status.sh --json` 抽样、`sh scripts/chuang-candidate-verify.sh`、`cargo fmt --all --check`、`git diff --check`。

# 2026-05-12 Feishu turn summary providerMeta fallback 回归
- 本轮继续沿 M7 app-server/channel/Feishu 输出面补边角：`scripts/chuang-feishu-turn-summary-smoke.js` 新增 providerMeta-only 场景，锁住 Feishu 过程摘要即使只从 `providerMeta` 读取 `tool_unified_execution_status`、`tool_unified_execution_failure_count`、`tool_protocol_error_count` 和 `tool_call_count`，也会显示稳定工具执行摘要。
- 脱敏边界同步覆盖 fallback：当 `providerMeta.tool_protocol_error_count > 0` 且 `providerMeta.tool_trace` 含 raw `ACTION` payload 时，Feishu 文本仍只显示协议错误计数，不输出原始 payload。验证已通过 `node scripts/chuang-feishu-turn-summary-smoke.js`、`cargo test -q --test app_server_tests --test cli_channel_tests --test runtime_report_tests`、`cargo fmt --all --check`。

# 2026-05-12 third-test clean-tree 复验覆盖本批 M5/M6/M7
- 本轮在 `4c655f3` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、complete-local、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 复验确认本批新增状态已进入第三测试链路：candidate 与 third-test 均打印项目 `GoalRun` checkpoint 摘要，当前 `project_goal_run_checkpoint_count=80`、latest checkpoint 为 `checkpoint-1778591135678614133`；MVP/complete-local 中的 `chuang-feishu-turn-summary-smoke.js` 也覆盖 Feishu 文本工具执行摘要和协议错误 raw payload 不外泄。provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 Feishu turn summary 透出工具执行摘要
- 本轮继续沿 M7 channel/Feishu 输出面补 runtime observability：`scripts/chuang-feishu-turn-summary.js` 的过程摘要现在会从 `runtimeObservability` / provider meta 读取 `tool_unified_execution_status`、`tool_unified_execution_failure_count` 与 `tool_protocol_error_count`，在 Feishu 文本里显示工具执行状态、失败数和协议错误计数。
- 安全边界同步收紧：存在工具协议错误时不再把 `tool_trace` 原样放进 Feishu 文本，避免 `ACTION: ...` raw payload 泄漏到通道消息；只保留稳定计数摘要。验证已通过 `node scripts/chuang-feishu-turn-summary-smoke.js`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check` 和 `git diff --check`。

# 2026-05-12 goal run status 并入项目 checkpoint 摘要
- 本轮继续沿 M6/M7 goal 状态可观测性补主链接线：`scripts/chuang-goal-run-status.sh` 现在除 watchdog、overnight run 与 tmux 观察外，也只读汇总项目 `GoalRun` 文件，输出 `project_goal_run.goal_id`、`checkpoint_count`、`checkpoint_log_complete`、latest checkpoint id/summary/created_at、completed workers 和 validation notes；文件缺失时只报告 unavailable，不启动 worker、不改 repo。
- `scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 的 goal run status 摘要同步断言并打印项目 checkpoint count/latest checkpoint，避免人工只看到交互态而看不到本轮主线是否已经落盘。验证已通过 `cargo test -q --test cli_smoke_tests`、`sh scripts/chuang-candidate-verify.sh`、`cargo fmt --all --check`、`sh -n` 和 `git diff --check`。

# 2026-05-12 live operator receipt 服务级 ready 边界加固
- 本轮继续沿 M5/M6/M7 只读 live acceptance 主链补高层门禁：`scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 现在在 operator receipt 模板结构断言里逐项锁住 `real_live_acceptance.services[*].manual_live_required=true` 与 `must_not_count_as_complete=true`，避免把只读 receipt 模板或未闭环 evidence 误升格为真实 live ready。
- `tests/live_operator_receipt_collect_tests.rs` 同步补 overlay 提权回归：即使 overlay 声称服务已 verified、关闭 manual/live 禁止字段或清空 required evidence，collector 仍保持 `can_mark_real_live_ready=false`、`real_live_acceptance.complete=false`，并恢复 7 个 canonical 服务的 manual/live 必填边界。验证已通过 `cargo test -q --test live_operator_receipt_collect_tests --test live_operator_scripts_tests --test cli_smoke_tests`、`cargo fmt --all --check`、`sh -n` 和 `git diff --check`；GoalRun 写入 `checkpoint-1778590077713349783`，checkpoint count 到 78。

# 2026-05-12 live runner rehearsal admission 身份字段加固
- 本轮继续沿 M6 subagent/report admission 证据链补运行态抽样：`scripts/chuang-live-runner-rehearsal-smoke.sh` 现在不仅断言 `report_admission.status=Accepted` 与 `reason_code=report_validated`，还在 run-once、report、collect 三个输出面锁住 `controller_agent_id=cli-subagent-controller`、task/agent/report id 与 dispatch/report 对齐，以及 `decided_at` 为 UTC 时间戳。
- 对应静态回归补到 `tests/cli_smoke_tests.rs`，确认 live runner rehearsal smoke 不会退化成只检查状态字符串。验证已通过 `sh scripts/chuang-live-runner-rehearsal-smoke.sh`、`cargo test -q --test cli_smoke_tests live_runner_rehearsal_smoke_uses_disabled_codex_runner_and_report_admission`、`cargo fmt --all --check`、`sh -n` 和 `git diff --check`。

# 2026-05-12 third-test runtime observability clean-tree 复验通过
- 本轮在 `ba70a70` 后从干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、live readonly preflight、complete-local、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 最新门禁确认 `runtime_report_surface` 仍为 11 个 artifact / 26 个 observability 字段，`policy_tool_status` 仍为 9/12 GA descriptors；candidate/third-test 均打印 `goal_run_status_interactive_state` 与 `goal_run_status_activity_hint`，provider readiness 继续只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 unified execution failure classes 高层门禁补齐
- 本轮继续沿 M4/M7 unified execution 状态面补高层抽样：MVP smoke、complete-local smoke、candidate verify、third-test smoke 现在在 `runtime_report_surface.observability_fields` 中同时断言 `tool_unified_execution_status`、`tool_unified_execution_failure_count` 与 `tool_unified_execution_failure_classes`，不再只靠底层 `runtime_report_tests` 覆盖 failure class 字段。
- status/doctor/app-server health、live runner readiness view 和相关静态 wrapper 测试也同步锁住这组三件套，确认 `runtime_report_surface` 的 26 字段在本地门禁、候选门禁和只读 readiness 面同口径。验证已通过 `cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests`、`cargo test -q --test cli_smoke_tests --test live_operator_scripts_tests --test live_runner_readiness_view_tests`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`sh -n` 和 `git diff --check`。

# 2026-05-12 app-server/channel runtime trace chars 回归补齐
- 本轮继续沿 M7 app-server/channel 输出面补 `runtime_report_surface` 字段抽样：`tests/app_server_tests.rs` 现在同时锁住 `turn/start` 响应与 `turn/completed` 事件中的 `runtimeObservability.runtime_response_trace_chars` 为可解析正整数；`tests/cli_channel_tests.rs` 也锁住 `channel simulate --json` 暴露同一字段。
- 这确认 app-server 和 channel 继续消费同一份 `runtime_observability_meta()`，让 runtime response trace 不只在 status/doctor/app-server health 的 surface 列表里可见，也在真实 turn 输出面可查。验证已通过 `cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config`、`cargo test -q --test cli_channel_tests cli_channel_simulate_runs_workspace_config_without_fake_responder`、`cargo fmt --all --check` 和 `git diff --check`。

# 2026-05-12 candidate/third-test goal run status 抽样补齐
- 本轮继续沿 M6/M7 goal 状态可观测性往高层门禁推进：`scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 的 goal run status 只读摘要现在除 `overall_status` / `ok` 外，也断言并打印 `interactive_state` 与 `activity_hint`，让人工和第三测试日志能直接判断终端 goal worker 是 working/thinking/idle/session_missing 等状态。
- 回归同步补到 `tests/cli_smoke_tests.rs` 与 `tests/live_operator_scripts_tests.rs`，锁住 candidate/third-test wrapper 必须保留这些字段；真实验收已通过 `cargo test -q --test cli_smoke_tests`、`cargo test -q --test live_operator_scripts_tests`、`bash scripts/chuang-goal-run-status.sh --json` 抽样、`sh scripts/chuang-candidate-verify.sh`、`cargo fmt --all --check` 和 `git diff --check`。

# 2026-05-12 third-test policy surface clean-tree 复验通过
- 本轮在 `f1df21f` 和 handoff 刷新后，从干净工作树完整跑通 `sh scripts/chuang-third-test-smoke.sh`，继承 final verify、candidate verify、live readonly preflight、complete-local、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 摘要；最终输出 `third_test_candidate_smoke_ok`。
- 关键新增状态面已进入第三测试链路：candidate 阶段输出 `candidate_policy_tool_status_ga_tool_descriptors=9/12`，third-test 阶段输出 `live_runner_readiness_view_policy_tool_status_ga_tool_descriptors=9/12`；`runtime_report_surface` 仍为 11/26，provider readiness 只显示 `api_key_state=<set>`，未打印 secret。

# 2026-05-12 MVP/complete-local 并入 policy tool status 门禁
- 本轮继续把 M5 governance/tool descriptor 状态面推进到基础本地验收：`scripts/chuang-mvp-smoke.sh` 现在在 status、doctor、app-server health 三个 JSON 面断言 `policy_tool_status`，`scripts/chuang-complete-local-smoke.sh` 进一步在 status、doctor、app-server health diagnostic 和 console snapshot 四个面断言同一组字段。
- 门禁抽样锁住 `active_permission_profile=local_ga`、`ga_tool_descriptor_mapped_count=9`、`tool_descriptor_count=12`，以及 `file_write.external_commit=false`、`file_write.requires_approval=false`、`write` risk tag。验证已通过 `sh scripts/chuang-mvp-smoke.sh`、`sh scripts/chuang-complete-local-smoke.sh`、`cargo test -q --test cli_smoke_tests` 和 `sh -n` 脚本语法检查。

# 2026-05-12 live runner readiness 并入 policy tool status
- 本轮继续把 M5 governance/tool descriptor 状态面推进到只读 readiness 聚合：`scripts/chuang-live-runner-readiness-view.sh` 现在从 status、doctor、app-server health 中聚合 `policy_tool_status`，JSON 输出保留完整 GA descriptor 风险字段，文本面打印 active profile、descriptor 映射数和 missing 摘要。
- `candidate verify` 与 `third-test smoke` 已在 live runner readiness 阶段断言 `policy_tool_status.active_permission_profile=local_ga`、`ga_tool_descriptor_mapped_count=9`、`tool_descriptor_count=12`，并抽样锁住 `file_write` 的 `external_commit=false`、`requires_approval=false` 和 `write` risk tag。验证已通过 `cargo test -q --test live_operator_scripts_tests --test live_runner_readiness_view_tests --test cli_smoke_tests`、`cargo fmt --all --check`、`git diff --check` 和 `sh scripts/chuang-candidate-verify.sh`。

# 2026-05-12 app-server health 透出 policy tool status
- 本轮继续把 M5 governance/tool descriptor 状态面推进到 app-server health：`app-server health --json` 现在随 `runtime_report_surface` 一起返回 `policy_tool_status`，让服务健康面也能查询 GA 工具 descriptor 的 `external_commit`、`requires_approval`、`risk_tags` 和本地治理决策。
- 新增回归在 `app_server_health_reports_workspace_runtime` 中锁住 `file_write` descriptor 的 non-readonly、mutating、non-destructive、non-external、descriptor-level no approval、`allow_with_audit` 以及 `write/audit` tags。验证已通过 `cargo test -q --test app_server_tests app_server_health_reports_workspace_runtime`、`cargo test -q --test app_server_tests --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests` 和 `cargo fmt --all --check`。

# 2026-05-12 policy tool status 风险字段可观测性补齐
- 本轮继续沿 M5 governance/tool descriptor 主链补状态面：`policy_tool_status.ga_tool_descriptors` 现在在 JSON 面除 name/read_only/mutating/destructive/local decision 外，也暴露 `external_commit`、`requires_approval` 和 `risk_tags`，让 operator 能直接查询 GA 映射工具的完整 descriptor 风险元数据。
- `tests/kernel_status_tests.rs` 已锁住 `file_write` 和 `code_execute` 在状态面保留 local mutating、non-destructive、non-external、no descriptor-level approval 以及 `write/audit`、`code_execution/shell` tags。验证已通过 `cargo test -q --test kernel_status_tests`、`cargo test -q --test cli_status_tests --test cli_doctor_tests --test app_server_tests` 和 `cargo fmt --all --check`。

# 2026-05-12 permission destructive risk tag 别名回归
- 本轮继续沿 M5 MCP/governance 主链补权限层回归：`local_ga_profile_requires_explicit_target_approval_for_destructive_tags` 现在把 `destructive` 和 `destructive_action` 纳入删除/破坏类别名矩阵，确保 MCP structural risk tag 补出的 `destructive` 不会在权限分类层被降级。
- 这和 `mcp_tool_descriptor_risk()` 的结构化风险标签对齐：服务端只给 `destructive=true` 而没有原始 `delete/rm` 标签时，治理仍会走 `RequireExplicitTargetApproval`。验证已通过 `cargo test -q --test permission_profile_slot_tests local_ga_profile_requires_explicit_target_approval_for_destructive_tags`、`cargo test -q --test mcp_fake_adapter_tests --test permission_profile_slot_tests --test tool_registry_slot_tests` 和 `cargo fmt --all --check`。

# 2026-05-12 runtime report protocol error 摘要脱敏回归
- 本轮继续沿 M7 runtime report 查询面加固协议错误 artifact 边界：`runtime_report_promotes_tool_report_metadata_to_artifact` 现在除断言 `runtime_meta.tool_protocol_errors_json` artifact 描述包含 `count=2`、`invalid_action_json`、`plain_text_response` 外，也显式断言 description 不包含 `ACTION payload is invalid`、原始 `ACTION: {`、`plain text is not accepted` 或 `hello` raw payload。
- 这保证高层 report artifact 摘要只暴露稳定 code/count，原始协议片段和错误 message 仍留在受控 JSON artifact/事件面里，不进入可扫摘要。验证已通过 `cargo test -q --test runtime_report_tests runtime_report_promotes_tool_report_metadata_to_artifact`、`cargo test -q --test runtime_report_tests` 和 `cargo fmt --all --check`。

# 2026-05-12 goal step JSON admission locator 回归加固
- 本轮继续沿 M6 goal 主链补查询面一致性：`tests/cli_goal_tests.rs` 的 `cli_goal_step_runs_manifest_workers_and_collects_reports_without_checkpointing` 现在在 `goal step --json` receipt 中直接断言 `collection.handoff_query_summary` 携带 `parent_context_handoff_count=2`、`report_admission_ref_count=2`、`report_validated=2`，并锁住两条 `goal-report-admission://...` admission locator、`Accepted` 状态、`report_validated` reason code 和 `report://...` evidence ref。
- 这把 `goal step --json` 和既有 `goal step` 文本面、`goal collect`、`goal show --json` 的 admission locator 口径对齐，避免主控只在文本或 show 面能查到 report admission 证据。验证已通过 `cargo test -q --test cli_goal_tests cli_goal_step_runs_manifest_workers_and_collects_reports_without_checkpointing`、`cargo test -q --test cli_goal_tests --test goal_dispatch_tests` 和 `cargo fmt --all --check`。

# 2026-05-12 third-test clean-tree 复验通过
- 本轮在干净工作树跑通 `sh scripts/chuang-third-test-smoke.sh`，完整继承 final verify、candidate verify、complete-local、live readonly preflight、live gaps、live runner readiness view、operator checklist/receipt 和 goal run status 只读摘要；最终输出 `third_test_candidate_smoke_ok`。
- 关键状态面继续保持 `live_runner_readiness_view_runtime_report_surface_artifacts=11`、`live_runner_readiness_view_runtime_report_surface_observability_fields=26`，provider readiness 只显示 `api_key_state=<set>` / `connects_real_provider=false`，没有打印 secret。下一轮继续推进剩余 M5/M6/M7 高层查询边角或主链接线代码缺口。

# 2026-05-12 candidate verify 复验通过
- 本轮在 app-server completed protocol artifact 和 channel 文本 protocol error code 面补齐后，完整跑通 `sh scripts/chuang-candidate-verify.sh`。链路覆盖 complete-local smoke、goal-mode 正/负 smoke、live runner rehearsal、live gaps、live runner readiness view、operator checklist/receipt、goal run status 和 provider readiness 只读检查。
- 输出确认 `candidate_runtime_report_surface_artifacts=11`、`candidate_runtime_report_surface_observability_fields=26`、provider readiness `api_key_state=<set>` 且 `connects_real_provider=false`，最终 `chuang_candidate_verify_ok`。下一步在干净工作树继续跑 third-test clean-tree 门禁，或继续补 app-server/channel 高层文本/JSON边角。

# 2026-05-12 channel text protocol error code 面补齐
- 本轮继续沿 M7 channel 输出面补协议错误可观测性：`channel simulate` 文本输出现在会在 `tool_protocol_error_count` 后打印 `tool_protocol_error_codes`，只展示稳定 code 列表，不打印 raw payload 或错误 message，避免文本回执里泄漏模型原始协议片段。
- 新增回归 `cli_channel_simulate_text_surfaces_protocol_error_codes_without_raw_payload` 通过本地 OpenAI-compatible HTTP provider 触发一次 `invalid_action_json`，确认文本面输出 `tool_protocol_error_count: 1`、`tool_protocol_error_codes: invalid_action_json` 和最终修正回复，同时不包含 `ACTION:` / 原始 tool_call JSON。验证已通过 `cargo test -q --test cli_channel_tests cli_channel_simulate_text_surfaces_protocol_error_codes_without_raw_payload`。

# 2026-05-12 app-server protocol error completed 事件面加固
- 本轮继续沿 M7 app-server/channel 主链补非零协议错误事件面：`tests/app_server_tests.rs` 的 `app_server_turn_surfaces_nonzero_tool_protocol_errors` 现在不只在 `turn/start` 响应里检查 `toolProtocolErrors`、provider meta 和 `toolEvents.kind=protocol_error`，也在 `turn/completed` 事件里锁住 `invalid_action_json`、`providerMeta.tool_protocol_errors_json` 和 protocol_error event。
- 这保证工具协议错误被模型修正后，订阅事件面和请求响应面都能查询同一份 `runtime_meta.tool_protocol_errors_json` 来源，而不会只剩 `toolProtocolErrorCount=1`。验证已通过 `cargo test -q --test app_server_tests app_server_turn_surfaces_nonzero_tool_protocol_errors`；下一步继续跑 app-server/channel/runtime_report 组合矩阵。

# 2026-05-12 M5/M6/M7 宽矩阵复验
- 本轮在 `bcc9699`、`4391a05`、`044ae20`、`26ea423` 后复跑 M5/M6/M7 相关宽矩阵：`cargo test -q --test mcp_fake_adapter_tests --test permission_profile_slot_tests --test tool_registry_slot_tests --test subagent_tree_events_tests --test subagent_tree_ledger_tests --test cli_goal_tests --test goal_dispatch_tests --test runtime_report_tests` 全部通过，覆盖 MCP structural risk tags、permission/tool descriptor、subagent children admission states、goal admission locator JSON/text、goal dispatch handoff summary 与 runtime report surface。
- `cargo fmt --all --check` 与 `git diff --check` 同步通过，当前主链仍保持 11 个 artifact / 26 个 observability 字段。下一轮继续看 app-server/channel 的协议错误非零路径是否还需要锁更多 report artifact 字段，或把 candidate/third-test 运行态门禁再收紧一层。

# 2026-05-12 readiness/candidate runtime surface 复验
- 本轮在 M5 structural risk、M6 goal JSON locator 和 subagent children 三态回归之后，复跑高层 readiness/candidate 静态矩阵：`cargo test -q --test live_runner_readiness_view_tests --test live_operator_scripts_tests --test cli_smoke_tests` 全部通过，覆盖 live runner readiness JSON/text、candidate/third-test wrapper 断言和 smoke wrapper 静态继承。
- 只读运行态抽查 `bash scripts/chuang-live-runner-readiness-view.sh --json` 确认 `runtime_report_surface` 仍为 11 个 artifact / 26 个 observability 字段，并且 `runtime_meta.tool_protocol_errors_json`、`tool_protocol_error_count`、`subagent_children_report_admission_refs` 均在聚合状态面可见。下一步继续跑更宽 M5/M6/M7 矩阵，必要时再补 candidate wrapper 的运行态门禁。

# 2026-05-12 subagent children listed 三态回归加固
- 本轮继续补 M6 subagent tree/list children 事件面：`tests/subagent_tree_events_tests.rs` 的 `list_event_snapshots_children_and_their_evidence_refs` 现在同时覆盖 accepted、rejected、missing 三类 child report 状态，锁住 `children_summary` 里的 `accepted_report_count=1`、`rejected_report_count=1`、`missing_report_count=1`、`report_admission_refs` 两条 admission refs，以及 `report_validated` / `command_protocol_report_rejected` reason-code 分布。
- 这保证 `subagent_children_listed` runtime bridge 事件不会只对 accepted report 暴露 admission locator，而漏掉 rejected report 的状态、reason code 和 evidence ref。验证已通过 `cargo test -q --test subagent_tree_events_tests list_event_snapshots_children_and_their_evidence_refs`；下一步继续跑 M6 矩阵并看这些状态是否还需要进入更高层 smoke/candidate 抽样。

# 2026-05-12 goal show JSON admission locator 回归加固
- 本轮继续收 M6 goal/subagent 查询面：`tests/cli_goal_tests.rs` 的 `cli_goal_show_surfaces_next_command_and_stage_readiness` 现在在 checkpoint-ready 的 `goal show --json` 路径里直接断言 `handoff_query_summary` 携带 `parent_context_handoff_count=2`、`report_admission_ref_count=2`、`report_validated=2`，并逐项锁住 `goal-report-admission://...` admission locator、`Accepted` 状态、`report_validated` reason code 和 `report://...` evidence ref。
- 这让 `goal show` 的 JSON 面和既有文本面、`goal collect` / `goal step` 面保持同一套 admission locator 口径，不再只验证 checkpoint worker/validation note。验证已通过 `cargo test -q --test cli_goal_tests cli_goal_show_surfaces_next_command_and_stage_readiness`；下一步继续扫 subagent tree/list children 或 candidate smoke 是否还缺同类 JSON locator 抽样。

# 2026-05-12 MCP structural risk tag 查询面补齐
- 本轮继续推进 M5 MCP fake adapter 到治理查询面的主链：`mcp_tool_descriptor_risk()` 现在会把 MCP spec 的结构化 `destructive=true` 显式补成 `destructive` risk tag，和已有 `open_world`、`external_commit`、`omitted_risk_tightened` 摘要保持一致；`classify_tag()` 也识别 `destructive` / `destructive_action` 为删除/破坏类风险，避免 MCP 服务端只给布尔高危而没给标签时，治理状态面无法按高危类别检索。
- 回归 `mcp_descriptor_conversion_marks_structural_risks_as_queryable_tags` 已锁住 destructive/open_world 结构风险都会进入 `ToolDescriptorRisk.risk_tags`。验证已通过 `cargo fmt --all --check` 和 `cargo test -q --test mcp_fake_adapter_tests --test permission_profile_slot_tests --test tool_registry_slot_tests`。下一轮继续扫 M6 goal/subagent 状态面和 M7 高层 runtime surface 是否还有类似查询摘要缺口。

# 2026-05-12 protocol error surface third-test 复验记录
- 本轮从干净工作树继续接上 M5/M6/M7 主链，确认 `runtime_meta.tool_protocol_errors_json` 与 `tool_protocol_error_count` 已进入 status/doctor/app-server health、live runner readiness、candidate verify、third-test smoke 和 MVP/complete-local 门禁；当前 `runtime_report_surface` 统一为 11 个 artifact / 26 个 observability 字段。
- 已复核高层回归：`tests/live_runner_readiness_view_tests.rs` 的 JSON/text 输出断言已锁住 11/26、`runtime_meta.tool_protocol_errors_json`、`tool_protocol_error_count`、goal handoff admission refs、subagent children admission refs 和 `context_compaction_summary_json`；`tests/live_operator_scripts_tests.rs` 也锁住 candidate/third-test wrapper 的 11/26 静态门禁。
- 上一轮 clean-tree `sh scripts/chuang-third-test-smoke.sh` 已通过并输出 `live_runner_readiness_view_runtime_report_surface_artifacts=11`、`live_runner_readiness_view_runtime_report_surface_observability_fields=26`、`third_test_candidate_smoke_ok`。本轮下一步继续扫 M5/M6/M7 是否还有 goal/subagent 状态面或 MCP governance 查询面未并入高层验收。

# 2026-05-12 app-server runtime compaction summary 事件面回归
- 本轮继续沿 M7 主链把 app-server turn 事件面补齐：`tests/app_server_tests.rs` 的 `app_server_turn_uses_workspace_provider_config` 现在在 `turn/start` 响应和 `turn/completed` 事件两处都断言 `runtimeObservability.context_compaction_summary_json` 存在，并包含稳定的 `dropped_count` 字段。这样 app-server 事件订阅面、channel simulate 面、status/doctor/health 面对 compaction summary 的可查询口径保持一致。
- 验证已通过 `cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config`。下一轮入口：继续扫 tool protocol correction 是否也需要进入 channel/app-server 高层事件面回归。

# 2026-05-12 channel runtime compaction summary 回归加固
- 本轮沿 M7 主链把 channel 输出面再锁一层：`tests/cli_channel_tests.rs` 的 `cli_channel_simulate_runs_workspace_config_without_fake_responder` 现在不仅检查 `context_pack_trace` 和 `context_compaction_events`，还断言 `runtime_observability.context_compaction_summary_json` 出现在 `channel simulate --json` 输出里，并包含稳定的 `dropped_count` 字段。这样 Feishu/app-server 通道模拟面也能查询 compaction summary，不只停在 status/doctor/app-server health。
- 验证已通过 `cargo test -q --test cli_channel_tests cli_channel_simulate_runs_workspace_config_without_fake_responder`。下一轮入口：继续看 app-server turn/completed 事件是否也需要把 `context_compaction_summary_json` 作为显式事件面回归。

# 2026-05-12 readiness view JSON runtime surface 回归加固
- 本轮把刚才的运行态只读抽查固化到 `tests/live_runner_readiness_view_tests.rs`：`live_runner_readiness_view_script_outputs_aggregated_json_view` 现在直接断言 JSON 顶层 `runtime_report_surface` 为 `artifact_count=10`、`observability_field_count=25`，并且包含 `runtime_meta.context_compaction_summary_json`、`goal_handoff_report_admission_refs`、`subagent_children_report_admission_refs` 和 `context_compaction_summary_json`。这样 candidate/third-test 依赖的 readiness view JSON 不会只保顶层键而漏掉 M6/M7 的关键字段。
- 验证已通过 `cargo test -q --test live_runner_readiness_view_tests live_runner_readiness_view_status_json_exposes_blocked_reason_and_next_action`。下一轮入口：继续看 candidate/third-test wrapper 是否也需要从静态字段名断言提升到运行态 JSON 抽查。

# 2026-05-12 M7/tool protocol 与 compaction 主链复验
- 本轮按验收矩阵补跑 M7/tool runtime 相关主链：`cargo test -q --test tool_runtime_tests --test agent_runtime_tests --test context_engine_tests --test app_server_tests` 已通过，覆盖 tool protocol typed failure/correction、context packing/compaction summary、agent runtime extra meta 注入，以及 app-server 的 runtime observability 事件面。
- 现有 smoke/candidate 已覆盖 `runtime_meta.context_compaction_summary_json`、`context_compaction_summary_json`、`context_pack_trace` 与 `context_compaction_events` 的状态/报告可见性。本轮不新增结构，只记录复验；下一轮入口：继续扫文档/脚本里旧 M7 字段计数或工具协议门禁是否有漂移。

# 2026-05-12 M5/M6/M7 矩阵复验
- 在 `goal collect` / `goal step` / `goal show` admission locator 回归补齐后，本轮重跑 M5/M6/M7 相关矩阵：`cargo test -q --test subagent_tree_ledger_tests --test subagent_tree_events_tests --test runtime_report_tests --test cli_goal_tests` 与 `cargo test -q --test runtime_event_ledger_tests --test mcp_fake_adapter_tests --test governance_tests --test kernel_status_tests` 均通过，覆盖 runtime ledger、MCP fake approval/elicitation、governance、subagent children summary/report admission refs、runtime report surface 和 goal 文本查询面。
- `cargo fmt --all --check` 与 `git diff --check` 同步通过。下一轮入口：继续按验收矩阵看 `tool_runtime_tests / agent_runtime_tests / context_engine_tests / app_server_tests` 的 M7/tool protocol correction 与 compaction 主链是否还需要提升到更高层 smoke/candidate 门禁。

# 2026-05-12 goal show admission locator 回归加固
- 本轮继续沿 M6 goal 查询面加固文本层验收：`tests/cli_goal_tests.rs` 的 `cli_goal_show_surfaces_next_command_and_stage_readiness` 现在在 checkpoint-ready 的 `goal show` 文本场景里显式断言 `goal_operability_handoff_query_report_admission_ref_count: 2`、`goal_operability_handoff_query_report_admission_refs:` 和 `admission_id=goal-report-admission://`。这样 `goal collect`、`goal step`、`goal show` 三个文本入口都锁住 admission locator 不会退化成只有字段名。
- 验证已通过 `cargo test -q --test cli_goal_tests cli_goal_show_surfaces_next_command_and_stage_readiness`。下一轮入口：继续看 subagent tree/list children 文本或 runtime event 面是否也需要同类具体 locator 回归。

# 2026-05-12 goal step admission locator 回归加固
- 本轮继续沿 M6 goal 查询面做小步回归加固：`tests/cli_goal_tests.rs` 的 `cli_goal_step_text_exposes_handoff_query_summary` 现在不只检查 `goal_step_handoff_query_report_admission_refs` 字段名，还显式断言文本输出包含 `admission_id=goal-report-admission://`。这样前台 bounded `goal step` 的 handoff query 文本面会持续携带可点击/可追踪的 admission locator，而不是只显示空字段名。
- 验证已通过 `cargo test -q --test cli_goal_tests cli_goal_step_text_exposes_handoff_query_summary`。下一轮入口：继续检查 `goal show` / candidate / third-test 是否需要对具体 admission locator 做运行态只读抽查，而不只检查字段集合。

# 2026-05-12 third-test clean-tree 复验与 handoff 刷新
- 刚提交 `8360ac3 feat(runtime): lock runtime report query surfaces` 后，已在干净工作树补跑 `sh scripts/chuang-third-test-smoke.sh`，完整串过 final verify、candidate verify、complete-local、live readonly preflight、live gaps、readiness view、operator checklist、receipt template 和 goal run status 只读摘要；第三测试入口确认 `runtime_report_surface` 仍为 10 个 artifact / 25 个 observability 字段。
- 同步刷新 `docs/handoff-current.md` 顶部过期的 20/23 字段口径到当前 25 字段，避免交接文档和已提交门禁漂移。下一轮入口：继续回到 goal run / subagent tree admission locator 的更高层文本摘要，或补 final/candidate/third-test 对 goal 查询面的只读抽查。

# 2026-05-12 final verify 继承 runtime surface 门禁锁定
- 本轮继续沿 M5/M6/M7 主链接线补齐 admission 查询面：`runtime_observability_meta` / `runtime_meta.observability` / `runtime_report_surface` 现在不仅暴露 goal handoff 与 subagent children 的 admission 计数和 reason-code 分布，还直接暴露 `goal_handoff_report_admission_refs` 与 `subagent_children_report_admission_refs` 两组 locator 摘要，`runtime_report_surface.observability_field_count` 从 23 提到 25。这样 operator 在 status/doctor/app-server/live-runner readiness 面可以直接看到 report admission 指针，不必再手动解析 JSON blob。
- 已同步 MVP smoke、complete-local、candidate verify、third-test smoke、live runner readiness 文本面和相关回归到 25 字段口径；验证通过 `cargo fmt --all --check`、`cargo test -q --test runtime_report_tests --test kernel_status_tests --test live_runner_readiness_view_tests --test live_operator_scripts_tests --test cli_smoke_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests`、`sh scripts/chuang-mvp-smoke.sh`、`sh scripts/chuang-candidate-verify.sh`、`cargo test -q`。`sh scripts/chuang-third-test-smoke.sh` 本轮按预期停在 clean-tree 门禁，因为当前工作树有本轮/既有未提交改动；下一轮在清洁工作树或最终 checkpoint 后复跑即可。
- 本轮把 final verify 的静态继承关系再钉实一层：`tests/cli_smoke_tests.rs` 的 `final_verify_wrapper_requires_clean_tree_and_candidate_verify` 现在除了确认 `scripts/chuang-final-verify.sh` 走 `scripts/chuang-candidate-verify.sh`，还会直接读取 candidate wrapper，锁住 `runtime_report_surface` 的 `artifact_count=10`、`observability_field_count=25` 和 `runtime_response.trace` / goal handoff / subagent children / admission reason-code / admission ref 等关键字段。这样 final verify 就不会只是在脚本顺序上看起来经过 candidate verify，而实际漏掉 M5/M6/M7 查询面门禁。
- 验证已通过 `cargo test -q --test cli_smoke_tests final_verify_wrapper_requires_clean_tree_and_candidate_verify`、`cargo test -q --test live_runner_readiness_view_tests --test live_operator_scripts_tests --test runtime_report_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：继续看 final/candidate/third-test 是否还要补更高层的只读 JSON 抽查，或者转回 goal run / subagent tree 查询边角。

# 2026-05-12 subagent children admission ref 计数并入 runtime surface
- 本轮把 M6 subagent tree/report handoff 的查询面再补一格：`runtime_observability_meta`、`runtime_meta.observability` 和 `runtime_report_surface` 现在显式暴露 `subagent_children_report_admission_ref_count`，operator 不必解析 `subagent_children_summary_json.report_admission_refs` 才能知道子树 report admission 指针数量。`runtime_report_surface` 的 observability 字段数从 22 提到 23。
- MVP smoke、complete-local、candidate verify、third-test smoke 和 live runner readiness 文本回归都已同步到 23 字段口径，并显式断言 `subagent_children_report_admission_ref_count`。下一轮入口：继续复验 full `cargo test -q` / MVP smoke，或看 goal run / subagent tree 是否还需要把 admission ref 的具体 locator 摘要并到更高层文本面。

# 2026-05-12 candidate/third-test live runner readiness reason-code 门禁加固
- 本轮把 M5/M6/M7 的查询面再往候选/第三测试门禁里收了一层：`scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 现在都会在 `live_runner_readiness_view` 的 `runtime_report_surface` 上显式断言 `goal_handoff_report_admission_reason_codes` 与 `subagent_children_report_reason_codes`，避免只保住 `artifact_count=10` / `observability_field_count=22` 却把两条 reason-code 分布漏掉。
- 对应静态回归也已同步补齐到 `tests/live_operator_scripts_tests.rs`，锁住 candidate/third-test wrapper 必须读取这两条 reason-code 字段。验证通过 `cargo test -q --test live_operator_scripts_tests --test cli_smoke_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`git diff --check`；本轮 checkpoint 已写入 `context/goal-runs/mainline-mvp.json` 的 `checkpoint-1778554680651095331`。下一轮入口：继续盯 candidate / third-test / final verify 是否还要补别的只读摘要，或回头把 goal run / subagent tree 的查询边角再收一层。

# 2026-05-12 runtime observability reason-code 分布补齐
- 本轮继续沿 M5/M6/M7 主链接线补齐可查询面：`runtime_meta.observability` 现在直接带出 `goal_handoff_report_admission_reason_codes` 和 `subagent_children_report_reason_codes`，`runtime_observability_meta` / `runtime_report_surface` 也把这两类 reason-code 分布纳入状态面。这样 goal collect / subagent tree 的 admission 口径不只剩计数，还能在 status/doctor/app-server health 里直接查 reason-code 分布，不必再翻 artifact 描述。
- `tests/runtime_report_tests.rs`、`tests/kernel_status_tests.rs`、`tests/cli_smoke_tests.rs`、`tests/live_operator_scripts_tests.rs`、`tests/cli_status_tests.rs`、`tests/cli_doctor_tests.rs` 和 `tests/app_server_tests.rs` 已同步更新到 `runtime_report_surface` 的 22 个 observability 字段；验证通过 `cargo test -q --test runtime_report_tests --test kernel_status_tests --test cli_smoke_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests --test live_operator_scripts_tests`、`cargo fmt --all --check`、`git diff --check` 和 `sh scripts/chuang-mvp-smoke.sh`。下一轮入口：继续把这条查询面往 goal collect / subagent tree / runtime report 的更细可查询摘要推进，或收口到 candidate / third-test / final verify 的最终一致性。

# 2026-05-12 runtime trace 查询面补齐
- 本轮继续沿 M5/M6/M7 主链接线补了一条 runtime trace 查询面：`runtime_response.trace` 现在会提升成 `runtime_response.trace` artifact，`runtime_response_trace_chars` 也会写进 `runtime_observability_meta` 和 `runtime_report_surface`，这样状态面和报告面都能直接看到模型 trace 的可查询摘要，不必只靠 `stdout_preview` 或 turn 输出。
- `tests/runtime_report_tests.rs`、`tests/kernel_status_tests.rs`、`tests/cli_smoke_tests.rs`、`scripts/chuang-mvp-smoke.sh` 和 `scripts/chuang-complete-local-smoke.sh` 已同步把这条 trace 面纳入静态回归与 smoke 门禁；下一轮入口可以继续盯 runtime trace 的只读查询摘要是否还要并到 final verify / candidate / third-test 的验收面。

# 2026-05-12 runtime observability artifact 补齐 tool event 计数
- 本轮继续沿 M5/M6/M7 主链接线做小步收口：`runtime_meta.observability` artifact 的描述现在显式带出 `tool_started` 和 `tool_finished`，与已经暴露的 `approval_requested`、`approval_resolved`、`elicitation_requested` 以及 `runtime_observability_meta` 的结构化字段保持一致。这样 operator 查询 runtime report artifact 时，不必再跳到 `runtime_event_ledger_json` 才能看见 MCP/tool event 的基础起止计数。
- `tests/runtime_report_tests.rs` 已补回归，锁住 observability map、runtime event ledger artifact 和 observability artifact 三个面上的 tool/approval/elicitation 计数一致；本轮验证已通过 `cargo fmt --all --check` 和 `cargo test -q --test runtime_report_tests`。下一轮入口：继续把这组可查询摘要往 candidate/third-test/final verify 的静态门禁补齐，或转去 goal run / subagent tree 剩余查询边角。

# 2026-05-12 final verify 入口收口到 candidate verify
- 本轮把 `scripts/chuang-final-verify.sh` 的主门禁从 `complete-local smoke` 提升为 `candidate verify`，让 final verify 在最终 diff check 前先复用 complete-local、live runner rehearsal、live gaps、runtime report surface、live operator checklist 和 goal run status 的只读候选链路。这样第三测试入口不再只验本地 smoke，而是继承更完整的 candidate 验收面。
- `tests/cli_smoke_tests.rs` 的回归也同步改成锁定 `sh scripts/chuang-candidate-verify.sh` 的顺序，避免 final verify 回退成仅跑 complete-local。验证已通过 `cargo test -q --test cli_smoke_tests --test live_operator_scripts_tests --test runtime_report_tests --test kernel_status_tests`。下一轮入口：继续盯 candidate / third-test / final verify 里还有没有别的静态门禁需要并到这条链路，或者回头补 goal run / subagent tree 的剩余查询边角。

# 2026-05-12 runtime report surface / candidate gate 复验
- 本轮在已有的 M5/M6/M7 收口改动基础上，重新跑通了 `cargo test -q --test runtime_report_tests --test goal_dispatch_tests --test cli_goal_tests --test cli_smoke_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests --test goal_mode_smoke_tests` 和 `cargo test -q`，确认 `runtime_report_surface` 的 10 个 artifact / 20 个 observability 字段、`goal handoff` / `subagent children` 只读摘要，以及 complete-local / candidate / third-test 门禁当前仍然同口径。
- 下一轮继续盯 final verify / candidate / third-test 是否还需要再并一层静态回归，或继续细化 goal run / subagent tree / runtime report 的只读查询面。

# 2026-05-12 complete-local 直验 runtime report surface
- 本轮把 M5/M6/M7 的 runtime report/status 查询面继续往 complete-local 门禁收口：`scripts/chuang-complete-local-smoke.sh` 的本地诊断 `status` / `doctor` / `app-server health` / `console snapshot` 现在直接断言 `runtime_report_surface` 的 `artifact_count=10` 和 `observability_field_count=20`，并把 `runtime_meta.goal_handoff_query_summary_json`、`runtime_meta.subagent_children_summary_json`、`runtime_event_tool_started_count`、`runtime_event_tool_finished_count`、`runtime_event_approval_requested_count`、`runtime_event_approval_resolved_count`、`runtime_event_elicitation_requested_count`、`goal_handoff_parent_context_handoff_count`、`goal_handoff_report_admission_ref_count`、`subagent_children_child_count`、`subagent_children_accepted_report_count` 和 `subagent_children_missing_report_count` 一并纳入门禁。
- `tests/cli_smoke_tests.rs` 同步补了 complete-local wrapper 的静态回归，锁住这组 runtime report surface 断言存在，避免 second-test smoke 过了但 complete-local diagnostic 面回退。验证已通过 `cargo test -q --test cli_smoke_tests`、`sh scripts/chuang-complete-local-smoke.sh`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：继续把这条 complete-local / second-test / goal-mode / runtime report surface 一致口径往 final verify 和候选门禁再并一层。


# 2026-05-12 goal run status latest-run 选择修正
- 本轮继续推进 overnight / interactive 监控面时，补了 `scripts/chuang-goal-run-status.sh` 的 latest-run 选择口径：`list_run_dirs()` 现在优先按 run 目录里的结构化时间字段（`status.json` / `run-status.json` / `latest-run-status.json` 的 `updated_at` / `generated_at` / `timestamp` / `last_updated_at`）排序，只有缺少结构化时间时才退回文件 mtime，避免字典序更靠后的旧目录或被误碰过的 stale 目录抢占“最新 run”。
- `tests/cli_smoke_tests.rs` 新增回归，显式构造一个名字更靠后但结构化时间更旧的 `zzzz-stale-run`，锁住 `scripts/chuang-goal-run-status.sh --json` 仍会返回真正最新的 `latest_run_dir`。验证已通过 `cargo test -q --test cli_smoke_tests goal_run_status_script_reads_watchdog_and_overnight_status_without_actions`。下一轮入口：继续看 `status/doctor/app-server` 的 runtime_report_surface 文案是否还有需要收口的细微差异。
# 2026-05-12 runtime report/status surface 继续对齐
- 本轮继续沿 M5/M6/M7 主链接线收口可查询面：`runtime_report` 现在把 `goal_handoff_query_summary_json` 和 `subagent_children_summary_json` 进一步派生为 `goal_handoff_parent_context_handoff_count`、`goal_handoff_report_admission_ref_count`、`subagent_children_child_count`、`subagent_children_accepted_report_count` 和 `subagent_children_missing_report_count`，状态面与 runtime report 面现在能直接读到这组 handoff/subagent 摘要计数，不必只看 JSON blob。`src/kernel_status.rs` 的 `runtime_report_surface` 也把观测字段从 14 扩到 19，继续保住 `runtime_meta.goal_handoff_query_summary_json` / `runtime_meta.subagent_children_summary_json` 这两条 artifact locator。
- 回归已补到 `tests/runtime_report_tests.rs`、`tests/kernel_status_tests.rs`、`tests/cli_smoke_tests.rs`、`tests/cli_status_tests.rs`、`tests/cli_doctor_tests.rs`、`tests/app_server_tests.rs` 和 `scripts/chuang-mvp-smoke.sh`。验证已通过 `cargo test -q --test runtime_report_tests --test kernel_status_tests --test cli_smoke_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests`、`sh scripts/chuang-mvp-smoke.sh`、`cargo fmt --all --check`、`git diff --check` 和 `cargo test -q`。下一轮入口：继续把这条 runtime report/status 只读摘要和 goal collect/subagent tree 的口径往 complete-local 与 final verify 门禁再对齐一层。

# 2026-05-12 runtime event 细分计数并入状态面
- 本轮继续沿 M5/M6/M7 主链接线收口可查询面：`runtime_report_surface` 的 observability 字段从 9 项扩到 14 项，直接暴露 `runtime_event_tool_started_count`、`runtime_event_tool_finished_count`、`runtime_event_approval_requested_count`、`runtime_event_approval_resolved_count` 和 `runtime_event_elicitation_requested_count`。这样 `status`、`doctor` 和 `app-server health` 能直接读到 MCP approval / elicitation / tool event 的细分计数，不必只进 runtime report artifact 反查。
- 对应门禁已同步到 `scripts/chuang-mvp-smoke.sh`、`tests/cli_status_tests.rs`、`tests/cli_doctor_tests.rs`、`tests/app_server_tests.rs` 和 `tests/cli_smoke_tests.rs`，锁住第二测试版本 smoke 的 status/doctor/app-server health 三个入口都能看见这组事件字段。
- 验证已通过 `cargo test -q --test runtime_report_tests --test cli_status_tests --test cli_doctor_tests --test cli_smoke_tests --test app_server_tests` 和 `sh scripts/chuang-mvp-smoke.sh`。下一轮入口：继续把这组 runtime event 细分字段和 goal/subagent handoff 查询摘要往 complete-local / final verify 门禁对齐，并关注 full `cargo test -q` 是否仍稳定。

# 2026-05-12 goal-query handoff summary smoke 收口
- 本轮把 M5/M6/M7 的 goal collect / goal step / goal show 查询摘要继续抬进可验收面：`GoalDispatchCollectionReceipt` 新增 `handoff_query_summary`，把 `parent_context_handoffs` 和 `report_admission_refs` 合成一份只读摘要；`goal collect`、`goal step` 和 `goal show` 文本面都能直接看到 `goal_*_handoff_query_*` 指针、计数和 reason code，不再只靠 JSON 反查。
- `runtime_report_surface` 也把 `runtime_meta.goal_handoff_query_summary_json` / `runtime_meta.subagent_children_summary_json` 固定为 10 个 artifact/observability 字段之一，`scripts/chuang-mvp-smoke.sh` 与 `scripts/chuang-goal-mode-smoke.sh` 直接把这组字段纳入门禁；`tests/goal_dispatch_tests.rs` 补了 legacy collect receipt / legacy admission ref 的兼容回归。
- 验证已通过 `cargo test -q --test goal_dispatch_tests --test cli_goal_tests --test goal_mode_smoke_tests --test runtime_report_tests`、`sh scripts/chuang-mvp-smoke.sh`、`sh scripts/chuang-goal-mode-smoke.sh`、`cargo fmt --all --check` 和 `git diff --check`；下一轮入口是继续把 runtime report surface、goal collect/show 和 subagent tree 的只读摘要口径对齐到 status/doctor/app-server 的最终一致性。

# 2026-05-12 goal step 查询摘要补齐
- 本轮把 `goal step` 的文本输出也并入 M5/M6/M7 查询链路：`goal step` 现在会直接显示 `goal_step_handoff_query_parent_context_handoff_count`、`goal_step_handoff_query_report_admission_ref_count`、`goal_step_handoff_query_report_admission_reason_codes` 和 `goal_step_handoff_query_report_admission_refs`，让前台 bounded step 和 `goal collect` / `goal show` 对同一份 handoff/admission 摘要口径可见。
- `tests/cli_goal_tests.rs` 新增文本面回归，锁住这些字段会出现在 `goal step` 输出里；定向验证已通过 `cargo test -q --test cli_goal_tests --test goal_dispatch_tests --test goal_mode_smoke_tests`，闭环脚本 `sh scripts/chuang-goal-mode-smoke.sh` 也已重跑通过。下一轮入口：继续把 goal step / collect / runtime report 的只读摘要再统一一层，优先看 status 和 smoke 门禁的最终一致性。

# 2026-05-12 goal-mode smoke 续接面补齐
- 本轮把 `scripts/chuang-goal-mode-smoke.sh` 的闭环再收紧一层：`goal show` 现在也显式传入同一个 `--subagent-queue-root`，因此 checkpoint 之后的 show 视图能从同一份队列里读到 `goal_operability.goal_collect.handoff_query_summary`，不会再因为默认队列根和本轮 smoke 队列不一致而漏掉查询摘要。
- 对应静态回归也补了一条：`tests/goal_mode_smoke_tests.rs` 现在同时锁定脚本里存在 `--subagent-queue-root "$queue_root"` 和 `goal_operability.goal_collect.handoff_query_summary`，避免以后只保住 `collect` 面却把 `show` 面漏掉。验证已通过 `cargo test -q --test goal_mode_smoke_tests`、`sh scripts/chuang-goal-mode-smoke.sh`、`cargo test -q --test cli_goal_tests --test goal_dispatch_tests`。下一轮入口：继续看 `goal show` / `goal collect` 的只读摘要是否还能进一步统一到 runtime report surface 的门禁里。

# 2026-05-12 goal collect legacy receipt 兼容回归
- 本轮在已经并入 M5/M6/M7 查询摘要门禁的基础上，补了一条历史收据兼容回归：`GoalDispatchCollectionReceipt` 新增 `handoff_query_summary` 后，旧的 `goal collect` JSON 仍必须可反序列化，不能因为新增只读摘要字段而破坏回放/续接。
- `tests/goal_dispatch_tests.rs` 新增 `goal_collect_receipt_deserializes_legacy_json_without_handoff_query_summary`，直接喂入不含 `handoff_query_summary` 的旧收据 JSON，确认它会默认回到空摘要并保持其余字段可读；验证已通过 `cargo test -q --test goal_dispatch_tests --test cli_goal_tests`。下一轮可以继续把这条 legacy 兼容面和 complete-local / full-test 验收面一起锁住。

# 2026-05-12 M5/M6/M7 查询摘要继续并入 smoke 门禁
- 本轮没有再扩新结构，而是把已经落地的 M5/M6/M7 查询摘要直接并进第二测试版本 smoke：`scripts/chuang-mvp-smoke.sh` 现在会在 `status` / `doctor` / `app-server health` 路径里显式验收 `runtime_report_surface` 的 10 个 artifact/20 个 observability 字段，并确认 `runtime_response.trace`、`runtime_meta.goal_handoff_query_summary_json`、`runtime_meta.subagent_children_summary_json` 与对应观测字段都在可见面内；`scripts/chuang-goal-mode-smoke.sh` 则在 `goal collect` 路径里显式验收 `handoff_query_summary`、`report_admission_refs` 和 `goal-report-admission://...` 指针，`scripts/chuang-goal-mode-negative-smoke.sh` 继续保持 not-ready 负例门禁。
- 对应的静态回归也补上了：`tests/cli_smoke_tests.rs`、`tests/goal_mode_smoke_tests.rs`、`tests/goal_mode_negative_smoke_tests.rs` 现在锁住这些 smoke 断言，避免后续脚本改动把查询面从门禁里悄悄拿掉。验证已通过 `cargo fmt --all --check`、`cargo test -q --test cli_smoke_tests --test goal_mode_smoke_tests --test goal_mode_negative_smoke_tests`、`sh scripts/chuang-mvp-smoke.sh`、`sh scripts/chuang-goal-mode-smoke.sh`、`sh scripts/chuang-goal-mode-negative-smoke.sh`。下一轮入口：继续把这组查询摘要门禁和 complete-local / full-test 验收面的口径再对齐一层。

## 2026-05-12 M5/M6/M7 runtime report handoff query surface 补齐
- 本轮继续把 M5/M6/M7 主链接线往“可查询、可回放、可续接”推进：`src/runtime_report.rs` 现在会把 `goal_handoff_query_summary_json` 和 `subagent_children_summary_json` 这两类只读摘要提升成 `runtime_meta.goal_handoff_query_summary_json` / `runtime_meta.subagent_children_summary_json` artifact，并纳入 `runtime_observability_meta`；`src/kernel_status.rs` 的 `runtime_report_surface` 也同步把这两个 locator 和观测字段计入可见面，避免 goal collect / subagent tree 的 admission 指针只停留在 CLI 文本层。
- 回归已补：`tests/runtime_report_tests.rs` 锁定 goal handoff summary 和 subagent children summary 的 artifact/observability 提升，同时确认不泄漏正文 payload；`tests/cli_status_tests.rs`、`tests/cli_doctor_tests.rs` 和 `tests/app_server_tests.rs` 的 runtime report surface 断言已从 `7/7` 收口到 `9/9`。专项验证已通过 `cargo fmt --all --check`、`cargo test -q --test runtime_report_tests --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests`、`cargo test -q --test goal_dispatch_tests --test cli_goal_tests`。下一轮入口：继续沿 M5/M6/M7 把 goal collect / subagent tree / runtime report 的只读摘要统一到同一条查询链路上，并补全 smoke 覆盖。

## 2026-05-12 M6 goal collect admission_id 查询指针补齐
- 本轮继续把 goal collect / goal show 的只读查询面往同一套 admission 指针口径收口：`GoalReportAdmissionRef` 新增 `admission_id`，`handoff_query_summary.report_admission_refs` 现在直接暴露 `goal-report-admission://...` 的 admission 指针，同时保留 report/task/agent id、status、reason code 和 evidence ref；`goal_collect_handoff_query_report_admission_refs` / `goal_operability_handoff_query_report_admission_refs` 文本面也同步展示该指针。
- 旧 JSON 通过 `serde(default)` 保持兼容，`GoalReportAdmissionRef` 的 legacy 反序列化回归也已补上，确认缺 `admission_id` 时仍可加载。验证已通过 `cargo fmt --all --check`、`cargo test -q --test goal_dispatch_tests --test cli_goal_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。下一轮继续沿着 goal collect / subagent tree / runtime report 的 admission 指针和 handoff 查询面往前推。

## 2026-05-12 M6 goal collect handoff query summary 口径收紧
- 这轮把 `goal collect` 的 `handoff_query_summary` 再收紧一层：只有 `ReportAdmissionStatus::Accepted` 的 admission 才会计入 `report_admission_ref_count` / `report_admission_reason_codes` / `report_admission_refs`，像 `unsupported_schema_version` 这类 rejected admission 继续留在 blocked evidence，不再混进只读查询摘要。
- `goal collect` 与 `goal show` 的文本输出保持同一口径，回归补了 rejected admission 的 collect 场景；定向验证已通过 `cargo test -q --test goal_dispatch_tests --test cli_goal_tests`。下一轮继续盯 goal collect / subagent tree / runtime report 是否还有类似“先记后筛”的查询口径偏差。

## 2026-05-12 M6 goal collect handoff query summary 再收口
- 本轮继续沿 M5/M6/M7 主链接线收口 goal collect / subagent tree 的统一只读摘要：`GoalDispatchCollectionReceipt` 新增 `handoff_query_summary`，把 `parent_context_handoffs` 与 `report_admission_refs` 合成一份可查询结构；其中只把成功接纳的 report admission 计入 `report_admission_ref_count` / `report_admission_reason_codes` / `report_admission_refs`，失败、错配或 parse blocked 仍只留在 blocked evidence，不会冒充 checkpoint material。
- `goal collect` / `goal show` 文本面也同步多出 `goal_*_handoff_query_*` 汇总，方便父线程一眼看出 handoff 指针、admission 计数和 reason code 分布；旧 JSON 继续保持兼容，`handoff_query_summary` 带 `serde(default)`，缺该字段的 legacy collect 记录仍能反序列化。
- 回归已补在 `tests/goal_dispatch_tests.rs` 与 `tests/cli_goal_tests.rs`，锁定 ready / blocked / readonly collect 的 handoff_query_summary 计数、reason code、文本输出和 legacy JSON 兼容。局部验证已通过 `cargo test -q --test goal_dispatch_tests --test cli_goal_tests`。下一轮继续把这个 handoff query summary 往 runtime report / subagent tree 的统一只读面并过去。

## 2026-05-12 M6 subagent tree admission refs 查询面补齐
- 本轮继续沿 M5/M6/M7 主链接线收口 subagent tree/report handoff：`SubagentChildrenSummary` 新增 `report_admission_refs`，把 children summary 里的 accepted/rejected report admission id、report id、status、reason code 和 evidence ref 直接列成无载荷只读摘要。父线程查询 `subagent_children_listed.children_summary` 时不再必须扫每个 child snapshot 才能找到 report admission ref。
- 兼容边界同步补齐：`report_admission_refs` 带 `serde(default)`，旧的 children summary JSON 缺该字段仍可反序列化；摘要只保留 admission/evidence 指针，不包含 report stdout/stderr 或正文 payload。`subagent_tree_events` 序列化回归也锁住该字段随 `children_summary` 一起出现在事件面。
- 验证已通过：`cargo fmt --all --check`、`cargo test -q --test subagent_tree_ledger_tests --test subagent_tree_events_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。下一轮可继续把 goal collect handoff、subagent tree admission refs 和 runtime report artifacts 对齐成同一份只读查询摘要口径。

## 2026-05-12 M6 goal collect handoff 查询面补齐
- 本轮继续沿 M5/M6/M7 主链接线收口 goal/run 续接面：`GoalDispatchCollectionReceipt` 现在会在成功接纳且身份匹配的 worker report 上生成 `parent_context_handoffs`，直接复用 `build_parent_context_handoff()` 暴露 accepted、report/task/agent id、`provenance_ref`、`admission_reason_code`、summary 和 context debug。`goal collect --json` 与 `goal show --json` 的 `goal_operability.goal_collect` 因此能直接查询 report handoff，不必只从 `checkpoint_suggestion.validation_notes` 反推。
- 安全边界同步收紧：goal collect 会重新跑 `SubagentReportValidator::admit_raw()`；status 非 Success、身份不匹配、parse failed 或 admission rejected 的 report 只进入 `blocked_report_run_ids` / `blocked_report_reasons`，不会生成 parent handoff，也不会进入 `completed_worker_ids` 或 checkpoint material。文本面新增 `goal_collect_parent_context_handoff_count` / refs，以及 `goal_operability_parent_context_handoff_count` / refs，方便人工排障。
- 回归已补并通过：`tests/goal_dispatch_tests.rs` 锁定 ready、missing、failed、identity mismatch 与 readonly collect 的 handoff 计数；`tests/cli_goal_tests.rs` 锁定 goal collect / goal show 文本面的 handoff 摘要。验证通过 `cargo fmt --all --check`、`cargo test -q --test goal_dispatch_tests --test cli_goal_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh` 和 `git diff --check`。下一轮可以继续把这层 handoff 摘要接到 runtime report 或 subagent tree report admission ref 的统一查询口径。

## 2026-05-12 M5/M6/M7 query surface 再补一轮
- 本轮继续沿着 M5/M6/M7 把“可查询、可回放、可续接”往前推：`src/agent_runtime.rs` 现在会把 `PackedContext::compaction_summary()` 序列化写入 `RuntimeResult.response.meta.extra.context_compaction_summary_json`；`src/runtime_report.rs` 把这份结构化摘要提升为 `runtime_meta.context_compaction_summary_json` artifact，并纳入 `runtime_observability_meta`；`src/kernel_status.rs` 的 `runtime_report_surface` 也把该字段纳入可见面，状态/报告不再只剩 `context_pack_trace` / `context_compaction_events` 文本预览。
- 同步补齐了 M5/M6 查询层：`src/mcp_fake_adapter.rs` 现在能把 operator input / secret elicitation 场景写成脱敏 `elicitation_requested` runtime events；`src/runtime_event_ledger.rs` 的 turn summary 也新增 `tool_started`、`tool_finished`、`approval_requested`、`approval_resolved` 和 `elicitation_requested` 计数；`src/subagent_tree_ledger.rs` / `src/subagent_tree_events.rs` 则把 child 计数、report 状态分布和 reason code 分布收成 `children_summary`，让父线程和回放端直接查摘要，不必扫原始 records。
- `subagent report` / `subagent collect` 这轮也继续把 `parent_context_handoff` 接到文本和 JSON 输出里，accepted report 会直接给出 `report_id` / `task_id` / `agent_id` / `summary` / `provenance_ref` / `admission_reason_code`，proposal-only 场景则维持 `memory_proposal_only=true` 的只读边界。
- 回归已通过：`cargo fmt --all --check`、`cargo test -q --test runtime_event_ledger_tests --test mcp_fake_adapter_tests --test subagent_tree_ledger_tests --test subagent_tree_events_tests --test runtime_report_tests`、`cargo test -q --test cli_subagent_dispatch_tests`。下一轮继续把 goal/run 的只读摘要和 subagent protocol 回放面并到同一条可查询链路上。

# 2026-05-12 M6 子代理 report/collect handoff 查询面补齐
- 本轮把 M6 的 report handoff 再往前推了一小步：`subagent report` / `subagent collect` 的 JSON 与文本输出现在都会带上 `parent_context_handoff`，直接复用 `build_parent_context_handoff()` 暴露 accepted / proposal_only 分支、`provenance_ref`、`admission_reason_code`、`summary` 和 `memory_proposal_only`，父线程和回放端不必再自己拼接这层语义。
- 对应回归已补到 `tests/cli_subagent_dispatch_tests.rs`，锁定 accepted report 的 handoff JSON 字段，以及 malformed / partial report 场景下 handoff 维持 `null`。定向验证已通过 `cargo fmt --all --check`、`cargo test -q --test cli_subagent_dispatch_tests` 和 `cargo test -q --test subagent_report_tests`。

## 2026-05-12 M5/M6/M7 子树查询面再收口
- 本轮把 subagent tree 的查询面继续往事件层推进：`src/subagent_tree_events.rs` 的 `SubagentTreeListRuntimeEvent` 现在直接携带 `children_summary`，所以 `subagent_children_listed` 不再只给原始 child snapshots，而是把 child_count、open/reported/closed 计数、accepted/rejected/missing report 计数、child ids 和 reason code 分布一起暴露成稳定 JSON。
- 为了让这条查询面和现有 runtime 合同保持一致，`tests/subagent_tree_events_tests.rs` 也补了 `children_summary` 断言，确认序列化里能直接读到汇总字段；同时修正了 `tests/scripted_responder_tests.rs` 对 `ResponderMeta.extra` 的旧假设，因为 `AgentRuntime::run()` 现在会写入 `context_compaction_summary_json`。
- 回归已通过：`cargo fmt --all --check`、`git diff --check`、`cargo test -q --test scripted_responder_tests --test subagent_tree_events_tests --test subagent_tree_ledger_tests`、`cargo test -q --test runtime_report_tests --test runtime_event_ledger_tests`、`cargo test -q`，以及 `sh scripts/chuang-mvp-smoke.sh`。下一轮继续盯 runtime report / goal run / subagent tree 的只读摘要统一口径。

## 2026-05-12 M5/M6/M7 runtime query surface 补强
- 本轮继续把 M5/M6/M7 主链接线往“可查询摘要”推进：`src/runtime_event_ledger.rs` 的 `RuntimeTurnSummary` 现在除了 event count、risk/evidence/call 计数，还会按 turn 统计 `tool_started`、`tool_finished`、`approval_requested`、`approval_resolved` 和 `elicitation_requested`；`src/runtime_report.rs` 的 `runtime_event_ledger_json` artifact 也会对同一批事件做只读聚合，输出 approval / elicitation / tool 计数，避免查询端只看见 tool started/finished。
- 这轮保持 ledger/report 的兼容性边界：`runtime_event_ledger_json` 的 artifact 解析仍接受最小 `event_type` 事件形状，不要求每条事件都补全完整 runtime event payload；新增的 turn summary 字段则由 `RuntimeEvent` 强类型查询补齐，方便后续把 runtime turn / MCP approval / elicitation / tool 事件统一查询。
- 回归已补并通过：`tests/runtime_event_ledger_tests.rs` 锁定 turn summary 的 approval / elicitation / tool 计数与脱敏边界；`tests/runtime_report_tests.rs` 锁定 runtime report 里的运行账本摘要能同时统计 approval / elicitation / tool 事件且不泄漏 secret-like 字段。专项验证已通过 `cargo fmt --all --check` 和 `cargo test -q --test runtime_event_ledger_tests --test runtime_report_tests`。

# 2026-05-12 M5/M6/M7 主链接线收口到可验收面
- 本轮把 M5/M6/M7 继续往“可查询、可回归、可续接”推进：`src/agent_runtime.rs` 现在会把 `PackedContext::compaction_summary()` 序列化写入 `RuntimeResult.response.meta.extra.context_compaction_summary_json`；`src/runtime_report.rs` 进一步把这份结构化摘要提升为 `runtime_meta.context_compaction_summary_json` artifact，并纳入 `runtime_observability_meta`；`src/kernel_status.rs` 的 `runtime_report_surface` 也把该字段计入可见面，避免状态面只剩 `context_pack_trace` / `context_compaction_events` 文本预览。
- 同步补了 M5/M6 查询面：`src/mcp_fake_adapter.rs` 新增 `McpElicitationRequest` 和 `elicitation_required` runtime event，operator input / secret elicitation 现在会以脱敏事件写进 ledger；`src/subagent_tree_ledger.rs` 新增 `SubagentChildrenSummary` / `summarize_children()`，父线程可直接查询 child 计数、report 状态分布和 reason code 分布，不必扫原始 records。
- 回归已经补齐并通过：`tests/runtime_report_tests.rs` 锁定 compaction summary artifact / observability；`tests/mcp_fake_adapter_tests.rs` 锁定 elicitation 事件脱敏与 ledger 查询；`tests/subagent_tree_ledger_tests.rs` 锁定 children summary 统计；`tests/runtime_event_ledger_tests.rs` 锁定 `ElicitationRequested` 序列化；`cargo test -q --test runtime_event_ledger_tests --test mcp_fake_adapter_tests --test subagent_tree_ledger_tests --test runtime_report_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests --test agent_runtime_sqlite_tests` 已通过。
- 额外验收已通过 `sh scripts/chuang-mvp-smoke.sh` 与 `git diff --check`。下一轮继续盯住 goal/run 续接和 subagent protocol 的查询/回放面，确保第二测试版本的入口与报告面保持同一条可回归链路。

## 2026-05-12 M5/M6/M7 查询面继续收口
- 本轮继续把 M7 从“能算摘要”推进到“能查询到摘要”：`src/agent_runtime.rs` 现在会把 `PackedContext::compaction_summary()` 序列化写入 `RuntimeResult.response.meta.extra.context_compaction_summary_json`，`src/runtime_report.rs` 也把这份结构化摘要提升为 `runtime_meta.context_compaction_summary_json` artifact，并纳入 `runtime_observability_meta`；`src/kernel_status.rs` 的 `runtime_report_surface` 同步把该 artifact / observability 字段计入面板，避免状态面只展示 `context_pack_trace` / `context_compaction_events` 文本预览。
- 回归已补：`tests/runtime_report_tests.rs` 新增 `runtime_report_promotes_context_compaction_summary_without_segment_payloads`，并把原有 `context_compaction_events` 场景也一起锁定 `runtime_meta.context_compaction_summary_json`；`tests/cli_status_tests.rs`、`tests/cli_doctor_tests.rs`、`tests/app_server_tests.rs` 的 `runtime_report_surface` 文案也从 6/6 提升到 7/7。接下来准备重跑 runtime_report / status / doctor / app-server 相关验收。
- 本轮继续把 M5/M6/M7 主链接线从“有事件”推进到“可查询摘要”：`src/runtime_event_ledger.rs` 新增 `ElicitationRequested` 事件类型，MCP fake 现在能把需要操作者补充输入的场景写成脱敏的 runtime ledger 事件；`src/mcp_fake_adapter.rs` 新增 `McpElicitationRequest` / `elicitation_required`，只保留 request id、reason code、policy ref 和 evidence ref，不把原始 prompt 或 secret 值写进账本。
- `src/subagent_tree_ledger.rs` 新增 `SubagentChildrenSummary` / `summarize_children()`，把父线程下的 child_count、open/reported/closed 计数、accepted/rejected/missing report 计数、child thread ids 和 reason code 分布变成只读汇总，供主控快速看树状态，不必再手动扫 records。
- 回归已补：`tests/runtime_event_ledger_tests.rs` 锁定 `elicitation_requested` 序列化；`tests/mcp_fake_adapter_tests.rs` 锁定 MCP elicitation runtime events 的 turn/call 查询与脱敏 evidence；`tests/subagent_tree_ledger_tests.rs` 锁定 children summary 的计数和 reason code 分布。定向验证已通过 `cargo test -q --test runtime_event_ledger_tests --test mcp_fake_adapter_tests --test subagent_tree_ledger_tests`。

## 2026-05-11 Codex + Claude 双参考架构补全
- M5/M6/M7 继续小步推进：`src/mcp_fake_adapter.rs` 新增 `mcp_call_runtime_events()`，把 fake MCP approval/started/finished 结果转成 runtime ledger 事件，供 `RuntimeEventLedger` 查询，不接真实 MCP/network、不打印 secret；`src/subagent_tree_events.rs` 新增 subagent message/wait 桥接事件，继续用审计事件表达子代理活动而不改真实队列执行；`src/context_engine.rs` 新增 `PackedContext::compaction_summary()`，给 compaction runtime trace 提供可查询摘要，不再要求上层解析 prompt 字符串。
- app-server health 与 status/doctor 继续对齐：`src/app_server.rs` 现在把 `runtime_report_surface` 作为结构化 JSON 字段直接带进 health 输出，文本面也同步打印 `runtime_report_surface: ok=true artifacts=6 observability_fields=6`，避免健康面比状态面少一块 runtime/report 可查询合同。
- app-server runtimeObservability 事件面继续补回归：`tests/app_server_tests.rs` 现在同时锁定 `turn/completed` 事件里的 `context_pack_trace` 和 `context_compaction_events`，避免只在 `turn/start` 响应或 channel simulate 输出面可见、事件订阅面丢失 compaction/runtime trace。
- Feishu 独立通道这轮再补一条 direct-startup 回归：`tests/cli_smoke_tests.rs` 新增 `feishu_bridge_script_rejects_forbidden_provider_env_on_direct_startup`，用只读 provider env 触发 `scripts/chuang-feishu-bridge.js` 顶层 `loadProviderEnvReadonly()`，确认它在真正进入 bridge 主循环前就拒绝 `CHUANG_FEISHU_*` / `HERMES_FEISHU_*` 变体，且错误输出仍只暴露变量名不暴露 secret 值。
- 第二批并行 goal worker 已继续收口：Node live preflight 复用 `scripts/chuang-feishu-bridge-config.js` 的 forbidden credential 名单，减少 JS 侧漂移；JS bridge 直启会只读加载 `CHUANG_PROVIDER_ENV_FILE` 并拒绝 provider env 中出现 `CHUANG_FEISHU_*` 或 legacy Feishu credential 名称，仍只报变量名不泄漏值；`chuang-goal-run-status.sh` 新增 `interactive_state` / `activity_hint`，从 tmux pane tail/watchdog tail 判断 working、thinking、idle_waiting_input、compacting_context、active_unclassified、session_missing、unknown，便于区分“还在跑”与“等输入/缺 session”。
- 多子代理 goal 模式继续推进：复用现有 3 个 worker 并行处理 Feishu JS 直启隔离、goal 状态可观测性、Feishu 模板/帮助文案隔离边界，主控审计时补了两个实现缺口。`scripts/chuang-feishu-bridge.js` 现在直启也会拒绝 generic `FEISHU_*`、`HERMES_FEISHU_*`、`CODEX_FEISHU_*` 凭据名，错误只报变量名不泄漏值；JS bridge 加载 env 文件后会重算 workspace/provider/session/SDK 派生路径，避免绕过 shell 时 env 覆盖失效。`scripts/chuang-goal-run-status.sh` 新增当前 tmux interactive goal 只读观察与 watchdog/overnight freshness，能区分旧 overnight smoke 与仍在运行的 `chuang-codex-claude-goal`。模板、checklist 与 `/help` 同步强调不复用 Hermes/Codex bridge、凭据、会话或队列。
- Feishu 独立通道隔离继续加固：`channel feishu-check` 的 legacy env 检测从少量硬编码扩展为明确的 forbidden credential namespace helper，覆盖 generic `FEISHU_*`、`HERMES_FEISHU_*`、`CODEX_FEISHU_*` 下的 app id、app secret、bot id、verification token、encrypt key，避免 Chuang bot env 文件混入 Hermes/Codex/旧桥密钥名后仍被误判为 ready。同步补 `cli_channel_tests` 回归和 `docs/feishu-dedicated-channel-checklist.md` 开源配置边界说明。
- 同一隔离口径已继续同步到 Node live preflight 与 operator checklist：`scripts/chuang-feishu-live-preflight.js` 和 `scripts/chuang-live-operator-checklist.sh` 现在也阻断 Hermes/Codex bot id、verification token、encrypt key 变体；对应 smoke/脚本测试覆盖继承环境只读忽略、env 文件内 forbidden 名称阻断、且不泄漏 secret value。
- Chuang Feishu bridge 启动脚本修正 env 文件覆盖顺序：`scripts/chuang-feishu-bridge.sh` 现在先读取 Chuang env 文件，再重算 `CHUANG_AGENT_ROOT`、`CHUANG_PROVIDER_ENV_FILE`、`CHUANG_FEISHU_SDK_NODE_MODULES` 派生路径，确保 env 文件内的 provider env 指针会被实际 source，而不是被脚本默认值提前固定。
- 已补官方 Codex 代码级架构审计：本地审计源为 `/tmp/openai-codex-audit`，commit `76845d7`，新增 `docs/codex-architecture-audit-v1.md`。结论是 Chuang 最初 Slot/trait/event/governance/memory-body 方向成立，但当前落地要吸收 Codex 的 SQ/EQ protocol、`Session`/`TurnContext`、`ToolRegistry` dispatch、`UnifiedExec`、`exec_policy`/sandbox/guardian、SQLite state 与 rollout trace。
- 新增 `docs/codex-claude-optimization-plan-v1.md`，把 Codex 与 `claude-rust` 分工合并：Codex 主导运行骨架、治理执行、安全沙箱、state/trace、多代理 agent tree；Claude 主导工具 descriptor/MCP 易迁移实现、`QueryEngine` 工具回灌/retry/compaction 细节和 allow/deny pattern UX。
- 优先级已从“直接补更多工具”调整为 M1-M3：先做 `RuntimeEventLedger`、`ToolRegistrySlot`、`PermissionProfileSlot`，把“普通本地完整能力默认执行，高危才询问/拒绝”落成 policy 和 contract，而不是只靠 prompt；随后再做 unified exec/actuator orchestrator、MCP fake adapter、SubagentTreeLedger。
- 这轮先把 M1-M3 的最小合同正式纳入 crate 公共边界：`src/lib.rs` 已导出 `runtime_event_ledger`、`tool_registry_slot`、`permission_profile_slot`、`subagent_tree_ledger`，四个对应测试也从 `#[path]` 旁路导入改成直接引用 `chuang_agent::...`。这一步把 slot/ledger 从“能测”推进到“能被主 crate 依赖”，后续 runtime/governance 接线可以直接复用同一套类型，不再分裂出第二套测试私有定义。
- 本轮验证已通过 `cargo test -q --test runtime_event_ledger_tests --test tool_registry_slot_tests --test permission_profile_slot_tests --test subagent_tree_ledger_tests` 和 `git diff --check`；当前仍保持 small batch，没有触碰 Hermes、真实 Feishu 或无关项目。
- 按老爸要求重派 M4/M5/M6 并行 worker，普通编码 worker 统一指定 `gpt-5.3-codex`，主控负责审计和合并。M4 落地 `unified_execution_slot`，补 `ExecutionRequest` / `ExecutionResult` / 结构化 `ExecutionFailure` / fake orchestrator，并把 failure reason 自身也做脱敏和 `reason_redacted` 审计；M5 落地 `mcp_fake_adapter`，覆盖 fake list/call/error/timeout/stderr/risk descriptor/arguments redaction；M6 落地 `subagent_tree_events`，把 spawn/report/close/list children 桥接成 runtime event，并给 list snapshot 补 root/parent/admission/evidence 与 consistency warnings。
- M4-M6 已正式纳入 crate 公共边界：`src/lib.rs` 导出 `unified_execution_slot`、`mcp_fake_adapter`、`subagent_tree_events`，对应测试从 `#[path]` 旁路导入改成 `chuang_agent::...`。本轮验证通过 `cargo test -q --test runtime_event_ledger_tests --test tool_registry_slot_tests --test permission_profile_slot_tests --test subagent_tree_ledger_tests --test unified_execution_slot_tests --test mcp_fake_adapter_tests --test subagent_tree_events_tests`、`cargo fmt --all --check`、`cargo check -q`、`git diff --check`。
- 第二批并行 N1/N2/N3 继续按 `gpt-5.3-codex` 跑编码 worker：N2 给 `RuntimeEventLedger` 补 `query_by_turn`、`query_by_call`、`summarize_turn` 只读查询/摘要，摘要只保留计数、时间边界和事件类型序列，不复述 call/evidence 原文；N3 给 MCP fake descriptor 增加到 `ToolDescriptorRisk` 的纯转换，并把 mutating 且 omitted risk 的 MCP 默认收紧为需要审批，避免静默安全；N1 新增 `turn_context` fake-first snapshot 合同，覆盖 thread/turn/workspace/provider/model/permission/tools/memory/history/env 状态，env 只保留 `<set>` / `<missing>` / `<redacted>`。
- N1-N3 已纳入 crate 公共边界：`src/lib.rs` 导出 `turn_context`，`turn_context_tests` 也从 `#[path]` 改为直接引用 `chuang_agent::turn_context`。主控审计时补了 provider/model 必填校验，避免 TurnContext 在身份不完整时 silent fallback。本轮验证通过 `cargo test -q --test runtime_event_ledger_tests --test tool_registry_slot_tests --test permission_profile_slot_tests --test subagent_tree_ledger_tests --test unified_execution_slot_tests --test mcp_fake_adapter_tests --test subagent_tree_events_tests --test turn_context_tests`、`cargo fmt --all --check`、`cargo check -q`、`git diff --check`。
- 第三批并行 P1/P2/P3 继续按 `gpt-5.3-codex` 跑编码 worker：P1 在 `agent_runtime` / `tool_loop_meta` 增加工具协议 correction context 和 `missing_final` typed failure metadata，覆盖 invalid ACTION JSON、ACTION+FINAL trailing、wrong tool name、tool loop exhausted，不改变 ACTION/FINAL 主协议；P2 在 `runtime_report` 聚合 typed tool failure classes/count，并在 observability artifact 中展示 `tool_typed_failures`；P3 在 `kernel_status` 增加 `policy_tool_status` 只读状态面，展示 active `local_ga`、普通本地动作 `allow_with_audit`、高危边界、GA tool descriptor 映射和每个工具的 local decision。
- 主控审计时修正 `tool_registry_slot` 的桌面交互 descriptor：`open_app` / `mouse` / `keyboard` 现在分别带 `open_app` / `click` / `input` 风险 tag，避免 status 用 descriptor 评估时把普通本地桌面动作误判为默认审批；同时调整 `runtime_report_tests` 的预算基线，适配系统保留段变大后的当前 token 成本。本轮验证通过 `cargo test -q --test agent_runtime_tests --test runtime_report_tests --test kernel_status_tests --test tool_registry_slot_tests --test permission_profile_slot_tests --test runtime_event_ledger_tests --test unified_execution_slot_tests --test mcp_fake_adapter_tests --test subagent_tree_events_tests --test turn_context_tests`、`cargo fmt --all --check`、`cargo check -q`、`git diff --check`。
- 第四批并行 Q1/Q2/Q3 继续收第三批留下的接线缺口：Q1 让 `runtime_report` 直接消费 `tool_protocol_typed_failure_code`，把协议层 typed failure 纳入 `tool_typed_failure_classes/count`，但不复述 failure message；Q2 把 `policy_tool_status` 展示到 `status` / `doctor` 文本输出，用户不看 JSON 也能看到 `local_ga`、`allow_with_audit` 和高危边界；Q3 给 `turn_context` 增加 `RuntimeTurnContextInput` 和 `from_runtime_config_summary` helper，从 `ConfigSummary` 进入 snapshot 合同，但仍不读取真实 secret env。
- Q1-Q3 验证通过 `cargo test -q --test runtime_report_tests --test turn_context_tests --test cli_status_tests --test cli_doctor_tests`、`cargo test -q --test agent_runtime_tests --test kernel_status_tests --test tool_registry_slot_tests --test permission_profile_slot_tests`、`cargo fmt --all --check`、`cargo check -q`、`git diff --check`。这批仍只做合同/状态/报告接线，不接真实 actuator、不启动服务、不改 Hermes。
- 第五批 Q1/Q2/Q3 已收口并准备进入夜间 goal 跑法：`runtime_report` 已直接消费 `tool_protocol_typed_failure_code`，`status` / `doctor` 文本面已输出 `policy_tool_status`，`turn_context` 已有从 `ConfigSummary` 构建 snapshot 的 runtime 风格 helper。验证通过 `cargo test -q --test runtime_report_tests --test turn_context_tests --test cli_status_tests --test cli_doctor_tests`、`cargo test -q --test agent_runtime_tests --test kernel_status_tests --test tool_registry_slot_tests --test permission_profile_slot_tests`、`cargo fmt --all --check`、`cargo check -q`、`git diff --check`。
- 本轮继续推进 M5/M6 主链接线：M5 的 `mcp_fake_adapter` 风险视图新增 `omitted_risk_defaults_tightened` 和 `permission_decision_hint`，让 read-only、local mutating、omitted-risk、open-world、external_commit 的治理语义在 fake MCP descriptor/list/call 审计里直接可见；MCP descriptor 到 `ToolDescriptorRisk` 的桥接也补 `external_commit` / `omitted_risk_tightened` 标签，避免高风险或缺省风险静默降级。M6 新增 `ParentContextHandoff` 与 `build_parent_context_handoff`，accepted report 带 provenance、summary、context_debug 进入父上下文，rejected report 不带报告载荷，只保留 `memory_proposal_only=true` 边界。验证通过 `cargo test -q --test mcp_fake_adapter_tests --test tool_registry_slot_tests --test governance_tests --test subagent_report_tests --test subagent_queue_tests --test context_engine_tests --test agent_runtime_tests --test runtime_report_tests`、`cargo fmt --all --check`、`git diff --check`。
- M2/M4 主链接线继续落地：`ExecutionSlot` 新增 ledger-aware 执行入口，工具执行前后会写入 `RuntimeEventLedger` 的 `ToolStarted` / `ToolFinished` 事件，并保留治理决策与 `tool://...` evidence ref；CLI tool loop 已改用该入口，最终 turn metadata 透出 `runtime_event_ledger_json` 与 `runtime_event_count`，治理拒绝也会进入同一条事件流。定向验证通过 `cargo test -q --test tool_runtime_tests execution_slot_records_runtime_ledger_events_for_tool_calls`、`cargo test -q run_with_options_executes_tool_calls_before_final_answer`、`cargo test -q run_with_options_feeds_governance_rejection_back_to_model`。
- M2/M4/M7 报告面继续接线：`runtime_report` 现在会把 `runtime_event_ledger_json` 提升为 `runtime_meta.runtime_event_ledger_json` artifact，并在 observability 描述中展示 `runtime_events=N`；`tool_loop_meta` 增加 unified execution 失败类汇总，报告面透出 `tool_unified_execution_status/failure_count/failure_classes`；`pack_trace` 与 `compaction_events` 也从 `packed_context_preview` 提升为 runtime observability metadata 和独立 artifacts，避免上下文压缩轨迹只藏在 prompt preview。验证通过 `cargo test -q --test runtime_report_tests --test unified_execution_slot_tests`、`cargo test -q --test tool_runtime_tests`。
- status/doctor 状态面继续补齐：`ChuangMvpStatus` 新增 `runtime_report_surface`，只读展示当前报告面支持的 artifact locators 与 observability fields，包括 `runtime_event_ledger_json`、`context_pack_trace`、`context_compaction_events` 和 `tool_unified_execution_status`；`status` / `doctor` 文本面同步输出该摘要，便于不跑真实 turn 也能诊断报告面合同是否齐全。验证通过 `cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests`、`cargo test -q --test runtime_report_tests`。
- app-server/channel 现有 runtimeObservability 通路补回归：不新增协议字段，只锁定 `runtime_observability_meta()` 已经透出的 `tool_unified_execution_status`、`tool_unified_execution_failure_count`、`context_pack_trace`、`context_compaction_events` 能从 app-server `turn/start` / `turn/completed` 和 `channel simulate --json` 输出面读到；无工具 turn 继续不伪造 `runtime_event_count`。验证通过 `cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config`、`cargo test -q --test cli_channel_tests cli_channel_simulate_runs_workspace_config_without_fake_responder`、`cargo test -q --test cli_channel_tests cli_channel_simulate_surfaces_tool_context_and_readonly_guidance`、`cargo test -q --test app_server_tests --test cli_channel_tests`、`cargo fmt --all --check`、`git diff --check`。
- 并行第一批实现已落最小合同：新增 `runtime_event_ledger`、`tool_registry_slot`、`permission_profile_slot`、`subagent_tree_ledger` 四个独立模块及定向测试，并新增 `docs/codex-claude-implementation-slices-v1.md` 拆解 M1-M7 工单、写入范围、验收命令和风险边界。当前仍是 fake/纯结构层，尚未接入真实 runtime/tool dispatch/governance 主链。

## 2026-05-11 claude-rust Slot 审计
- 已对 `/home/user/projects/claude-rust` 做代码级 Slot 审计，并新增 `docs/claude-rust-slot-audit-v1.md` 与 `docs/claude-rust-integration-plan-v1.md`：结论是 `claude-rust` 值得吸收，但不能整体替换 Chuang 主链；优先吸收 `Tool` trait / `ToolRegistry` / MCP fake adapter、`QueryEngine` 的流式 tool-use loop 与 overload retry、`permission` 的模式和 allow/deny pattern。
- 对老爸给出的映射做了校准：`AgentLoop`、`Execution`、`Governance`、`Provider`、`Context` 映射成立；`GroupCoordinator` 需要降级为“有 nested sub-agent / explore adapter 原型”，`claude-rust-coordinator` 当前偏 scaffold，不是完整群体协同系统；`claude-rust-memory` 只是 JSON session repository，不适合作为 Chuang 核心记忆层。
- 第一阶段集成计划锁定 M1/M2：先做 `ToolRegistrySlot` 设计与 fake contract，再做 MCP fake stdio adapter；禁止真实 MCP 绕过 Chuang governance/allowlist/audit，禁止 `Bypass` 模式成为开源默认。

## 2026-05-10 live 现场证据第一轮
- Feishu 现场证据已推进：老爸在 Chuang 专用 Feishu 通道返回 `/health`、`/session`、`/tools` 结果，确认 bridge=ready、app-server=running、workspace=`/home/user/projects/chuang-agent`、session=`chuang-thread-1`，Feishu env 与 provider env 只显示 `<set>`，且 `/tools` 展示 `/new`、`/session`、`/health`、`/receipt`、`/live-check`、普通文本、图片 OCR 和主链工具边界；这证明 Feishu 本地命令面与绑定证据可用。
- provider readiness 本地只读检查通过：`scripts/chuang-provider-readiness-check.sh --json` 输出 `overall_state=ready`、`provider_kind=openai_compatible`、`transport=native`、`api_key_state=<set>`、`connects_real_provider=false`、`request_timeout_ms=120000`。同轮 Feishu 普通文本先遇到一次上游 `502 Bad Gateway`，随后老爸重试 `哈喽` 成功返回“哈喽，老爸，我在。有什么要我处理的？”，模型 `gpt-5.5`、耗时 2.0s、API 1 次、runtime report `report-turn-1`，因此 provider live request 证据从 blocked 修正为 verified。
- single worker rehearsal 本地边界继续通过：`sh scripts/chuang-live-runner-rehearsal-smoke.sh` 输出 `live_runner_rehearsal_smoke_ok`，保留 queue/run evidence；它证明 gate/allowlist/capability/report admission rehearsal 可复验，但仍不等于 runner 池 ready。
- desktop/browser/wiki/GBrain 证据第一轮已跑本地只读合同：`scripts/chuang-real-actuator-adapter.py` 的 `observe` / `screenshot` 都返回 `read_only=true`、`live_gate_required=false`，但当前终端环境没有图形 `DISPLAY`，窗口标题与截图 evidence 均为 unavailable；`cargo test -q --test actuator_tests --test browser_read_tests --test knowledge_read_tests` 和 `cargo test -q --test tool_runtime_tests tool_runtime_can_execute_desktop_atomic_tools_with_fake_actuator` 通过。`memory knowledge source-contract --source wiki|gbrain --json` 均确认 read-only/source-contract 可用、`connects_real_service=false`、`live_adapter_configured=false`。
- Kubuntu 桌面会话已确认：X11 socket 为 `:0`，Xauthority 为 `/run/user/1000/.Xauthority`。用该环境重跑 `scripts/chuang-real-actuator-adapter.py` 只读 observe/screenshot 成功：observe 读取到 `current_window_title=飞书 source=xdotool`，screenshot 生成 `file:///tmp/chuang-actuator-evidence/screenshot-1778423348649.png`，文件为 1920x1080 PNG；两条回执均保持 `real_execution=false`、`read_only=true`、`live_gate_required=false`。
- Chuang 主链自测第一轮已证明模型会调用只读观察工具：老爸在 Feishu 让 Chuang “只读观察当前桌面窗口标题”后，主链执行本地工具 1 次并返回 `report-turn-2`，但 app-server 子进程当时缺 `DISPLAY/XAUTHORITY`，所以 evidence 为 `chuang-actuator://observe/unavailable`。随后 `scripts/chuang-feishu-bridge.sh` 改为启动时自动探测桌面环境：按当前用户 id 发现 `XDG_RUNTIME_DIR` / `XAUTHORITY`，从 `/tmp/.X11-unix/X*` 发现 X11 `DISPLAY`，必要时发现 `WAYLAND_DISPLAY`；本机 env 里不再硬编码桌面路径。`ops/systemd` 示例也改成 `/absolute/path/to/...` 占位和自动探测说明，避免开源后携带本机路径。重启 `chuang-feishu-bot.service` 后服务保持 active；验证通过 `bash -n scripts/chuang-feishu-bridge.sh`、`cargo test -q --test cli_smoke_tests feishu_bridge_script_discovers_desktop_env_without_host_specific_display`、`git diff --check`。
- Chuang 主链第二轮自测确认桌面只读观察已修通：老爸在 Feishu 复测后，`locate` 返回 `current_window_title=飞书`、`source=xdotool`、`evidence_uri=chuang-actuator://observe/xdotool`、`read_only=true`、`real_execution=false`。同轮让 Chuang “点一下飞书右上角的关闭”时，真实点击因当时缺少完整 actuator 执行配置未完成；已修正工具循环耗尽路径，遇到未配置的桌面动作会返回明确“未执行点击、输入或修改，需要补 adapter/live gate/action allowlist”的回执，而不是 `tool_loop_exhausted` runtime failure。
- 按“GA 原子工具默认开启”方向推进：`config/actuator-allowlist.example.json` 现在默认允许 `click` / `input_text` / `screenshot`；`scripts/chuang-real-actuator-adapter.py` 在 `CHUANG_REAL_ACTUATOR_ENABLE=1` 时用 `xdotool` 执行坐标点击和非 secret 文本输入，gate 未开时仍返回 dry-run 审计回执；`scripts/chuang-feishu-bridge.sh` 启动 Chuang Feishu 通道时默认给本进程设置 `CHUANG_REAL_ACTUATOR_ENABLE=1`，同时保留现有桌面 env 自动探测。治理分类也把 `mouse` / `keyboard` 从只读观察改成 `LocalDesktopInteraction`，避免真实交互在审计里伪装成 observe。
- 补齐 `open_app` 受治理桌面工具：主链工具协议、atomic tool 映射、doctor schema、CLI runtime 和能力 primer 现在都显式暴露 `open_app`；它作为辅助桌面工具接入 actuator，不是任意启动器，真实打开仍走 adapter、allowlist、live gate、治理和审计。示例 allowlist 已加入 `Chrome -> google-chrome-stable`，Feishu 侧可直接要求 Chuang `open_app Chrome`，再用 `locate` 观察当前窗口确认。
- 执行模式口径已按老爸要求调整：普通本地能力默认直接执行，不再因为常规打开应用、点击或输入要求人工审批；`open_app` / `mouse` / `keyboard` 仍走 actuator gate、allowlist、治理和审计。删除、清理、重置、卸载、支付、验证码、服务或网络变更、密钥访问等高危操作才询问或拒绝。后续开源时再把“审批模式/非审批模式”作为可配置策略暴露。
- 工具协议容错补强：如果模型把 `ACTION: {...}` 和后续 `FINAL:` / 下一条 `ACTION:` 粘在同一次输出里，runtime 现在会先解析并执行第一段合法 JSON，避免 Chrome 已打开但后续 `locate` 因 `trailing characters` 中断；任意普通尾随文本仍保留为协议错误。回归覆盖 `ACTION locate + FINAL` 粘连、任意 trailing text 拒绝和既有 tool loop protocol error 反馈。
- 工具协议主路径同步收紧：能力提示、工具循环续问和协议文档都明确“每次回复只能输出一个结构”，`tool_call` 后必须等待工具结果，禁止把 `ACTION` 和 `FINAL` 粘在同一次输出里；容错只是兜底，不再作为模型应遵循的正常行为。
- 当前结论：Feishu health/session/tools 现场证据已拿到，provider 配置 ready 且 Feishu 普通文本 live request 已有 `report-turn-1`；desktop 只读 observe/screenshot evidence 已拿到，但 browser DOM/URL/title live adapter 和 wiki/GBrain live adapter 仍缺，因此 real live 仍未 100% 完成。

## 2026-05-10 live readiness wrapper coverage checkpoint
- 已提交 `1b71aef feat: extend live readiness wrapper coverage`，把 `scripts/chuang-candidate-verify.sh` 与 `scripts/chuang-third-test-smoke.sh` 的 `live runner readiness view` 顺序接入候选链路，并在 `tests/cli_smoke_tests.rs`、`tests/live_operator_scripts_tests.rs` 里补齐同口径断言，保证它稳定位于 `live gaps` 之后、`operator checklist` / `goal run status` 之前。
- clean-tree 复验继续通过：`sh scripts/chuang-final-verify.sh` 与 `sh scripts/chuang-third-test-smoke.sh` 都回到 `chuang_final_verify_ok` / `third_test_candidate_smoke_ok`，当前工作树已清空。
- 当前仍然后置的是真实外部验收链路，尤其是 Feishu/provider/single worker rehearsal/desktop/browser/wiki/GBrain 的人工 evidence 和最终 `real_live` 验证。

## 2026-05-10 live runner readiness view 收口
- 新增 `scripts/chuang-live-runner-readiness-view.sh`，把 `subagent live-preflight`、`status --json`、`doctor --json` 和 `app-server health --diagnostic --json` 聚合成一份本地只读视图，输出 `ready_for_live`、`starts_external_worker`、`capability_mismatch_blocks_live`、`blocked_reason`、`next_action` 和 source evidence refs；它只做聚合，不启动 worker，不接真实外部服务，也不把 blocked 证据改写成 ready。
- 新增 `tests/live_runner_readiness_view_tests.rs`，锁定新视图的 CLI、JSON 键、read-only 边界和聚合字段；`docs/multi-worker-orchestration.md` 和 `docs/acceptance-next-matrix.md` 也同步收入口径。
- `tests/cli_smoke_tests.rs` 和 `tests/live_operator_scripts_tests.rs` 也同步补上 candidate/third-test wrapper 顺序断言，确认 `live runner readiness view` 已接在 `live gaps` 之后、`operator checklist` / `goal run status` 之前；本轮定向验证继续通过，不启动真实 worker，不连接外部服务。

## 2026-05-10 live receipt / acceptance matrix / runbook 文档同步
- collector 口径已收成 standalone overlay/merge layer：`docs/live-receipt-collection.md` 现在明确它位于 readiness / preflight 之后、最终 live receipt 之前，脚本以 base template 为底接收 partial receipt overlay，再深度合并成 canonical live receipt；同时把 `subagent_live_rehearsal` 的输入输出关系写清楚，`real_live_acceptance` 明确单 worker rehearsal 不是 runner pool ready。
- 这轮只做文档口径同步，不改脚本逻辑、不碰真实服务：`docs/live-operator-test-runbook.md`、`docs/acceptance-next-matrix.md`、`docs/third-test-candidate.md` 统一了最新 live receipt 结构说明，把 `service_evidence` / `service_receipts` / `real_live_acceptance.services` 明确成 7 项 1:1 对齐（Feishu、provider、single worker rehearsal、desktop、browser、wiki、GBrain）。
- 三份文档把对外叙述收成同一组词：Feishu 只算 bridge/contact 证据；provider readiness 只算 `<set>/<missing>` 和 live readiness 证据，provider live receipt 仍要单独留痕；single worker rehearsal 作为 live gate + allowlist + report admission 的单项证据，脚本内 `subagent_live_rehearsal` 只作为 receipt id 锚点保留。
- `docs/acceptance-next-matrix.md` 的 7 项 acceptance matrix 也同步到同一口径，`real_external_acceptance_pending`、operator receipt template 和 manual live check 现在都明确区分 readiness、receipt 和真实 live。

## 2026-05-10 knowledge/browser/actuator/app-server 收口
- provider readiness / live receipt 证据字段再收口：`scripts/chuang-provider-readiness-check.sh` 现在对外复述 `source_status_surface`，`scripts/chuang-live-operator-checklist.sh` 的 provider readiness evidence 也对齐同名字段，`scripts/chuang-live-operator-receipt.sh` 的 provider 证据项改成 `provider_live_request_receipt_ref`，把本地只读预检和人工 live receipt 的口径分开说清楚；仍然不发真实 provider 请求、不泄露 secret、不改变 fallback 拒绝边界。
- live operator receipt flow 继续增强：`scripts/chuang-live-operator-receipt.sh --json` 现在输出 `request_id`、`approval_scope`、`rollback_condition`、`readonly_boundaries`、`service_evidence`、`service_receipts` 和 `real_live_acceptance`，覆盖 Feishu/provider/single worker rehearsal/desktop/browser/wiki/GBrain 七类 evidence 槽位；`scripts/chuang-live-operator-checklist.sh` 的 real-live acceptance matrix 也对齐为 7 项。模板仍固定 `can_mark_real_live_ready=false`，不连接真实服务、不读 secret、不启动 worker、不修改仓库。`scripts/chuang-candidate-verify.sh` 和 `scripts/chuang-third-test-smoke.sh` 已纳入 receipt 模板结构断言，`/receipt` 飞书文案和 smoke 同步更新。
- provider readiness 诊断再补一层：`status --json` 现在在不读取 secret、不 source env 文件的前提下，暴露 `provider_env_file` / `provider_env_file_state`，用于解释“当前进程 api key missing，但默认 `~/.config/chuang-agent/provider.env` 存在，可通过 `scripts/chuang-provider-readiness-check.sh` 吸收”的状态差异；这仍不代表发起真实 provider 请求。
- `knowledge_read` 已从旧的 `external_knowledge` 预览概念里拆出真实 wiki/GBrain live-read 合同：`src/knowledge_read.rs` 提供 fake/unavailable adapter、结构化 `knowledge_read_unavailable`、wiki/gbrain 双源 preflight，`RuntimeConfig.external_knowledge` 和配置文件解析已统一到 `KnowledgeReadConfig`，`status --json` 通过 `knowledge_readiness` 明确展示“本地 preview 可用，但真实 wiki/GBrain adapter 未配置”。当前仍不声明已连接真实 wiki/GBrain。
- `knowledge_read` 的 preflight 语义继续收紧：即使 endpoint、token env 和 token 状态都齐全，在真实 adapter 未接线前也只返回 `preflight_ready_adapter_missing` / `available=false`，避免把“配置已齐”误报为“live 查询可用”。
- `browser_read` 继续保持独立合同边界：`desktop_read` 只代表 `locate/screenshot` 只读屏幕证据，`browser_read` 才代表 URL/title/DOM live read；当前状态固定为 `desktop_read_ready_browser_read_unavailable`，避免把窗口标题或截图证据冒充成浏览器 DOM/URL/title。
- actuator 只读证据回执补强：`Observation` / `EvidenceRef` 增加 `audit_message`，command adapter 会保留外部 adapter message，`tool_runtime` 对 `locate` / `screenshot` 输出结构化 JSON，包含 `summary`、`evidence_uri` 和 `audit_message`，便于飞书侧判断“确实取证了什么”。
- app-server 增加 thread/workspace sticky check：已存在 thread 如果被不同 `workspaceRoot` 复用会被拒绝，避免飞书会话绑定漂移到别的工作区；bridge 侧仍保留 app-server 子进程退出后的下一请求自愈重启。
- 本轮定向验证已通过：`cargo check -q`、`cargo fmt --all --check`、`cargo test -q --test kernel_status_tests`、`cargo test -q --test cli_status_tests`、`cargo test -q --test agent_runtime_tests`、`cargo test -q --test context_engine_tests`、`cargo test -q --test browser_read_tests --test knowledge_read_tests --test actuator_tests --test tool_runtime_tests`、`cargo test -q --test app_server_tests app_server_rejects_workspace_change_for_existing_thread`、`cargo test -q --test app_server_tests app_server_second_turn_injects_recent_thread_history`、`node --check scripts/chuang-feishu-bridge.js`、`node --check scripts/chuang-feishu-bridge-commands.js`、`node --check scripts/chuang-feishu-command-smoke.js`。
- 整体验证复核通过：`cargo check -q`、`cargo test -q --test browser_read_tests --test knowledge_read_tests --test kernel_status_tests --test cli_status_tests`、`cargo run --quiet -- status --json`。状态面确认 `knowledge_readiness.overall_state=local_preview_ready_knowledge_read_unavailable`，`browser_readiness.overall_state=desktop_read_ready_browser_read_unavailable`，真实 wiki/GBrain 与浏览器 URL/title/DOM adapter 仍未声明可用。
- 全量门禁复核通过：修正 `cli_runtime`、`agent_runtime_sqlite_tests`、`runtime_report_tests` 里受新默认能力 primer/系统段影响的过小测试预算；`app_server_tests` 的本地 HTTP stub 改为按 `Content-Length` 读完整请求，避免 prompt 变长后提前断流导致 provider metadata 误报 transport/config error。最终通过 `cargo test -q`、`sh scripts/chuang-complete-local-smoke.sh`、`sh scripts/chuang-candidate-verify.sh`、`cargo fmt --all --check`、`git diff --check`、`cargo check -q`、`node --check scripts/chuang-feishu-bridge.js`、`node --check scripts/chuang-feishu-bridge-commands.js`、`node --check scripts/chuang-feishu-command-smoke.js`。
- 候选门禁继续补齐：`scripts/chuang-candidate-verify.sh` 现在除了 complete-local、live runner rehearsal、live-gaps 和 provider readiness，还会跑 operator checklist 只读摘要与 goal run status 只读摘要，作为 dirty-tree friendly 的第三测试本地等价门禁；仍不连接真实 Feishu/provider，不启动服务，不启 live gate，不打印 secret。验证通过 `bash -n scripts/chuang-candidate-verify.sh`、`cargo test -q --test cli_smoke_tests candidate_verify_wrapper_sequences_dirty_tree_friendly_candidate_gates`、`sh scripts/chuang-candidate-verify.sh`。
- 已提交 checkpoint `844dd7f feat: tighten live readiness contracts`。提交后在 clean tree 上复跑通过 `sh scripts/chuang-final-verify.sh` 和 `sh scripts/chuang-third-test-smoke.sh`，最终 marker 分别为 `chuang_final_verify_ok` 与 `third_test_candidate_smoke_ok`；第三测试候选仍明确显示真实外部验收 `real_live=pending`，只读 operator checklist 的 real live acceptance 为 `not_verified`。

## 2026-05-10 context packing 核心段保留
- 这轮把 `ContextPacker` 收成“先保留 core/session/tool/history，再挤普通 recall/memory”的分层：`system-capabilities`、`tool-instructions`、`session-context`、`recent-conversation-history` 和系统段会先进入保留层，预算压力下优先丢普通记忆与召回，而不是把工具指令、会话/工作区和最近对话裁掉。
- `recent-conversation-history` 的 priority 也上调到保留层，CLI 的 `RunCliRequest` 注入链路继续把它作为短期会话上下文送进 prompt，并补了预算压力回归，确认 session/tool/history 在受压时仍可见。
- 回归补在 `tests/context_engine_tests.rs`、`tests/agent_runtime_tests.rs` 和 `src/cli_runtime.rs`，并更新 `agent_runtime` / `cli_runtime` 里的相关断言。后续已修复 `external_knowledge` 相关编译阻塞，当前定向上下文、状态和 runtime 回归可通过。

## 2026-05-10 browser live read 最小合同
- 新增 `browser_read` 最小接口线，把 `desktop_read` 和 `browser_read` 明确拆开：`desktop_read` 仍是 actuator 的 `locate/screenshot` 只读观察证据，`browser_read` 才代表 URL/title/DOM live read。
- 本轮只做 trait/contract/fake/status/docs，不接真实 CDP/Playwright。`FakeBrowserReadAdapter` 只返回注入快照用于合同测试；默认 `UnavailableBrowserReadAdapter` 会返回结构化 `browser_read_unavailable`，不允许把桌面观察结果冒充为已读 DOM、URL 或 title。
- `status --json` 新增 `browser_readiness`，当前固定为 `desktop_read_ready_browser_read_unavailable`，同时 `live_readiness.terms` 增加 `browser_read_unavailable`，让验收矩阵可以直接看到 browser live read 仍缺真实 adapter。
- 验证通过：`cargo fmt --all --check`、`cargo test -q --test browser_read_tests --test kernel_status_tests --test cli_status_tests`。

## 2026-05-10 capability primer 默认注入再收口
- 这轮把共享 `assets/capability_primer.txt` 收成更短的同源边界摘要，普通 turn、`status`、`doctor`、`console` 和 Feishu `/tools` 继续读同一份文本；内容现在直接点明 `file_read/file_write/code_execute/list_dir`、`locate/screenshot`、`memory/session`、`goal/subagent` 和 `subagent dispatch/list/run-once/run-loop/report/collect` 的真实边界，避免普通对话先发 `/tools` 才知道能力范围。
- Feishu `/tools` 文案补了一句“普通文本转发到主链时会默认注入同一份 primer”，`src/cli_runtime.rs` 的普通 turn 注入回归也改成断言这份短版 primer 能进入 prompt / packed context。
- 回归通过：`cargo fmt --all --check`、`git diff --check`、`cargo test -q --test kernel_status_tests kernel_status_exposes_mvp_config_slots_and_kernel_snapshot -- --nocapture`、`cargo test -q --test cli_status_tests cli_status_prints_mvp_health_summary -- --nocapture`、`cargo test -q --test app_server_tests app_server_health_reports_workspace_runtime -- --nocapture`、`cargo test -q --test agent_runtime_tests agent_runtime_surfaces_readonly_desktop_browser_and_knowledge_guidance_in_prompt -- --nocapture`、`node scripts/chuang-feishu-command-smoke.js`。工作树里仍有其他已有改动带来的独立编译回归，和这轮 primer 收口无关。

## 2026-05-10 context budget 默认值收口
- 对照 GPT-5.5 / OpenClaw / Hermes 口径后，确认 Chuang 之前把“模型上下文窗口”和“运行时打包预算”混得过保守：主配置、示例配置和 `AgentRuntime` fallback 仍残留 512，而 `RuntimeConfig` 默认和状态面曾临时收口到 1536。
- 本轮把 `config.toml`、`config.example.toml`、`config.example-provider-fallback.toml`、历史当前报告里的 `context_max_tokens` 统一到 272000，并让 `src/agent_runtime.rs` 复用 `runtime_config::default_context_budget()`，避免出现配置默认 1536、裸 runtime 默认 512 的分叉。该值仍是 Chuang 自己的打包预算，不代表 GPT-5.5 模型最大窗口。
- 验证通过：`cargo fmt --all --check`、`git diff --check`、`cargo run --quiet -- status --config config.toml --json` 确认 `context_max_tokens=272000 / context_budget_max_tokens=272000`、`cargo test -q --test runtime_config_tests --test agent_runtime_tests --test cli_status_tests`。

## 2026-05-10 recent conversation history 注入
- 修复“同一会话两句话之间不连续”的根因：原来 Chuang 主要靠当前输入去语义检索历史 `turn_summary`，遇到“继续 / 刚才那个 / 他说的对吗”这类短承接句时没有关键词，召回容易为空。现在 `RunCliRequest` 支持显式 `conversation_history`，`app-server` 会在同一 thread 的下一轮自动注入最近 6 轮 user/assistant 原文对话，形成 `[recent-conversation-history]` Working context 段，不再只依赖 memory search。
- 该 history 是短期会话上下文，不写核心记忆；每条文本有单条截断，仍走 ContextEngine 预算、排序和 trace。Feishu 走 app-server，因此普通飞书会话会自然继承这条链路；CLI 仍默认空 history，除非调用方显式传入。
- 补充运行观测字段：`recent_conversation_history_item_count`、`recent_conversation_history_turn_count`、`recent_conversation_history_injected`、`recent_conversation_history_dropped`、`recent_conversation_history_model_facing` 会进入 provider meta / runtime observability。新增 app-server 级回归：同一进程连续两次 `turn/start`，第二轮必须看到第一轮 user/assistant 历史已注入，避免飞书侧短追问再次表现成“失忆”。
- 对齐 Codex 式上下文分层的第一步收口：ContextEngine 增加精确重复内容去重阶段，只对 Identity / Memory / Goal 这类稳定上下文生效，保留更高优先级或更新的段，低优先级重复段以 `duplicate_content` 进入 drop reasons 和 `dedupe` trace；ToolResult / Working 不去重，避免误删真实多次工具证据或用户重复表达。
- 继续修正工具循环收口：`run_governed_turn_with_tools` 现在保留最后一段可读的 plain text 作为兜底最终答复，若模型在工具轮里没写 `FINAL:` 但最后确实给了自然语言收尾，也不会再直接抛 `tool_loop_exhausted`；该回退会写入 `tool_loop_status=implicit_final_plain_text`，避免飞书侧因为严格协议而出现假失败。
- 桥侧再补一层自愈：`scripts/chuang-feishu-bridge.js` 的 `AppServerClient` 在 app-server 退出后会把 child 清空，并在下一次请求前自动重启 app-server，避免桥还在但本体已死的假死状态。
- 验证通过：`cargo fmt --all --check`、`git diff --check`、`cargo test -q --lib`、`cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config -- --nocapture`、`cargo test -q --test cli_status_tests --test agent_runtime_tests --test runtime_config_tests`、`cargo test -q --bin chuang-agent run_with_options_injects_recent_conversation_history`、`cargo test -q --test app_server_tests app_server_second_turn_injects_recent_thread_history`、`cargo test -q --test runtime_report_tests`。

## 2026-05-09 skill 生命周期 monitor / rollback 收口
- `src/cli_skill.rs` 现在把 `skill monitor` 和 `skill rollback` 接到现有 proposal -> validation -> solidify -> retire 线：`monitor` 只读盘点 active / deprecated / retired 技能，输出 decay 候选和 rollback 候选；`rollback` 以保留的 `Previous Version Snapshot` 恢复为新的 active 版本，仍不删除文件。`skill solidify` 和 `skill retire` 也都开始保留快照块，方便后续恢复。回归补了 `tests/cli_skill_tests.rs` 的监控与回滚覆盖，并通过 `cargo test -q --test cli_skill_tests`、`cargo fmt --all --check`、`git diff --check`。


## 2026-05-09 memory maintenance archive/decay 边界结构化
- `memory maintenance report/apply` 现在固定输出 `boundary` 对象，把 archive、maintenance runtime、decay review 和唯一允许的 `experiences.md` 写回路径拆开：历史 `turn_summary` archive 只读、maintenance 先 dry-run 后显式 apply、decay 只做人工 review 且不是写回候选、核心 `MEMORY.md` / `USER.md` 不允许被维护命令自动重写。
- `memory_recall` 注入的 context segment 和 agent input 现在带 `memory_layer` / `memory_boundary` / `archive_read_only` / `maintenance_writeback_allowed` / `decay_review_only` / `writeback_target`，避免运行时把 recall 证据误当成可写维护任务。通用 `memory_store` 补了轻量层级分类和边界对象，回归覆盖 archive 与 decay 口径。

## 2026-05-09 tool surface 只读桌面/浏览器观察显式化
- `src/atomic_tool.rs` 的工具指令块把 `screenshot / locate` 进一步写成“桌面/浏览器只读观察工具”，明确说明它们只用于取证、不执行点击或输入，并把“当前屏幕、窗口标题、浏览器页面内容”这类请求的默认取证路径说得更直接。
- `src/tool_runtime.rs` 的 `ToolSurfaceStatus` 新增 `desktop_browser_read_only_atomic_tools`，当前固定暴露 `["screenshot", "locate"]`，让上游可以不靠推断就区分只读观察和交互工具；回归补了 `tests/tool_runtime_tests.rs` 的指令块与 surface JSON 断言。

## 2026-05-09 desktop/browser 只读观察提示补强
- 为避免模型在“看当前屏幕/窗口标题/页面内容”这类请求上直接退回能力说明，`assets/capability_primer.txt` 与 `src/atomic_tool.rs` 的工具指令块都补了明确提示：`locate` / `screenshot` 是只读观察工具，遇到桌面/浏览器当前状态查询时先取证，不要直接答“无法读取”。`config.toml` 已从安全示例 actuator 切到 `scripts/chuang-real-actuator-adapter.py --json --allowlist ./config/actuator-allowlist.example.json`，该 adapter 的 `observe` 现在能用 `xdotool` 读取当前窗口标题，`screenshot` 能用本机截图工具把证据落到 `/tmp/chuang-actuator-evidence`；点击/输入仍默认关闭，`CHUANG_REAL_ACTUATOR_ENABLE` 仍只控制更高风险 live 动作，不用于放开只读观察。回归补了 `tests/atomic_tool_tests.rs`、`tests/tool_runtime_tests.rs`、`tests/cli_status_tests.rs` 和 `tests/actuator_tests.rs`。
- 同轮再补一层确定性兜底：`src/cli_runtime.rs` 在检测到“桌面/屏幕/窗口/浏览器只读观察”请求时，会在模型回合前先自动执行一次 `locate target=screen`，把观察结果注入首轮输入，避免模型继续吐“当前会话未提供工具能力”之类的纯能力说明；回归锁住自动观察后的 `tool_call_count=1`、`tool_protocol_error_count=1` 和 `tool_trace`/`tool_calls_json` 证据。
- 会话上下文继续细化：`src/cli_runtime.rs` 现在会把 `workspace_root` 写入 runtime metadata，`src/chuang_kernel.rs` 会把 `session_id` / `workspace_root` / `memory_scope` 生成短版 `[session-context]` 段并和 identity 段一起进入 prompt；这段不再靠后续 metadata 猜测，优先级也高于普通记忆，目的是让 `/new` 后的新会话仍能稳定看到当前聊天、工作区和接续语义。回归补了会话上下文注入测试，并通过 fmt / tool / cli 回归。

## 2026-05-09 live operator checklist 默认 provider env 自动吸收
- `scripts/chuang-live-operator-checklist.sh` 现在会在 `CHUANG_PROVIDER_ENV_FILE` 未显式设置时自动尝试标准默认路径 `~/.config/chuang-agent/provider.env`，并把该路径作为只读证据继续输出到 `paths.provider_env_file`、`suggested_provider_env_file`、`provider_env_next_step` 和 `provider_required CHUANG_PROVIDER_ENV_FILE=<set>/<missing>`，同时仍只做脱敏检查、不连接真实 provider、不打印 secret。默认文件存在时 checklist 不再把 provider env 视为缺失；默认文件不存在时仍保留原 blocker。回归已补：`cargo test -q --test cli_smoke_tests live_operator_checklist -- --nocapture`、`cargo fmt --all --check`、`git diff --check`。

## 2026-05-09 补充 checkpoint
- 2026-05-09 单 worker live rehearsal 证据补齐：在 `CHUANG_CODEX_RUNNER_ENABLE=1` 且 `CHUANG_CODEX_RUNNER_WORKSPACE=/home/user/projects/chuang-agent` 的前提下，`cargo run --quiet -- subagent live-preflight --runner-command scripts/chuang-codex-runner.py --allow-runner-command scripts/chuang-codex-runner.py --requires-capability rust --capability rust --json` 返回 `ready_for_live=true`；随后用临时 queue root 执行单次 `subagent dispatch` + `subagent run-once --runner command --runner-command scripts/chuang-codex-runner.py --approve-exec --capability rust`，拿到 `ReportAdmission.status=Accepted/reason_code=report_validated`，并由 `subagent report` / `subagent collect` 复读出同一 report。runner stdout 成功返回简短 `ok`，`report.status=Success`，`runtime_report_id` 仍只在主线 turn 里可见，不把 live rehearsal 解释成真实 worker 池 ready。
- 2026-05-09 provider live 证据补齐：在 `source /home/user/.config/chuang-agent/provider.env` 之后，`cargo run --quiet -- run --config config.toml --input "只回复一个字：好。"` 成功拿到真实 provider 响应，输出 `provider=local-openai-compatible`、`transport=openai-compatible`、`transport_mode=native`、`status_code=200`、`runtime_report_id=report-turn-1`，且 `api_key` 只以长度信息出现在 trace 中。随后同一环境下 `cargo run --quiet -- status --json` 也显示 `provider_readiness.overall_state=ready`、`api_key_state=<set>`、`transport=native`。这条证据把 provider 从本地只读 readiness 推进到可实际发起 live 请求，但仍不把 Feishu/provider live 与桌面/browser/wiki/GBrain 真实验收混写成同一层。
- 2026-05-09 live operator checklist shell 兼容性修复：`scripts/chuang-live-operator-checklist.sh` 顶部从 `set -euo pipefail` 收敛为 POSIX 可用的 `set -eu`，因此既可直接执行也可由 `sh scripts/chuang-live-operator-checklist.sh --json` 调用，不再在只读人工 live 前置检查阶段卡在 shell 方言差异上；脚本仍只输出脱敏状态，不连接真实 Feishu/provider，也不做写操作。
- 2026-05-09 Feishu `/tools` 与 runtime/status 能力 primer 单源化：新增 `assets/capability_primer.txt` 作为唯一能力 primer 文本源，Rust `capability_primer` 通过 `include_str!` 使用同一文本，Feishu bridge `/tools` / `/capabilities` 通过 Node 读取同一文件并已补 command smoke 断言，避免普通 turn、status/health 和桥命令能力描述再次漂移。`chuang-feishu-bot.service` 已重启，`ExecStartPre` 通过，node bridge 与 app-server 已常驻运行，本地解析确认 `/tools` 包含共享 primer。
- 2026-05-09 默认能力 primer 状态面外露：新增共享 `capability_primer` 模块，`AgentRuntime` 默认上下文、`status` / `doctor` / `console snapshot` / `app-server health` 现在使用同一份 `runtime_capability_primer` 文案，飞书侧或调度侧不需要先发 `/tools` 也能从 status/health JSON 读到 `file_read/file_write/code_execute/list_dir`、memory/session、goal/subagent 和 live runner `preflight/rehearsal` 边界。已补 kernel/status/app-server 断言，并通过 runtime、kernel、status、doctor、console、app-server 定向回归。
- 2026-05-09 普通 turn 默认能力 primer 下沉：`AgentRuntime` 现在会在每轮默认上下文中注入独立的高优先级 capability primer 段，稳定展示 governed file tools、`code_execute`、`list_dir`、memory/session、goal/subagent 派活入口，以及 live runner 仍是 `preflight-only / rehearsal-only` 的边界；同时把 system 段 token 口径改成按实际内容估算，避免普通回合必须靠 `/tools` 才知道自己可用能力。本轮已补 `agent_runtime` / `chuang_kernel` / `sqlite runtime` 回归，验证 primer 在预算压力下仍可见，且普通 memory 段会按预算正常被挤掉。
- 2026-05-09 provider readiness check 口径再收口：`scripts/chuang-provider-readiness-check.sh` 现在会在存在时自动吸收标准 `CHUANG_PROVIDER_ENV_FILE`，仍只做只读 `status --json` 评估、不连接真实 provider、不打印 secret；`scripts/chuang-candidate-verify.sh` 因此在本机标准 provider env 文件可用时不再把“未手动 export”误报成 blocker，候选门禁更贴近真实操作路径。
- 2026-05-09 并行子代理 A 状态面覆盖收口：`app-server health --json` 现在显式输出 `subagent_live_worker` status-only 摘要，和已有文本面保持同源；`status` / `doctor` / `console snapshot` / `app-server health` 的 JSON 与文本回归都锁定 `available=false`、`starts_worker=false` 和 no live worker reason，防止 status-only 配置被误读成真实 worker 可用。本轮只改状态输出和测试，不启动 worker、不连接外部服务、不碰 Hermes/Feishu。
- 2026-05-09 文档与进度审计口径收口：`docs/subagent-runner-protocol.md`、`docs/third-test-candidate.md`、`docs/acceptance-next-matrix.md` 和 `docs/live-operator-test-runbook.md` 已统一状态词：`ga_local_mapped_only` 只代表 GA 9 tools 本地映射与命令面可见，`desktop_browser_live_gated` 代表真实桌面/浏览器动作仍在 live gate 与 receipt 之后，`live_worker_available=false` 是当前 preflight/rehearsal 合同，`real_external_acceptance_pending` 代表 Feishu/provider/desktop/browser/wiki/GBrain 真实验收仍未完成。本轮只改文档，不改核心代码，不连接真实服务，不声明 live 完成。
- 2026-05-09 并行子代理 D 验收矩阵补强：新增 `scripts/chuang-live-gaps-check.sh`，只读 `status --json` 和 `subagent live-preflight --json`，输出 `local_contract=ready / preflight=ready_but_no_start / real_live=pending` 三段矩阵与 `live_gaps_check_ok` marker；`chuang-candidate-verify.sh` 和 `chuang-third-test-smoke.sh` 已接入该检查，用于防止 candidate/third-test 把本地合同或 ready-but-no-start 预检误写成真实 live。该入口会主动关闭 live gates，不启动 worker，不连接真实 Feishu/provider，不打印 secret。
- 2026-05-09 并行子代理 E live worker/readiness 小步收口：`subagent live-preflight` 现在显式输出 `live_worker_available=false`、`worker_runtime_state`、`worker_runtime_reason` 和 `adapter_entrypoint`；当 runner command、allowlist、capability route、ReportAdmission 与 audit prerequisites 都满足但 `CHUANG_CODEX_RUNNER_ENABLE` 未开启时，状态固定为 `configured_but_gate_disabled`，说明下一步应包住 command-runner adapter，而不是把 preflight 误报成真实 worker 可用。本轮未启动真实外部 worker、不改 GA tool 文件。
- 2026-05-09 GA 原子工具 9/9 mapped 本地闭环：`mouse` / `keyboard` / `screenshot` / `locate` 从 interface-only 状态推进为 actuator port 映射，`wait` 映射到 bounded runtime sleep，`human_suspend` 新增结构化 `human_input_required` runtime 调用；`status` / `doctor` / `app-server` / `console` 的 atomic tool 口径同步为 `mapped_count=9`、`interface_only_count=0`，MVP 和 complete-local smoke 已锁住该状态。该推进只代表本地 runtime/actuator port 可路由，不等价于真实桌面、浏览器或外部服务 live 已验收；真实动作仍要求 live gate、allowlist、治理和审计回执。
- 2026-05-09 Worker/status docs 对齐 skill 生命周期新口径：`local_contract_readiness` 仍保持 6 个 ready 合同，但 skill 线已切到 `skill_proposal_review` 的自评分审阅、canonical identity、重复合并证据，以及 `skill_lifecycle_write_retire` 的 policy self-approval、canonical `data/skills` upsert、deprecated/retired 标记和 no-delete 历史保留。该状态不连接外部服务、不写核心记忆、不执行插件；受控 skill 生命周期会写 repo skill 文件，因此单项合同显式 `writes_repo_files=true`。
- 2026-05-09 并行推进 tools/live runner/status 面：Feishu `/tools` / `/capabilities` 静态能力面已补齐主链工具能力、goal/subagent 派活入口和 live runner `preflight-only / rehearsal-only` 边界；`status` / `doctor` / `app-server health --diagnostic` 的 `atomic_tools` 现在显式拆出 `governed_executable_atomic_tool_names=file_read,file_write,code_execute`、desktop/browser interface-only 原因和本地自检入口；`scripts/chuang-live-runner-rehearsal-smoke.sh` 在 disabled Codex runner 执行后会复查 `subagent list`，确认同一 run id 的 `required_capabilities`、claim 和 `has_report=true` 仍可见。本轮仍不启用真实 runner 池、不连接外部平台、不碰 Hermes/Codex bridge、不打印 secret。
- 2026-05-09 并行任务 D 文档入口补齐：`docs/live-operator-test-runbook.md` 和 `docs/multi-worker-orchestration.md` 已补“今晚可操作入口”，把 Feishu `/tools` / `/capabilities` 看到能力之后的本地接续路径写清楚：优先跑 `sh scripts/chuang-goal-mode-smoke.sh` 和 `sh scripts/chuang-live-runner-rehearsal-smoke.sh` 收 `goal_mode_smoke_ok` / `live_runner_rehearsal_smoke_ok`；如只想停在 collect 前人工判断，则用临时 `GOAL_ROOT` / `QUEUE_ROOT` 手工跑 `goal plan -> goal dispatch -> goal step -> goal collect`，不写 checkpoint。live-preflight-only 继续要求 `ready_for_live=false`、`starts_external_worker=false`，不启动真实 runner、不接外部服务、不碰 Feishu bridge JS。
- 2026-05-09 session memory 超限先 compact：`remember_session_turn()` 现在在本轮 turn summary 超过 `DEFAULT_MEMORY_WRITE_MAX_CHARS=2200` 时，会先只压缩本轮 session summary 并重新 admission，成功后写入 `session_memory_write_status=compacted`、`session_memory_summary_kind=compacted_turn_summary`、`session_memory_compacted_from_chars` / `session_memory_compacted_to_chars`；compact 后仍超限才保留 `hard_limit_exceeded` 告警降级。显式 `--remember` / 全局 `remember_turn()` 仍保持硬失败，不吞错、不写入。Feishu footer 对 compacted 显示为“会话记忆 已压缩写入”，对 hard limit 显示为“会话记忆 超限未写入”。验证通过 kernel、runtime report、app-server、CLI hard-limit 和 Feishu summary 定向测试，`cargo fmt --all --check`、`git diff --check` 通过。
- 2026-05-09 session memory 硬限故障降级：`run_with_options()` 现在不会因为 `remember_session_turn()` 的 `memory_write_hard_limit_exceeded` 直接让整轮 turn 失败；它会保留正常 turn 返回，把 `session_memory_write_status=hard_limit_exceeded` 和 `session_memory_write_error` 写入 runtime meta，并由 Feishu bridge 在状态尾巴里显式提示。`--remember` 的显式内存写回仍保持硬失败语义。验证通过 `cargo test -q --test app_server_tests`、`cargo test -q --test cli_smoke_tests cli_run_reports_memory_write_hard_limit_clearly`、`cargo fmt --all --check` 和 `git diff --check`。
- 2026-05-09 live operator checklist 再收口：`scripts/chuang-live-operator-checklist.sh` 现在把 `/tools` / `/capabilities`、provider readiness check、`local_readonly_evidence` 和更细的只读边界一起输出；`tests/cli_smoke_tests.rs` 已锁住这些字段和 `--help`/JSON 输出。运行时实际检查显示 checklist 会在缺 provider env 时按候选现场状态报 `blocked`，但仍保持脱敏、只读和不连真实 provider。验证通过 `bash -n scripts/chuang-live-operator-checklist.sh`、`bash scripts/chuang-live-operator-checklist.sh --help`、`cargo test -q --test cli_smoke_tests live_operator_checklist_reports_redacted_manual_live_steps`、`cargo fmt --all --check`、`git diff --check`。
- 2026-05-09 live operator 文档口径收口：`docs/live-operator-test-runbook.md` 已同步当前状态，明确 Chuang 专用 Feishu bridge 已可联系、老爸已能在 Feishu 联系上 Chuang；后续 live runbook 不再把“桥是否挂上”列为未完成项，而是优先收集 `/health`、`/session`、`/tools`/`/capabilities`、provider `<set>/<missing>` 和 live receipt 证据。provider readiness 继续只读 `status --json` / health 证据，不发真实 provider 请求；`candidate-verify` 与 `live-readonly-preflight` 只把 provider readiness 纳入脱敏门禁，不等价于真实模型调用成功。
- 2026-05-09 多子代理并行推进后集成收口：Feishu bridge 新增 `/tools` / `/capabilities` 本地命令，用于在飞书里直接展示已挂载的会话、健康、live-check、图片 OCR、普通文本转 app-server 等能力和不复用 Hermes/Codex、不打印 secret 的边界；goal/subagent 只读证据补上 `goal_operability_*` 的 report/completed worker/validation note 字段和 `subagent list` 文本面的 queue/claim/report/capability 可见性；provider/live readiness 把 `candidate-verify` 和 `live-readonly-preflight` 的 provider readiness check 纳入显式证据链；本地生成的 `__IDENTITY__/` 运行时空目录加入 `.gitignore`，不删除现有文件。集成验收通过 `node --check scripts/chuang-feishu-bridge-commands.js && node scripts/chuang-feishu-command-smoke.js`、`python3 -m py_compile scripts/chuang-real-control-adapter.py scripts/chuang-real-actuator-adapter.py`、`bash -n scripts/chuang-provider-readiness-check.sh scripts/chuang-candidate-verify.sh scripts/chuang-live-readonly-preflight.sh`、相关 Rust 定向测试、`git diff --check` 和 `bash scripts/chuang-candidate-verify.sh`；final verify 仍保持 clean-tree 语义，需提交后复跑。
- 2026-05-09 control / actuator dry-run 证据再加厚：`chuang-real-control-adapter.py` 现在把 `required_env=CHUANG_REAL_CONTROL_ENABLE` 显式挂到 list metadata，并在 receipt message 里统一输出 `allowed=true / dry_run=<bool> / live_enabled=<bool> / audit_label=control.apply.live / required_env=CHUANG_REAL_CONTROL_ENABLE`；`chuang-real-actuator-adapter.py` 的 message 也统一补出 `allowed=true / dry_run=<bool> / real_execution=<bool> / audit_label=actuator.operation.live / required_env=CHUANG_REAL_ACTUATOR_ENABLE`。对应 CLI / adapter 回归已补，live gate 关闭时的可见证据不再只剩“dry-run”字样。本轮未执行真实系统控制或桌面操作。
- 2026-05-09 provider/candidate 验收入口补强：新增 `scripts/chuang-provider-readiness-check.sh`，只读 `status --json` 并脱敏输出 provider kind、transport、timeout、`api_key_state=<set>/<missing>`、current/next action，缺 env 时给出 `provider_api_key_env_missing` 但不连接真实 provider；新增 `scripts/chuang-candidate-verify.sh`，不要求 clean tree，串起 complete-local smoke、live runner rehearsal smoke 和 provider readiness check，provider 非 live 阻断只作为候选证据继续本地门禁，最终 marker 为 `chuang_candidate_verify_ok`。
- 2026-05-09 live runner / actuator 本地可验收边界推进：新增 `scripts/chuang-live-runner-rehearsal-smoke.sh`，在 live gate 关闭时验证 `subagent live-preflight` 不启动 worker、runner command allowlist/capability route 可见、disabled Codex runner 仍产出标准 `SubagentReport` 且 `ReportAdmission=Accepted/report_validated`；同时 real actuator adapter 的 dry-run message 带出 `real_execution=false`、`audit_label=actuator.operation.live` 和 `required_env=CHUANG_REAL_ACTUATOR_ENABLE`，对应回归已补。本轮未执行真实 runner、真实桌面操作或真实服务控制。
- 2026-05-09 real control adapter live-gate 边界补强：`chuang-real-control-adapter.py` 的 list metadata 现在显式暴露 `dry_run/live_enabled/audit_label/allowed_actions`，apply receipt message 也带 dry-run/live gate/audit label；`cli_control_tests` 新增直接脚本回归，锁住未设置 `CHUANG_REAL_CONTROL_ENABLE=1` 时 allowlisted command 不执行、marker 不生成，以及 allowlisted unit 上未列动作会被拒绝。本轮不控制真实服务、不碰 Hermes/Feishu secret。
- 2026-05-09 live preflight / goal collect 边界继续加厚：`subagent live-preflight` 的 JSON/text 回归锁住 `starts_external_worker=false`、operator approval、governance approval、audit receipt 和 dispatch evidence 要求；`goal collect` 现在把 malformed report 作为 blocked evidence 暴露在 `blocked_report_run_ids/reasons`，保持 `ready_to_checkpoint=false` 并让 `checkpoint --from-collect` 拒绝，避免坏 JSON 报告中断收集或被误当 checkpoint 材料。
- 2026-05-09 `status` / `console` 顶层 `subagent_readiness` 汇总行继续对齐，把 `capability_mismatch_reason` 一并带出；对应文本回归已补，和 doctor/app-server 的同类只读面保持一致。
- 2026-05-09 `status` 只读回归再补一条 `queued_external` 断言，确认 `capability_mismatch_reason` 和 `worker_runtime_blocked_reason` / `capability_route_state` / `capability_mismatch_blocks_live` 一起锁定；这是 status 面最后一层可见性收口，不碰 Hermes/Feishu，不接真实 live runner。
- 2026-05-09 live runner / capability mismatch 读面继续收口：`status` / `doctor` / `console snapshot` / `app-server health` 现在都能看见 `worker_runtime_blocked_reason`、`capability_route_state`、`capability_mismatch_blocks_live`、`capability_mismatch_reason`，并由 `cli_*` / `app_server` / `kernel_status` 回归锁住 JSON/text 一致性；`scripts/chuang-complete-local-smoke.sh`、`cargo test -q`、`cargo fmt --all --check` 和 `git diff --check` 已通过，`scripts/chuang-final-verify.sh` 仍以 clean tree + complete-local + diff check 为最终门禁。
- 2026-05-09 goal operability 只读回归继续加厚：`goal show` 的 JSON/text 现在由测试锁定显式 `--subagent-queue-root`、pipeline state、next command/reason、collect missing/blocked evidence；新增回归确认换成不存在的 queue root 做只读查看不会创建队列目录，blocked report 也会在 `goal_operability` 里保留阻断证据并保持 checkpoint not-ready。
- 2026-05-09 smoke gate 覆盖补强：MVP 与 complete-local smoke 现在明确断言 `goal_run.plan_exists=true`，并通过 doctor 的 `goal_run_readiness` check 锁住 `goal_id=mainline-mvp` / `plan_exists=true` 详情；wrapper 静态回归同步锁住这些断言存在，避免状态面已声明 goal run readiness 但本地 smoke 未验收。本轮只改 smoke/wrapper，不接外部服务、不碰 Hermes/Feishu。
- 2026-05-09 provider diagnostics 只读复核补强：`runtimeObservability` 现在会保留顶层 `request_message_count`，与已存在的 `request_url` / `request_method` / `config_error_field` / `provider_timeout_*` 一起用于 timeout、capacity、missing-content 等 provider 排障；新增 runtime report 单元断言和 app-server timeout 端到端断言，未接外部服务、不碰 Hermes/Feishu、不暴露 secret。
- 2026-05-09 goal/subagent operator UX 文档收口：`docs/multi-worker-orchestration.md` 现在作为下一阶段 live runner preflight 的统一 runbook，集中列出 6 worker 派活线、任务卡模板、live gate / runner allowlist / `required_capabilities` / worker capability / `ReportAdmission` / governance receipt / blocked evidence 的验收字段，以及 capability mismatch 和 `goal_collect_*` 阻断证据的复派规则；`docs/goal-mode-operating-plan.md` 只保留通用 GoalRun 顺序并指向该 runbook。本轮只改文档，不碰代码、Hermes/Feishu 或 secret。

## 2026-05-08 补充 checkpoint
- 2026-05-08 继续补强 goal-mode 的可操作性状态面：`goal show` 现在会带出只读 operability 摘要，直接显示下一步该跑 `goal dispatch` / `goal step` / `goal checkpoint --from-collect` / `goal show` 中的哪一个，并把 dispatch/step/collect/checkpoint 的准备状态和 collect 阻断证据同步落到文本和 JSON 面；对应回归已加，complete-local smoke 和全量测试已通过。
- 2026-05-08 验收门禁覆盖扫描补强：发现 `provider_readiness` 和 subagent live-worker 口径已经进入 status/doctor/app-server/console 状态面，但 MVP/complete-local smoke 之前只覆盖旧的 provider config 与 live adapter 片段；代码和测试面已补上 provider readiness、stub provider 边界、`live_worker_available=false`、`worker_runtime_state=local_contract_only` 的 JSON 门禁和静态 smoke wrapper 回归。不接外部服务、不碰 Hermes/Feishu。
- 2026-05-08 提交前风险审计：检查当前 dirty diff 的删除/cleanup/reset 命令、Hermes/Feishu bridge 误触、secret 泄露、硬编码本地-only secret/path、日期口径和测试绕过风险；本轮未发现新增删除/清理命令、secret 泄露或 Hermes/Feishu 误触，最终日期口径已统一。
- 2026-05-08 goal/subagent operator UX 文档补强：`docs/goal-mode-operating-plan.md` 和 `docs/multi-worker-orchestration.md` 新增 6 worker 派发 runbook，固定 `goal plan -> show -> dispatch -> step -> collect -> checkpoint --from-collect -> show` 命令顺序，并列出 `goal_dispatch_ready/count/manifest_path`、`goal_step_checkpoint_recorded=false`、`goal_collect_ready_to_checkpoint`、`missing_run_ids`、`blocked_report_run_ids`、`blocked_report_reasons`、checkpoint source 和 checkpoint log 完整性等验收字段；失败处理明确先看 blocked evidence，不手工绕过 collect。
- 2026-05-08 app-server health live-runner readiness 口径对齐：文本面补齐 `live_worker_available`、`worker_runtime_state`、`subagent_worker_runtime_reason` 和逐层 `subagent_layer` 输出；JSON 回归同步锁定顶层与 layer 的 live worker 字段，继续只做只读状态面，不启动真实 runner、不接外部服务。
- 2026-05-08 app-server health 本地可用性输出补强：文本面补齐 `subagent_readiness_local_contract_reason` / `subagent_readiness_live_adapter_reason`，并逐条输出 `live_adapter_gate` 的 env、audit、preflight、must_reject、reason 和 next action；对应 app-server health JSON/text 回归已加厚，锁定 goal_mode、provider_readiness、subagent_readiness、live_adapter_gates 的可见性一致性，不接外部服务。
- 2026-05-08 文档/交接面补齐下一阶段派活清单：基于当前 goal-mode 正负门禁已到位、live runner 仍未启用的状态，`docs/multi-worker-orchestration.md` 新增“可派活缺口清单”和 worker 任务包模板，明确下一阶段最大缺口是真实 worker/live adapter 启用前的 allowlist、capability route、治理审批、ReportAdmission 和状态面一致性；首个低风险任务建议只做 live-runner readiness 只读状态面对齐，不碰 Hermes/Feishu、不接真实 worker、不删除文件。
- 2026-05-08 多代理继续推进 provider 可观测性：新增 `provider_readiness` 状态面，进入 status/doctor/console/app-server health，显式展示 provider kind、transport、fallback configured/policy、request timeout、api key state 和 provider placeholder warning count；其中 warning count 只统计 provider 相关 warning，避免 actuator/subagent placeholder 误伤 provider readiness。
- 2026-05-08 provider 错误诊断继续加厚：429 plain-text `at capacity` 现在也会稳定归类为 `provider_failure_reason_code=model_capacity` / `provider_failure_category=capacity`，并在 app-server `providerMeta` 与 `runtimeObservability` 里端到端可见。
- 2026-05-08 provider timeout 口径收紧：http/native/curl 超时都会补 `provider_timeout_reason_code=request_timeout` / `provider_timeout_category=timeout`，并透传到 runtimeObservability；HTTP read timeout 现在显式标为 `http_timeout`，避免被普通 transport failure 淹没。
- 2026-05-08 fallback 诊断元数据补强：fallback 输出会把 primary 的 `config_error_field`、`provider_timeout_ms`、`provider_error_message`、`request_url` 等排障字段复制到 `provider_fallback_primary_*`，让 app-server turn 里能直接看出 primary 为什么失败。
- 2026-05-08 多代理继续推进本地 95% 可用性：`local_contract_readiness` 新增 `goal_mode_smoke_gate`，把正向/负向 goal-mode smoke 纳入 status/doctor/console/app-server health 的本地合同面；合同数更新为 6，仍保持不连接外部服务、不写核心记忆、不执行插件。
- 2026-05-08 app-server health 继续补可观测性：JSON 现在包含 `live_adapter_gates`，文本面新增 `subagent_readiness` 和 `live_adapter_gates` 摘要，避免只跑 health 时看不到 runner/live gate 是否仍 disabled/deferred。
- 2026-05-08 provider missing-content 端到端口径收紧：OpenAI-compatible 200 响应但缺 assistant content 时不再沿用 provider 的 `finish_reason=stop`，改为 `provider-error-missing-content`；app-server turn/event 状态会基于 `provider_response_ok=false` 显示 `provider_error`，runtimeObservability 同步暴露 missing-content failure code/category。
- 2026-05-08 live subagent preflight 继续收紧 capability route：即使 live gate 已开启、runner 命中 allowlist、worker 自报 capability，如果 dispatch 没有声明 `required_capabilities` 也不会 `ready_for_live`，避免无路由能力的真实 runner 被误放行。
- 2026-05-08 本轮继续推进 goal-mode 本地可用性：新增 `scripts/chuang-goal-mode-negative-smoke.sh` 和 `tests/goal_mode_negative_smoke_tests.rs`，覆盖 `goal step --max-runs 1` 后 collect 仍 not-ready、`goal checkpoint --from-collect` 必须拒绝、未完成 worker 不会落 checkpoint 的负例闭环；`scripts/chuang-complete-local-smoke.sh` 现在同时调用正向和负向 goal-mode smoke。
- 2026-05-08 增量修正 goal-mode 状态：`goal plan -> goal dispatch -> goal step -> goal collect -> goal checkpoint --from-collect -> goal show` 的 happy path 已进入 `scripts/chuang-goal-mode-smoke.sh`，并由 `scripts/chuang-complete-local-smoke.sh` 直接调用；not-ready 负例门禁也已进入 `scripts/chuang-goal-mode-negative-smoke.sh`，继续锁定 `goal checkpoint --from-collect` 拒绝、failed / identity-mismatched report 只进 `blocked_report_*` 证据、`goal step` 保持 manifest allowlist 与显式 `max-runs` / `max-concurrency` 约束。
- 2026-05-08 新增独立 `goal-mode` smoke 入口 `scripts/chuang-goal-mode-smoke.sh`，覆盖 `goal plan -> goal dispatch -> goal step -> goal collect -> goal checkpoint --from-collect -> goal show` 的本地闭环；对应回归在 `tests/goal_mode_smoke_tests.rs`，并且脚本保持前台、临时目录、无 Feishu/Hermes、无隐式 checkpoint 之外写入。
- 2026-05-08 继续补 goal collect 的文本面：现在也会输出 `blocked_report_run_ids` / `blocked_report_reasons`，避免 failed / identity-mismatch 报告在手工检查时只剩 JSON 可看；对应回归已补。
- 2026-05-08 继续把 goal 闭环落到可复跑 smoke：`scripts/chuang-complete-local-smoke.sh` 现在直接调用 `scripts/chuang-goal-mode-smoke.sh`；这样这条 goal 流程就不只是单测存在，而是也进入了本地验收门禁。
- 2026-05-08 文档边界补齐 `goal step`：它应被描述为前台、bounded、goal-scoped 的 `subagent run-loop` 包装，只在一个显式 goal 范围内按 `max-runs` / `max-concurrency` 执行一批已派发 worker；它不是 daemon，不自动 checkpoint，不写 progress-log / handoff / memory，不做 cleanup/delete/release，不连接 Feishu/Hermes。`goal collect` 和 `goal checkpoint` 仍是后续显式人工/主控步骤。
- 2026-05-08 继续把 goal 计划推进到“派活后可收证据、再显式接 checkpoint”的层级：`goal dispatch` 现在会把已存的 `GoalRun.worker_plan` 扇出为多条 queued subagent dispatch，写入 `subagent-queue/dispatch/*.json`，并在 goal root 下落本地 dispatch manifest；新增只读 `goal collect`，会按 manifest 聚合队列 report，输出 available/missing run ids、completed workers、report summaries 和 `ready_to_checkpoint`。`collect` 只收证据，不自动写 checkpoint、progress-log 或 handoff；当前回归已补 collect->checkpoint 的手动接力 happy path 和 not-ready 拒绝路径。
- 2026-05-08 下一阶段清单：goal-mode 正向/负向本地门禁已经补齐；接下来优先收紧 worker runner/status/doctor/console 的一致口径，以及 live adapter 启用前的 allowlist、治理审批和审计回执边界。
- 2026-05-08 继续把 goal budget 从文字变成硬约束：`GoalSpec` 默认并发子任务预算从 `0` 调整为 `4`，`goal plan` 也新增 `--max-subtasks N`，`GoalRun` 会拒绝超过预算的 worker plan；这样 goal 才能真的用于组织多个子代理并行。对应回归已补。
- 2026-05-08 继续把 app-server health 的文本口径抬齐：`app-server health` 现在也补出 `goal_run_readiness` 摘要，和 `status` / `doctor` 一样把 `goal_run` 的 checkpoint 续接证据直接落到只读文本面；JSON 结构不变，回归已补。
- 2026-05-08 继续把 goal mode 的状态面抬高：`status` / `doctor` / `console snapshot` / `app-server health` 现在都会显式暴露 `checkpoint_policy` 和 `final_report_policy`，并在文本面带出最新 checkpoint 的 `created_at`、完成 worker 和 validation notes；这轮 `goal checkpoint` 还补了结构化 `checkpoint_writeback` 回执/诊断字段，把 `docs/progress-log.md` 和 `docs/handoff-current.md` 的手动回写提示单独亮出来，继续保持不自动执行。对应 JSON/text 回归已补。
- 2026-05-08 继续把 app-server health 的只读面和 skill solidify 的语义一起收口：`app-server health` 现在也显式输出 `goal_mode` / `goal_run` 及其 checkpoint 续接摘要，`skill solidify` 的默认 approval source 也从 approve 语义分离出来，避免文本面误导；对应 JSON/text 回归已补。`goal checkpoint` 的 progress-log / handoff 回写现在也能通过本地诊断字段直接看到，不再只靠口头约定。
- 2026-05-08 继续把只读状态口径拉齐：`status` / `console snapshot` 现在都显式输出 `goal_mode` / `goal_run` 及其 checkpoint 续接摘要，把 console 和 status/doctor 的目标面保持一致；对应 text/JSON 回归已补。下一步再看是否需要把这组 goal 摘要进一步并入更高层的 app-server/health 只读面。
- 2026-05-08 继续收口 `skill approve` / `skill solidify` 的本地 receipt 语义：approve 现在输出 `approval_receipt`，solidify 输出 `solidify_receipt`，默认来源也拆成各自的 `cli skill ...` 本地标签；当时两条命令仍然只生成 local-only receipt，不写 `data/skills`，不连外部服务。新增了 solidify 默认本地回执回归，下一步只看是否还需要把这条语义并入更高层只读状态面。
- 2026-05-08 继续把 skill 线推进到显式固化边界：`skill solidify` 当时也有本地只读命令面，能复用已验证提案生成拒绝型固化回执，但仍然不写 `data/skills`、不接外部服务；`skill approve` / `skill propose` / `skill solidify` 的票据链路已经贯通到 CLI。下一步是看是否需要把这条边界再接入更高层状态面。
- 2026-05-08 继续把本地 readiness 面抬高：`local_contract_readiness` 新增 `skill_approval_flow`，把 `skill approve` 的本地审批票据和结构化回执显式纳入 status/doctor/console 的只读合同面；对应文本/JSON 回归已同步到 5 个本地合同。下一步等 skill 核心 approval helper 收口后，再把 approval ticket 和后续固化边界接起来。
- 2026-05-08 继续把 skill review 往“可批准回执”推进：`skill approve` 已补本地只读审批入口，复用 dry-run proposal/validation 生成 `approved=true` 的 `SkillSolidifyTicket` 回执，但当时仍不改 `solidify` 写入路径、不写 `data/skills`、不接外部服务；`skill propose` 也继续保留 pending approval ticket 输出。下一步再看是否要把 approved ticket 接到后续固化边界。
- 2026-05-08 继续把 subagent readiness 往前推一档：`status`/`doctor` 的 `subagent_readiness` 新增 `live_runner_rehearsal` 层，明确本地只读 rehearsal contract 已 ready，但真实 worker 仍 deferred；同时 `CommandControlPlane` 和 `CommandActuator` 的 command-arg parser 改为对未闭合引号和尾随转义直接报错，避免静默归一化。验收通过 `cargo test -q --test kernel_status_tests --test cli_status_tests --test control_plane_tests --test actuator_tests`。
- 2026-05-08 Worker 1 继续把 skill proposal review 的审批边界协议补显式：新增本地可序列化 `SkillApprovalReceipt` / `SkillSolidifyTicket`，`skill propose` JSON/text 现在会随 dry-run proposal 和 validation report 一起输出 pending approval ticket，标明 `approved=false`、`approval_source=pending_operator_approval`、`writes_skills=false`、`solidifies_skill=false`、`local_only=true`。本轮仍不新增 `approve` 子命令、不改 `solidify` 写入路径、不写 `data/skills`、不接外部服务。
- 2026-05-08 转向非 Feishu 主线，继续把 skill proposal 从“只生成候选”推进到“可审阅候选”：`skill propose` 现在会对每个 dry-run proposal 立即执行 `SkillEvolver::validate()`，JSON 输出 `validation_count / validation_accepted_count / proposal_validations[]`，文本输出同步显示 validation accepted 和 reasons；当时仍保持 `writes_skills=false`、不 solidify、不写 `data/skills`、不接 LLM。`local_contract_readiness.skill_proposal_review` 文案同步为 provenance + validation report。验收通过 `cargo fmt --all`、`cargo test -q --test cli_skill_tests`、`cargo test -q --test kernel_status_tests`、`cargo test -q --test cli_status_tests`、`cargo test -q`、`git diff --check`。
- 2026-05-07 继续把 CLI 外脑知识上下文计数提升到共享 runtime observability：`runtime_observability_meta()` 现在会把 `knowledge_context_preview_count / injected_count / dropped_count / dropped_segment_ids` 以及只读边界字段纳入 allowlist；默认未启用 preview 时也稳定输出短名 count=0 和 dropped ids `[]`，因此 app-server `runtimeObservability` 和 channel `runtime_observability` 会自然继承该结构化观测面。本轮不新增 app-server/channel 的 knowledge public params，避免扩大协议面。新增 runtime report / app-server / channel 回归锁定提升字段，验收通过 `cargo fmt --all`、`cargo test -q --test runtime_report_tests runtime_report_observability_meta_promotes_goal_session_tool_provider_fields`、`cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config`、`cargo test -q --test cli_channel_tests cli_channel_simulate_runs_workspace_config_without_fake_responder`。
- 2026-05-07 继续收紧 CLI 外脑知识上下文回执：`run` 的知识 preview metadata 不再只看 preview 是否非空，而是按最终 `dropped_segment_ids` 回填 `knowledge_context_preview_count / knowledge_context_injected_count / knowledge_context_dropped_count / knowledge_context_dropped_segment_ids`，兼容保留旧键但不再误判预算裁剪后的“已注入”状态；新增回归锁定 preview 存在但实际被预算丢弃时，CLI meta 和 packed context preview 都能一致暴露 dropped 信息。验收通过 `cargo fmt --all`、`cargo test -q cli_runtime::tests::run_with_options_can_inject_readonly_knowledge_context_when_enabled -- --exact`、`cargo test -q cli_runtime::tests::run_with_options_reports_knowledge_context_drops_under_tight_budget -- --exact`、`cargo test -q`、`git diff --check`。
- 2026-05-07 继续把 memory maintenance 的 approved writeback 回执面接到只读状态层：`status` / `console snapshot` 现在都会从 `experiences.md` 摘要最近一次 `memory maintenance apply` 回执，输出 candidate/source_record_id/approved_at/note/provenance_preserved 等字段；空回执会明确显示 `state=missing`，但仍不改变写回语义。新增回归锁定 JSON/text 两条路径，验收通过 `cargo fmt --all`、`cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_console_tests`、`cargo test -q`、`git diff --check`。
- 2026-05-07 继续补 Feishu 图片 OCR 的可用性：`chuang-feishu-bridge` 现在会自动探测本机 tesseract 已安装语言，并按候选顺序尝试 `chi_sim+eng` / `chi_sim` / `chi_tra+eng` / `chi_tra` / `eng`；也支持 `CHUANG_FEISHU_OCR_LANGS` 显式覆盖候选顺序。当前机器只装了 `eng` 和 `osd`，所以本地行为不变，但以后装中文包就能直接吃到。
- 新回归已补 `scripts/chuang-feishu-image-smoke.js`，锁定候选顺序和显式 override；桥本身仍然只做下载 + OCR + 文本上下文注入，不碰真实多模态模型接入。
- 2026-05-07 继续把 Feishu 图片消息接入主链：bridge 现在会下载 `image_key`、落本地临时文件、跑 OCR，再把图片上下文作为文本块送进 app-server，不再只提示“暂不支持图片”。帮助命令也同步说明图片会先下载并 OCR。
- 2026-05-07 继续把 Feishu 普通消息的收尾摘要收窄：`chuang-feishu-bridge` 现在用纯 helper 生成更短的 footer/process summary，不再把整段 trace 原文塞进飞书回复；`runtime report id` 也会在完成态 footer 里直接出现，便于人工复查和消息对齐。
- 2026-05-07 到家前继续推进本地可验收状态面：新增顶层 `local_contract_readiness`，把已完成的外脑 context preview、skill proposal dry-run、plugin registry evidence、wiki/GBrain source contract 统一暴露到 `status` / `doctor` / `console snapshot` / `app-server health`。该状态只代表本地合同 ready，显式保持 `connects_real_external_services=false`、`writes_core_memory=false`、`executes_plugins=false`，不替代人工 Feishu live 验收。
- 2026-05-07 第七批继续推进除人工 Feishu live 之外的模块：`run` 新增显式 `--enable-knowledge-context-preview --knowledge-context-root PATH --knowledge-context-query TEXT`，默认关闭，开启后只把本地外脑 preview segment 注入本轮 context，并输出 `knowledge_context_*` 边界 metadata；新增 `skill propose` 审阅入口，只生成 dry-run skill proposal，不写 skill、不接 LLM；`plugin_registry` 摘要现在把 evidence/check-only/capability 边界汇总进 `status`/`console`；`memory knowledge source-contract --source wiki|gbrain` 固化 wiki/GBrain 只读 adapter 合同。
- 本批仍不接真实 wiki/GBrain、不读取外部 secret、不写核心记忆、不 solidify skill、不执行插件；新增 smoke 覆盖外脑 context preview、source contract、runtime opt-in 和 skill proposal dry-run。主控复验已通过 `cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`，最终干净工作树复跑 `sh scripts/chuang-third-test-smoke.sh` 输出 `third_test_candidate_smoke_ok`；operator checklist 仍按预期显示人工 env `blocked`。
- 2026-05-07 第六批并发 `/goal` worker 已收口，转向 Feishu live 之外的可推进模块：外脑新增 `memory knowledge preview-context` 只读 context segment preview；`skill_evolver` 新增 dry-run proposal adapter，保留 provenance 且拒绝 solidify；`plugin check` 输出 capability/boundary/evidence/readiness 证据；`status/doctor/console/app-server health` 新增独立 `third_test_candidate` 状态面，继续保持 `release_readiness.release_name=second_test_version` 不变。
- 本批仍未连接真实 wiki/GBrain、真实 Feishu、真实 runner、真实桌面或外部服务；外脑 preview 不注入 runtime、不写核心记忆，skill dry-run 不写 `data/skills`，plugin check 不执行插件、不读 secret。主控复验已通过 `cargo fmt --all --check`、`git diff --check`、专项测试、`cargo test -q`、`sh scripts/chuang-complete-local-smoke.sh`；干净工作树最终复跑 `sh scripts/chuang-third-test-smoke.sh` 通过，输出 `third_test_candidate_smoke_ok`，其中 operator checklist 仍按预期显示人工 env `blocked`。
- 2026-05-07 第五批并发 `/goal` worker 已收口：`7e493db docs: add third test candidate guide` 增加第三测试候选一页入口，`a982e32 feat(smoke): add third test candidate wrapper` 新增 `scripts/chuang-third-test-smoke.sh`，`48f2062 feat(feishu): add live receipt bridge command` 给 Chuang Feishu 本地命令增加 `/receipt` / `/live-receipt` 静态回执入口。
- 主控集成复验通过：`bash -n scripts/chuang-third-test-smoke.sh`、`node --check scripts/chuang-feishu-bridge-commands.js && node scripts/chuang-feishu-command-smoke.js`、`cargo fmt --all --check`、`cargo test -q --test cli_smoke_tests`、`sh scripts/chuang-third-test-smoke.sh`，最终输出 `third_test_candidate_smoke_ok`。其中 live operator checklist 仍显示人工 env `blocked`，但第三测试候选本地门禁已通过。
- 2026-05-07 第四批并发 `/goal` worker 已收口：`d6d2735 feat(feishu): suggest provider env for live checklist` 给 live checklist 增加默认 provider env 候选诊断，`26f09c1 feat(feishu): add live operator receipt template` 新增人工 live 测试回执模板，`687170d feat(feishu): clarify live check command states` 明确 `/live-check` 的 ready/blocked/warning 和保密边界，`4fc1b76 docs: define third test acceptance path` 定义第三测试版/100% 前最后一跳验收路径。
- 主控集成时修正 worker 合并后的两个 smoke 断言回归，提交 `c06c0e1 test(smoke): restore live operator script assertions`；复验通过 `cargo test -q --test cli_smoke_tests --test live_operator_scripts_tests`、`node --check scripts/chuang-feishu-bridge-commands.js && node scripts/chuang-feishu-command-smoke.js`、`sh scripts/chuang-final-verify.sh`，最终输出 `chuang_final_verify_ok`。
- 2026-05-07 本轮继续增强 live operator checklist 的 provider env 诊断：当 `CHUANG_PROVIDER_ENV_FILE` 未设置时，`scripts/chuang-live-operator-checklist.sh --json` 现在会只读提示默认候选 `~/.config/chuang-agent/provider.env` 是否存在，并在 JSON/text 里给出 `suggested_provider_env_file` 和下一步操作，方便 operator 直接把 Chuang Feishu env 指向该路径或显式设置变量；它仍然不写 env、不打印 secret、不连 Feishu。
- 验证通过：`bash -n scripts/chuang-live-operator-checklist.sh`、`cargo test -q --test cli_smoke_tests live_operator_checklist_reports_redacted_manual_live_steps`、`cargo test -q --test cli_smoke_tests live_operator_checklist_suggests_default_provider_env_when_missing`、`git diff --check`。
- 2026-05-07 本轮继续补晚间人工 live 记录面：新增 `scripts/chuang-live-operator-receipt.sh`，只输出脱敏回执模板字段 `tested_at/operator/env_file/workspace_root/preflight_status/health_status/new_thread_status/session_status/runtime_report_id/provider_status/codex_hermes_isolation/notes/blockers/boundaries`，支持 `--json`，不连接 Feishu、不读 secret、不启动服务、不修改仓库。新增独立回归 `tests/live_operator_scripts_tests.rs` 锁定脚本仅是模板输出，并把 `docs/live-operator-test-runbook.md` 补上“测试后生成回执”入口。
- 2026-05-07 继续给 Feishu 本地命令补人工 live 回执入口：`/receipt` 和别名 `/live-receipt` 现在只返回 `scripts/chuang-live-operator-receipt.sh --json` 的静态模板说明、字段清单和保密边界，不执行脚本、不读 secret、不进 Agent 主链。新增 `scripts/chuang-feishu-command-smoke.js` 回归锁定该本地命令和 `/help` 列表同步，`docs/feishu-dedicated-channel-checklist.md` 已同步 bridge 本地命令清单。
- 2026-05-07 本轮补晚上人工 live 测试包：新增 `scripts/chuang-live-operator-checklist.sh` 和 `docs/live-operator-test-runbook.md`，只读汇总 Chuang Feishu env、workspace、provider env 状态，输出 `<set>/<missing>`、本地预检命令和人工飞书测试步骤；它不连接 Feishu、不发送消息、不启动服务、不修改仓库、不打印 secret。`docs/feishu-dedicated-channel-checklist.md` 已链接该入口，`docs/acceptance-next-matrix.md` 将 live cutover runbook 从下一步推进到进行中。
- 本轮验证通过：`bash -n scripts/chuang-live-operator-checklist.sh`、默认本机 checklist JSON 脱敏输出检查、`cargo test -q --test cli_smoke_tests live_operator_checklist_reports_redacted_manual_live_steps`、`cargo test -q --test cli_smoke_tests`、`git diff --check`。默认本机检查当前显示 `blocked` 且 `CODEX_PPTOKEN_API_KEY=<missing>`，这是晚间人工 live 前要确认的 operator env 状态，不是本地合同回归。
- Feishu bridge 本地命令新增 `/live-check` / `/live`：只显示人工 live 测试步骤和本地预检命令，不进入 Agent 主链、不执行 checklist、不读取密钥、不连接外部服务。验证通过 `node --check scripts/chuang-feishu-bridge-commands.js`、`node scripts/chuang-feishu-command-smoke.js`、`cargo test -q --test cli_smoke_tests`、`git diff --check`。
- 2026-05-07 第三批并发 `/goal` worker 已收口：`62ca560 docs: refresh acceptance matrix after readiness evidence` 刷新下一阶段矩阵，`0035d40 feat(goal): write overnight runner heartbeat` 让 `run-chuang-goal-overnight.sh` 每轮写 `status.json`/heartbeat，`ee1c9a4 feat(goal): add readonly run status script` 新增 `scripts/chuang-goal-run-status.sh` 只读聚合 watchdog 与 overnight run 状态。它解决“夜里停了以后无法快速判断停在哪”的可观测性缺口，但不自动重启、不清理日志、不派活、不改 repo。
- 第三批主控复验已通过：`bash -n scripts/run-chuang-goal-overnight.sh scripts/chuang-goal-run-status.sh`、`cargo test -q --test cli_smoke_tests`、overnight dry-run + `chuang-goal-run-status.sh --json` 手动串联、`sh scripts/chuang-complete-local-smoke.sh`、`sh scripts/chuang-final-verify.sh`，最终输出 `chuang_final_verify_ok`。
- 2026-05-07 第二批并发 `/goal` worker 已收口并通过最终验证：`37a7e38 feat(memory): add knowledge search provenance evidence`、`ad7b00a feat(subagent): expand live preflight evidence`、`57ef912 feat(console): diagnose watchdog report freshness`、`9efbb9e feat(feishu): expand live preflight evidence`。本批把外脑检索 provenance、subagent live-preflight 证据、watchdog report freshness/missing/invalid 诊断、Feishu live preflight 只读 evidence 链补厚，但仍不连接真实 Feishu、不启动真实 runner、不控制服务。
- 本批主控复验已通过：`cargo test -q --test cli_console_tests --test memory_maintenance_cli_tests --test cli_subagent_live_preflight_tests`、`node --check scripts/chuang-feishu-live-preflight.js && node scripts/chuang-feishu-live-preflight-smoke.js && node scripts/chuang-feishu-command-smoke.js`、`sh scripts/chuang-complete-local-smoke.sh`、`sh scripts/chuang-final-verify.sh`，最终输出 `chuang_final_verify_ok`。
- 第二批后追加 live-readiness 主入口复验：`cargo fmt --all --check`、`bash -n scripts/chuang-live-readonly-preflight.sh scripts/chuang-live-readiness-preflight.sh`、`sh scripts/chuang-live-readonly-preflight.sh` 均通过，最终输出 `live_readiness_preflight_ok`。
- 2026-05-07 最终本地验收已在干净工作树上通过：`sh scripts/chuang-final-verify.sh` 先确认 clean worktree，再跑 complete-local smoke 和最终 `git diff --check`，输出 `chuang_final_verify_ok`。这确认当前第二测试版本地闭环、watchdog 只读接管面、console/app-server health 诊断和 Feishu 本地命令 smoke 已经一起可复验。
- 2026-05-07 并发 `/goal` 子代理推进后补一条主控验收约束：关键最终验证前必须先确认所有会写生产文件的子代理已经停写或已提交，再跑最终专项栈、complete-local smoke 和 `git diff --check`；否则主控验证可能短暂读到半写入工作树，造成一次性误报失败。
- 本轮并发收口已落两条最新提交：`7a0a134 fix(provider): tighten fallback diagnostics` 修正 provider `status_code=200` 但缺 assistant content 时的 `missing_content/response` 归类，并提升 `provider_response_ok`；`3f74e77 feat(subagent): tighten live preflight gate` 补齐 subagent live-preflight 稳定 gate 字段，并锁定默认 gate 关闭时 `ready_for_live=false`。
- 2026-05-07 `console snapshot` 现在也会带上共享的 `app_server_health` 摘要，JSON/text 两条输出都能直接看到 `diagnostic_status`、`diagnostic_summary` 和 `next_actions`；它复用了 `app-server health` 的同一组 placeholder warning 逻辑，不再让主控单独跑 health 才知道 workspace 配置哪里还在用占位实现。
- 新回归覆盖 console JSON 和 text 两条路径，验证通过 `cargo test -q --test cli_console_tests --test app_server_tests`、`sh scripts/chuang-complete-local-smoke.sh`、`git diff --check`。
- 2026-05-07 App-server `health` 现在会在 JSON/text 里一起输出 `diagnostic_status`、`diagnostic_summary` 和 `next_actions`，直接把运行态里的 provider env 缺失和 placeholder warnings 翻成可执行的排障提示；诊断模式仍然不失败，只是不再只给原始 config 块。
- 这条诊断面复用了 runtime config summary 的 placeholder warnings 和 api_key 状态，新增回归已覆盖正常 workspace health 和 `--diagnostic` 缺 env 两条路径；验证通过 `cargo test -q --test app_server_tests`、`sh scripts/chuang-complete-local-smoke.sh`、`git diff --check`。
- 2026-05-07 本轮再补一个只读 live readiness preflight 总入口 `scripts/chuang-live-readonly-preflight.sh`：它先跑 watchdog `--once` 只读快照，再串起临时 stub config 的 `status/doctor/app-server health/console snapshot` 诊断，最后再过一遍 complete-local smoke，最终输出 `live_readiness_preflight_ok`。
- 这个 preflight 仍然只用临时目录和 stub provider，执行前会 unset live adapter gate env；它不连接真实 Feishu、不读取真实 secret、不控制真实服务。
- 2026-05-07 Worker J 本轮补真实外部 subagent runner 启用前 rehearsal：新增只读 `subagent live-preflight`，检查 `CHUANG_CODEX_RUNNER_ENABLE` live gate、runner command 显式 allowlist、required/worker capability routing、ReportAdmission 证据，以及 unscoped external worker pool、直接写核心记忆、登录态/session mutation 等 forbidden capability 是否仍拒绝。
- 该 rehearsal 不 claim dispatch、不启动 runner、不写 report、不触碰服务；`ok=true` 表示只读合同检查通过，`ready_for_live=true` 还要求 live gate 已由操作员显式开启。
- 2026-05-07 Worker F 本轮把 terminal watchdog 只读状态接入 `console snapshot`：console 现在默认读取 `/home/user/.codex/chuang-goal-interactive/latest-watchdog-report.json`，JSON 输出 `terminal_watchdog` 摘要，文本输出显示 available/readonly/session/tmux/codex process/git dirty/next_action。
- 该接入只读已有 report，不执行 `chuang-goal-watchdog.sh`、不派活、不重启、不修改仓库、不触碰服务；测试通过 `CHUANG_GOAL_WATCHDOG_REPORT_FILE` 指向临时 report 覆盖，真实路径仍按 SOP 默认读取。
- 2026-05-07 Worker G 本轮新增完整本地可用闭环验收入口 `scripts/chuang-complete-local-smoke.sh`：它串起第二测试 smoke、watchdog `--once` 只读报告、临时 stub config 下的 `status/doctor/app-server health/console snapshot` 诊断读面，以及 Feishu 本地 command/session/rich message smoke，最终输出 `complete_local_smoke_ok`。
- 该 wrapper 明确保持 local-only：使用临时目录和 stub provider，主动 unset live adapter gate env，不连接真实 Feishu、不读取真实 secret、不控制真实服务；新增 `tests/cli_smoke_tests.rs` 合同测试锁定复用安全 smoke、watchdog 一次性模式和稳定 marker。
- 2026-05-07 Worker D 本轮补 Chuang 专用 Feishu 通道真实使用面：bridge 新增 `/session` 与 `/health`/`/status` 本地命令，分别查看 chat->thread 绑定和 bridge/app-server/workspace/env/provider-env 诊断；命令只读本地状态，不连接真实飞书、不打印 secret。
- 本轮继续补强 `/session` 和 `/health` 的可读诊断：`/session` 现在明确显示当前飞书聊天是否已绑定，`/health` 在未绑定时显示默认 thread、在已绑定时显示 chat binding，并把 provider env 拆成 `CHUANG_PROVIDER_ENV_FILE=<set|missing>` 与 `CODEX_PPTOKEN_API_KEY=<set|missing>`；command event log 也会带 thread id，便于排查真实聊天路由。
- 本轮 Feishu/channel 验证已通过 `node --check scripts/chuang-feishu-bridge.js`、`node scripts/chuang-feishu-command-smoke.js`、`node scripts/chuang-feishu-session-smoke.js`、`node scripts/chuang-feishu-rich-message-smoke.js`、`cargo test -q --test cli_channel_tests`、`git diff --check`。
- `/new` 的 `thread/start` 失败和普通消息 `turn/start` 失败现在会回复脱敏错误卡片，给出失败阶段和下一步 `/health`/`/new` 建议，避免真实飞书用户只看到无响应；富消息卡片补 Feishu message id 字段，用于和 runtime report id、bridge 事件日志对齐。
- `channel feishu-check` 新增 `diagnostic_status`、`diagnostic_summary`、`next_actions`，本地 preflight 能结构化提示缺失 Chuang env、误用 legacy/Codex/Hermes env、workspace/config/mode 问题；验证通过 `node scripts/chuang-feishu-command-smoke.js`、`node scripts/chuang-feishu-rich-message-smoke.js`、`node --check scripts/chuang-feishu-bridge.js`、`cargo test -q --test cli_channel_tests`、`cargo fmt --all --check`、`git diff --check`。
- 2026-05-07 Worker B 继续推进 memory maintenance 人工批准写回闭环：`memory maintenance apply` 现在会输出结构化 `approval` 回执和 `selected_candidates`，`apply --dry-run` 仍只预览不写，真实写回仍必须显式 `--approve-writeback`，且 `writes_automatically=false` 保持不变。
- 经批准写入 `experiences.md` 的 LIM 候选现在会附带 `writeback=memory_maintenance_apply`、批准来源、批准时间、可选 `approval_note` 和 `provenance_preserved=true`，并保留原始 `source=lim_dry_run / source_record_id / created_at / lesson` provenance；decay 候选仍不是写回候选。
- 2026-05-07 Worker A 本轮补 provider/model 满载与 fallback 诊断边界：OpenAI-compatible provider 失败会输出稳定 `provider_failure_reason_code` / `provider_failure_category`，`Selected model is at capacity` 归类为 `model_capacity/capacity`；无 fallback 配置时显式标记 `provider_fallback_configured=false`、`provider_fallback_used=false`，避免 silent fallback。
- 显式 fallback 使用时，fallback 响应会保留 primary 的 retryable、status/error_class 以及 failure reason/category；runtime observability 已提升这些字段，新增 `docs/provider-fallback-diagnostics.md` 记录边界和字段。
- 2026-05-07 Worker C 本轮补齐 live adapter 启用前审计面：`LiveAdapterGate` 现在区分 `env_value_state`，并为 subagent runner、control apply、actuator operation 固定输出 `preflight_checks`、`must_reject_capabilities` 和 `next_action`。这只是 preflight/audit/diagnostic，不打开真实桌面、真实服务控制或真实外部 worker。
- `status` / `doctor` 文本和 JSON 会直接暴露每个 live gate 的 env/gate 状态、审计标签、仍必须拒绝的能力和下一步动作；即使 env 被设为 `1`，仍必须走 allowlist、治理审批和审计 receipt，且任意服务控制、Codex/Hermes 控制、删除、直接写核心记忆、登录态/验证码等能力仍拒绝。
- 2026-05-07 本轮把终端 worker watchdog 从纯文本日志推进到只读结构化状态快照：`scripts/chuang-goal-watchdog.sh` 现在每轮写 `latest-watchdog-report.json`，同时维护 pane/process/git 的最新文件路径与摘要，报告里给出 `takeover.next_action`，方便主控判断 attach、review diff 或继续观察。
- 新增 watchdog `--once` / `WATCHDOG_ONCE=1` 一次性模式，专门用于人工接管检查、smoke 和未来只读控制台读取；报告明确声明 `readonly=true`，并把 `dispatches_tasks / modifies_repo / restarts_worker / touches_services` 全部标为 `false`，没有增加飞书命令层、常驻服务、自动修复或自动重启。
- 已补 `tests/cli_smoke_tests.rs` 回归锁定一次性 watchdog 会生成有效 JSON 状态报告，并验证只读边界字段；已通过 `bash -n scripts/chuang-goal-watchdog.sh`、手动 `--once` JSON 解析、`cargo fmt --all --check`、`cargo test -q --test cli_smoke_tests`、`git diff --check`、`sh scripts/chuang-second-test-smoke.sh`。
- 2026-05-07 本轮确认昨天已经写过终端长跑脚本：`scripts/start-codex-goal-terminal.sh`、`scripts/run-chuang-goal-overnight.sh`、`scripts/chuang-goal-watchdog.sh`。补充 `docs/terminal-goal-watchdog-sop.md`，把已跑通方案收口为“真实终端 Codex worker + watchdog 可视化日志”的最小 SOP，并明确它不是 Chuang 内部子代理自动执行、不是飞书命令层、不是常驻服务。
- 2026-05-07 本轮按老爸选择推进后续清单第 3/4 项：新增 `src/live_adapter_gate.rs`，把 subagent runner、control apply、actuator operation 的 live adapter 启用门禁收成统一结构，默认全部 disabled，只认精确 env 值 `CHUANG_CODEX_RUNNER_ENABLE=1`、`CHUANG_REAL_CONTROL_ENABLE=1`、`CHUANG_REAL_ACTUATOR_ENABLE=1`，并暴露 audit label。
- `status` / `doctor` 现在新增 `live_adapter_gates` JSON 与文本输出，明确 gate_count、enabled/disabled 数量、required_env、audit_label 和 next_action；这只增加启用前门禁和诊断面，没有打开真实桌面、控制面或 live worker。
- ContextEngine 第一版继续收口：`PackedContext` 现在记录 `normalize_tokens -> trim -> rank -> reserve_working -> merge_under_budget` 的 `trace`，并由 `PackedContext::render_prompt()` 统一渲染 prompt 诊断，runtime 不再保留私有重复渲染逻辑。
- 已补 `tests/live_adapter_gate_tests.rs`、`context_engine_tests`、`kernel_status_tests`、`cli_status_tests`、`cli_doctor_tests` 回归；专项验证已通过 `cargo test -q --test live_adapter_gate_tests --test context_engine_tests` 与 `cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests`。
- 2026-05-06 本轮把 Chuang Feishu `\/new` 从静态说明升级为真会话切换：bridge 现在会为当前 Feishu chat 创建新的 app-server thread，并把 chat->thread 绑定写到本地 session state 文件，后续同一 chat 的普通消息会路由到这个新 thread，不再沿用旧上下文。
- 已新增 `scripts/chuang-feishu-session-store.js` 与 `scripts/chuang-feishu-session-smoke.js`，并把 session smoke 纳入 `scripts/chuang-mvp-smoke.sh`；验证通过 `node scripts/chuang-feishu-command-smoke.js`、`node scripts/chuang-feishu-session-smoke.js`、`cargo test -q --test cli_status_tests --test cli_doctor_tests --test kernel_status_tests`、`cargo test -q`、`sh scripts/chuang-second-test-smoke.sh`。
- 2026-05-06 本轮继续推进 subagent readiness 可诊断性：`SubagentReadinessStatus` 和 `SubagentLayerStatus` 新增 `local_contract_reason` / `live_adapter_reason`，`status` / `doctor` 文本输出也会直接展示本地合同已 ready 与 live adapter 尚未接入的结构化原因，避免 UI 或值守脚本只能解析状态字符串。
- 已补 `kernel_status_tests`、`cli_status_tests`、`cli_doctor_tests` 回归锁定这些原因字段；验证通过 `cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests`，随后已跑 `cargo fmt --all`。
- 已重新确认本轮门禁：`cargo fmt --all --check`、`cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests`、`sh scripts/chuang-second-test-smoke.sh` 均通过，second-test smoke 输出 `second_test_smoke_ok`。
- 老爸看到的 `Selected model is at capacity` 属于上游模型/provider 满载，不是本地 Rust 测试或第二测试 smoke 失败；后续可考虑给真实对话入口配置备用模型或 fallback provider。
- 2026-05-06 Worker B 推进 memory maintenance dry-run 闭环：`memory maintenance report` 支持多次 `--query` 批量 batches，输出 `explicit_writeback_required=true`、LIM 候选和 decay review candidates；decay 只作为人工审查建议，不允许写回。
- `memory maintenance apply --dry-run` 已补为只预检/选择 LIM 候选、不写 `experiences.md`；真实写回仍必须显式 `--approve-writeback`，且 decay 候选会稳定拒绝为 `memory_maintenance_apply_candidate_not_writeback_candidate`。新增 `tests/memory_maintenance_cli_tests.rs`，验证通过 `cargo test -q --test memory_maintenance_cli_tests`、`cargo test -q --test cli_identity_memory_tests`、`cargo test -q --test memory_policy_tests --test memory_recall_tests --test memory_store_tests --test memory_store_sqlite_tests --test memory_admission_tests`。
- 2026-05-06 Worker C 本轮只推进 control/actuator/Genesis 边界：`control_plane` 新增显式 `ControlCommandContract` / allowlisted action 校验 / reusable receipt 校验，`actuator` 新增 command contract / action allowlist / audit label 校验，`GenesisCommandSpec` 暴露稳定 `audit_label()` 并在 AutoCLI runner 入口校验 program/timeout。
- 已补 `tests/control_actuator_contract_tests.rs` 覆盖 control allowlist 拒绝、receipt 模型不匹配、actuator action allowlist/audit label、Genesis userDataDir/CDP 审计标签；相关验证通过 `cargo test -q --test control_actuator_contract_tests`、`cargo test -q --test control_plane_tests --test control_workflow_tests --test control_intent_tests --test control_surface_tests --test cli_control_tests --test control_actuator_contract_tests`、`cargo test -q --test actuator_tests --test genesis_actuator_tests`。
- 本轮没有接真实桌面/浏览器执行，没有改飞书/Hermes，也没有把 fake 标成真实控制；`cargo fmt --all --check` 仍被既有 `tests/memory_maintenance_cli_tests.rs` 格式漂移挡住，Worker C 未触碰该文件。
- 2026-05-06 本轮把 `workspace adapter` / `tool_runtime` / `app_server` 的路径归一化收成共享模块 `src/path_utils.rs`，同一套 lexical-normalize / symlink-parent resolve 现在由三处共用，减少路径边界分叉。
- 已补回归覆盖共享路径解析后的 `write_file` / `apply_patch Add File` / `execute_tool_call(WriteFile ...)` 三条路径；验证通过 `cargo fmt --all --check`、`git diff --check`、`cargo test -q --test tool_runtime_tests workspace_file_adapter`、`cargo test -q --test tool_runtime_tests symlink_parent`。
- 2026-05-06 本轮把 workspace path 逃逸防护收成单一共享模块 `src/path_utils.rs`，`tool_runtime` 与 `workspace_file_adapter` 现在共用同一套“canonicalize 已存在父路径 + 拼回缺失尾部”的解析逻辑，避免以后两处路径边界分叉。
- 已补回归覆盖共享路径解析后的 `execute_tool_call(WriteFile { path: "linked/created.txt" })`、`write_file("linked/created.txt")`、`apply_patch Add File: linked/created.txt` 三条路径；验证通过 `cargo fmt --all --check`、`git diff --check`、`cargo test -q --test tool_runtime_tests workspace_file_adapter`、`cargo test -q --test tool_runtime_tests symlink_parent`。
- 2026-05-06 本轮把 `tool_runtime` 的路径解析和 `workspace_file_adapter` 对齐：不存在的目标文件也会先 canonicalize 最近已存在的父路径，再拼回缺失尾部，避免 `execute_tool_call` / `write_file` / `apply_patch Add File` 经 workspace 内 symlink 父目录写到工作区外。
- 已补 Unix 回归覆盖 `execute_tool_call(WriteFile { path: "linked/created.txt" })`：当 `linked` 指向外部目录时会返回 `path_outside_workspace`，外部文件不会被创建；局部验证通过 `cargo fmt --all --check`、`git diff --check`、`cargo test -q --test tool_runtime_tests workspace_file_adapter`、`cargo test -q --test tool_runtime_tests symlink_parent`。
- 2026-05-06 本轮继续收紧 workspace adapter 路径边界：不存在的目标文件现在会先 canonicalize 最近已存在的父路径，再拼回缺失尾部，避免 `write_file` / `apply_patch Add File` 通过 workspace 内 symlink 父目录写到工作区外。
- 已补 Unix 回归：`write_file("linked/created.txt")` 和 `apply_patch Add File: linked/created.txt` 在 `linked` 指向外部目录时都会返回 `path_outside_workspace`，外部文件不会被创建；局部验证通过 `cargo fmt --all --check`、`git diff --check`、`cargo test -q --test tool_runtime_tests workspace_file_adapter`。
- 2026-05-06 本轮把 `WorkspaceFileAdapter::apply_patch()` 改成两阶段执行：先解析、路径校验、delete/move 拒绝、hunk 校验，全部通过后才创建目录、备份和写文件，避免 patch 前半段已落盘、后半段失败导致部分写入。
- 已补回归：混合 `Add File` + `Delete File` 的 patch 会因 `apply_patch_delete_not_allowed` 整体拒绝，新增文件不会被创建，既有文件保持原样。
- 2026-05-06 本轮继续补 workspace adapter 安全边界：`WorkspaceFileAdapter::apply_patch()` 现在拒绝 `*** Delete File` 和 `*** Move to` 这类会删除源文件的 patch 语义，返回稳定 `apply_patch_delete_not_allowed` / `apply_patch_move_not_allowed`，符合“删除必须显式批准”的项目规则。
- 已补回归锁定拒绝后文件仍保留、move 目标不会被创建；验证通过 `cargo test -q --test tool_runtime_tests workspace_file_adapter`、`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-second-test-smoke.sh`。
- `repl` 默认输出现在只保留模型正文，调试诊断字段改为 `--verbose` 才显示；对应的 REPL smoke / transport tests 已回归，避免日常对话刷出一长串运行报告。
- 最新 `goal checkpoint` 已写入 `checkpoint-1777990193168985465`。
- `goal show` 文本输出现在也补齐了 checkpoint 续接诊断：`goal_checkpoint_log_complete`、`goal_last_checkpoint`、`goal_last_summary`、`goal_incomplete_reasons`，并新增文本回归覆盖。
- 最新 `goal checkpoint` 已写入 `checkpoint-1777988338607449831`。
- `status` / `doctor` 文本输出现在会固定打印 `goal_run_checkpoint_log_complete`、`goal_run_last_checkpoint`、`goal_run_last_checkpoint_summary` 和 `goal_run_incomplete_reasons`，把 checkpoint 续接诊断从结构体字段真正落到可读 CLI 输出；相关 JSON 字段已同步回归覆盖。
- 最新 `goal checkpoint` 已写入 `checkpoint-1777988021122441409`。
- 本轮从 Feishu 细节切回 Chuang 主线：`GoalCheckpoint` 新写入现在带 RFC3339 `created_at`，旧 checkpoint 缺该字段仍兼容读取；带字段但非法/空时间戳会在严格写入和 persisted load 时拒绝。
- `goal_run` readiness 现在透出 `checkpoint_log_complete`、`last_checkpoint_summary` 和 `incomplete_reasons`；旧弱 checkpoint 仍可加载，但缺完成者/验证证据会显式显示为不完整，不再静默看起来完全可续接。
- 子代理文件队列已加 `run_id` 路径安全约束：dispatch/report/claim/release 等文件入口只接受 ASCII 字母、数字、`-`、`_`，CLI `subagent report --run-id ../escape` 会拒绝，不会越出 queue root。
- `subagent report/collect` 现在先读取 raw report 并生成 `ReportAdmission`：坏 JSON、缺字段等 report 文件会稳定返回 `Rejected` 和 `reason_code`，不再直接把 CLI 命令打成 Decode 失败；完整 report 仍保留 collect 身份校验。
- `subagent_readiness` 已拆出 `local_contract_ready` / `local_contract_state` 与 `live_adapter_ready` / `live_adapter_state`：第二测试版可以明确表达本地队列、runner、multi-worker、external-AI downstream 合同可验收，但真实外部 worker/live adapter 仍未接入。
- `ReportAdmission` 新增可选 `upstream_reason_code`：command runner 协议报告被包装成 `command_protocol_report_rejected` 时，会保留底层 `missing_required_field`、`agent_id_mismatch` 等稳定原因，UI 不需要再解析 stderr 文本。
- 最新 `goal checkpoint` 已写入 `checkpoint-report-admission-upstream-reason-code-20260505`，checkpoint count 到 68。
- 最新 `goal checkpoint` 已写入 `checkpoint-subagent-readiness-contract-live-split-20260505`，checkpoint count 到 67。
- 最新 `goal checkpoint` 已写入 `checkpoint-subagent-raw-report-admission-20260505`，`goal_run_diagnostics.checkpoint_log_complete=true`，checkpoint count 到 66。
- 第二测试 smoke 已锁定 checkpoint `created_at` 和 `last_checkpoint_summary`；readiness 文档说明 read/parse failures 与结构化 incomplete reasons 是可验收诊断面。
- 本轮验证已通过：`cargo fmt --all --check`、`git diff --check`、`cargo test -q --test cli_subagent_dispatch_tests --test subagent_queue_tests`、`cargo test -q`、`sh scripts/chuang-second-test-smoke.sh`。
- Chuang 的真实 provider key 已接到仓库外私有 env：`~/.config/chuang-agent/provider.env`，只包含 `CODEX_PPTOKEN_API_KEY=<set>`，权限 `600`；项目根 `config.toml` 继续使用 `https://api.pptoken.org/v1` + `gpt-5.5` + `api_key_env`，不写明文 key。`launch-chuang-agent-repl.sh`、`chuang-feishu-bridge.sh`、`chuang-app-server-health.sh` 会自动加载该私有 env；真实 REPL 验证返回 `status_code=200`。
- 第二测试 smoke 已补临时 provider env 验收：`[smoke] repl launcher` 现在同时验证 stub REPL、`CHUANG_PROVIDER_ENV_FILE` 驱动的 `chuang-app-server-health.sh`，以及未带外部 shell env 时 REPL 可从临时 provider env 启动退出；不读取真实 key、不发真实模型请求。
- Feishu/channel/readiness 文档已同步 provider env 边界：`CHUANG_PROVIDER_ENV_FILE` 是仓库外 operator secret path，和 Feishu app credential env 分离；第二测试验收仍是不连接真实外部服务的 acceptance gate，不把本机真实 pptoken 配置误报成 smoke 的 live integration。
- Chuang Feishu bridge 富消息现在会把 app-server `turn.runtimeReportId` / `runtimeObservability.runtime_report_id` 传进卡片“报告”字段，真实飞书消息可直接关联本轮 runtime report；`node scripts/chuang-feishu-rich-message-smoke.js` 已锁定报告字段渲染。
- 本地终端对话入口已收口：`scripts/launch-chuang-agent-repl.sh` 现在默认读取项目根 `config.toml` 启动 REPL，缺 `CODEX_PPTOKEN_API_KEY` 时只提示不回落 fake；`CHUANG_REPL_STUB=1` 可显式跑 stub 链路验证。第二测试 smoke 已纳入 `[smoke] repl launcher`。
- Chuang Feishu bridge 的 `/new` 文案已收紧成“开新窗口/新上下文命令”：桥层明确不进入 Agent 主链、不消耗任务轮次，并说明飞书客户端窗口需由用户在飞书内新开；命令 smoke 和第二测试 smoke 已验证通过。
- Chuang Feishu bridge 的 `/new` 命令已抽成纯本地模块 `scripts/chuang-feishu-bridge-commands.js`，`node scripts/chuang-feishu-command-smoke.js` 不加载 Feishu SDK 也能验证；`/new` 不会转发给 app-server/runtime。最新 checkpoint 是 `checkpoint-second-smoke-new-command`。
- Chuang Feishu bridge 本地命令补上 `/help`，用于列出 `/new` 和 `/help`；普通文本仍转发到 app-server，命令 smoke 覆盖 `/new`、`/help` 和普通文本不误吞。
- 第二测试版本 smoke 已纳入 Feishu bridge 本地命令验证：`[smoke] feishu bridge commands` 会运行 `node scripts/chuang-feishu-command-smoke.js`，确保 `/new`、`/help` 不依赖 Feishu SDK、不连接飞书。
- 当前 repo-root 状态已收口：`status --json` 报 `project_readiness=ready`、`release_readiness=second_test_version_ready`、`channel_readiness=ready`、`subagent_readiness=ready`、`memory_readiness=ready`。
- 最新 `goal checkpoint` 已写入 `checkpoint-second-test-version-ready-20260505`，`goal_run_diagnostics.checkpoint_log_complete=true`。
- 子代理本地多 worker 缺口已补：`subagent run-loop --max-concurrency 1..8` 现在会启动 bounded worker batch，通过文件队列 claim/report admission 收口；`command_runner` 和 `multi_worker` readiness 在 queued_external 配置下升为 `ready`，第二测试版 `subagent_protocol_acceptance` 也升为 `ready`。
- MVP smoke 已新增并发子代理验收：连续 dispatch 两个 smoke task 后用 `run-loop --max-concurrency 2 --max-runs 2` 执行并 collect 两个 report；局部验证已通过 `cargo fmt --all --check`、`cargo test -q --test cli_subagent_dispatch_tests`、`cargo test -q --test kernel_status_tests --test cli_status_tests --test cli_doctor_tests --test cli_console_tests`。
- 完整门禁已重新通过并写入 `checkpoint-subagent-bounded-multi-worker-20260505`：`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。
- 记忆五层本地验收状态已从 partial 推到 ready：LIM 支持 provenance candidate + 显式 `maintenance apply --approve-writeback` 写入 experiences，external knowledge 支持本地只读 provenance search，maintenance loop 保持 dry-run report + 显式 apply，不连接真实 wiki/GBrain、不自动写长期记忆。
- 完整门禁已重新通过并写入 `checkpoint-memory-layered-local-ready-20260505`：`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。
- 第二测试版本当前仍保持可交付状态：`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh` 全部通过。
- 当前接续点保持不变：继续盯 `goal/run`、`subagent protocol`、`readiness`、`memory`、`channel`、`control` 的边界收口，以及真实 runner / adapter 的后置风险。
- 这轮只做了验证与接续整理，没有新增外部服务接入，也没有碰 Hermes 或真实飞书桥。
- `cargo check -q` 在独立 `CARGO_TARGET_DIR` 下通过；`cargo clippy --all-targets --all-features -- -D warnings` 仍在当前环境里报 `E0786 invalid metadata files`，更像本机 build artifact / toolchain 问题而不是代码回归。
- 最新 `goal checkpoint` 已写入记忆五层本地 ready 验证，`goal show --json` 里的 `goal_run_diagnostics.checkpoint_log_complete=true`，`last_checkpoint_id=checkpoint-memory-layered-local-ready-20260505`。
- `docs/goal-mode-operating-plan.md` 现在明确：checkpoint 最好带验证备注，否则 `goal_run` 诊断会把它视为不完整。
- 子代理报告校验现在会拒绝 `finished_at` 早于 `started_at` 的时间倒挂报告；`control_plane_tests` 里的 command adapter 用例也改成串行，避免并发干扰导致的偶发失败。
- 子代理报告时间校验再收紧一层：现在直接比较解析后的时间，避免同一字段重复解析；最新 checkpoint 是 `checkpoint-subagent-report-order-and-control-test-serial-20260505`。
- 子代理报告时间错误现在保留原始输入字符串，便于排错，同时仍只解析一次时间；最新 checkpoint 是 `checkpoint-subagent-report-time-compare-20260505`。
- control plane receipt 验证现在复用共享的 action mismatch helper，减少重复错误格式化但不改变协议行为；最新 checkpoint 是 `checkpoint-control-action-mismatch-helper-20260505`。
- `ReportAdmission` 已补回归锁定 `invalid_timestamp_order` 作为时间倒挂报告的稳定 `reason_code`，`docs/subagent-runner-protocol.md` 也列出该错误码；最新 checkpoint 是 `checkpoint-subagent-timestamp-order-admission-code-20260505`。
- `ReportAdmission` 的 accepted 路径不再在轻量合同校验后强反序列化完整 `SubagentReport`，避免合同有效但完整结构字段不足的最小 report 触发 panic；最新 checkpoint 是 `checkpoint-report-admission-no-full-deserialize-20260505`。
- `ReportAdmission` 继续补稳：`invalid_utf8` 现在也有稳定 `reason_code` 回归，`docs/subagent-runner-protocol.md` 与实现对齐；最新 checkpoint 是 `checkpoint-report-admission-invalid-utf8-code-20260505`。
- 本轮协议硬化已重新跑完整验收并写入 checkpoint：`checkpoint-report-admission-validation-pass-20260505`；验证项为 `cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。
- `ReportAdmission` 剩余 validator reject 稳定码已补回归：`unsupported_schema_version`、`empty_required_field`、`invalid_enum_format`、`invalid_timestamp_format`、`size_limit_exceeded`；协议文档同步列全示例 reason_code。最新 checkpoint 是 `checkpoint-report-admission-reason-codes-20260505`。
- command runner 协议候选判断已收紧：stdout 只要像 `SubagentReport` JSON（带 `schema_version` 和任一报告身份字段），就会进入 admission；缺 `status` 这类不完整报告现在会被拒绝并写失败审计，不再被当普通成功输出。最新 checkpoint 是 `checkpoint-command-runner-protocol-candidate-20260505`。
- 上面这两项改动之后，`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh` 又完整重跑一次并通过。
- `subagent run-loop --json` 的 `report_admissions` 现在有回归覆盖，批量执行多个 dispatch 时每条 report admission 都会保留 `Accepted/report_validated` 状态，不只覆盖 `run-once`。最新 checkpoint 是 `checkpoint-run-loop-admission-coverage-20260505`。
- 第二测试版本专用 smoke wrapper 的契约已加回归：测试同时锁定 wrapper 设置 `CHUANG_SMOKE_NAME=second_test`、复用安全 MVP smoke，以及底层 smoke 用环境名生成最终 marker；本轮已实跑 `sh scripts/chuang-second-test-smoke.sh` 并输出 `second_test_smoke_ok`。最新 checkpoint 是 `checkpoint-second-test-smoke-wrapper-20260505`。
- `docs/subagent-runner-protocol.md` 已同步 command runner 协议候选规则：带 `schema_version` 和任一报告身份字段的 JSON stdout 会进入 admission，缺字段也会拒绝，不会落到普通 stdout 成功包装。随后完整重跑 `cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh` 并通过。最新 checkpoint 是 `checkpoint-protocol-doc-full-validation-20260505`。
- `GoalRun` 多 worker 写入范围继续收紧：同一个 `write_scope_id` 现在只能归一个 worker，避免 set 诊断误把重复归属当成 scope complete；已补 `goal_run_tests` 回归并复验 `cli_goal_tests`。最新 checkpoint 是 `checkpoint-goal-run-scope-owner-20260505`。
- GoalRun 单一 scope owner 收紧后，第二测试版本完整门禁已重跑通过：`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。最新 checkpoint 是 `checkpoint-goal-run-scope-owner-full-validation-20260505`。
- `GoalRun` checkpoint 现在必须声明至少一个 `completed_worker_id`，避免“无完成者”的 checkpoint 被误当成可恢复进度；MVP smoke 的 checkpoint 验收也改为写入 `main-process` 和 validation note，并断言 `checkpoint_log_complete=true`。最新 checkpoint 是 `checkpoint-goal-checkpoint-worker-required-20260505`。
- GoalRun checkpoint 完成者校验后，完整门禁已再次通过：`cargo fmt --all --check`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`。最新 checkpoint 是 `checkpoint-goal-checkpoint-worker-required-full-validation-20260505`。
- `GoalRun` checkpoint 现在也必须带 `validation_note`，避免没有验证证据的 checkpoint 落盘；`docs/goal-mode-operating-plan.md` 和 `docs/mvp-scope.md` 已同步 CLI 用法，局部测试和 MVP smoke 通过。最新 checkpoint 是 `checkpoint-goal-checkpoint-validation-note-required-20260505`。
- checkpoint validation note 硬要求后，`cargo test -q` 已完整重跑并通过；随后补了 external-ai dispatch dry-run checkpoint，最新 id 是 `checkpoint-external-ai-dispatch-dry-run-20260505`。

## 2026-05-05

### 最新进展
- 第二测试版本 readiness 已建立顶层状态名：`status` / `doctor` / `app-server health --diagnostic` / `console snapshot` 现在将 `release_readiness.release_name` 报为 `second_test_version`，整体状态为 `second_test_version_ready`；含义是 readiness、smoke、goal/run 续接和 subagent protocol 已成为当前验收面，但不误报成真实外部服务已接通。
- `scripts/chuang-mvp-smoke.sh` 和 status/doctor/app-server/console 回归断言已同步第二测试版本状态名，继续把 smoke 作为端到端门禁。
- `status` / `doctor` / `console snapshot` / `app-server health` 的文本输出也补上 `release_acceptance` 摘要，直接显示 `connects_real_external_services=false / verifies_real_external_services=false / uses_stub_or_local_fixtures=true`，不用只看 JSON 才能确认第二测试版本没有接真实外部服务。
- 新增 `scripts/chuang-second-test-smoke.sh` 作为第二测试版本专用验收入口；它设置 `CHUANG_SMOKE_NAME=second_test` 后复用安全 `chuang-mvp-smoke.sh`，最终输出 `second_test_smoke_ok`，方便后续区分第二版验收和旧 MVP 入口。该 wrapper 不连接真实服务、不做系统控制。
- Chuang 专用 Feishu bridge 新增本地 `/new` 命令：收到 `/new` 时直接回复“如何新开飞书聊天/话题/线程或重新绑定”的说明，不转发给 app-server、不进入 Agent runtime、不消耗一轮任务；新增 `scripts/chuang-feishu-command-smoke.js` 只读验证命令解析。
- 外脑知识库层从只读 status 往前推进一格：新增 `memory knowledge search --root PATH --query TEXT [--limit N] [--json]`，只扫描本地 markdown/text 根目录并输出 `source/path/line/score/preview` provenance hit；仍显式 `dry_run=true / read_only=true / connects_real_service=false / writes_automatically=false / runtime_retrieval_wired=false`，不连接真实 wiki/GBrain、不写核心记忆、不注入 runtime。
- 本地外脑检索会跳过隐藏路径和疑似 secret/token/password/private/credential 文件；`cli_identity_memory_tests` 增加本地知识检索只读回归，MVP smoke 也加入 `memory knowledge search` 验收。
- 外脑知识库层补上只读 contract CLI：`memory knowledge status [--json]` 会输出 `external_knowledge` adapter 的当前边界，明确 `dry_run=true / read_only=true / connects_real_service=false / writes_automatically=false / runtime_retrieval_wired=false`，并列出 `wiki`、`gbrain` 仍为 `documented_only`。该入口不连接真实 wiki/GBrain、不做检索、不写核心记忆。
- 已补 `cli_identity_memory_tests` 回归，锁定外脑 contract 输出必须保持只读、非自动写回、非真实服务连接，避免外脑 readiness 被误报成已接通运行时能力。
- 子代理 command runner 报告补上 controller 侧治理证据：通过 `--approve-exec` 启动的外部 runner 会在 `SubagentReport.governance_decision` 中记录 `action_id=subagent-command-runner:<run_id>`、`decision=needs_approval`、`reason=approved_by_cli_flag: --approve-exec`；如果 worker 自己返回了治理字段则保留 worker 值。该字段只说明主控允许启动 runner，不代表 worker 内部动作自动获批。
- `docs/subagent-runner-protocol.md` 已同步 command runner 治理证据语义，`cli_subagent_dispatch_tests` 覆盖 plain wrapper report、完整 protocol report 和 protocol reject report 三条路径。
- Chuang 专用飞书预检继续补强：`channel feishu-check` 现在会输出 `env_file_is_chuang_scoped` 和 `env_file_scope_warnings`，仅按 env 文件路径判断是否像 Chuang 专用配置；明确的 `.codex-im/.env`、`codex-feishu`、`hermes-gateway`、`hermes-feishu` 路径会产生 warning 并让 `ok=false`，但不会读取服务状态、不会连接飞书、不会输出 secret 值。
- `docs/feishu-dedicated-channel-checklist.md` 已补 expected fields，`cli_channel_tests` 覆盖正常 Chuang env 和旧 `.codex-im/.env` 路径误用两条路径。
- MVP smoke 已把 Chuang 专用飞书预检纳入端到端门禁：临时生成 `chuang-feishu.env`，断言 `env_file_is_chuang_scoped=true`、workspace/config/mode 均通过、legacy 变量为空，并确认 secret 值不会出现在 JSON 输出中；该步骤仍不连接真实飞书、不修改服务。
- `memory knowledge status` 已纳入 MVP smoke 的只读验收，固定检查外脑 contract 不连接真实服务、不自动写回、不接 runtime retrieval；`memory_readiness.external_knowledge` 文案同步为“已有只读 contract CLI，但运行时检索仍未接入”。
- 阶段 checkpoint：第二测试版本主线继续保持可跑，当前新增能力集中在“只读诊断”、“记忆维护 dry-run”、“goal/run 续接诊断”和“subagent protocol acceptance”，没有接入新的真实外部服务，没有自动写长期记忆，没有动 Codex/Hermes 飞书桥。
- 记忆维护闭环补上最小 dry-run 入口：`memory maintenance report --query TEXT [--session-id ID] [--limit N] [--json]` 会复用 identity memory snapshot、session summary search 和 LIM candidate extraction，输出 `identity_health`、`lim_candidates`、`recommendations`；它显式 `dry_run=true / writes_automatically=false`，不自动写 `MEMORY.md` 或 `experiences.md`。
- MVP smoke 已增加 `memory maintenance report` 验收，确认维护报告能在临时目录里只读生成，并固定 `experiences.md` 仍只是人工确认后的写回目标。
- `app-server health` 新增只读 `--diagnostic` 模式：默认健康检查仍严格要求 provider env，诊断模式允许缺失 `api_key_env` 时继续输出 `api_key_state`、`placeholder_warnings`、`project_readiness`、`release_readiness`、channel/subagent/external-AI readiness，便于飞书桥和控制台排障，不连接 provider、不触碰真实服务。
- `scripts/chuang-mvp-smoke.sh` 已对齐当前测试版 readiness：新增 `release_readiness` 断言，固定检查测试版本 readiness 状态；同时把 `memory_readiness.external_knowledge / maintenance_loop` 和 `subagent_readiness.multi_worker / external_ai_downstream` 的 `partial` 状态纳入 smoke，避免状态面和端到端门禁漂移。
- 本轮第二测试版本门禁重新跑通：`cargo fmt --all`、`git diff --check`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh` 全部通过。smoke 最终输出 `mvp_smoke_ok`，使用临时目录和 stub provider，不连接真实飞书、不触碰真实服务、不读取真实密钥。
- 当前结论保持不变：Chuang 已达到当前测试版本 `ready_with_partial_modules` 状态，可以继续试跑主链；后续重点从“主链是否能跑”转为 adapter/plugin 边界硬化、真实 provider/工具 UX、记忆维护闭环和子代理执行层增强。

## 2026-05-04

### 最新进展
- `status` / `console snapshot` / `config show` 现在支持缺 provider env 的只读诊断模式：遇到 `api_key_env` 未设置时会继续输出状态，并在 `api_key_state` 和 `placeholder_warnings` 里点名缺失的 env；`doctor` 仍保持严格失败，继续当健康门禁。
- `status` / `doctor` 新增顶层 `release_readiness`：当时显示为第一测试版 ready-with-partial-modules，表示第一版测试交付已经到位，后续继续以 adapter/plugin 方式补边界，不再以“主链未通”为前提。
- 子代理后置边界也补成正式文档：新增 `docs/multi-worker-orchestration.md` 和 `docs/external-ai-downstream.md`，把多 worker 计划与外部智能体下游 contract 从“待补”推进到“已定义边界、仍不自动执行”。
- `status` / `doctor` 的 `subagent_readiness` 同步更新：`multi_worker` 和 `external_ai_downstream` 现在都显示为 `partial`，把子代理计划层和外部智能体下游层从 deferred 往前挪一格。
- 记忆层的后置边界补成正式文档：新增 `docs/external-knowledge-adapter.md` 和 `docs/memory-maintenance-loop.md`，把 wiki/GBrain 外脑与 dry-run 维护闭环从“待补”推进到“已定义边界、仍未接入运行时”。
- `status` / `doctor` 的 `memory_readiness` 同步更新：`lim_long_term`、`external_knowledge`、`maintenance_loop` 在本地第二测试版边界内显示为 `ready`，并继续在文本状态里带出各自路径；真实 wiki/GBrain 和自动调度仍留在 live adapter / scheduler 后续边界。
- `console snapshot` 文本摘要补上 `project_readiness / channel_readiness / subagent_readiness / external_ai_readiness`，未来桌面/服务控制台不需要只读 JSON 才能看到关键模块状态。
- Chuang 专用飞书通道预检继续收紧：`channel feishu-check` 现在会只读检查 `workspace_root_exists`、`workspace_config_exists`、`connection_mode_ok` 和 `legacy_var_names`，避免通道环境看起来有 app id/secret 但实际没绑定到可运行 workspace，或误混 Codex/Hermes 的旧变量名。
- 子代理 command runner 的受理面补齐：`subagent run-once/run-loop/report/collect` 现在会在 JSON 输出里显式带上 `ReportAdmission`，让控制面能直接看到 `Accepted / Rejected`，不必只从 report 文本倒推控制器状态。
- `docs/subagent-runner-protocol.md` 已补 `ReportAdmission` 说明，明确 worker execution status 与 controller admission status 是两层不同状态。
- 外部 AI 分身调度 SOP 已落成 `data/skills/external_agent_dispatch_sop.md`：明确 `主进程 -> 子代理 -> 外部智能体` 的二级委派链、平台选择表、任务翻译模板、质量评级、追问上限、记忆写回边界和审计禁区。它只是 Skill/contract，不接真实浏览器、不新增第十个 Slot。
- `external_ai_readiness.dispatch_sop` 已从 `deferred` 调整为 `partial`：表示调度策略骨架已存在，但统一身份引擎和真实 browser/HTTP adapter 仍未接入。
- 统一身份引擎 adapter contract 已落成 `data/skills/unified_identity_engine_adapter.md`：定义平台/任务/context/session_hint 的输入契约、结构化输出、失败类、审计边界和后续可替换实现。它仍不直接执行真实浏览器或外脑，只是 lower adapter contract。
- `external_ai_readiness.unified_identity_engine` 已从 `deferred` 调整为 `partial`：表示统一身份引擎的契约已存在，但真实登录态/session adapter 仍后置。
- `status` / `doctor` 新增 `subagent_readiness`：按 dispatch 队列、report collect、command runner、multi_worker、external AI downstream 拆分子代理状态；在 `queued_external` 配置下当前子代理层为 `ready`，默认 fake 配置仍会因队列层 deferred 保持 partial。
- `status` / `doctor` 新增 `channel_readiness`：按 `app_server / channel_simulate / dedicated_feishu_bridge / codex_hermes_isolation / rich_messages` 拆分通道状态，明确 Chuang 的飞书桥是独立 adapter，当前只检查仓库本地脚本和协议边界，不触碰 Codex/Hermes 服务。
- `status` / `doctor` 新增 `memory_readiness`：按内部记忆、历史会话、LIM 长期沉淀、外脑知识库、自动维护闭环五层给出 ready/partial/deferred/blocked，和项目级 `project_readiness` 一起把主链与记忆骨架分开诊断。
- `status` / `doctor` 新增项目级 `project_readiness` 汇总：按 `main_chain / identity / memory / context / governance / execution_tools / reporting / channel / subagent / goal / plugins / external_ai` 12 个模块给出 `ready / partial / deferred / blocked`、当前实现、下一步动作和核心边界，避免只盯零散字段而看不清整项目状态。
- `doctor` 已把 `project_readiness` 纳入健康检查：只要出现 blocked 模块就会明确失败；当前目标态为 `ready`，表示主链可跑，live adapter 继续按边界后置。
- `scripts/chuang-mvp-smoke.sh` 已增加项目级 readiness 断言，固定检查 `main_chain=ready`、`execution_tools=ready`、`channel=ready`、`external_ai=ready`，后续改 live adapter 状态必须同步解释原因。
- docs/smoke 主线验收顺序已补清楚：当前固定先读 `status --json` readiness，再跑 `doctor --json` 安全健康检查，最后由 `scripts/chuang-mvp-smoke.sh` 串起临时目录端到端冒烟；该流程不接真实飞书、不碰真实服务、不读取真实密钥。
- smoke 的 status/doctor JSON 断言已覆盖新 readiness 字段：GA mapped/interface-only 原子工具名单、identity bootstrap presence、provider request timeout、`goal_run` readiness、plugin registry 和预期 stub provider placeholder warning。
- `GoalRun` 已从纯内存结构推进到本地可恢复记录：新增 `goal plan/show/checkpoint` CLI，默认写入已忽略的 `./context/goal-runs/<goal_id>.json`，支持创建目标计划、读取当前计划、追加 checkpoint；当前仍只记录计划和续跑状态，不执行命令、不新增 core slot、不绕过治理。
- Chuang 专用 Feishu bridge 已独立以 `chuang-feishu-bot.service` 运行：user unit 指向仓库本地 `scripts/chuang-feishu-bridge.sh`，预检通过后已成功拉起长连接进程，桥日志显示 `Feishu long connection started`，不再复用 Codex 的 `codex-feishu-bot.service` 作为主入口。
- Chuang 专用 Feishu bridge 已从 Codex 桥内部依赖里剥离：`scripts/chuang-feishu-bridge.js` 改为只引用仓库本地的 `scripts/chuang-feishu-client-adapter.js`，并移除了 `.codex-im/.env` 兜底加载；桥现在只认 `CHUANG_*` 专用环境变量，文本消息仍直发 `app-server`，不再借用 Codex/Hermes 的桥脚本。
- 长期记忆内部经验层补上第一条真实写入路径：`DualFileMemoryStore` 新增 `append_experience()`，`FileDualFileMemoryStore` 会把带 `## id` 的经验条目追加到 `experiences.md`，复用 Hermes 风格硬上限 admission、重复 id 拒绝和无变更失败语义。
- CLI 新增显式经验沉淀入口：`run --remember-experience` 会把本轮 `runtime_turn` 按 provenance 写入 `experiences.md`，内容包含 `turn_id / report_id / agent_id / governance / user / summary / lesson`；普通运行不自动写，避免主进程乱写长期记忆。
- `memory identity append-experience --id ID --content TEXT` 已补手动入口，用于人工或上层治理确认后写入经验层；`run` 完成后会输出 `experience_memory_recorded: ID`，方便通道和报告关联。
- 已补回归：Hermes 双文件 store 可追加带来源经验；CLI 可手动追加 experience；`run_with_options()` 可通过 `--remember-experience` 生成带 provenance 的经验条目。
- 历史会话层补上只读 `session_search` 入口：`memory session search --query TEXT [--session-id ID] [--limit N] [--json]` 直接复用现有 SQLite `turn_summary` 记忆，默认按 `kind=turn_summary` 检索，传 `--session-id` 时额外按 `memory_scope=session,session_id=...` 隔离过滤，不新增存储、不写入、不删除。
- LIM 长期沉淀层补上 dry-run 候选入口：`memory lim extract --query TEXT [--session-id ID] [--limit N] [--json]` 从历史 `turn_summary` 生成 `experiences` 候选，输出 `candidate_id / source_record_id / confidence / proposed_scope / content / metadata`，只读不写回，为后续人工确认和自动维护闭环预留。

## 2026-05-03

### 最新进展
- 真实子代理 command runner 的协议报告入口再收紧：stdout 返回完整 `SubagentReport` JSON 时，现在先跑 `SubagentReportValidator` required-field / status / timestamp 校验，再做 task/agent/parent 身份校验；缺 `truncated` 等必填字段会写 Failed report，不会被当成功报告吸收。
- `SubagentReportValidator` 改为按 JSON 结构读取字段，接受标准 RFC3339 秒级或毫秒级时间，避免外部 runner pretty JSON 或无小数秒时间被字符串匹配误伤。
- 长期记忆方案已修正为五层组合系统：新增 `docs/memory-architecture-layering.md`，明确内部记忆、历史会话、LIM 长期沉淀、wiki/GBrain 外脑知识库和自动维护闭环；以后不能再把记忆简化成三层，也不能漏掉 wiki/知识库。
- 已记录迁移顺序：先身份与内部记忆，再历史会话召回，再 LIM extractor/provenance，之后接 wiki/GBrain 外脑，最后做 health/decay/evolver/extractor dry-run 的自动维护闭环。
- 飞书架构终稿 `M21pw0qGki7emUkdmsUcdnfEnag` 已更新：在 `3.1 Codex → 单Agent任务闭环` 下补入 `2026-05-03 修正：Codex Rust 优先移植原则`，明确“少造轮子，多复制成熟实现”，本地执行、安全边界、审批、沙箱、验证、回传、goal-style 长任务和子代理组织方式优先审计/移植 Codex Rust。
- 本地 handoff/goal 记录已同步：后续 Chuang 开发默认先查 Codex Rust 是否已有成熟实现，能移植/裁剪/适配就不自研；只有与记忆本体、可拔插边界或本机安全约束冲突时才新写实现。
- Codex 新版目标驱动推进方式已固化到 `docs/goal-mode-operating-plan.md`：记录了本轮多子代理并行的 `GOAL_SPEC` 契约、分写入范围、主进程统一集成验证、阶段提交和关闭子代理流程；后续迁移到 Chuang 时只吸收组织模式，不硬编码 Codex CLI 细节。
- Control / Actuator command adapter 的输出契约再收紧一层：control list/apply 和 actuator response 现在拒绝未知顶层字段，control receipt 对 `change_model` 会报显式 `model_name` mismatch，非换模型动作夹带 `model_name` 也会拒绝。协议文档已同步，避免外部 adapter 静默漂移。
- `status` / `doctor` 现在会把 GA 9 原子工具拆成 `mapped_atomic_tool_names` 和 `interface_only_atomic_tool_names` 两组名单，文本和 JSON 都能直接看出当前可执行映射与仅接口登记的桌面能力；doctor 也补了精确名单校验。
- `channel simulate` 的结构化输出补了一处薄桥锚点：现在 JSON / 文本输出都会暴露 `runtime_report_id`，方便未来飞书插件把通道消息和本轮报告稳定关联起来，同时继续保留 `runtimeObservability` 和工具循环元数据。
- `runtime_config` 的配置摘要补齐了 `provider_request_timeout_ms`：`status` / `config show` 现在能直接暴露 provider 端请求超时，CLI 也支持 `--provider-request-timeout-ms` 覆盖，便于在不触碰密钥的前提下排查 provider 卡死或长尾请求。
- runtime report identity 的结构化输出补齐：`run_with_options()` 现在会把 `runtime_report_id / runtime_report_task_id / runtime_report_agent_id / runtime_report_status` 写入 runtime meta，`runtime_observability_meta()` 同步提升，app-server `turn/start` response 和 `turn/completed` event 直接输出 `runtimeReportId`，上层不必从 CLI 文本旁路推断报告身份。
- provider fallback 的只读诊断面补齐：`runtime_observability_meta()` 现在会提升 `provider_fallback_primary_retryable / provider_fallback_primary_status_code / provider_fallback_primary_error_class`，app-server 的 `turn/start` response 和 `turn/completed` 事件不再只依赖原始 `providerMeta` 才能看懂 fallback 根因。
- identity bootstrap 快照补了结构化存在性诊断：`status` / `doctor` 现在会同时暴露 `identity_bootstrap_present`，能区分“文件缺失”和“文件存在但为空”，避免 bootstrap 挂载问题只剩 0 字符这一种模糊信号。
- GA 9 原子工具 manifest 契约补齐：`AtomicToolManifest` 现在暴露 `schema_version=1` 和字段列表，`status --json` / `doctor` 会同步校验并回传 manifest schema，smoke 与状态测试也锁住该契约。
- 主线治理回传补齐一处输出面缺口：`runtime_observability_meta()` 现在会提升 `governance_action_id / governance_decision / governance_reason`，app-server `turn/start` response 和 `turn/completed` event 的 `runtimeObservability` 可直接读取治理决策，不必从 provider meta 间接解析。
- 已补 runtime report 和 app-server 回归，锁定治理字段会随结构化观测面回传。
- Control / Actuator command adapter 安全边界补回归：control apply receipt 现在额外覆盖 `change_model` 模型名不匹配，真实 control adapter 在未设置 `CHUANG_REAL_CONTROL_ENABLE=1` 时只返回 dry-run receipt，不执行 allowlisted 命令。
- Actuator command adapter 补 timeout 回归：外部 adapter 卡住时会按 `actuator_timeout_ms` 终止本次启动的进程并返回结构化错误，不接真实桌面/浏览器。
- `doctor` 的 command control smoke 已补只读回归：测试 adapter 的 `apply` 会写 marker，`doctor --json` 只能调用 `list`，不得触发 apply 或真实服务控制。
- MVP readiness / doctor / smoke 验收面已对齐到当前主线能力：`scripts/chuang-mvp-smoke.sh` 现在会断言 `status --json` 里的 `execution=generic_agent_mvp`、GA 原子工具 action/report schema、goal mode、plugin registry，以及 smoke 配置只出现预期的 stub provider placeholder warning。
- smoke 脚本的 `doctor` 步骤改为 JSON 验收，明确要求 config、identity、slots、atomic_tools、goal_mode、actuator/control smoke、isolated runtime smoke、subagent queue smoke、plugin_registry 全部存在并通过。
- smoke 脚本新增 goal/session/channel 验收：`run --goal` 必须写入 `goal_context_injected=true`，session memory 必须暴露 isolated recall/filter/writeback meta，`channel simulate --goal` 必须把 goal objective 传到 provider meta。
- `docs/mvp-readiness-2026-05-02.md` 和 `docs/mvp-scope.md` 已刷新“已实现 vs 目标态/插件态”边界：goal mode 只是轻量 runtime context，GA interface-only 原子工具不是现实桌面控制，plugin registry 只是 manifest/path readiness，不代表插件已启用或运行。
- Memory/Context 会话稳定性 MVP 补了一层可诊断元数据：`run --session-id ID` 的 runtime meta 会暴露 `session_id`、`session_memory_scope`、`session_memory_recall_isolated`、`session_memory_recall_filter`、`session_memory_recall_hit_count`；`--remember-session` 写回后会额外暴露 `session_memory_write_requested`、`session_memory_summary_kind`、`session_memory_record_id`。
- 会话记忆隔离回归已增强：同一 SQLite store 内 `alpha` 和 `beta` 都写入 session summary 后，`alpha` 查询 `beta` 的锚点必须保持 `recall_hit_count=0`，防止不同 session 串记忆。专项验证已通过 `cargo fmt --all` 和 `cargo test -q cli_runtime::tests::run_with_options_remembers_and_recalls_session_turns`。
- 记忆面再补硬上限无变更回归：`hermes_memory` 新增 `write_memory()` 超限不落盘测试，`cli memory identity write-memory` 新增 CLI 入口超限失败不改文件测试；`memory identity show` 的 JSON 回归也补了 `user_max_chars / memory_max_chars` 可见性。
- Goal mode 状态面已补：`status --json` 现在有 `goal_mode` 摘要，标明它是 `lightweight_runtime_context`、入口为 `run --goal TEXT`、不新增 core slot、不绕过 governance；`doctor` 增加只读 `goal_mode` 检查，确保默认 `GoalSpec` 和 context segment 可渲染。
- Goal mode 已接到 channel/app-server 输入协议：`channel simulate --goal TEXT` 会把目标写入 app-server `turn/start.params.goal`，app-server 再转为 `RunCliRequest.goal_spec`；输出的 provider meta 可看到 `goal_id / goal_objective / goal_context_injected`。默认飞书桥不强制传 goal，能力只是作为独立通道参数预留。
- 当前阶段结论：主进程工具口已进入收尾细化阶段，近期连续完成 action/report schema 契约、工具循环元数据统一视图、治理决策标签收口、治理拒绝路径结构化；下一步开始把注意力从工具细节转向 Memory/Context 会话稳定性、真实 subagent runner、Chuang 自身 goal mode 最小实现。
- GoalSpec CLI 入口已补最小版：`run --goal TEXT` 会生成 `GoalSpec::mainline_mvp(TEXT)` 并注入 runtime extra context；runtime meta 会输出 `goal_id / goal_objective / goal_context_injected`，方便通道和控制台确认目标上下文已生效，同时不改变原始 `user_input`。
- GoalSpec -> Runtime extra context 最小接入已完成：`GoalSpec::render_context_segment()` 会把目标 spec 渲染为 `ContextSegment`，CLI runtime 的 `RunCliRequest.goal_spec` 可选注入该 segment，并继续复用 `run_governed_turn_with_extra_context()`；未传 goal 时默认空上下文，用户输入不被污染，不新增 slot、不绕过 governance。
- Codex 自身升级已完成：本机 Codex CLI 来源确认为全局 npm 包 `@openai/codex`，已从 `0.125.0` 升级到 `0.128.0`；验证命令 `/home/user/.npm-global/bin/codex --version` 返回 `codex-cli 0.128.0`。飞书桥通过该路径启动 Codex app-server，重启 `codex-feishu-bot.service` 后会加载新版；升级过程只处理 Codex，不碰 Hermes，不提交私有 `config.toml`。
- Codex 0.128.0 的 `goals` feature 已低风险验证：`codex --enable goals features list` 会显示 `goals ... true`，但当前没有新增显式 `goal` 子命令；因此暂不默认开启到飞书主通道。
- Chuang 自身 goal mode 最小骨架已开始落地：新增 `src/goal_mode.rs` 和 `tests/goal_mode_tests.rs`，只定义 `GoalSpec`、校验和 runtime context block 渲染，不执行命令、不绕过治理、不新增 slot。
- `GoalRun` 规划原语已明确：它把 `goal_spec / worker_plan / validation_plan / checkpoint_log` 组织成可恢复的目标计划，当前 continuation model 是 checkpoint-first，尽量靠恢复 checkpoint 续接，而不是反复让操作员输入 `continue`。
- 已新增 `docs/goal-mode-operating-plan.md`，把当前 Codex 侧的目标驱动推进方式固化为协作流程：每轮固定 Goal / Acceptance / Budget / Checkpoint，先用于推进 Chuang 主线，后续再迁移成 Chuang 自己的轻量 goal 能力。
- goal mode 当前不新增核心 slot，不改变主链；未来目标态是 `GoalSpec -> Governance -> Context -> Execution Slot -> Report -> Memory` 的长期任务外壳。
- `ACTION` 协议也开始暴露 schema 契约：`ToolActionEnvelope::schema_version()`、`schema_fields()`、`call_schema_fields()` 已补齐，和 `ToolLoopReport` 一样可被测试和文档引用。
- `parse_tool_action_envelope_result()` 已补结构化错误返回，`ACTION` 前缀缺失和 JSON 错误不再只能被旧 `Option` 入口吞掉；旧 `parse_tool_action_envelope()` 继续保留兼容。
- `status / doctor / console snapshot` 现在会暴露 `tool_action_schema_version=1`，并在 JSON 状态里输出 action schema 字段，控制台可以同时确认 action schema 和 report schema。
- `doctor` 的工具协议检查已从抽样字段升级为完整字段契约匹配：action schema、report schema、call schema 字段顺序和内容漂移都会被发现。
- `ToolLoopMeta` 已从只解析 count/trace/report 扩展为完整工具循环视图，统一承载 calls / protocol_errors / events；`app_server` 和 `cli_channel` 进一步减少各自手写解析。
- 治理决策标签格式已收口到 `governance::risk_decision_label / risk_decision_reason / risk_decision_parts`，替换了 kernel、tool runtime、control workflow、Genesis、CLI runtime 里的重复格式化逻辑。
- `execute_or_reject` 的治理拒绝路径已改为直接复用 `RiskDecision` 生成结构化失败记录，不再从 `tool_needs_approval:` 这类错误字符串反解析；严格执行接口的外部错误字符串保持兼容。
- 主进程工具元数据继续收口：`tool_loop_meta` 已抽成共享解析层，`app_server` 和 `cli_channel` 不再各自重复解析 `tool_*_json`。
- `tool_report_json` 已正式提升为 runtime report 的 `Log` artifact，工具事件不只停留在 meta/trace 里。
- `write_operation` 已从字符串收紧为枚举，`created / modified / unchanged` 现在是结构化结果。
- 当前验证仍保持通过：`cargo fmt --all`、`git diff --check`、`cargo test -q --test tool_runtime_tests`、`timeout 240s cargo test -q`；`tool_runtime_tests` 当前 22 条通过。
- 主进程 Execution Slot 工具回传继续硬化：`ToolLoopReport` schema 升到 v6，新增 `stderr_bytes / stderr_lines`、`write_operation`、`output_redacted / stdout_redacted / stderr_redacted`。
- `read_file` 和 `code_execute` 对疑似密钥路径或内容返回脱敏占位，并保留原始字节数/行数统计，避免工具输出把密钥带进飞书、日志或报告。
- `file_write` 回执新增 `write_operation` 枚举（`created|modified|unchanged`），上层不用再从 `write_before_bytes / write_changed` 反推写入类型。
- `toolEvents[]` 新增 `atomic_tool_name / failure_class / duration_ms / retryable`，事件流可以直接用于控制台和通道调试，`summary` 只作为人工可读兼容字段。
- 工具循环里的普通文本处理也收紧了：首轮可直答，但一旦进入工具往返，后续普通文本会被回灌成 `plain_text_response` 协议错误，逼模型回到 `ACTION` 或 `FINAL`。
- 工具提示词本身也同步收紧了，明确要求首轮尽量用 `FINAL` 收口，进入工具往返后只允许 `ACTION` / `FINAL`。
- `tool_events_json` 现在也会提升为 runtime report 的 `Log` artifact，子代理和报告面可以直接吃到结构化事件流，不只看 summary。
- `app_server` 和 `cli_channel` 的 `tool_*_json` 解析已收口到共享 `ToolLoopMeta` / JSON helper，少一套重复解析逻辑。
- `docs/execution-slot-tool-protocol.md` 已同步 v6 字段和事件流语义。
- 验证通过：`cargo fmt --all`、`git diff --check`、`timeout 240s cargo test -q`。

## 2026-05-02

### 最新进展
- `ToolLoopReport` schema 升到 v6：工具调用记录新增 `stderr_bytes / stderr_lines`、`write_operation` 枚举、`output_redacted / stdout_redacted / stderr_redacted`，app-server `toolCalls[]` 同步输出对应 camelCase 字段。
- `read_file` / `code_execute` 现在遇到疑似密钥路径或内容会返回脱敏占位，并保留原始字节数/行数统计，避免工具结果把密钥带进通道、日志或报告。
- `file_write` 回执新增 `write_operation` 枚举，上层不用再靠 `write_before_bytes + write_changed` 反推写入语义。
- `toolEvents[]` 新增 `atomic_tool_name / failure_class / duration_ms / retryable`，事件流不再只能靠 `summary` 判断工具身份、失败类型和重试建议。
- 工具执行记录新增结构化定位字段：`target_path / resolved_path / cwd / command`，app-server `toolCalls[]` 同步输出 `targetPath / resolvedPath / cwd / command`，上层不必再从 `summary` 里拆路径和命令。
- `list_dir` 新增结构化 `entries[]`，每个条目包含 `name/kind`，上层不必再解析 `output=entries[...]`。
- `read_file` / `list_dir` / `code_execute` 新增 `output_bytes / output_lines`，上层不必再从 `summary` 或 stdout 文本计算基础规模。
- `ToolLoopReport` schema 升到 v5，对应工具执行定位、目录条目和输出规模字段扩展。
- 主进程 `file_write` 回执新增有界 diff 预览：`ToolExecutionRecord` 现在带 `write_diff_preview / write_diff_truncated`，app-server `toolCalls[]` 同步输出 `writeDiffPreview / writeDiffTruncated`。
- diff 预览会对 `.env`、token、secret、password、private key 等疑似敏感路径或内容脱敏，只给审计面一个安全占位，避免把密钥写进通道或日志。
- 工具报告协议随新增字段持续升级，当前 `ToolLoopReport` schema 为 v6，控制台/通道可用 schema version 判断字段支持范围。
- `code_execute` 风险分类已从纯硬编码收口到 `ShellRiskRules`：默认规则保持原行为，同时配置文件可通过 `[tool_loop.risk]` 覆盖删除/清理、服务变更、网络调用、密钥访问四类模式。
- 主进程工具循环现在会把治理拒绝结构化回灌给模型：高风险工具不再直接打断整轮对话，而是返回 `ok=false / failure_class=governance_rejected` 的工具记录，并保留 `.rejected` 审计。
- 工具协议解析新增可恢复错误：格式错误、字段缺失、调用未开放工具的 `ACTION` / `TOOL_CALL` 会变成 `protocol_error` 回灌给模型，不再被误当普通最终回复。
- runtime meta / app-server / channel simulate 已暴露 `tool_protocol_error_count` 和 `tool_protocol_errors_json`/`toolProtocolErrors`，协议修正过程可审计，但不强塞进飞书正文。
- `ACTION` envelope 正式增加可选 `schema_version`，当前版本为 v1；工具提示已优先要求输出 `schema_version=1`，缺省继续兼容 v1，不支持的版本会返回 `unsupported_action_schema_version`。
- 新增结构化 `tool_events_json` / `toolEvents`，记录每一轮 `tool_call` 与 `protocol_error` 事件；旧 `tool_trace` 保留为人工可读兼容字段。
- GA 9 原子工具骨架已接入主线可见面：`status` / `status --json` 现在会输出 GenericAgent 来源、9 个原子工具、当前 mapped/interface_only 数量和每个工具的实现入口。
- `doctor` 新增 `atomic_tools` 只读检查，确认当前 Execution Slot 骨架仍是 9 个 GA 原子工具，且 MVP 已映射的工具只有 `file_read / file_write / code_execute`。
- `atomic_tool` 已补 actuator 绑定关系：`mouse -> actuator.click`、`keyboard -> actuator.input_text`、`screenshot -> actuator.screenshot`、`locate -> actuator.observe`；这只是接口层映射，不表示真实桌面控制已经完成。
- 工具协议已开始向 GA 原子名迁移：模型现在可输出 `file_read / file_write / code_execute`，旧的 `read_file / write_file / shell_exec` 继续兼容；工具执行记录新增 `atomic_tool_name`，`list_dir` 明确仍是辅助工具。
- `AtomicToolRegistry::generic_agent_mvp()` 已落地，工具执行记录现在通过 registry 映射 `atomic_tool_name`；接口态桌面工具仍不会被 `ACTION` 协议解析为可执行工具。
- 工具说明文本也已改为由 `AtomicToolRegistry` 生成，避免 `tool_runtime` 继续硬编码 GA 工具名、辅助工具和 interface-only 桌面工具列表。
- `ExecutionSlot` 已作为主进程工具骨架 wrapper 接入：它组合 `AtomicToolRegistry + ToolExecutionConfig`，`cli_runtime` 已通过它执行本地工具；`RuntimeSlotsSummary/status` 现在暴露 `execution=generic_agent_mvp`。
- 工具审计 operation 已开始改用 GA 原子名：如 `tool.file_write / tool.file_read / tool.code_execute.rejected`，`list_dir` 继续是辅助工具审计名。
- 治理 `ProposedAction` 也开始使用 registry 派生身份：action_id 改成 `tool:file_write / tool:code_execute` 这类 GA 原子名，summary 会标记 `atomic_tool=...` 或 `auxiliary_tool=list_dir`。
- 顺手修复 command subagent runner 的一个稳定性问题：外部命令如果不读取 stdin 并提前退出，`BrokenPipe` 不再直接让 run-once 失败，后续仍会读取 stdout 中的协议报告。
- 新增 `docs/execution-slot-tool-protocol.md`，固化 Execution Slot 当前工具协议、GA 原子工具映射、治理审计命名、`ToolExecutionRecord` 字段和当前不可越过的边界。
- app-server/channel 工具回传字段已对齐协议：`turn/completed` 事件和 `turn/start` response 都输出 `toolCallCount/toolTrace/toolReport/toolCalls`；`channel simulate --json` 输出 `tool_call_count/tool_trace/tool_report/tool_calls`。
- `ToolLoopReport` 已暴露 schema 常量和字段列表：`schema_version()`、`schema_fields()`、`call_schema_fields()`，避免协议文档和 Rust 结构悄悄漂移。
- `console snapshot` 文本和 JSON 都能看到 `execution=generic_agent_mvp` 与 GA 原子工具数量，后续桌面控制台能直接展示主进程工具骨架状态。
- `status/doctor/console` 的 Execution Slot 健康信息已增强：状态面暴露 `atomic_tools.ok`、tool report schema version 和字段列表；doctor 会校验 manifest 与 schema 是否仍符合当前协议。
- app-server 的工具循环已改成复用现成治理/审计链：`slot_registry::build_governance_slot()` 先装配规则治理，再由 `tool_runtime::execute_tool_call_with_governance()` 先分类、后执行、再记审计，不再单独绕一套平行控制逻辑。
- 工具执行回执现在会把治理决策写进 `toolTrace`，方便后续继续收口到统一报告层。
- Chuang Feishu bridge 已去掉 `replyToMessageId/reply_in_thread` 的发送方式，当前会按 `chatId` 直发新消息，避免误进话题线程。
- OpenAI-compatible provider 的 assistant content 提取已放宽：除标准 `choices[0].message.content` 外，也能识别 `output_text`、`output[]`、`delta.content` 和 `content[]` 这类常见兼容返回。
- 非 2xx HTTP 响应现在不会再被误当成“内容缺失”，而是直接回 `PROVIDER_HTTP_ERROR`，并把 `status_code / provider_error_class / provider_error_message` 结构化带出来，方便触发 fallback 或排障。
- 新增 provider content / error 路径单测，覆盖 chat completion、response `output_text`、数组内容片段拼接，以及 429 错误体透出。

## 2026-05-01

### 最新进展
- OpenAI-compatible provider 的 `native` transport 现在通过 `hyper-rustls` 支持 `https://` 目标，不再只限明文 `http://`；`http` transport 仍保持纯明文 raw socket。
- 新增回归覆盖 native transport 的 HTTPS scheme 接受与本地 TLS 尝试，`https` 不再直接落到 `unsupported_http_scheme`。
- 子代理 command runner 输出读取改为限量捕获：stdout/stderr 后台持续 drain，但最多保留 64KiB 原始输出，避免真实 runner 大量输出撑爆主控内存。
- command runner report 仍只暴露 1200 字符 preview，并在输出超过 preview 或捕获上限时设置 `truncated=true`。
- 新增回归覆盖 command runner 大输出场景，确认 report 可写入、preview 有界、truncated 标记正确；子代理 CLI 专项测试通过。
- OpenAI-compatible provider adapter 新增统一请求超时保护，默认 60000ms；测试/后续配置可通过 adapter 覆盖，不改变默认 stub 行为。
- `http` transport 改用 `connect_timeout` 并设置 read/write timeout，服务端卡住时会返回结构化 `http_read/http_connect` 错误，不会无限阻塞。
- `native` transport 的 request 和 response body collect 已加 `tokio::time::timeout`，卡住时返回 `native_http_timeout`。
- `curl` transport 除了继续传 `--max-time`，现在还有 Chuang 自己的进程级 timeout，会终止本次启动的 curl 并返回 `curl_wait` 证据。
- 新增回归覆盖 http/native/curl 三条真实 provider transport 的卡死超时路径，provider transport 专项测试通过。
- `subagent run-once --runner command` 新增超时保护：使用 dispatch 自带的 `idle_timeout_ms`，真实外部 runner 卡住时会终止本次启动的进程并写入 Failed `SubagentReport`。
- command runner 超时 report 会保留 `timed_out=true` 摘要和 `stderr_preview` 证据，主控/桌面控制台不会因为外部子代理卡住而阻塞。
- 新增回归覆盖 command runner 超时后仍写 failed report；子代理 dispatch/list/report/collect/run-once 专项测试通过。
- Genesis `SystemGenesisCommandRunner` 新增进程级超时兜底：即使 AutoCLI 自身没有按 `--timeout` 返回，Chuang 也会按 `GenesisCommandSpec.timeout_ms` 终止本次启动的进程并返回 command failed 证据。
- `GenesisCommandSpec` 现在结构化暴露 `timeout_ms`，`genesis ask --dry-run` 可看到主/备通道的进程级超时。
- 新增回归覆盖 Genesis 外部命令卡住时超时返回；Genesis/CLI/slot registry 专项测试通过。
- command-backed 控制面新增 `timeout_ms`，默认 30000ms，配置文件可通过 `[control] timeout_ms = ...` 或扁平 `control_timeout_ms` 覆盖。
- `CommandControlPlane` 会在 list/apply 外部命令超时后终止自己启动的命令进程，并返回清晰的超时错误，避免桌面控制台被卡死。
- 新增回归覆盖 command list 卡死超时、配置解析默认 timeout、显式 timeout、零 timeout 拒绝。
- command control 的 `list_args/apply_args` 新增轻量引号解析，不走 shell；token 开头的 `"agent with space"` 这类参数会作为单个 argv 传给外部脚本，同时不破坏 JSON 字符串里的字段引号。
- 新增回归覆盖带空格的 quoted 参数，保证真实控制脚本接入时不用为标签/显示名再绕一层 shell。
- `ConfigSummary` / `status --json` / `config show` 现在会暴露 `control_command_timeout_ms`，桌面控制台可直接展示 command 控制脚本超时配置。
- 新增回归覆盖 status 从配置文件读取并输出 command control timeout。
- command control 脚本类测试改为通过 `sh script ...` 启动，并使用 `try_list_units()` 暴露真实错误，避免 Linux 临时脚本偶发 `Text file busy` 被旧兼容接口吞成空列表。
- CLI `control apply` 现在在 fallible list 解析出 unit 后，直接进入 `run_control_workflow_for_unit()`，避免 workflow 内部第二次调用旧兼容 `list_units()` 把真实 command list 故障弱化成 unknown unit。
- 新增回归覆盖 command apply 只执行一次 list，确保后续控制台 apply 路径不会重复探测外部脚本。
- command-backed 控制面补上显式失败路径：`CommandControlPlane::try_list_units()` 会把 list 命令非 0、坏 JSON、spawn/stdin/wait 错误作为 `ControlError` 返回，不再只表现为空列表。
- CLI `control list --config PATH` 现在对 command 控制面走 fallible list；真实控制脚本故障会输出 `control_failed`，避免桌面控制台误判“没有服务/Agent”。
- 新增回归覆盖 command list 非 0 和 malformed JSON 两类失败，adapter 层和 CLI 层都已验证。
- command control apply 增加 receipt 一致性校验：外部脚本返回的 `unit_id/action/model` 必须和请求一致，防止脚本错配目标后仍被控制台当成成功。
- 新增回归覆盖 command apply receipt 的 unit/action 错配拒绝。
- `doctor` 新增 control plane 只读冒烟：会通过当前 slot 执行 list 检查，command 控制脚本失败时以 `doctor_control_plane_list_failed` 报出，不执行 apply。
- `cli_doctor_tests` 已覆盖 command control list 故障路径，防止健康检查漏报控制面不可用。
- 已运行 `cargo fmt`、`git diff --check`、`cargo test`，当前全仓测试通过。
- 控制面新增 `CommandControlPlane` adapter：通过外部命令的 JSON stdout/stdin 完成 list/apply，可用于后续 systemd、Agent 管理脚本或桌面服务桥。
- 配置文件新增 `control = "command"` / `[control] kind = "command"` 支持，可配置 `program / list_args / apply_args`；默认仍是 `fake_local`。
- CLI `control list/apply` 现在会读取 `--config PATH`，已能通过配置切到 command 控制面；真实 apply 仍经过治理和 `--approve`。
- 新增测试覆盖 command 控制面 adapter、配置解析、CLI control 经配置调用 command 控制面。
- 新增 `docs/control-command-protocol.md`，固化外部控制脚本协议：list 输出单位数组，apply 从 stdin 接请求并输出 receipt。
- 新增脚本级回归，确认 command control apply 会把 JSON 请求写入外部命令 stdin，外部脚本能读出 `model_name` 并回传 receipt。
- `Genesis Actuator` 的具体构造已收口到 `slot_registry::build_genesis_actuator()`，`cli_genesis` 不再直接 new 具体实现，后续更容易替换 runner / adapter。
- `build_genesis_actuator()` 现在返回 `GenesisSlot` wrapper，而不是裸的 `AutoCliGenesisActuator`，CLI 只看 slot 接口。
- 新增 core boundary test，防止 `cli_genesis.rs` 重新直接持有 `AutoCliGenesisActuator` / `SystemGenesisCommandRunner`。
- 新增 `tests/slot_registry_tests.rs` 的 Genesis 构造 contract，确认 profile dir、CDP 端口和 timeout 会完整透传到 AutoCLI 命令规格。
- 老爸确认：`BrowserWorker` 旧实现先舍弃，不继续沿旧浏览器 worker 方案推进；后续网页 AI 查询能力改走新的 `Genesis Actuator` 插件线。
- `Genesis Actuator` 定位已记录：核心只需要统一 `genesis_ask(prompt)` / search port，具体 AutoCLI、DeepSeek、主通道 userDataDir、备用 CDP 真人浏览器、登录态检测和修复都留在 adapter/plugin。
- 安全边界已明确：不得用自动删除 profile 作为自愈手段；任何真实浏览器控制、登录态修复、profile 改写都必须可审计，并由治理层显式约束。
- 新增 `src/genesis_actuator.rs` 插件线：包含 `GenesisActuator` trait、`FakeGenesisActuator`、`AutoCliGenesisActuator`、`GenesisConfig`、双通道错误类型和修复计划结构。
- `AutoCliGenesisActuator` 已实现主通道 userDataDir + 备用 CDP 的最小容灾：主通道命中“请登录 / 验证码 / 登录后查看”等登录态失效 marker 时切备用通道，备用成功后只返回需审批的修复建议，不改写或删除 profile。
- 新增 `tests/genesis_actuator_tests.rs`，覆盖 fake 查询、AutoCLI 命令形状、主通道成功、CDP fallback、双通道失败。
- CLI 新增 `genesis ask --prompt TEXT --approve-exec`：可手动验证 Genesis 查询入口；缺少 `--approve-exec` 时拒绝执行外部程序。
- 新增 `tests/cli_genesis_tests.rs`，覆盖 Genesis CLI 审批拒绝和已审批执行路径。
- Genesis CLI 现在会先走 `StaticRuleGovernance`：外部网页 AI 查询按 `ExternalSend` 分类为 `needs_approval`，显式审批后执行，并写入审计记录；JSON 输出带 `governance_decision` 和 `audit_recorded`。
- Genesis CLI 新增 `--dry-run`：不用 `--approve-exec`，只渲染主通道 userDataDir 和备用 CDP 的 AutoCLI 命令规格，不执行外部程序，方便排查配置。
- MVP 主链路补上 kernel 级治理入口：`ChuangKernel::run_governed_turn()` 会先通过 `Governance` trait 分类，非允许决策会在 runtime 前阻断，允许后再执行并写 audit。
- `ChuangKernelTurn` 现在可携带 `governance_decision`，普通 `run_turn` 保持兼容并留空；CLI `run/repl` 默认通过 slot registry 的治理 slot 走 governed turn。
- CLI `run` 输出新增 `governance_decision: allowed:...`，让 `input -> context -> runtime -> governance -> report` 的 MVP 链路有可见证据。
- `runtime_report` 和 `SubagentReport` 也已携带治理元数据，`governance_action_id / governance_decision / governance_reason` 会随 report 结构化输出，避免只靠文本日志理解治理结果。
- `run` 的治理结果现在也写进 `RuntimeResult.response.meta.extra`，CLI runtime 输出层可以直接打印结构化治理字段，`main.rs` 不再单独重复打印。
- `cli_runtime` 新增单测，直接验证 `run_with_options()` 的返回值里带有治理元数据，避免以后只改 stdout 忘了结构化字段。
- 治理元数据 key 的组装已经抽成共享 helper，`cli_runtime` 和 `runtime_report` 现在共用同一份字段拼装逻辑。
- `summary_compression` 现在不是纯壳了：会对长 memory / tool result 段做本地截断压缩，再交给同一预算 packer。
- `context_engine_tests` 新增回归，确认长 memory 段会被轻量压缩并保留结构化压缩元数据。
- CLI 新增 `doctor` 命令：安全校验配置、身份记忆、slot 装配，并用临时 fake provider / 临时 DB 跑隔离 runtime smoke，用临时队列跑子代理 dispatch smoke。
- `doctor --json` 已支持结构化输出且会继续脱敏 provider key，方便后续桌面控制台直接读取健康状态。
- README 已从早期协作说明更新为当前 MVP 入口，直接列出 `doctor/status/run/subagent` 等最小可用命令。
- 已按 `docs/mvp-scope.md` 跑过一组端到端 MVP 验收：`status -> doctor -> run --remember -> run --remember-identity -> subagent dispatch/list/run-once/report/collect -> status --json -> doctor --json` 全部通过。
- 新增 `docs/mvp-checkpoint-2026-05-01.md`，记录当前 MVP 已验收命令、可用能力、下一阶段边界。
- 刚刚补完的治理元数据已经通过 `cargo test` 全量验证，当前主链保持全绿。
- CLI 展示层已从 `main.rs` 拆到 `src/cli_output.rs`：usage、JSON 输出、status/config/runtime/control 打印逻辑不再挤在入口文件里，运行链路行为保持不变。
- `browser_worker` 已明确标记为 adapter/plugin 能力线，并新增核心边界测试，防止 MVP 主入口、runtime、kernel、slot registry 直接依赖浏览器外脑实现。
- `main.rs` 中重复的 runtime 参数白名单已收口为 `is_runtime_value_flag()` / `copy_runtime_value_arg()`，降低后续新增配置字段时多处漏改的风险。
- `slot_registry_tests` 新增 control plane slot contract：通过 `ControlPlaneSlot` 执行服务重启和 Agent 换模型，确认控制台能力仍走 trait 边界。
- `slot_registry_tests` 新增 queued external subagent 回收 contract：外部 report 写入文件队列后，`SubagentRuntimeSlot::collect()` 会经 slot wrapper 吸收并返回结构化报告。
- CLI 参数解析继续瘦身：新增 `take_value_or_usage()` / `skip_value_arg()`，把 `--flag value` 的取值和索引推进逻辑收口，减少入口文件重复细节。
- CLI DTO 已拆到 `src/cli_types.rs`：request/output 数据结构离开 `main.rs`，入口继续保留命令执行和解析流程。
- CLI 运行时组合已拆到 `src/cli_runtime.rs`：一轮运行、SQLite 记忆种子、身份记忆写回、kernel 配置从 `main.rs` 移出。
- `slot_registry_tests` 新增 provider slot 错误路径：无效 fake/openai-compatible provider 配置会在 slot 装配前返回 `ConfigError`。
- `slot_registry_tests` 新增 queued external subagent 错误路径：空队列根目录会拒绝装配，非法 spawn 不会写入 dispatch 文件。
- CLI 参数解析已拆到 `src/cli_args.rs`：`main.rs` 从 1264 行降到约 612 行，入口文件只保留命令分发和执行流程。
- subagent CLI 命令执行已拆到 `src/cli_subagent.rs`：dispatch/list/report/run-once 队列适配逻辑离开 `main.rs`。
- control CLI 命令执行已拆到 `src/cli_control.rs`：服务/Agent 控制台逻辑离开 `main.rs`，继续作为组合层使用 slot。
- config CLI 命令执行已拆到 `src/cli_config.rs`：配置 check/show/init 逻辑离开 `main.rs`。
- 核心边界测试新增薄入口护栏：`main.rs` 不应重新直接持有 subagent queue、control workflow、config template 等具体命令适配细节。
- `run` 新增显式 `--dispatch-subagent`：当配置为 `--subagent queued_external` 时，会把本轮 runtime report 通过 `SubagentRuntimeSlot` 写入子代理 dispatch 队列，并输出 report/dispatch id。
- `run --dispatch-subagent` 增加失败路径覆盖：未选择 `queued_external` 时明确拒绝，不创建 dispatch 队列。
- CLI 端到端测试已覆盖 `run --dispatch-subagent -> subagent run-once -> subagent report`，确认 run 产生的 dispatch 可被 fake runner 回写并读回 report。
- CLI 新增 `subagent collect --run-id ID`：从持久化 dispatch 恢复 queued spawner，再经 `SubagentRuntimeSlot::collect()` 回收 report；身份不匹配会拒绝，避免只读文件绕过子代理协议校验。
- Context engine 新增非默认 `summary_compression` 轻量压缩策略：实现独立 engine wrapper、配置文件字段 `context_engine`、CLI 参数 `--context-engine` 和 status 展示；默认仍保持 `deterministic_budget`。
- Runtime 已实际接入 context engine 选择：`ChuangKernelConfig` 会把配置传给 `AgentRuntime`，`RuntimeResult` 和 CLI run 输出会显示本轮实际使用的 `context_engine`。
- 配置文件体验已简化：`config.example.toml` 改为扁平字段，适合长期手工维护。
- 配置解析保持向后兼容：旧的 `[provider]` / `[context]` 分段写法继续可用，同时新增 `provider / provider_id / model / context_max_tokens` 等扁平字段。
- CLI 新增 `config check` 与 `config show`：可只校验或查看脱敏配置摘要，不执行任务。
- CLI 新增 `config init`：显式生成默认 `config.toml`，目标存在时拒绝覆盖。
- 未显式传 `--config` 时，CLI 会自动读取当前目录 `config.toml`；不存在则继续使用内置默认值。
- 核心边界第一轮瘦身已开始：`AgentRuntime` / `ChuangKernel` 不再默认构造 `FakeResponder`，由 CLI 或测试显式注入。
- 新增 `docs/core-boundary.md` 与 core 边界测试，防止核心文件继续引入具体 provider / browser / control plane / subagent adapter。
- `responder.rs` 已拆出 OpenAI-compatible 具体实现到 `provider_openai_compatible.rs`，并继续拆出 fake/scripted 子模块；`responder` 主文件只保留抽象 trait 与统一壳。
- `subagent_spawner` 已拆出 `fake` / `queued` 子模块，主文件只保留子代理协议类型、trait、slot 转发和共用校验。
- `control_plane` 已拆出 fake 子模块，主文件只保留控制面协议、治理/审计辅助函数和共用校验。
- `actuator` 已拆出 fake 子模块，主文件只保留桌面/浏览器/人类级操作面的协议定义。
- `skill_evolver` 已拆出 noop 子模块，主文件只保留进化层协议类型、trait 和共用校验。
- `memory_store` 已拆出 in-memory 子模块，主文件只保留通用记忆接口和数据结构。
- `hermes_memory` 已拆出 file 子模块，主文件只保留 Hermes 双文件记忆的配置、快照、条目、错误和 trait。
- `context_engine` 已拆出 deterministic 子模块，主文件保留上下文数据结构、packer 算法、trait 和错误类型。
- `governance` 已拆出 static-rule 子模块，主文件只保留动作、风险决策、错误类型和治理 trait。
- `AgentRuntime` 已改为直接使用核心 `ContextPacker`，不再在主链路里构造 deterministic engine 包装实现。
- `slot_registry` 已引入 provider、治理、操作面、进化、控制面的 slot wrapper；`RuntimeSlots` 不再把字段类型直接绑定到当前 fake/noop/static/openai-compatible 实现。
- OpenAI-compatible provider adapter 的实例化已从 `runtime_config` 移到 `slot_registry`；配置层只保留配置描述、校验和脱敏摘要。
- 新增边界测试，防止 `runtime_config` 重新引入 `OpenAICompatibleProviderAdapter` 构造逻辑。
- `RuntimeSlotsSummary` 和 CLI `status` 已补 provider slot，控制台视角下所有当前 slot 都能统一展示。
- `SubagentConfig::QueuedExternal` 在 slot registry 中已接入 `FileSubagentQueue`：spawn 会写入 dispatch 文件，collect 会尝试吸收 report 文件。
- 新增 `/new` 接续文档：`docs/handoff-current.md`，记录当前目标、用户约束、最新提交、验证状态、重要文件和下一步建议。

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
- 小承（DeepSeek）：并行补上下文引擎 DS-3 规格草案，供主线 context engine 收口参考

### 待做
1. 把 DS-3 规格草案收口进本地 context engine 设计与测试计划
2. 优先推进 context engine 主线：结构化 segment + budget packing + runtime 接缝
3. BrowserWorker 继续作为并行能力线推进，不抢三大核心主线
4. DeepSeek 并行任务拆分文档保留，但 context engine 已上升为当前第一优先级

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
- 长期记忆主线已新增 SQLite 持久化测试文件：`tests/memory_store_sqlite_tests.rs`
- `SqliteMemoryStore` 当前已实测跑通 5 条：`put/get`、`persist_across_reopen`、`search/delete`、`expire_removes_expired_records_only`、`search_excludes_expired_records_after_expire`
- 长期记忆主线新增最小检索编排层：`src/memory_recall.rs`
- 新增测试：`tests/memory_recall_tests.rs`，当前已实测跑通 5 条：`returns_ranked_hits_for_query`、`respects_metadata_filter_and_limit`、`rejects_zero_limit_request`、`builds_agent_input_block_from_hits`、`builds_memory_segments_from_hits`
- `MemoryRecallPipeline` 最小闭环已跑通：`RecallRequest -> store.search -> ranked hits -> summary -> agent_input`
- **新增最小运行时主线：`src/agent_runtime.rs`**
- **新增 responder 抽象：`src/responder.rs`，当前已落下 `Responder trait + FakeResponder + ScriptedResponder`**
- **新增测试：`tests/agent_runtime_tests.rs`，当前已实测跑通 6 条，覆盖 packed context debug artifacts / empty recall / zero limit / packed context loop / dropped recall / context pack errors**
- **新增 SQLite 运行时集成测试：`tests/agent_runtime_sqlite_tests.rs`，当前已实测跑通 2 条：`runs_with_sqlite_memory_store`、`returns_structured_trace_fields`**
- **新增 responder 集成测试：`tests/agent_runtime_responder_tests.rs`，当前已实测跑通 2 条：`uses_fake_responder_output`、`preserves_prompt_and_trace_with_fake_responder`**
- **新增 responder 结构化 payload：`ResponderOutput { model_name, body, trace, meta }`，并补 `ResponderMeta { provider, recall_hit_count, finish_reason }`**
- **`ScriptedResponder` 已从纯 string 返回推进到结构化对象，可稳定模拟模型名 / 正文 / trace / meta**
- **`RuntimeResult` 已收口为结构化响应：`prompt / response{ model_name, body, trace, meta } / recall_summary / recall_hit_count`**
- **新增/更新测试覆盖结构化返回链路：`scripted_responder_tests`、`agent_runtime_tests`、`agent_runtime_sqlite_tests`、`agent_runtime_responder_tests` 已实测通过**
- **新增可执行入口：`src/main.rs`，当前支持最小命令：`cargo run -- run --input "..."`，可选 `--db PATH`**
- **新增 CLI 冒烟测试：`tests/cli_smoke_tests.rs`，已实测通过；本机已实际跑通 `cargo run --quiet -- run --input "创项目现在启动试试"`**
- **CLI 已继续推进到最小 REPL：支持 `cargo run -- repl`，已实测单轮输入 + `exit` 正常收口**
- **新增 DeepSeek 并行任务拆分文档：`docs/deepseek-parallel-task-split-20260430.md`，已把适合网页外包的剩余模块重新拆成 3 份明确任务**
- **新增 responder provider seam：`Responder::provider() -> ResponderProvider { provider_id, model_name }`，Fake / Scripted responder 已统一暴露 provider 身份**
- **新增测试：`tests/provider_seam_tests.rs`，已实测通过；runtime 已能稳定保留 responder provider identity，给后续接真模型 provider 预留了接缝**
- **新增 provider adapter entry 验证：`tests/provider_adapter_entry_tests.rs`，已证明 runtime 可无结构改动地接收一个“类真实 provider responder”实现**
- **新增 context engine 主线最小闭环：`src/context_engine.rs` 已落地，包含 `ContextSegment / ContextBudget / ContextPacker / PackedContext`**
- **新增测试：`tests/context_engine_tests.rs`，8 条测试已全绿，覆盖 system reservation / missing-token normalize / tool trim / rank / recent-memory trimming / working restore / dropped ids / budget exceeded**
- **`ContextPacker` 已补 segment normalize：缺失 `tokens` 时会按内容长度估算，避免预算合并前出现空 token 段**
- **`ContextPacker` 已补 memory trimming 最近访问保留策略：超过 `max_memory_segments` 时，优先保留最近访问的 memory segment**
- **`ContextPacker` 已补 working budget 失败信号：当 `min_working_tokens` 约束无法满足时，会显式打出 `budget_exceeded=true`**
- **`ContextPacker` 现在会结构化保留 drop/debug 信息：新增 `drop_reasons` 与 `budget_exceeded_reasons`，便于上层解释为什么丢、为什么超预算**
- **`MemoryRecallPipeline` 已升级为双输出：保留 `summary/agent_input` 的同时新增 `segments: Vec<ContextSegment>`**
- **`AgentRuntime` 已接入 context seam：`RuntimeRequest` 新增 `context_budget`，`RuntimeResult` 新增 `packed_context_preview / packed_token_count / dropped_segment_ids`**
- **`AgentRuntime` 的 packed preview 已升级为 explain/debug 视图：会输出 `drop_reasons / budget_exceeded / budget_exceeded_reasons`**
- **`RuntimeResult` 已新增结构化 `context_debug` 字段，直接暴露 `drop_reasons / budget_exceeded / budget_exceeded_reasons`，不再只靠 preview 字符串**
- **CLI 已把 context debug 结构化字段打印出来：`context_drop_reasons / context_budget_exceeded / context_budget_exceeded_reasons`**
- **新增测试：`tests/agent_runtime_tests.rs`、`tests/agent_runtime_sqlite_tests.rs`、`tests/cli_smoke_tests.rs` 已补 explain/debug 断言，当前全绿**
- **CLI / sqlite runtime / responder / provider / context 相关测试已补齐兼容更新，当前全仓 `cargo test` 再次全绿**
- **新增 responder 半层抽象：`ProviderAdapterResponder + ProviderIdentity + ProviderAdapterResponse` 已落地，`Responder` 退成上层统一壳，下一步可以自然接本地 provider / OpenAI-compatible / 其它 backend**
- **新增最小 provider 配置校验与请求封装：`ProviderConfigError / OpenAICompatibleRequestEnvelope / OpenAICompatibleMessage` 已落地**
- **`OpenAICompatibleProviderAdapter` 已补 `validate_config()` 与 `build_request_envelope()`，现在能先校验 `base_url/model_name`，再构造结构化 messages 请求体**
- **新增测试：`tests/openai_compatible_adapter_request_tests.rs`，4 条已全绿，覆盖空配置拒绝 / request envelope 构造 / trace 暴露 message_count**
- **CLI 已新增 provider 参数直通：`--provider-base-url / --provider-api-key / --provider-model / --provider-id`，可直接把最小 openai-compatible 配置接进 runtime**
- **新增测试：`tests/cli_provider_smoke_tests.rs`，2 条已全绿，覆盖 provider 参数跑通 + 残缺配置拒绝**
- **已实际跑通命令：`cargo run --quiet -- run --input "创项目继续推进 provider" --provider-base-url "https://api.example.com/v1" --provider-api-key "test-key" --provider-model "gpt-4.1-mini" --provider-id "custom-openai"`**
- **当前 CLI 实测输出已稳定带出：`model_name/provider/transport/message_count`，真 provider 调用前的接线骨架已通**
- **新增 HTTP 请求预览骨架：`HttpRequestPreview { method, url, headers, body_json }` 已落地，adapter 现在能先收口到“可发送”的 HTTP 形状**
- **`OpenAICompatibleProviderAdapter::build_http_request_preview()` 已落地：会把 `/chat/completions` URL、Bearer Header、JSON body 统一组出来**
- **新增测试：`tests/openai_compatible_http_preview_tests.rs`，2 条已全绿，覆盖 HTTP preview 构造 + respond 透出 request_url/request_method/request_message_count**
- **当前 trace/meta 已能稳定暴露 request 级骨架字段，为下一步真 POST 调用留好了接口面**
- **新增扩展元数据通道：`ResponderMeta.extra: BTreeMap<String, String>` 已落地，adapter 可带 transport / backend 等非核心字段，不污染 runtime 主结构**
- **新增最小 OpenAI-compatible adapter：`OpenAICompatibleProviderAdapter { provider_id, base_url, api_key, model_name }` 已落地，先按 Hermes 现有思路收成最小配置面，只保留 key/url/model 三要素**
- **新增红转绿测试：`openai_compatible_adapter_exposes_minimal_config_shape` 已实测通过，验证 provider identity + base_url/transport 元数据透传 + 统一 responder 壳接入正常**
- **新增 stub POST 骨架：`StubHttpCallResult { status_code, url, request_body_json, response_body_json }` 已落地**
- **`OpenAICompatibleProviderAdapter::execute_stub_post_call()` 已落地：当前会在本地生成 chat.completion 风格 stub 响应，先打通“request preview -> post result -> assistant content extract”闭环**
- **新增测试：`tests/openai_compatible_stub_post_tests.rs`，2 条已全绿，覆盖 stub post 返回 request/response body + respond 透出 stub_status_code/stub_response_kind**
- **CLI 现在会打印 `response.meta.extra` 全部扩展字段；新增测试：`tests/cli_provider_stub_metadata_tests.rs` 已实测通过，确认 stdout 能看到 `stub_status_code: 200` 与 `stub_response_kind: chat.completion`**
- **provider transport 开关已落地：`ProviderTransport::{Stub,Http,Native,Curl}` + `OpenAICompatibleProviderAdapter::with_transport()` 已接上，CLI 新增 `--provider-transport stub|http|native|curl`**
- **`ProviderTransport` 现已实现 `FromStr/as_str/Display`，CLI 能识别 `stub/http/curl`，非法 transport 会在参数层稳定拒绝**
- **transport 失败态会保留 request 预览证据：`respond()` 在报 `invalid-config` 时也会把 `request_url/request_method/request_message_count/transport_mode` 带出来，便于定位 provider 接线问题**
- **新增测试：`tests/cli_provider_transport_flag_tests.rs`、`tests/cli_provider_transport_reject_tests.rs`、`tests/cli_repl_provider_transport_tests.rs`、`tests/openai_compatible_transport_mode_tests.rs`、`tests/cli_provider_default_transport_tests.rs`、`tests/cli_repl_default_transport_tests.rs`、`tests/provider_transport_parse_tests.rs`、`tests/cli_provider_http_not_implemented_tests.rs`、`tests/openai_compatible_http_transport_preview_tests.rs` 已全绿，确认 run/repl/adapter 三层 transport seam 都通了，默认回落到 `stub`**
- **新增 runtime->subagent_report 桥接层：`src/runtime_report.rs` 已落地，可把 `RuntimeResult` 收口成结构化 `SubagentReport`**
- **新增测试：`tests/runtime_report_tests.rs`，2 条已全绿，覆盖 runtime context debug -> report 映射、report metadata 收口**
- **`SubagentReportBuilder::from_runtime()` 已接上最小运行时输入结构 `RuntimeReportInput`，为后续子代理执行结果统一出 report 铺平**
- **新增 provider http 执行层：`OpenAICompatibleProviderAdapter::execute_http_post_call()` 现在已能对 `http://` 目标发真实 POST，并解析返回的 status/body**
- **`ProviderTransport::Http` 当前能力边界已明确：`http://` 本地/明文链路可真实打通；`https://` 先返回结构化配置错误 `unsupported_http_scheme`，同时保留 request preview 证据，不再假装成功**
- **新增测试：`tests/openai_compatible_http_live_transport_tests.rs`、`tests/openai_compatible_http_live_transport_local_tests.rs`，并同步更新 `cli_provider_transport_reject_tests.rs`、`cli_provider_http_not_implemented_tests.rs`、`openai_compatible_http_transport_preview_tests.rs`、`openai_compatible_http_preview_tests.rs`，当前全绿**
- **HTTP 边界现在补齐了失败闭环：本地端口不可达时会稳定返回 `config_error_field=http_connect`，并保留 `request_url/request_method/request_message_count/transport_mode` 供上层解释**
- **HTTP 输入形状错误也已补验证：非法端口（如 `http://127.0.0.1:notaport/v1`）会稳定返回 `config_error_field=base_url` + `invalid_port:...`，CLI/adapter 两层都已实测收口**
- **HTTP 响应解析失败闭环已补：远端回 malformed response 时，`missing_header_separator` / `missing_status_code` 会收口成 `config_error_field=http_response`，不再静默吞掉**
- **HTTP 非 200 返回的证据链也补上了：429 等状态现在会稳定保留 `status_code / response_kind / response_finish_reason` 并返回 `PROVIDER_HTTP_ERROR`；2xx 但缺少 assistant content 的情况改为结构化 `PROVIDER_MISSING_CONTENT`，不再复用旧的 `provider_response_missing_content` 占位**
- **新增回归测试：`openai_compatible_http_transport_preserves_non_200_status_with_structured_metadata` 与 `cli_run_http_transport_reports_invalid_port_shape`，已红转绿并纳入全量 `cargo test`**
- **新增 curl provider transport：`--provider-transport curl` 会通过系统 `curl` 执行真实 POST，支持 HTTP/HTTPS 交给 curl 处理，默认仍是 `stub`，核心 runtime 不直接依赖 TLS/网络实现**
- **新增 native provider transport：`--provider-transport native` 会直接用 Rust HTTP client 发真实 POST，作为真实 provider 的主实现路径，默认仍是 `stub`**
- **新增验证：`tests/openai_compatible_curl_transport_tests.rs` 与 `cli_run_with_provider_and_curl_transport_executes_local_post`，覆盖 adapter 和 CLI 两层 curl POST 闭环**
- **新增子代理 command runner 最小接缝：`subagent run-once --runner command --runner-command PATH --approve-exec` 会显式执行外部进程，不走 shell，把 dispatch JSON 写入 stdin，并把 stdout/stderr/exit_code 收成 `SubagentReport`**
- **安全边界：command runner 缺少 `--approve-exec` 会拒绝执行；默认 runner 仍是 `fake`，不会自动启动真实 Agent**
- **新增验证：`cli_subagent_run_once_command_runner_requires_explicit_approval` 与 `cli_subagent_run_once_command_runner_writes_report_from_process_output`，覆盖审批拒绝和外部进程输出写 report**
- **新增 context engine 保底预留能力：当 `min_working_tokens` 配置生效时，`ContextPacker` 现在会优先为 working segment 预留预算，再决定是否挤掉较低优先级 segment**
- **新增红转绿测试：`pack_reserves_minimum_working_tokens_before_lower_priority_segments`，验证 working 保底预算会先于 lower-priority memory 生效**
- **顺手修掉一个隐藏重复上报问题：`budget_exceeded_reasons` 不再把 `min_working_tokens_unmet` 重复写两次，`runtime_report_tests` 已重新全绿**
- **CLI explain 已补一层显式输出：新增 `context_working_reservation:` 字段，当前只要 pack 过程中出现 `budget_limit` 型挤出，就会显示 `working_budget_reserved`**
- **新增验证：`agent_runtime_tests.rs` 补了 `agent_runtime_exposes_working_reservation_reason_when_memory_is_dropped`，`cli_smoke_tests.rs` 补了 `context_working_reservation:` 断言**
- 小创刚重新实测：当前仓库 `cargo test` **再次全绿**
- **working reservation 已从“启发式提示”升级为正式结构化原因：`WorkingReservationReason::MinimumWorkingTokens` 已落地到 `context_engine`，并贯通到 runtime / CLI / runtime_report / subagent_report**
- **新增验证：`context_engine_tests.rs`、`agent_runtime_tests.rs`、`runtime_report_tests.rs`、`subagent_report_tests.rs` 已补 reason 断言，当前全绿**
- **注意：这版 `context_working_reservation` 还是启发式输出——它根据 drop reason 推断“发生过为 working 腾预算”，不是单独的结构化 reason enum；先够用，但下一版最好把这个原因从 packer 里显式产出**
- **下一步最值当：把 `working reservation` 从 CLI 字符串提示，升级成 runtime/context_debug 的正式字段或 reason 枚举，避免外层靠推断**
- 已通过 opencli + 可见 Chrome 向 DeepSeek 派发 DS-3（上下文引擎规格草案）并收回结果
- 新增外脑原文收档：`docs/deepseek-ds3-context-engine-spec.md`
- 新增小创收口版设计：`docs/context-engine-design-v1.md`
- 结论已收口：**当前主线优先级切到 context engine，不再继续扩 BrowserWorker 抢主线**
- BrowserWorker MVP 模块骨架已落下：`types / session / transcript / coordinator / adapters::deepseek_web`
- BrowserWorker session 最小闭环已落地：`new / apply_task / apply_receipt / apply_output`
- BrowserWorker transcript 最小闭环已落地：`BrowserTranscript::new / start_record / complete_record`
- BrowserWorker coordinator 最小闭环已落地：`enqueue / attach_receipt / attach_output`
- BrowserWorker adapter trait 最小闭环已落地：`BrowserWorkerAdapter + adapter_session / adapter_ensure_expert_mode / adapter_mark_ready`
- BrowserWorker 错误返回最小闭环已落地：`BrowserWorkerError` + coordinator/session 关键路径 `Result` 化
- BrowserWorker 真实稳定 hash 已落地：`src/browser_worker/hash.rs`，当前使用 FNV-1a 64-bit 十六进制输出
- BrowserWorker adapter 已新增最小 dispatch/read 抽象：`submit_task / read_output`
- 已引入 provider-facing 抽象：`BrowserProviderDriver`
- `DeepSeekWebAdapter` 已改为通过 provider driver 获取 `DispatchReceipt / WorkerOutput`
- 当前默认底层 driver：`FakeBrowserProviderDriver`，用于可测试的 simulated workflow
- 新增最小可执行 demo/service：`src/browser_worker/service.rs`
- 已能跑通一条 simulated DeepSeek web workflow：expert mode -> ready -> enqueue -> submit -> read -> transcript -> completed
- **新增 real-browser bridge 最小闭环：`ProviderBackedRealBrowserDriver<D: RealBrowserDriver>` 现在会按 `EnsureMode -> OpenPage -> FocusComposer -> TypePrompt -> SubmitPrompt -> WaitForAssistantTurn -> CaptureOutput` 顺序执行真实浏览器命令序列，可作为 BrowserWorker 接 opencli/Chrome 的最小骨架**
- **新增测试：`tests/browser_worker_opencli_real_driver_tests.rs`，验证 BrowserWorker service 可通过 real browser driver 跑通一条 opencli 风格 workflow，并保留 `opencli://...` snapshot anchor**
- **新增 real-browser 证据保留闭环：`BrowserTranscriptRecord` 现在显式带 `raw_snapshot_ref`，opencli/真实浏览器抓到的 snapshot anchor 不会在 transcript 层丢失**
- **新增红转绿验证：`tests/browser_worker_opencli_real_driver_tests.rs` 先卡住 `run.record.raw_snapshot_ref`，随后补齐 transcript 层实现并带动 `deepseek_web_workflow_integration_tests.rs` 同步更新**
- BrowserWorker real-driver 现在会校验每一步 observation 形状：`EnsureMode/OpenPage/FocusComposer/TypePrompt/SubmitPrompt/WaitForAssistantTurn/CaptureOutput` 任一步返回错形状都会立刻失败，不再默默吞掉
- 新增错误类型：`BrowserWorkerError::UnexpectedBrowserObservation { command, observation }`
- 新增测试：`provider_backed_real_driver_rejects_unexpected_browser_observation`，已实测通过
- 已实际验证本机 opencli bridge 可用：`opencli doctor` 显示 Extension connected；`opencli browser state` 当前可读 `about:blank`
- 刚实测：`cargo test --test browser_worker_opencli_real_driver_tests -- --nocapture` 通过（2/2）
- **新增真实 opencli driver 最小实现：`src/browser_worker/opencli_driver.rs`**
- **新增 `OpenCliRunner / SystemOpenCliRunner / OpenCliRealBrowserDriver`，已能把 `RealBrowserCommand` 映射到真实 `opencli browser ...` 命令**
- **新增测试：`tests/opencli_real_browser_driver_tests.rs`，已实测通过 3 条：state evidence、open page、command failure**
- **真实 opencli driver当前已保留 browser state 原始 stdout 作为 output content，并生成 `opencli://state/<prompt_hash>` snapshot anchor**
- BrowserWorker service 已收口为“以 driver 输出为准”，不再在 service 层二次覆盖 output；这让 fake / injected driver / future opencli driver 的行为边界更清晰
- 新增/更新测试：
  - `browser_worker_adapter_trait_tests.rs` 扩到 5 条
  - `browser_worker_coordinator_tests.rs` 扩到 8 条
  - `browser_worker_service_demo_tests.rs` 保持 2 条并已按 driver-first 语义更新
  - `deepseek_web_workflow_integration_tests.rs` 保持 2 条
  - `browser_worker_runtime_integration_tests.rs` 保持 3 条
  - `browser_worker_opencli_real_driver_tests.rs` 新增 1 条
  - session 单测维持 7 条
- 小创刚重新实测：当前仓库 `cargo test` **全绿**

## 2026-05-01

### 小策接手审计
- 老爸明确创项目来源路线：Codex CLI 取 Core Loop / SQ-EQ，Hermes 取 MemoryStore，OpenClaw 取子代理执行层，GenericAgent 取技能进化机制。
- 已做只读来源项目初审：
  - Codex CLI：本机 `codex-cli 0.125.0`，npm 包为单二进制；结合本机 app-server JSON-RPC bridge 与官方协议文档抽取 Submission/Event 思路。
  - Hermes Agent：确认 `MemoryStore` 硬上限、双文件、冻结快照、文件锁、原子写入、风险扫描机制。
  - OpenClaw：从 npm `dist` 中确认 subagent spawn / registry / announce / depth / isolated-or-fork context 等机制。
  - GenericAgent：确认极简 agent loop、L1-L4 记忆、No Execution No Memory、自进化 SOP 与 subagent SOP。
- 新增审计文档：`docs/source-project-audit-v1.md`
- 当前结论：创项目不是拼接四套系统，而是抽象成统一身份、记忆、执行、风险、进化五个协议。
- 老爸补充目标：GenericAgent 不只提供进化思想，还提供人类级桌面操作工具；创项目目标应包含 `Actuation Layer`，让 Agent 能打开软件、读屏、输入、发消息、操作真实登录态，同时通过治理层约束验证码、发送、支付、删除、密钥等高风险动作。
- 新增总蓝图：`docs/blueprint-v1.md`
  - 定义创项目为“本地智能体操作系统”，不是聊天机器人。
  - 固定本体论：记忆才是本体，Agent 只是壳。
  - 收敛八层架构：Identity / Memory / Core Loop / Context / Execution / Governance / Evolution / Interface。
  - 收敛七类协议：身份、记忆、执行、子代理、桌面操作、风险、进化。
  - 收敛 V0.1-V0.5 路线：工程闭环 -> 记忆本体 -> 子代理执行 -> 桌面操作 -> 外脑和进化。
- 老爸强调可插拔设计极其重要，要求最大程度解耦。
- 新增可插拔架构文档：`docs/pluggable-architecture-v1.md`
  - 固定原则：接口优先，内核只认协议，不认具体实现。
  - 定义核心插槽：Provider / MemoryStore / ContextEngine / SubagentSpawner / Actuator / Governance / SkillEvolver。
  - 要求每个插槽有 Fake 实现、contract tests、错误测试、序列化测试、配置选择测试。
- `docs/blueprint-v1.md` 已同步“接口优先、最大解耦”原则。
- 新增项目级规则：`AGENTS.md`
  - 固化创项目本体论、来源项目定位、可插拔工程规则、风险规则、记忆规则和进度规则。
- 新增可插拔插槽最小代码：
  - `src/governance.rs`：`Governance` trait、`StaticRuleGovernance`、`RiskDecision`、`ProposedAction`。
  - `src/actuator.rs`：`Actuator` trait、桌面操作相关 request/target/evidence 类型、`FakeActuator`。
  - `src/lib.rs` 已导出 `governance` 和 `actuator` 模块。
- 新增 contract tests：
  - `tests/governance_tests.rs`：覆盖低风险允许、高风险需确认、空 target 阻断、审计记录。
  - `tests/actuator_tests.rs`：覆盖 Fake 执行序列、证据引用、secret 输入不记录明文。
- 已运行 `cargo fmt`。
- 已运行 `cargo test`，当前全仓测试通过。
- 已提交并推送到 GitHub：
  - `1c34fdd feat: add pluggable agent runtime foundations`
- 新增运行时配置主线：
  - `src/runtime_config.rs`：`RuntimeConfig / ProviderConfig / OpenAICompatibleConfig / ConfigSummary / ConfigError`
  - 默认 provider 明确为 fake，不静默联网。
  - OpenAI-compatible provider 配置可构建 adapter，但 summary 只显示 `api_key=<set>` 语义，不泄露明文。
  - CLI 已从直接拼 provider 参数改为先收口到 `RuntimeConfig`，再进入 runtime。
- 新增测试：
  - `tests/runtime_config_tests.rs`，覆盖默认 fake provider、zero recall 拒绝、OpenAI-compatible 配置校验、adapter 构建、API key 脱敏 summary。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 已提交并推送到 GitHub：
  - `1168d53 feat: add runtime config slot selection`
- 新增进化层最小插槽：
  - `src/skill_evolver.rs`：`SkillEvolver` trait、`RuntimeEvent`、`EvolutionScope`、`SkillProposal`、`ValidationReport`、`NoopEvolver`。
  - `NoopEvolver` 当前只记录观察事件，不生成、不验证通过、不固化技能，避免早期自动写入技能造成失控。
- 新增测试：
  - `tests/skill_evolver_tests.rs`，覆盖事件记录、空 proposal、非法 scope、proposal shape 校验、拒绝固化、非法事件拒绝。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 新增子代理调度层最小插槽：
  - `src/subagent_spawner.rs`：`SubagentSpawner` trait、`SpawnRequest`、`SpawnReceipt`、`RunId`、`SubagentToolPolicy`、`ContextIsolation`、`FakeSubagentSpawner`。
  - Fake spawner 当前支持 `spawn / steer / kill / collect`，并通过 `SubagentReport` 返回结构化报告。
  - 已明确 Analyze 策略不能开启递归 spawn，保留老爸要求的安全闸门。
  - 已保留 isolated/forked 两种上下文隔离形状，但不把 parent context payload 塞进 Fake 运行态，避免上下文边界混淆。
- 新增测试：
  - `tests/subagent_spawner_tests.rs`，覆盖 isolated spawn、fork context budget、Analyze 递归拒绝、steer 记录、collect report、kill 后禁止 steer。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 运行时配置已扩成多插槽 summary：
  - `RuntimeConfig` 新增 `governance / actuator / subagent / evolution` 配置字段。
  - 当前可用 kind：`static_rule / fake / fake / noop`，不声明未实现 adapter，避免 silent fallback。
  - `ConfigSummary` 现在能暴露 provider/governance/actuator/subagent/evolution 全部 slot kind，供后续桌面控制台或飞书控制面板读取。
  - Governance 当前没有 disabled 变体，保持“治理不可拔掉”的硬约束。
- 新增/更新测试：
  - `runtime_config_tests` 扩到 6 条，新增 all-slot summary 断言。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 新增 slot registry 装配层：
  - `src/slot_registry.rs`：`build_runtime_slots()` 和 `summarize_runtime_slots()`。
  - 当前可从 `RuntimeConfig` 构造 `StaticRuleGovernance / FakeActuator / FakeSubagentSpawner / NoopEvolver`。
  - 这一步把“配置声明了插槽”推进到“配置能装配当前实现”，后续新增 adapter 时只扩 registry，不污染 runtime 主链路。
- 新增测试：
  - `tests/slot_registry_tests.rs`，覆盖四个当前 slot 的构造、FakeSubagentSpawner spawn/collect、summary 与 config kind 一致。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 新增控制面板协议层：
  - `src/control_plane.rs`：`ControlPlane` trait、`ManagedUnit`、`ControlAction`、`ControlRequest`、`ControlReceipt`、`FakeControlPlane`。
  - 统一服务/Agent 的 `Start / Stop / Restart / ChangeModel` 操作语义。
  - 默认 fake units 已显式列出：小创、小承、小云、小策、`codex-feishu-bot.service`，并用 metadata 保留 channel/manager 区分，避免混用通道。
  - `ChangeModel` 只允许 Agent，不允许 Service。
- 新增测试：
  - `tests/control_plane_tests.rs`，覆盖默认本地单位、start/stop/restart、agent-only model switch、unknown unit 和空 reason 拒绝。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 控制面板已接入治理前置链路：
  - `proposed_action_for_control()` 可把 `ControlRequest + ManagedUnit` 转成 `Governance` 可分类的 `ProposedAction`。
  - 控制类操作统一映射为 `ActionKind::ServiceChange`，当前静态治理会判定为 `NeedsApproval`。
  - 这保证后续桌面按钮执行 start/stop/restart/change model 前，能先走同一套审批链路。
- 更新测试：
  - `control_plane_tests` 扩到 5 条，新增 control request -> governance classify 断言。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 控制面板已纳入 runtime 配置和 slot registry：
  - `RuntimeConfig` 新增 `control_plane: ControlPlaneConfig`，默认 `fake_local`。
  - `ConfigSummary` 和 `RuntimeSlotsSummary` 都新增 `control_plane` kind。
  - `RuntimeSlots` 现在包含 `FakeControlPlane`，后续桌面控制台可直接从 slots 获取默认本地服务/Agent 列表。
- 更新测试：
  - `runtime_config_tests` 和 `slot_registry_tests` 已覆盖 `control_plane=fake_local` summary 与装配。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- CLI 已新增控制面板最小入口：
  - `cargo run -- control list`：列出默认本地 Agent/服务。
  - `cargo run -- control apply --unit ID --action start|stop|restart|change-model --reason TEXT [--model MODEL] [--approve]`。
  - `control apply` 会先通过 `proposed_action_for_control()` 转成治理动作，再由 `Governance` 分类。
  - 对 `NeedsApproval` 的控制动作，必须显式传 `--approve` 才会执行。
- 新增测试：
  - `tests/cli_control_tests.rs`，覆盖 list 输出、未审批拦截、显式审批后 change-model 执行。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 控制操作已补审计记录链路：
  - `audit_record_for_control()` 可把 `ManagedUnit + ControlRequest + approved` 转成统一 `AuditRecord`。
  - CLI `control apply` 在治理允许/显式审批后，会调用 `Governance::audit()` 记录控制操作。
  - CLI 成功执行后输出 `control_audit: recorded`。
- 更新测试：
  - `control_plane_tests` 扩到 6 条，新增 control audit record 断言。
  - `cli_control_tests` 已验证审批执行后输出审计记录提示。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 新增控制工作流抽象：
  - `src/control_workflow.rs`：`run_control_workflow()`、`ControlWorkflowRequest`、`ControlWorkflowResult`、`ControlWorkflowError`。
  - 统一封装：查 unit -> 构造 ProposedAction -> Governance classify -> approval gate -> audit -> ControlPlane apply。
  - CLI `control apply` 已改成调用 `run_control_workflow()`，飞书和桌面控制台后续可复用同一条链路。
- 新增测试：
  - `tests/control_workflow_tests.rs`，覆盖未审批拒绝、审批后执行并审计、未知 unit 在治理前失败。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 控制工作流新增结构化视图：
  - `ControlWorkflowView` 暴露 `unit_id / display_name / decision / action / previous_status / next_status / model_name / audit_recorded`。
  - `run_control_workflow()` 成功时返回 `view`，审批拒绝路径可用 `build_decision_view()` 渲染。
  - CLI 已改为渲染 `ControlWorkflowView`，避免 CLI/飞书/桌面 UI 各自拼控制状态字符串。
- 更新测试：
  - `control_workflow_tests` 已覆盖 view 字段。
  - `cli_control_tests` 继续验证审批、执行、审计输出。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- 控制列表也改为结构化视图：
  - 新增 `ControlUnitView`、`build_unit_view()`、`build_unit_views()`。
  - `control list` 改为渲染 `ControlUnitView`，字段包含 `unit_id / display_name / kind / status / model_name / channel`。
  - 列表页和操作页现在都不直接依赖内部 `ManagedUnit` 细节，方便后续桌面 UI / 飞书消息复用。
- 更新测试：
  - `control_workflow_tests` 扩到 4 条，新增默认本地 units -> view 断言。
- 已再次运行 `cargo fmt`。
- 已再次运行 `cargo test`，当前全仓测试通过。
- CLI 控制命令错误信息已收窄：
  - 缺少 `--unit / --action / --reason` 时返回明确字段错误，不再只吐通用 usage。
  - `--unit / --action / --reason / --model` 后缺值时返回明确缺值错误。
  - 不支持的 action 会返回 `unsupported control action: <action>`。
- 更新测试：
  - `cli_control_tests` 新增缺少 action 和不支持 action 的错误提示断言。
- 已运行 `cargo test --test cli_control_tests`，当前专项测试通过。
- 控制 CLI 新增 JSON 输出模式：
  - `cargo run -- control list --json` 输出 `ControlUnitView[]`。
  - `cargo run -- control apply ... --json` 输出 `ControlWorkflowView`。
  - 文本输出保持兼容，JSON 输出复用同一份 view，避免飞书/桌面 UI 另起一套拼接逻辑。
- 更新测试：
  - `cli_control_tests` 新增 list JSON 和 apply JSON 断言。
- 已运行 `cargo test --test cli_control_tests`，当前 7 条专项测试通过。
- 新增控制意图解析模块：
  - `src/control_intent.rs`：`ControlIntentInput -> ControlRequest`。
  - 支持 CLI 风格 action，也支持飞书/按钮更适合的中文别名：启动、关闭、停止、重启、换模型、切模型。
  - 仍然保持无静默 fallback：缺字段、缺模型、不支持 action 都返回结构化错误。
  - CLI `control apply` 已改为复用该模块，后续飞书和桌面控制台可绕开命令行字符串，直接构造 intent。
- 新增测试：
  - `tests/control_intent_tests.rs` 覆盖英文 action、中文别名、缺字段、缺模型、不支持 action。
- 已运行 `cargo test --test control_intent_tests --test cli_control_tests`，当前专项测试通过。
- 控制意图解析继续补人类友好 unit 解析：
  - `resolve_control_unit_id()` 可把 `unit_id` 或显示名解析成唯一 `unit_id`。
  - 当前支持从“小策”解析到 `codex-xiaoce`，也保留直接传 `codex-feishu-bot.service` 的路径。
  - 未知 unit 和歧义 unit 返回结构化错误，不做猜测。
- 更新测试：
  - `control_intent_tests` 扩到 8 条，新增显示名解析、unit_id 解析、未知 unit 断言。
- 已运行 `cargo test --test control_intent_tests --test cli_control_tests`，当前专项测试通过。
- 新增最小 MVP 控制台入口层：
  - `src/control_surface.rs`：`list_control_surface_units()` 和 `run_control_surface_intent()`。
  - 该层把飞书/桌面/CLI 的人类输入统一成 `ControlIntentInput`，支持显示名解析，再进入既有 `ControlWorkflow`。
  - 仍然不做真实 systemd/Agent 变更，当前只走 fake control plane，保证 MVP 协议先稳定。
  - CLI `control list/apply` 已切到 `control_surface`，终端、飞书、桌面后续可共用同一条路径。
- 新增/更新测试：
  - `tests/control_surface_tests.rs` 覆盖 UI-ready list、显示名重启需审批、已审批换模型、未知显示名治理前失败。
  - `cli_control_tests` 扩到 8 条，新增 `--unit 小策 --action 重启` 的人类入口回归。
- 已运行 `cargo test --test control_surface_tests --test cli_control_tests`，当前专项测试通过。
- 控制台 MVP 入口补飞书友好结果信封：
  - 新增 `ControlSurfaceOutcome { status, view }`，状态当前为 `applied / needs_approval / rejected`。
  - 新增 `run_control_surface_outcome()`，审批缺失不再必须作为错误给 UI 处理，而是返回 `needs_approval + ControlWorkflowView`。
  - 真错误仍然保留结构化 `ControlSurfaceError`，未知对象、无效 intent、底层 workflow 失败不会静默吞掉。
- 更新测试：
  - `control_surface_tests` 扩到 6 条，新增 `needs_approval` outcome 和已审批 `applied` outcome。
- 已运行 `cargo test --test control_surface_tests`，当前专项测试通过。
- 从飞书适配转回创项目核心 MVP：
  - 新增 `src/chuang_kernel.rs`，定义 `ChuangKernel`、`ChuangKernelConfig`、`ChuangKernelTurn`、`ChuangKernelSnapshot`。
  - `ChuangKernel::run_turn()` 现在把 `AgentRuntime` 的记忆检索、上下文打包、responder 调用和 `SubagentReport` 审计报告连成一条最小主链路。
  - `snapshot()` 暴露 agent_id、turn_count、recall_limit、metadata keys、context budget，用于未来桌面/插件查看核心状态。
  - 失败请求不会推进 turn_count，避免错误轮次污染最小运行状态。
- 新增测试：
  - `tests/chuang_kernel_tests.rs` 覆盖最小可审计 turn、健康快照、失败不计轮次。
- 已运行 `cargo test --test chuang_kernel_tests`，当前专项测试通过。
- CLI 主运行入口已接入 `ChuangKernel`：
  - `cargo run -- run ...` 现在通过 `ChuangKernel::run_turn()` 执行，再复用原有 `RuntimeResult` 输出。
  - OpenAI-compatible provider 路径也通过 `ChuangKernel::with_responder()`，不再绕过 MVP 内核。
  - 文本输出保持兼容，已有 CLI 冒烟测试无需改断言。
- 已运行 `cargo test --test cli_smoke_tests --test cli_provider_smoke_tests --test chuang_kernel_tests`，当前专项测试通过。
- 内核新增最小记忆写入闭环：
  - `AgentRuntime::memory_store_mut()` 和 `MemoryRecallPipeline::store_mut()` 暴露受控写入口。
  - `ChuangKernelTurn` 新增 `user_input`，用于生成可读 turn 摘要。
  - `ChuangKernel::remember_turn()` 将执行后的 turn 写成普通 `turn_summary` 记忆，metadata 包含 `kind / agent_id / turn_id`。
  - 该能力只追加普通记忆，不删除、不压缩、不改身份记忆。
- 更新测试：
  - `chuang_kernel_tests` 扩到 4 条，验证写入后的 turn 摘要可在下一轮 recall 命中。
- 已运行 `cargo test --test chuang_kernel_tests`，当前专项测试通过。
- CLI 新增最小记忆写回开关：
  - `cargo run -- run ... --remember` 会在本轮执行成功后调用 `ChuangKernel::remember_turn()`。
  - 成功写入时输出 `memory_recorded: <record_id>`。
  - 默认不写回，避免普通调试污染记忆库。
- 更新测试：
  - `cli_smoke_tests` 新增端到端写回验证：第一次 `--remember` 写入，同 DB 第二次查询可 recall 到该 turn summary。
- 已运行 `cargo test --test cli_smoke_tests --test chuang_kernel_tests`，当前专项测试通过。
- 新增核心 MVP 状态入口：
  - `src/kernel_status.rs`：`ChuangMvpStatus { config, slots, kernel }`。
  - 状态视图聚合 `RuntimeConfig::summary()`、`RuntimeSlotsSummary`、`ChuangKernelSnapshot`。
  - `ConfigSummary / RuntimeSlotsSummary / ChuangKernelSnapshot` 已支持 `Serialize`，方便 CLI/桌面/插件复用。
  - CLI 新增 `cargo run -- status` 和 `cargo run -- status --json`。
  - JSON 状态只暴露 `api_key_state=<set>`，不输出密钥明文。
- 新增测试：
  - `tests/kernel_status_tests.rs` 覆盖状态聚合与无效配置拒绝。
  - `tests/cli_status_tests.rs` 覆盖文本状态、JSON 状态和密钥不泄露。
- 已运行 `cargo test --test kernel_status_tests --test cli_status_tests`，当前专项测试通过。
- 新增 MVP 边界文档：
  - `docs/mvp-scope.md` 明确当前最小闭环、已具备能力、当前不做事项、下一步优先级和 MVP 可用判定标准。
  - 重点约束：飞书只作为未来插件入口，不再作为核心主线；真实服务/Agent 控制仍保持 adapter 化和审批化。
- 内核记忆写入新增最小硬上限策略：
  - `ChuangKernelConfig` 新增 `memory_write_max_chars`。
  - `ChuangKernelSnapshot` 暴露当前写入上限。
  - `ChuangKernel::remember_turn()` 写入前检查 turn summary 字符数，超限返回 `ChuangKernelMemoryError::HardLimitExceeded`。
  - 超限错误包含 `limit_chars / attempted_chars / existing_record_ids`，给后续模型自主压缩或人工处理留接口。
  - 仍然不自动删除、不自动压缩、不改身份记忆。
- 更新测试：
  - `chuang_kernel_tests` 扩到 5 条，新增硬上限拒绝写入，并确认下一轮 recall 不会命中失败写入。
  - `cli_smoke_tests / kernel_status_tests` 已同步适配内核配置。
- 已运行 `cargo test --test chuang_kernel_tests --test cli_smoke_tests --test kernel_status_tests`，当前专项测试通过。
- CLI 记忆写入超限错误已明确化：
  - `--remember` 触发硬上限时输出 `memory_write_hard_limit_exceeded`。
  - 错误信息包含 `limit_chars / attempted_chars / existing_record_ids`。
  - 普通 store 错误仍输出 `memory_write_failed`。
- 更新测试：
  - `cli_smoke_tests` 扩到 4 条，新增超长输入触发硬上限错误断言。
- 已运行 `cargo test --test cli_smoke_tests`，当前专项测试通过。
- 内核记忆写入默认值收口：
  - 新增 `DEFAULT_MEMORY_WRITE_MAX_CHARS = 2200`。
  - 新增 `ChuangKernelConfig::mvp_default(agent_id)`，默认启用 2200 字符写入硬上限。
  - CLI 和测试不再各自硬编码 2200，统一引用内核常量。
- 更新测试：
  - `chuang_kernel_tests` 扩到 6 条，新增 MVP 默认配置断言。
- 已运行 `cargo test --test chuang_kernel_tests --test kernel_status_tests --test cli_smoke_tests`，当前专项测试通过。
- 硬上限错误扩展现有记忆条目视图：
  - 新增 `MemoryEntryView { id, content_preview, chars }`。
  - `ChuangKernelMemoryError::HardLimitExceeded` 从 `existing_record_ids` 升级为 `existing_entries`。
  - 超限时返回已有 turn summary 的轻量预览，后续可交给模型或人工决定如何压缩，而不是自动删除。
  - CLI 超限错误同步输出 `existing_entries=id:chars` 概要。
- 更新测试：
  - `chuang_kernel_tests` 验证超限错误包含已有条目的 ID、预览和字符数。
  - `cli_smoke_tests` 验证 CLI 超限错误包含 `existing_entries` 字段。
- 已运行 `cargo test --test chuang_kernel_tests --test cli_smoke_tests`，当前专项测试通过。
- 硬上限策略抽成可复用记忆准入模块：
  - 新增 `src/memory_admission.rs`。
  - 定义 `TextMemoryAdmission`、`TextMemoryAdmissionDecision`、`MemoryEntryView`、`DEFAULT_MEMORY_WRITE_MAX_CHARS`。
  - `ChuangKernel::remember_turn()` 改为调用 `TextMemoryAdmission`，不再把字符上限判断写死在内核方法里。
  - `preview_chars()` 也移入准入模块，后续 USER/MEMORY 双文件可复用。
- 新增测试：
  - `tests/memory_admission_tests.rs` 覆盖准入成功、超限拒绝、字符预览截断和默认上限。
- 已运行 `cargo test --test memory_admission_tests --test chuang_kernel_tests --test cli_smoke_tests`，当前专项测试通过。
- 新增 Hermes 风格双文件记忆 MVP：
  - `src/hermes_memory.rs` 定义 `DualFileMemoryStore` trait 和 `FileDualFileMemoryStore` 文件实现。
  - 默认文件为 `USER.md / MEMORY.md`，默认硬上限为 `1375 / 2200` 字符。
  - `write_user()` 只在整份 USER 文本未超限时原子写入。
  - `append_memory()` 以 `## id` 条目追加 MEMORY，重复 ID 拒绝。
  - 超限时返回 `HardLimitExceeded`，包含 scope、limit、attempted chars 和现有条目预览；不删除、不压缩、不改写现有文件。
  - `snapshot()` 返回 USER/MEMORY 双文件的会话快照，为后续“会话开始冻结快照”接内核做准备。
- 新增测试：
  - `tests/hermes_memory_tests.rs` 覆盖文件创建、快照读取、USER 超限不变更、MEMORY 追加超限不变更、重复 ID 拒绝、默认上限。
- 已运行 `cargo test --test hermes_memory_tests --test memory_admission_tests`，当前专项测试通过。
- 运行时配置新增 identity memory 插槽：
  - `RuntimeConfig` 新增 `identity_memory: IdentityMemoryConfig`。
  - 当前默认实现为 `hermes_dual_file`，root 为 `./data/hermes-memory`，上限沿用 `USER=1375 / MEMORY=2200`。
  - `IdentityMemoryConfig::build_dual_file_config()` 可生成 `DualFileMemoryConfig`，后续内核接入时不需要硬编码文件路径或上限。
  - `ConfigSummary` 和 CLI `status` 已暴露 identity memory kind/root/limits。
- 更新测试：
  - `runtime_config_tests` 新增 identity memory 配置构建和零上限拒绝。
  - `cli_status_tests` 验证文本/JSON 状态能看到 `hermes_dual_file` 且不泄露密钥。
- 已运行 `cargo test --test runtime_config_tests --test kernel_status_tests --test cli_status_tests --test hermes_memory_tests`，当前专项测试通过。
- 内核已接入 identity snapshot 上下文接缝：
  - `RuntimeRequest` 新增 `extra_context_segments`，用于接收非 recall 来源的上下文。
  - `SegmentSource` 新增 `Identity`，避免身份记忆和普通检索记忆混淆。
  - `ChuangKernelConfig` 新增 `identity_snapshot: Option<DualFileMemorySnapshot>`。
  - `ChuangKernel::run_turn()` 会把 USER/MEMORY 快照转为 `identity-user / identity-memory` segments 注入 runtime prompt。
  - `ChuangKernelSnapshot` 暴露 `identity_user_chars / identity_memory_chars`，用于确认本轮内核是否带着冻结记忆启动。
- 新增/更新测试：
  - `agent_runtime_tests` 新增 extra identity context packing 断言。
  - `chuang_kernel_tests` 新增 identity snapshot 注入 prompt 和 snapshot 字符数断言。
- 已运行 `cargo test --test agent_runtime_tests --test chuang_kernel_tests --test kernel_status_tests --test cli_smoke_tests --test runtime_report_tests --test agent_runtime_sqlite_tests`，当前专项测试通过。
- CLI 启动路径已读取 identity memory 快照：
  - `kernel_config_from_runtime()` 现在从 `RuntimeConfig.identity_memory` 构造 `DualFileMemoryConfig`。
  - CLI `run / repl / status` 进入内核前打开 `FileDualFileMemoryStore`，读取一次 `USER.md / MEMORY.md` snapshot。
  - 读取失败会返回 `identity_memory_open_failed` 或 `identity_memory_snapshot_failed`，不静默降级。
  - CLI status 文本现在显示 `identity_snapshot_chars`，默认空文件为 `user=0 memory=0`。
- 已运行 `cargo test --test cli_status_tests --test cli_smoke_tests --test cli_provider_smoke_tests --test cli_repl_default_transport_tests`，当前专项测试通过。
- CLI 新增 identity memory root 配置：
  - `run / repl / status` 支持 `--identity-memory-root PATH`。
  - 该参数会覆盖 `RuntimeConfig.identity_memory` 的 root，仍保留 Hermes 默认上限。
  - 这让测试、桌面控制台、未来不同 Agent 身份目录可以用不同 USER/MEMORY 文件，不再被默认 `./data/hermes-memory` 绑死。
- 更新测试：
  - `cli_status_tests` 新增临时 identity root，预写 `USER.md / MEMORY.md` 后验证 JSON status 暴露 snapshot 字符数。
- 已运行 `cargo test --test cli_status_tests --test cli_smoke_tests --test runtime_config_tests`，当前专项测试通过。
- CLI 新增显式身份热记忆写入：
  - 新增 `--remember-identity`，运行成功后会把本轮 turn summary 追加到 `MEMORY.md`。
  - 该能力和 `--remember` 分离：`--remember` 仍只写 SQLite 普通 recall，`--remember-identity` 才写 Hermes 双文件热记忆。
  - 写入 ID 形如 `identity-turn-1-<pid>-<nanos>`，避免 CLI 每次都是 `turn-1` 时发生重复。
  - 写入失败会明确返回 `identity_memory_write_failed / identity_memory_duplicate_entry / identity_memory_hard_limit_exceeded`。
  - 默认不写，仍然不删除、不压缩、不改 `USER.md`。
- 更新测试：
  - `cli_smoke_tests` 新增 `--remember-identity + --identity-memory-root` 端到端验证，确认 `MEMORY.md` 被追加。
  - `docs/mvp-scope.md` 已同步 CLI 能力边界。
- 已运行 `cargo test --test cli_smoke_tests --test cli_status_tests`，当前专项测试通过。
- 双文件热记忆条目解析补强：
  - `MEMORY.md` 中位于第一个 `## id` 前的自由文本现在会作为 `MEMORY.md:preamble` 返回。
  - 这样手写热记忆或旧格式内容在追加超限时也会出现在 `existing_entries` 中，便于后续模型/人工决定如何压缩。
  - 仍然不自动删除、不自动压缩、不改写原文件。
- 更新测试：
  - `hermes_memory_tests` 新增自由文本 preamble 超限拒绝断言。
- 已运行 `cargo test --test hermes_memory_tests --test cli_smoke_tests`，当前专项测试通过。
- 子代理调度层新增 queued external 协议：
  - `src/subagent_spawner.rs` 新增 `SubagentDispatch`，用于把 spawn 请求打包成外部 runner 可消费的派发信封。
  - 新增 `QueuedSubagentSpawner`：`spawn()` 只入队 dispatch，`collect()` 在 report 回填前返回 `None`，不伪装已完成。
  - 新增 `take_next_dispatch()` / `pending_dispatches()` / `attach_report()`，让未来真实 Codex/OpenClaw 子代理或外部进程可以按协议接任务、回传 report。
  - 新增 `SubagentSlot` 包装 `Fake / Queued`，slot registry 可以在不改上层调用方式的情况下替换子代理实现。
  - `RuntimeConfig::SubagentConfig` 新增 `QueuedExternal`，summary kind 为 `queued_external`。
  - 默认仍然是 `fake`，不执行真实命令、不打开危险能力。
- 更新测试：
  - `subagent_spawner_tests` 新增 queued dispatch、report 回填、身份不匹配拒绝、kill 移除 pending dispatch。
  - `slot_registry_tests` 新增 queued external slot 构建和 collect 返回 None。
  - `runtime_config_tests` 新增 queued subagent kind summary。
- 已运行 `cargo test --test subagent_spawner_tests --test slot_registry_tests --test runtime_config_tests`，当前专项测试通过。
- CLI 新增子代理槽位选择：
  - `run / repl / status` 支持 `--subagent fake|queued_external`。
  - `status --json --subagent queued_external` 会在 config 和 slots summary 中显示 `queued_external`。
  - 默认仍为 `fake`。
- 更新测试：
  - `cli_status_tests` 新增 CLI 选择 queued external 子代理槽位断言。
- 已运行 `cargo test --test cli_status_tests --test runtime_config_tests --test slot_registry_tests`，当前专项测试通过。
- 子代理跨进程边界补序列化：
  - `TaskId / AgentId / ReportId / Timestamp / RunId` 新增 serde 透明序列化。
  - `SubagentDispatch / SpawnRequest / SpawnReceipt / SubagentToolPolicy / ContextIsolation / KillReason / SubagentState` 新增 `Serialize / Deserialize`。
  - `SubagentReport` 及其嵌套结构新增 `Serialize / Deserialize`。
  - 这让 queued external 后续可以落成 JSON 文件队列、IPC 消息或外部 runner 协议，而不是只能在内存里流转。
- 更新测试：
  - `subagent_spawner_tests` 新增 dispatch JSON roundtrip。
  - `subagent_report_tests` 新增 report JSON roundtrip。
- 已运行 `cargo test --test subagent_spawner_tests --test subagent_report_tests --test slot_registry_tests`，当前专项测试通过。
- 新增文件型子代理队列 adapter：
  - `src/subagent_queue.rs` 新增 `FileSubagentQueueConfig / FileSubagentQueue`。
  - `write_dispatch()` 会把 `SubagentDispatch` 原子写入 `dispatch/<run_id>.json`。
  - `read_report()` 从 `reports/<run_id>.json` 读取 `SubagentReport`，不存在时返回 `None`。
  - 当前 adapter 只负责文件协议，不启动外部进程、不执行任务。
- 新增测试：
  - `tests/subagent_queue_tests.rs` 覆盖 dispatch JSON 写入、缺失 report 返回 None、report JSON 读取。
- 已运行 `cargo test --test subagent_queue_tests --test subagent_spawner_tests`，当前专项测试通过。
- 文件型子代理队列补充 spawner 整合点：
  - `FileSubagentQueue::flush_pending_dispatches()` 可把 `QueuedSubagentSpawner::pending_dispatches()` 批量写入 `dispatch/<run_id>.json`。
  - 这一步仍不启动 runner，只建立主内核到外部 runner 的文件边界。
- 更新测试：
  - `subagent_queue_tests` 新增从 queued spawner flush 两个 dispatch 文件的断言。
- 已运行 `cargo test --test subagent_queue_tests --test subagent_spawner_tests --test subagent_report_tests`，当前专项测试通过。
- 文件型子代理队列补充 report 回填：
  - `FileSubagentQueue::attach_report_if_present()` 会读取 `reports/<run_id>.json`。
  - report 存在时调用 `QueuedSubagentSpawner::attach_report()`，缺失时返回 `false` 且保持 run 为 Running。
  - 这形成了文件队列半闭环：主内核写 dispatch，外部 runner 写 report，主内核再 attach/collect。
- 更新测试：
  - `subagent_queue_tests` 新增 report 存在时 attach 成功、report 缺失时返回 false。
- 已运行 `cargo test --test subagent_queue_tests --test subagent_spawner_tests`，当前专项测试通过。
- RuntimeConfig 新增子代理文件队列配置：
  - 新增 `SubagentQueueConfig { root }`，默认 `./data/subagent-queue`。
  - `ConfigSummary` 和 `status` 文本/JSON 现在会暴露 `subagent_queue_root`，让外部 runner 明确知道扫描 `dispatch/` 与写回 `reports/` 的根目录。
  - CLI 新增 `--subagent-queue-root PATH`，可在 `run / repl / status` 中覆盖队列目录。
  - 该配置只暴露文件队列边界，不启动外部进程、不执行真实子代理。
- 更新测试：
  - `runtime_config_tests` 覆盖默认队列目录、queued external 自定义目录、文件队列配置构造、空 root 拒绝。
  - `cli_status_tests` 覆盖文本 status 默认队列目录和 JSON status 自定义队列目录。
- 已运行 `cargo test --test runtime_config_tests --test cli_status_tests`，当前专项测试通过。
- CLI 新增子代理 dispatch 入口：
  - 新增 `subagent dispatch --task TEXT`，可把任务打包成 queued external dispatch JSON。
  - 支持 `--subagent-queue-root PATH / --task-id ID / --agent-name NAME / --policy analyze|execute|orchestrate / --token-budget N / --idle-timeout-ms MS / --fork-parent-tokens N / --json`。
  - 输出 `run_id / agent_id / task_id / dispatch_path / queue_root`，外部 runner 可扫描 `dispatch/<run_id>.json`。
  - 该命令只排队，不启动外部进程，不执行真实子代理。
- 新增测试：
  - `cli_subagent_dispatch_tests` 覆盖 JSON 输出、dispatch 文件内容、缺失 `--task` 拒绝。
- 已运行 `cargo test --test cli_subagent_dispatch_tests --test subagent_queue_tests --test runtime_config_tests`，当前专项测试通过。
- CLI 新增子代理 report 读取入口：
  - 新增 `subagent report --run-id ID`，会读取 `reports/<run_id>.json`。
  - JSON 输出包含 `run_id / available / report`；缺失 report 时返回 `available=false`，用于轮询，不把“还没回来”当错误。
  - 文本输出会显示 `subagent_report_available` 或 `subagent_report_missing`。
  - 该命令只读 report 文件，不删除、不移动、不 attach 到持久状态。
- 更新测试：
  - `cli_subagent_dispatch_tests` 新增 report 可读与缺失 report 两条 CLI 覆盖。
- 已运行 `cargo test --test cli_subagent_dispatch_tests --test subagent_queue_tests`，当前专项测试通过。
- RuntimeConfig / CLI 新增 context budget 可配置面：
  - `ConfigSummary` 现在暴露 `context_reserve_system_tokens / context_min_working_tokens / context_max_tool_results / context_max_memory_segments`。
  - CLI 新增 `--context-max-tokens / --context-reserve-system-tokens / --context-min-working-tokens / --context-max-tool-results / --context-max-memory-segments`，可用于 `run / repl / status / subagent` 的 runtime 配置解析。
  - `RuntimeConfig::validate()` 新增保护：`reserve_system_tokens` 不能超过 `max_tokens`。
  - `status` 文本新增完整 `context_budget` 行，JSON status 暴露全部字段。
  - `docs/mvp-scope.md` 已同步当前已具备能力和下一步优先级。
- 更新测试：
  - `runtime_config_tests` 新增 context reserve 越界拒绝。
  - `cli_status_tests` 新增 context budget CLI 覆盖。
- 已运行 `cargo test --test runtime_config_tests --test cli_status_tests --test cli_smoke_tests --test cli_subagent_dispatch_tests`，当前专项测试通过。
- Context engine 新增可插拔策略接口：
  - `src/context_engine.rs` 新增 `ContextEngine` trait。
  - 新增 `DeterministicContextEngine`，当前包装现有 `ContextPacker`，kind 为 `deterministic_budget`。
  - `AgentRuntime` 改为通过 `DeterministicContextEngine` 执行 pack，行为保持不变。
  - `RuntimeConfig` 新增 `ContextEngineConfig::DeterministicBudget`，`ConfigSummary` 和 `status` 暴露 `context_engine_kind`。
  - 这为后续摘要压缩、自适应策略、对话树策略预留替换点。
- 更新测试：
  - `context_engine_tests` 新增 deterministic engine trait 覆盖。
  - `runtime_config_tests` 新增 context engine kind 覆盖。
  - `cli_status_tests` 新增 status 输出 context engine kind 覆盖。
- 已运行 `cargo test --test context_engine_tests --test agent_runtime_tests --test runtime_config_tests --test cli_status_tests`，当前专项测试通过。
- 子代理 CLI 多任务派发修复：
  - `QueuedSubagentSpawner` 新增 `spawn_with_ids()`，允许调用方显式指定 `run_id / agent_id`。
  - CLI `subagent dispatch` 改为生成 `queued-cli-<pid>-<nanos>` 形式的全局唯一 run id。
  - 同一个 `--subagent-queue-root` 目录现在可以连续派发多个 dispatch，不会反复覆盖 `queued-run-1.json`。
  - 内存 spawner 默认 `spawn()` 仍保留顺序 `queued-run-N` 行为，已有测试和协议不变。
- 更新测试：
  - `subagent_spawner_tests` 新增显式 ID 派发和重复 run id 拒绝。
  - `cli_subagent_dispatch_tests` 新增同目录连续派发两个任务并保留两个 dispatch 文件。
- 已运行 `cargo test --test cli_subagent_dispatch_tests --test subagent_spawner_tests --test subagent_queue_tests`，当前专项测试通过。
- 子代理文件队列新增只读列表能力：
  - `FileSubagentQueue::list_dispatches()` 可读取 `dispatch/*.json` 并按 run id 排序。
  - `FileSubagentQueue::list_report_run_ids()` 可读取 `reports/*.json` 对应 run id。
  - CLI 新增 `subagent list [--json]`，输出 `dispatch_count / report_count / items[]`。
  - 每个 item 标出 `run_id / agent_id / task_id / agent_name / tool_policy / has_report`。
  - 该能力只读扫描目录，不删除、不移动、不 attach 状态。
- 更新测试：
  - `subagent_queue_tests` 新增 dispatch/report 列表与忽略非 JSON 文件。
  - `cli_subagent_dispatch_tests` 新增 list 命令识别两个 dispatch 且正确标记 report presence。
- 已运行 `cargo test --test cli_subagent_dispatch_tests --test subagent_queue_tests`，当前专项测试通过。
- 子代理文件队列新增 fake runner MVP：
  - `FileSubagentQueue` 新增通用 `write_report()`，`write_report_for_test()` 保留兼容测试。
  - CLI 新增 `subagent run-once [--runner fake] [--json]`。
  - fake runner 会只读找到第一个尚无 report 的 dispatch，并写入一个模拟成功 `SubagentReport`。
  - 没有 pending dispatch 时返回 idle，不报错。
  - 当前 runner 不执行 shell、不启动真实 agent，只用于端到端证明文件队列协议。
- 更新测试：
  - `cli_subagent_dispatch_tests` 新增 run-once 写 report 和 idle 两条覆盖。
- 已运行 `cargo test --test cli_subagent_dispatch_tests --test subagent_queue_tests`，当前专项测试通过。
- 新增简单配置文件支持：
  - 新增 `src/runtime_config_file.rs`，实现轻量 `config.toml` 解析，不引入复杂配置层。
  - CLI 新增 `--config PATH`，会先加载配置文件，再用 CLI 参数覆盖。
  - 配置文件支持顶层 `db_path / recall_limit / identity_memory_root / subagent / subagent_queue_root`。
  - 支持 `[provider]`：`kind=fake` 或 `kind=openai_compatible`；真实 key 使用 `api_key_env`，不要求明文写入配置。
  - 支持 `[context]`：`max_tokens / reserve_system_tokens / min_working_tokens / max_tool_results / max_memory_segments`。
  - 新增 `config.example.toml` 作为可维护模板。
- 更新测试：
  - `runtime_config_file_tests` 覆盖 fake 配置、OpenAI-compatible env key、缺失 env、非法行。
  - `cli_status_tests` 覆盖 `--config` 加载和 CLI 覆盖配置文件字段。
- 已运行 `cargo test --test runtime_config_file_tests --test cli_status_tests --test runtime_config_tests`，当前专项测试通过。
- provider native 继续前推到真 HTTPS：
  - `OpenAICompatibleConfig` 新增 `tls_ca_cert_path`，`status` / `config show` / runtime config / slot registry 已同步暴露。
  - `provider_openai_compatible` 的 `native` 传输现已支持 `https://`，可通过自定义 CA 证书路径建立本地 TLS 回归。
  - `openai_compatible_http_live_transport_tests` 新增本地 HTTPS 握手和成功返回回归，已补全动态生成 CA + server cert 的测试链路。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q`，当前全量测试通过。
- provider 稳定性与会话记忆继续前推：
  - OpenAI-compatible provider 成功/失败 metadata 新增 `provider_retryable`，失败时新增 `provider_error_class` 和 `provider_timeout_ms`，用于上层判断是否重试或切 fallback。
  - `run` 新增显式 `--session-id ID` 与 `--remember-session`；同一 session 会用 `session_id + memory_scope=session` 过滤 recall，并写入独立 session turn summary。
  - `app-server` 已使用 thread id 作为 session id，并默认写会话记忆；这只完善本体对话能力，不接入或修改任何飞书桥。
  - `--remember-session` 缺少 `--session-id` 会明确拒绝，避免写出无法归属的会话记忆。
  - 已运行 `cargo check`、`cargo test -q session`、`cargo test -q openai_compatible_curl_transport`，并最终通过 `cargo test -q` 与 `git diff --check`。
- 真实子代理 command runner 协议继续前推：
  - `subagent run-once --runner command` 现在支持真实 runner 在 stdout 输出完整 `SubagentReport` JSON。
  - Chuang 会校验 report 的 `schema_version / task_id / agent_id / parent_agent_id / summary`，身份一致才接纳并写入队列 report。
  - 协议报告解析失败或身份不匹配时，会写入 Failed report，`stderr_preview` 保留拒绝原因，避免坏 runner 被误当成功。
  - 普通 stdout/stderr 包装模式保持兼容；只有 stdout 明确像 `SubagentReport` JSON 时才走协议解析。
  - 已运行 `cargo check`、`cargo test -q --test cli_subagent_dispatch_tests`，当前专项测试通过。
- 子代理 worker loop MVP 已落地：
  - CLI 新增 `subagent run-loop [--max-runs N]`，默认最多处理 10 个 pending dispatch，不做无限常驻。
  - `run-loop` 复用 `run-once` 的 fake/command runner 和协议校验，逐个处理未生成 report 的 dispatch。
  - 队列清空会返回 `idle=true`，达到 `max_runs` 会停，便于后续接 systemd timer 或独立 worker 服务。
  - `--max-runs 0` 明确拒绝，避免配置错误导致不可预期行为。
  - 已运行 `cargo check`、`cargo test -q --test cli_subagent_dispatch_tests`，当前专项测试通过。
- 子代理队列领取锁 MVP 已落地：
  - `FileSubagentQueue` 新增 `claims/<run_id>.json` 状态文件，用 `create_new` 原子创建完成领取，避免多个 worker 同时处理同一 dispatch。
  - `run-once / run-loop` 会跳过已有 report 或已有 claim 的 dispatch；如果全都被领取则返回 idle。
  - `subagent list` 新增 `is_claimed` 字段，桌面控制台可以看到任务是否已被 worker 领取。
  - 该机制不删除 dispatch、不删除 claim、不移动文件，保持可追溯；后续再补显式过期/人工释放策略。
  - 已运行 `cargo check`、`cargo test -q --test subagent_queue_tests --test cli_subagent_dispatch_tests`，当前专项测试通过。
- 子代理 claim release MVP 已落地：
  - CLI 新增 `subagent release-claim --run-id ID --reason TEXT`，会写入 `claim-releases/<run_id>.json`。
  - release 不删除旧 claim；重新领取会覆盖 claim 文件，`is_claimed()` 按 claim/release 文件修改时间判断当前活跃状态。
  - 已覆盖释放后重新领取、CLI release 后继续 run-once 的回归。
  - 已运行 `cargo check`、`cargo test -q --test subagent_queue_tests --test cli_subagent_dispatch_tests`，当前专项测试通过。
- 版本默认配置去 fake 化：
  - 项目根 `config.toml` 已改为真实 `openai_compatible` provider + `native` transport + `queued_external` 子代理队列。
  - `config.example.toml` 已从 fake provider / fake subagent 改为 OpenAI-compatible + queued external 模板，真实密钥仍只通过 `api_key_env` 引用。
  - README / MVP 文档中的推荐子代理路径已从 fake runner 改为 command runner / run-loop。
  - `cargo run --quiet -- status --config config.toml` 已确认 provider 为 `openai_compatible`、model 为 `gpt-5.5`、subagent 为 `queued_external`。
  - 当前仍保留 `actuator=fake` 和 `control_plane=fake_local`，因为真实桌面操作面与服务控制 command adapter 尚未配置；不会伪装成已实现。
- 运行态 fake 可见性修正：
  - `ConfigSummary` 新增 `placeholder_warnings`，统一标出 `provider=fake`、`transport=stub`、`actuator=fake`、`subagent=fake`、`control_plane=fake_local` 等占位/测试配置。
  - `status`、`config check/show`、`doctor` 文本输出会打印 `placeholder_warning`；JSON 输出也包含同名数组，便于后续桌面控制台和飞书插件展示。
  - 当前项目根配置实测只剩 `actuator=fake` 与 `control_plane=fake_local` 两条 warning；对话 provider 和子代理 slot 已不是 fake。
  - 修正 `cli_smoke_tests` 中一个测试隔离问题：仓库根存在真实 `config.toml` 后，未显式指定 `--subagent fake` 的用例会被当前配置影响。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q --test cli_status_tests --test cli_config_tests --test cli_doctor_tests`、`cargo test -q --test cli_smoke_tests`、`cargo test -q`，当前全量通过。
- 最小身份启动层已落地：
  - 新增 `identity/SOUL.md`、`identity/STORY.md`、`identity/FIRST_WAKE.md`、`identity/agents.toml`，补上创自己的身份锚点、故事、首次醒来规则和 Agent 注册表。
  - `config.toml` / `config.example.toml` 新增 `identity_root / soul_path / story_path / first_wake_path / agents_registry_path`，保持简单扁平格式。
  - `RuntimeConfig` 新增 `IdentityBootstrapConfig`，配置摘要和 `status/config show` 会展示身份启动文件路径。
  - `kernel_config_from_runtime()` 会读取身份启动文件，缺失时按空内容处理，不静默伪装、不自动创建或删除。
  - `ChuangKernel` 会把 `FIRST_WAKE / SOUL / STORY / agents.toml` 作为 `SegmentSource::Identity` 注入本轮上下文，并在 snapshot/status 里暴露字符数。
  - 已运行 `cargo check`、`cargo test -q --test runtime_config_file_tests --test chuang_kernel_tests --test cli_status_tests --test kernel_status_tests`，当前专项通过。
- 项目当前情况报告已生成：
  - 新增 `docs/chuang-project-current-report-2026-05-02.md`，整理当前状态、配置文件、目录结构、模块职责、缺口和后续规划。
  - 已通过本机 `lark-cli docs +create --as user` 用当前已登录用户身份创建飞书云文档，文档链接：`https://www.feishu.cn/docx/ME3ddJocIolj2OxkTCGcLKWxnoh`。
  - 已把飞书文档操作规则写入 `AGENTS.md`：后续创项目报告优先使用本机 `lark-cli docs +create --as user`，不再先绕 OAuth；仍禁止输出 token/secret。
- Karpathy-style Markdown rules MVP 已接入治理层：
  - 新增 `rules/core.md`，用极简 Markdown 规则固化“先澄清、最小实现、精准修改、目标可验收、身份边界、密钥保护、禁止自行删除、插件槽位、真实标注”等核心原则。
  - `RuntimeConfig` 新增 `RulesConfig { root, core_path }`，`config.toml` / `config.example.toml` 新增 `rules_root` 和 `rules_core_path`。
  - 新增 `MarkdownRuleSet`，会加载 `rules/core.md`，拒绝空文件或没有规则条目的文件，并计算稳定 fingerprint。
  - `StaticRuleGovernance` 现在可持有规则集；slot 构建时加载规则，治理决策 reason 会附带 `rules=<fingerprint>`，便于追溯本轮使用的规则版本。
  - `status` / `config show` 会显示 rules 路径。
  - 已运行 `cargo check`、`cargo test -q --test governance_tests --test runtime_config_file_tests --test runtime_config_tests --test slot_registry_tests --test cli_status_tests --test cli_doctor_tests`。
  - 已冒烟 `cargo run --quiet -- run --config config.toml --input ...`，确认输出 `governance_reason: ... rules=07e555f2418cc032`；本次 provider 返回 401，属于本地 provider/key 状态问题，未触碰密钥。
- 安全自我实验 MVP 已接入：
  - 新增 `src/self_experiment.rs`，提供 `SelfExperimentPlanner` 和 `ExperimentRequest/ExperimentReceipt`。
  - CLI 新增 `experiment plan --goal TEXT --success TEXT [--time-budget-minutes N] [--root PATH] [--json]`。
  - 当前只生成 `experiment.md`，写入固定时间预算、目标、验收标准和安全约束；不执行外部命令、不创建分支、不回滚、不删除、不清理。
  - 实验计划明确禁止 `git reset --hard`、删除文件、清理队列/报告/记忆/凭证、purge/uninstall/destructive rollback。
  - 新增 `tests/self_experiment_tests.rs` 与 `tests/cli_experiment_tests.rs`，覆盖计划写入、CLI 输出和非法参数拒绝。
  - 已运行 `cargo fmt --all`、`cargo check`、`cargo test -q --test self_experiment_tests --test cli_experiment_tests`。
  - 已冒烟生成 `./experiments/provider-fallback-1777710884502618374/experiment.md`，仅追加计划文件。
- 安全自我实验结果报告已接入：
  - `SelfExperimentPlanner::complete()` 可为已有实验生成 `report.md`。
  - CLI 新增 `experiment complete --experiment-id ID --outcome success|failure|inconclusive --summary TEXT --next TEXT [--root PATH] [--json]`。
  - 报告写入使用 create-new 语义，`report.md` 已存在时拒绝覆盖，保持实验结果不可被静默改写。
  - 报告内容包含 outcome、summary、next step 和安全确认，明确没有执行 reset/delete/cleanup。
  - 已运行 `cargo fmt --all`、`cargo check`、`cargo test -q --test self_experiment_tests --test cli_experiment_tests`。
  - 已冒烟为 `provider-fallback-1777710884502618374` 生成 `./experiments/provider-fallback-1777710884502618374/report.md`，结果为 `inconclusive`。
- 安全自我实验只读列表已接入：
  - `SelfExperimentPlanner::list()` 只读扫描实验目录，返回 `planned/completed/unknown`、plan/report 路径和 presence 状态。
  - CLI 新增 `experiment list [--root PATH] [--json]`。
  - 已补测试覆盖库层列表和 CLI 输出。
  - 已冒烟 `cargo run --quiet -- experiment list --root ./experiments`，当前显示 `provider-fallback-1777710884502618374` 为 completed。
- Provider fallback MVP 已接入：
  - `ProviderConfig` 新增显式 `Fallback { primary, fallback }`，只有配置里写 `fallback_*` 时启用，不做 silent fallback。
  - `ProviderSlot` 新增 fallback wrapper；核心 runtime 仍只依赖 `Responder`，不认识 fallback 细节。
  - fallback 触发条件基于结构化 meta：`provider_retryable=true` 或 `status_code >= 400`，不解析模型自然语言输出。
  - fallback 输出会写入 `provider_fallback_used / provider_fallback_from / provider_fallback_reason`，便于飞书、桌面控制台和日志追溯。
  - 配置解析支持维护友好的扁平字段：`fallback_provider / fallback_provider_id / fallback_base_url / fallback_model / fallback_api_key_env / fallback_transport`。
  - `config.example.toml` 已加入注释模板；真实密钥仍只通过环境变量读取。
  - `app-server` 的模型覆盖和路径归一化已兼容 fallback provider。
  - 已运行 `cargo fmt --all`、`cargo test -q --test runtime_config_file_tests`、`cargo test -q --test slot_registry_tests`，当前专项通过。
- app-server 真实 provider 配置回归已补：
  - 新增 `tests/app_server_tests.rs`，通过临时 workspace 的 `config.toml` 启动 `app-server`，执行 `model/list` 和 `turn/start`。
  - 测试确认 app-server 会读取 workspace provider 配置，输出 `gpt-app-server-test` 和 OpenAI-compatible stub 响应，而不是回落到 `fake-responder`。
  - 这条测试专门防止后续接飞书/插件入口时误接到默认 fake responder。
  - 已运行 `cargo fmt --all`、`cargo test -q --test app_server_tests`。
- 安全自我实验只读详情已接入：
  - `SelfExperimentPlanner::show()` 可按 `experiment_id` 只读返回 `experiment.md` 和可选 `report.md` 内容。
  - CLI 新增 `experiment show --experiment-id ID [--root PATH] [--json]`，不会写入、覆盖、删除任何实验文件。
  - README 与 MVP 边界文档已补命令说明。
  - 已补库层和 CLI 回归测试。
- command control 示例 adapter 已接入：
  - 新增 `scripts/chuang-control-adapter-example.sh`，实现 `list --json` 和 `apply --json` 协议，返回确定性 JSON，不触碰真实服务或 Agent。
  - 新增 `config.example-control.toml`，可直接用 `program = "sh"` 加脚本路径跑 command 控制面。
  - `docs/control-command-protocol.md` 已补 checked-in example 和手动 smoke 命令。
  - 新增 CLI 回归，确认 `control list --config config.example-control.toml --json` 能列出示例 unit，`control apply ... --approve` 能经治理和审计后返回成功。
  - 已冒烟 `cargo run --quiet -- control list --config config.example-control.toml --json` 和 `cargo run --quiet -- control apply --config config.example-control.toml ... --json`。
- 安全 MVP 端到端 smoke 脚本已接入：
  - 新增 `scripts/chuang-mvp-smoke.sh`，会在临时目录生成独立配置，使用 OpenAI-compatible `stub` provider、queued subagent、示例 command control 和独立 SQLite/记忆/队列路径。
  - 验收链包含：`status`、`doctor`、两轮 `run --remember-session`、`subagent dispatch/run-once/collect`、`control list/apply --approve`、`experiment plan/show`。
  - 脚本不删除任何文件，不触碰真实服务，不读取真实 API key；临时 work_dir 会留存用于排查。
  - 已实测 `sh scripts/chuang-mvp-smoke.sh` 通过，输出 `mvp_smoke_ok`。
- MVP readiness 文档已补：
  - 新增 `docs/mvp-readiness-2026-05-02.md`，记录当前 root 配置状态、验收命令、ready/not ready 边界和下一步构建顺序。
  - README 已链接该文档，方便后续接飞书机器人或换会话时快速判断当前版本状态。
- channel adapter 协议边界已接入：
  - 新增 `src/channel_adapter.rs`，定义 `ChannelInboundMessage / ChannelOutboundMessage`，以及转换到 app-server `turn/start` JSON-RPC 的纯函数。
  - 该模块不认识 Feishu 凭证、不启动桥服务、不复用 Codex/Hermes 通道，只做外部消息和 app-server 事件的薄转换。
  - 新增 `docs/channel-adapter-protocol.md`，明确新飞书机器人必须使用独立 bot/channel id，不能复用 Codex/Hermes bridge。
  - 新增 `tests/channel_adapter_tests.rs`，覆盖 turn/start 构造、空文本拒绝、app-server delta 转 outbound、忽略非消息事件。
- 子代理 worker 能力声明和并发上限已接入：
  - `subagent run-once/run-loop` 新增 `--capability NAME`，JSON/Text 输出会带 `worker_capabilities`，供控制台或调度层识别 runner 能力。
  - `subagent run-loop` 新增 `--max-concurrency 1`，当前 MVP 只支持单 worker 顺序处理；传大于 1 会明确拒绝，不假装并行。
  - `SubagentRunLoopCliOutput` 新增 `max_concurrency`，方便后续 UI/插件展示当前 worker 限制。
  - `cli_subagent_dispatch_tests` 已扩到 23 条并通过，覆盖能力声明输出和并发拒绝。
- 子代理 claim/release 稳定性修正：
  - claim 和 release payload 新增 `claimed_at_unix_nanos / released_at_unix_nanos`。
  - `is_claim_released()` 优先比较 payload 中的纳秒时间，避免释放后立刻重新领取时受文件系统 mtime 精度影响。
  - 不删除旧 claim/release 文件；mtime 仍作为旧格式 payload 的兼容 fallback。
  - 已运行 `cargo test -q --test subagent_queue_tests file_subagent_queue_can_release_and_reclaim_without_deleting_history` 和 `cargo test -q --test cli_subagent_dispatch_tests`。
- 新飞书独立通道检查清单已补：
  - 新增 `docs/feishu-dedicated-channel-checklist.md`，明确 Chuang 必须使用新 Feishu bot/app id，不复用 Codex/Hermes bridge、service、credentials、session 或队列。
  - 清单记录了 adapter 最小形状、上线前预检命令、bot 侧要求和首次 live test 检查点。
  - README 已链接该清单。
- channel 本地演练命令已接入：
  - CLI 新增 `channel simulate --workspace-root PATH --message-id ID --sender-id ID --text TEXT [--thread-id ID] [--json]`。
  - 该命令会读取 workspace `config.toml`，通过当前 runtime 跑一轮，写 session memory，并输出 `ChannelOutboundMessage`；不会连接真实飞书。
  - 新增 `tests/cli_channel_tests.rs`，确认 channel simulate 使用 workspace provider 配置，输出不包含 `fake-responder`，并拒绝空文本。
  - `scripts/chuang-mvp-smoke.sh` 已纳入 channel simulate。
- channel app-server 事件批处理去重已接入：
  - 新增 `outbounds_from_app_server_events()`，批量处理 app-server events 时优先采用最终 `item/completed`，避免通道同时发送 delta 和 completed 的重复回复。
  - `tests/channel_adapter_tests.rs` 已覆盖 completed 优先逻辑。
- 真实 control adapter 安全计划已补：
  - 新增 `docs/real-control-adapter-safety-plan.md`，规定真实服务/Agent 控制必须走独立 command adapter、显式 allowlist、治理审批、receipt 一致性校验。
  - 明确禁止 broad `systemctl` passthrough、删除/清理、隐藏重启、直接控制 Codex/Hermes。
  - README 和 readiness 文档已链接该安全计划。
- command actuator adapter 已接入：
  - `ActuatorConfig` 新增 `command`，配置字段为 `actuator_program / actuator_args / actuator_timeout_ms`。
  - 新增 `CommandActuator`，通过 stdin/stdout JSON 协议调用外部 adapter；不走 shell 拼接，不把桌面/浏览器/微信/ADB 细节写进 core。
  - `ObserveTarget / OpenAppRequest / FocusTarget / ClickTarget / InputTarget / SecretOrPlainText / ScreenshotTarget / EvidenceRef` 已补 serde，作为 actuator command 协议的数据边界。
  - 新增 `scripts/chuang-actuator-adapter-example.sh`，返回确定性 JSON，不执行真实桌面操作。
  - `scripts/chuang-control-adapter-example.sh` 已扩成安全控制台 fixture，会列出小创、小承、小云、小策和 Codex 飞书桥等 unit，但 apply 仍只返回 receipt，不触碰真实服务。
  - 根 `config.toml` 与 `config.example.toml` 已切到 `actuator=command` 和 `control=command` 的安全示例 adapter；`status` / `doctor` 当前显示 `placeholder_warnings: none`。
  - `doctor` 已新增 `actuator_smoke`，会调用一次 `observe(Screen)` 验证当前 actuator adapter 可用。
- Provider fallback 策略配置已接入：
  - `ProviderConfig::Fallback` 新增 `ProviderFallbackPolicy`，只在显式配置 fallback provider 时启用。
  - 默认策略为 `on_retryable=true` 且额外允许 `401,402`，用于余额/鉴权类主 provider 失效时切备用；普通未列出的 4xx 不再被盲目 fallback 掩盖。
  - 配置支持 `fallback_on_retryable / fallback_status_codes / fallback_error_classes`，`status/config show` 会输出脱敏策略摘要。
  - `slot_registry_tests` 已覆盖 retryable 主 provider 错误会切备用，以及策略关闭后不会误切。
  - `runtime_config_file_tests` 已覆盖 fallback 策略解析和摘要输出。
- 子代理 claim 过期重领已接入：
  - `FileSubagentQueue` 新增 `claim_dispatch_with_timeout()` 和 `is_claim_stale()`。
  - `subagent run-once/run-loop` 领取任务时使用 dispatch 自带 `idle_timeout_ms` 作为 claim 过期阈值；worker 崩溃且超时后，新 worker 可以重领。
  - 不删除 dispatch、report、claim 或 release 文件；重领只覆盖 claim payload，与现有 release/reclaim 语义一致。
  - `subagent list` 输出新增 `is_claim_stale` 字段，便于控制台看出任务是否被过期 claim 卡住。
  - 已运行 `cargo test -q --test subagent_queue_tests` 和 `cargo test -q --test cli_subagent_dispatch_tests`。
- 子代理能力需求匹配已接入：
  - `subagent dispatch --requires-capability NAME` 可声明任务需要的 worker 能力，写入 dispatch metadata。
  - `subagent run-once/run-loop --capability NAME` 只领取满足全部需求的 dispatch，避免 Python/浏览器/文件系统等不同 runner 抢错任务。
  - capability 名会 trim、转小写、去重，并拒绝逗号，避免 metadata 逗号分隔格式产生歧义。
  - `subagent list` 输出新增 `required_capabilities`，方便控制台展示派发需求。
  - 新增 `docs/subagent-runner-protocol.md`，记录 dispatch metadata、worker capability 匹配、command runner stdin/stdout、claim/stale claim、report 校验和安全边界。
  - 新增 `scripts/chuang-subagent-runner-example.sh`，安全读取 dispatch stdin 并输出标准 `SubagentReport`，不执行真实工具。
  - `scripts/chuang-mvp-smoke.sh` 已从 fake runner 改为 command runner 示例 + capability matching。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q --test cli_subagent_dispatch_tests`、`sh scripts/chuang-mvp-smoke.sh`、`cargo test -q`。
- 长期记忆压缩写回操作面已接入：
  - CLI 新增 `memory identity show`，只读展示 `USER.md / MEMORY.md` 全文、字符数和硬上限。
  - CLI 新增 `memory identity append --id ID --content TEXT`，显式追加 `MEMORY.md` 热记忆，仍受硬上限约束。
  - CLI 新增 `memory identity write-user|write-memory --content TEXT --approve-overwrite`，用于超限后由模型/老爸决定压缩内容，再显式覆盖写回；不自动压缩、不自动删除。
  - `FileDualFileMemoryStore` 新增 `write_memory()`，和 `write_user()` 一样先做硬上限检查，失败时不改变原文件。
  - `scripts/chuang-mvp-smoke.sh` 已覆盖 append -> write-memory -> show。
- app-server 常驻化准备已接入：
  - CLI 新增 `app-server health --workspace-root PATH --json`，只读加载并校验 workspace runtime 配置，不发起模型请求。
  - 新增 `scripts/chuang-app-server-health.sh`，读取 `CHUANG_AGENT_ROOT / CHUANG_AGENT_WORKSPACE_ROOT` 后执行健康检查。
  - 新增 `ops/systemd/chuang-agent-app-server.service.example` 和 `chuang-agent-app-server.env.example`，提供 Chuang-only systemd 模板、journald 日志和 `ExecStartPre` 健康检查。
  - 模板没有自动安装或启动；app-server 仍是 stdin/stdout JSON-lines 协议，真实 Feishu 插件需要单独持有连接策略。
  - `scripts/chuang-mvp-smoke.sh` 已覆盖 app-server health。
- 新飞书专用通道骨架已接入：
  - CLI 新增 `channel feishu-check --env-file PATH --json`，长连接模式下只检查 `CHUANG_FEISHU_APP_ID / APP_SECRET / CHUANG_AGENT_WORKSPACE_ROOT` 是否存在，并脱敏输出 `<set>`。
  - `feishu-check` 会标记 `FEISHU_APP_ID / FEISHU_APP_SECRET / FEISHU_BOT_ID / HERMES_FEISHU_APP_ID` 等旧变量名，避免复用 Codex/Hermes 通道。
  - 新增 `ops/systemd/chuang-feishu-bridge.env.example` 和 `chuang-feishu-bridge.service.example`，只使用 Chuang 专用变量名。
  - 新增 `scripts/chuang-feishu-bridge.sh` / `scripts/chuang-feishu-bridge.js`：长连接 Feishu bridge 已改为真实运行，收到消息后转发到 Chuang `app-server`，再把结果回发飞书。
  - Chuang 专用 bridge env 已放到 `~/.codex-im/chuang-feishu-bridge.env`，与 Codex/Hermes 的凭证和会话分开。
  - 当前桥已按 websocket 模式启动并保持运行；真实回复链路依赖 workspace `config.toml`、`app-server` 和 `CODEX_LIUSU_API_KEY`。
- 真实子代理 Codex runner 脚手架已接入：
  - 新增 `scripts/chuang-codex-runner.py`，读取 dispatch JSON stdin，输出标准 `SubagentReport`，可被现有 `subagent run-once --runner command` 接收和校验。
  - 默认返回 Failed 协议报告，不调用 Codex；只有 `CHUANG_CODEX_RUNNER_ENABLE=1` 时才运行 `codex exec <prompt>`。
  - 支持 `CHUANG_CODEX_BIN` 和 `CHUANG_CODEX_RUNNER_WORKSPACE`，并使用 dispatch `idle_timeout_ms` 作为进程超时。
  - `cli_subagent_dispatch_tests` 已覆盖该 runner 默认禁用时仍能产出可校验 report。
- 真实 control/actuator allowlist 骨架已接入：
  - 新增 `config/control-allowlist.example.json`，只列 Chuang-owned service 示例，不包含 Codex/Hermes 服务。
  - 新增 `scripts/chuang-real-control-adapter.py`，实现 `list/apply --json --allowlist PATH`；默认 dry-run，只有 `CHUANG_REAL_CONTROL_ENABLE=1` 时才执行 allowlist command array，status 命令也默认不跑。
  - 新增 `config/actuator-allowlist.example.json`，显式列可打开 app，并默认关闭 click/input/screenshot。
  - 新增 `scripts/chuang-real-actuator-adapter.py`，默认 dry-run；`open_app` 仅接受 allowlist app，真实打开必须 `CHUANG_REAL_ACTUATOR_ENABLE=1`。
  - `cli_control_tests` 已覆盖 control allowlist dry-run 和 unallowlisted 拒绝；`actuator_tests` 已覆盖 allowlisted app 与 click 默认拒绝。
- 插件注册/发现 MVP 已接入：
  - 新增 `src/plugin_registry.rs`，定义 `PluginRegistry / PluginManifest / PluginKind` 和只读 `check_plugin_registry()`。
  - 新增 `plugins/registry.example.json`，登记 Chuang Feishu bridge、Codex runner、real control、real actuator、Genesis AutoCLI adapter。
  - CLI 新增 `plugin list|check --registry PATH [--json]`，只读取 manifest 和检查命令/配置路径是否存在，不执行插件、不读取密钥。
  - `status` 已暴露只读 `plugin_registry` 摘要：registry path、available、ok、plugin_count、enabled_count、issue_count；其中 `ok` 只统计已启用插件的 readiness，禁用插件即使路径缺失也只作为 manifest 级提示，方便未来桌面控制台直接读取插件槽位健康状态。
  - 示例注册表当前 5 个插件都已登记但默认 disabled，符合安全默认不开启真实外部能力的策略。
  - `scripts/chuang-mvp-smoke.sh` 已加入 plugin registry check。
  - 新增 `docs/actuator-command-protocol.md`，说明 request/response shape 和安全约束。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q --test plugin_registry_tests --test cli_status_tests --test kernel_status_tests`、`cargo check`、`cargo test -q --test actuator_tests`、`cargo test -q --test runtime_config_file_tests`、`cargo run --quiet -- config check --config config.toml`、`cargo run --quiet -- status --config config.toml`、`cargo run --quiet -- doctor --config config.toml`。
- 只读控制台快照 MVP 已接入：
  - CLI 新增 `console snapshot [--json]`，聚合当前 `status`、插件注册表摘要、control unit 列表和插件清单。
  - 该命令只读调用 control list，不执行 `control apply`，不启动服务，不连接飞书，作为未来桌面/工具/服务控制台的数据源。
  - `scripts/chuang-mvp-smoke.sh` 已加入 console snapshot。
  - 新增 `tests/cli_console_tests.rs`，覆盖 JSON 输出和文本摘要。
  - 已运行 `cargo fmt --all`、`cargo test -q --test cli_console_tests --test cli_control_tests --test cli_status_tests`。
- 主线工具闭环已补到 CLI runtime：
  - `run_with_options()` 现在会识别 `TOOL_CALL`，并在本地执行 `list_dir / read_file / write_file / shell_exec` 后继续回灌给模型，直到收到 `FINAL:` 收口或达到最大工具回合数。
  - 这条工具循环复用了现有 `tool_runtime` 和 `governance`，没有再新增一套平行执行面。
  - 当工具回合出现时，最终 `RuntimeResult.response.meta.extra` 会附带 `tool_call_count` / `tool_trace`，方便 CLI / report 层可审计输出。
  - 已新增回归测试，确认 `TOOL_CALL` 会真正写文件并被最终 `FINAL:` 收口。
  - 工具协议已从 `user_input` 中移出，改为通过 ChuangKernel 的 turn-level extra context 注入，避免污染记忆检索、会话摘要和用户原始输入。
  - 本地测试已和项目根真实 `config.toml` 解耦：CLI smoke/status/provider/subagent 等测试显式使用 fake config 或测试 env，避免本机 `CODEX_PPTOKEN_API_KEY` 缺失影响主线回归。
  - 已运行 `cargo fmt --all` 和 `timeout 240s cargo test -q`，全量通过。
- 主进程工具口优先级已纠偏：
  - 当前推进顺序明确为：先补主进程最小工具集和统一 tool port，再补治理/审计/结构化回传，然后才是子代理，最后才是外部智能体。
  - `ToolExecutionRecord` 已从字符串摘要扩展为结构化结果：`tool_name / decision / output / stdout / stderr / exit_code / changed_files / failure_class`，同时保留 `summary` 兼容旧输出。
  - `execute_tool_call_with_governance()` 会把治理决策写入工具记录，CLI/app-server 可直接展示结构化结果，不需要只解析 `tool_trace`。
  - 修复了不存在路径中的 `..` 词法归一化问题，避免非现存路径绕过 workspace 边界检查。
  - app-server 不再自带第二套工具循环，改为直接复用 `run_with_options()` 主线，并从 `tool_calls_json` 取结构化工具调用结果；CLI 和 Feishu/app-server 入口工具行为保持一致。
  - 已运行 `cargo fmt --all`、`cargo test -q --test tool_runtime_tests`、`cargo test -q --test app_server_tests`、`cargo test -q cli_runtime::tests::run_with_options_executes_tool_calls_before_final_answer`、`timeout 240s cargo test -q`，全量通过。
- 主进程工具口结构化回传继续加固：
  - `ToolExecutionRecord` 新增 `duration_ms / retryable / output_truncated / stdout_truncated / stderr_truncated`，工具结果可直接被 CLI、app-server、飞书桥和后续报告面读取，不再依赖自然语言判断。
  - `shell_exec` 非 0 退出现在会标记为 `ok=false`、`failure_class=exit_nonzero`，同时保留 `exit_code/stdout/stderr`。
  - `write_file` 新增 `write_before_bytes / write_after_bytes / write_changed`，可区分新建、真实修改和重复写入同样内容。
  - 新增 `ToolLoopReport`，最终响应 meta 会写入 `tool_report_json`，包含 schema version、workspace root、rounds、call count 和完整 calls；`tool_trace` 继续保留为兼容展示字段。
  - app-server 的 `turn/completed` 和 RPC 返回会透出 `toolReport`，仍从同一条 `run_with_options()` 主线读取。
  - shell 工具调用会先复用现有 `ActionKind` 做风险分类：删除/清理、服务变更、网络命令、疑似密钥读取分别进入 `DeleteOrCleanup / ServiceChange / NetworkChange / SecretAccess`；`NeedsApproval` 和 `DraftOnly` 不执行，只写拒绝审计。
  - 新增 `ToolModelOutput`，把模型输出先解析成 `ToolCall / FinalAnswer / PlainText`，runtime 主线不再到处手写 `TOOL_CALL` / `FINAL` 前缀判断；后续替换成正式 action schema 会更便宜。
  - 新增正式 `ACTION` 协议兼容入口：`ACTION: {"type":"tool_call","call":{...}}` 和 `ACTION: {"type":"final","answer":"..."}`；旧 `TOOL_CALL` / `FINAL` 继续兼容，便于平滑迁移。
  - `RunCliRequest` 新增 `workspace_root`，app-server 和 channel simulate 会显式传入飞书/工作区根目录；CLI 交互默认仍用当前目录。工具执行工作区不再隐式绑定 app-server 进程启动目录。
  - app-server workspace 配置路径归一化继续补齐：`identity_bootstrap` 和 `rules` 路径现在也会按 workspace 根目录解析，health JSON 会暴露 `identity_soul_path / rules_core_path` 便于检查。
  - 工具循环最大轮次已配置化：新增 `ToolLoopConfig { max_rounds }`，配置文件支持 `tool_max_rounds` 或 `[tool_loop] max_rounds`，默认 4，校验范围 1..=32；`config/status` 输出会展示 `tool_loop_max_rounds`。
  - `shell_exec` 超时也已配置化：`tool_shell_timeout_ms` 或 `[tool_loop] shell_timeout_ms`，默认 30000ms，校验范围 1..=600000；工具执行通过 `ToolExecutionConfig` 接收，不再把 30 秒写死在执行器里。
  - runtime report 已正式提升工具报告：`build_runtime_report()` 会把 `tool_report_json` 转成 `SubagentReport.artifacts` 里的 `Log` artifact，summary 同步带 `tool_calls=N`，后续子代理/控制台不用再专门解析 provider meta。
  - 已运行 `cargo fmt --all`、`cargo test -q --test tool_runtime_tests`、`cargo test -q --test app_server_tests`、`cargo test -q cli_runtime::tests::run_with_options_executes_tool_calls_before_final_answer`、`git diff --check`、`timeout 240s cargo test -q`。

### 10 分钟推进节奏
- 长期记忆 experiences.md MVP 入口已补：
  - `DualFileMemoryConfig` 新增默认 `experiences.md` contract，`FileDualFileMemoryStore::open()` 会保证该文件存在；只建空文件，不自动写经验、不删除、不迁移旧内容。
  - `DualFileMemorySnapshot` / `memory identity show` 可只读展示 experiences 内容和字符数；当前 runtime prompt 仍只注入 USER/MEMORY，不把 experiences 直接塞进上下文。
  - `ConfigSummary`、`status`、`config show` 暴露 `identity_experiences_path`，`doctor` 新增 `identity_experiences` 检查，便于后续桌面/维护器诊断内部经验层是否有入口。
  - `docs/memory-architecture-layering.md` 和 `docs/mvp-scope.md` 已记录当前边界：入口/状态面已做，写入策略、admission、provenance 和 LIM/session 自动抽取仍待补。
- 当前会话内按阶段推进：每轮优先选主线阻塞点，不扩外部智能体，不先做子代理优化。
- 第 1 轮：主进程工具口配置化和协议稳定。
- 第 2 轮：继续把工具请求/结果收进正式 runtime/report shape。
- 第 3 轮：补治理策略可配置性，尤其 shell/write 的 allowlist/denylist。
- 第 4 轮：再看 app-server/飞书通道展示和健康检查是否缺主线字段。
- 跨会话不会自动唤醒；如需真实定时提醒，后续单独做 Chuang 专用提醒插件或 systemd timer，不能复用 Codex/Hermes 通道。
- GA 9 原子工具方向已重新对齐：
  - 文档目标是 `Execution Slot = GenericAgent 9 原子工具 + Codex 安全管道 + OpenClaw 治理/隔离`，当前 4 个本地工具只是 MVP 映射，不是最终工具体系。
  - 新增 `src/atomic_tool.rs`，定义 GA 9 原子工具 manifest：mouse、keyboard、screenshot、locate、file_read、file_write、code_execute、wait、human_suspend。
  - 当前已映射：`read_file -> file_read`、`write_file -> file_write`、`shell_exec -> code_execute`；`list_dir` 明确为辅助工具 `AuxiliaryListDir`，不算 GA 9 原子之一。
  - 桌面类工具暂为 `InterfaceOnly`，通过现有 `actuator` port 承接，不把真实鼠标/键盘/截图实现写死进核心。
  - 防跑偏提醒：后续 Execution 主线先围绕 GA 原子工具骨架、manifest、治理和 adapter 映射推进，不继续把 4 个 MVP 工具当最终体系打磨。
  - 已运行 `cargo fmt --all`、`cargo test -q --test atomic_tool_tests`。
- 当前模型配置已对齐 cliproxy 的 `gpt-5.5` 路径：项目根 `config.toml` 仍使用 `openai_compatible + gpt-5.5`，密钥通过 `CODEX_LIUSU_API_KEY` 环境变量读取，不写入仓库文件；后续新飞书机器人只需要独立的 bot id 和通道配置。
- 飞书架构终稿已补当前实现修正：
  - 使用 `lark-cli docs +fetch/+update --as bot` 读取并追加更新老爸给的飞书 wiki 文档。
  - 已在文档末尾明确“目标态 vs 当前仓库实现”分离，避免把最终蓝图误读为已落地能力。
  - 已修正当前实际路径：身份启动层为 `identity/SOUL.md`、`identity/STORY.md`、`identity/FIRST_WAKE.md`、`identity/agents.toml`；Hermes 热记忆为 `data/hermes-memory/USER.md`、`data/hermes-memory/MEMORY.md`。
  - 已明确 `BrowserWorker` 是冻结线，后续网页 AI 查询/搜索能力走 `GenesisActuator` 和外部 adapter/plugin。
  - 已明确 `evolution` 当前是预留槽位，不应表述为完整自进化已落地。
- 搜索能力与外部智能体调度方案已补进飞书架构终稿：
  - 搜索/外部 AI 分身调度不新增第十个核心 Slot，而是作为 `AgentSlot` 的实现线。
  - `统一身份引擎` 负责登录态管理、浏览器/HTTP 会话执行、结果解析、审计和熔断；底层可接 `agent-browser`，但登录态/Cookie 不进入核心记忆。
  - `data/skills/external_agent_dispatch_sop.md` 作为后续 Skill 目标，负责平台选择、任务翻译、追问策略、效果评估和记忆回写。
  - 重要协作原则已确定为二级委派：`主进程 -> 子代理 -> 外部智能体`。主进程只拆解/派发/终审/汇报/归档，子代理负责调度外部智能体并做第一次审核提炼，主进程只接收整理后的 `SubagentReport`。
- GoalRun / tool surface / readiness 本轮收尾已整合：
  - `GoalRun` 已从单纯 runtime goal context 扩展为 checkpoint-first 本地计划记录：`goal plan/show/checkpoint` 会读写 `./context/goal-runs/<goal_id>.json`，该目录被 git 忽略，只用于恢复开发目标状态。
  - `GoalRun` 仍不是后台执行器，不自动续跑、不调度 worker、不绕过治理；当前能力是让下一轮优先从 checkpoint 恢复，减少反复口头说 `continue`。
  - app-server / channel simulate 即使本轮工具调用数为 0，也会透出 `toolSurface`，包含 `available=true`、`governed=true`、GA MVP 可调用工具、schema version 和来源，避免飞书侧误以为创“没有工具”。
  - `runtime_observability` / provider meta 已带出 tool surface 摘要，thread 快照也保留该字段，恢复会话时不会丢工具面信息。
  - `status` / `doctor` 新增只读 `goal_run` readiness，默认检查 `mainline-mvp` 计划是否存在、checkpoint 数量、worker 数量、validation command 数量和最后 checkpoint id；只读取 JSON，不执行 goal。
  - `scripts/chuang-mvp-smoke.sh` 已加入 `goal plan -> goal checkpoint -> goal show` JSON 验收，明确验证 checkpoint-first 记录闭环。
  - 本轮 checkpoint 已写入 `context/goal-runs/mainline-mvp.json`：`checkpoint-tool-surface-goal-status`。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q --test goal_run_tests`、`cargo test -q --test app_server_tests`、`cargo test -q --test cli_channel_tests`、`cargo test -q --test cli_doctor_tests`、`cargo test -q --test kernel_status_tests`、`cargo test -q --test cli_status_tests`、`cargo test -q --test tool_runtime_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`，全部通过。
- 主线继续补齐本轮完成：
  - `goal plan` 现在支持显式 `--scope scope_id=path[,path...]` 和 `--worker worker_id|scope_id[,scope_id...]|objective`，可以记录多个 worker 与不重叠写入范围；它仍只写 GoalRun JSON，不自动执行、不调度 worker。
  - `goal plan` 的默认 worker 现在会跟随已声明的 scope：如果用户显式声明了多个 scope 但没有传 `--worker`，默认 worker 会绑定这些 scope，而不是强行只认 `mainline`。
  - 新增 `tests/cli_goal_tests.rs`，覆盖多 worker 计划落盘和重叠 scope 拒绝，防止 goal 计划边界失控。
  - `memory_recall` 已作为受治理只读辅助工具接入主进程工具面：它不属于 GA 9 原子工具，`atomic_tool_name=null`，只复用 SQLite + `MemoryRecallPipeline`。
  - `memory_recall` 强制过滤 `memory_scope=session, session_id=<当前会话>`；未配置 memory/session、DB 不存在、跨会话请求都会返回结构化失败记录，不接外部知识库，不写入记忆，不碰 Hermes/Feishu/密钥。
  - CLI runtime 会把已有 `db_path` 与当前 `session_id` 注入 `MemoryToolContext`；app-server/channel 因本来带 thread session id，会继承同一条主线工具能力。
  - `status` / `doctor` 新增 governance readiness：展示 `rules_loaded`、规则数量/指纹、`tool_surface_governed=true`、read-only allowed、危险 write/shell needs_approval、secret shell draft_only、`goal_run_executes=false`。
  - `scripts/chuang-mvp-smoke.sh` 已增强 status/doctor JSON 断言，覆盖 GA 工具名单、identity bootstrap presence、provider timeout、GoalRun readiness、plugin registry 和 placeholder warning。
  - 本轮 checkpoint 已写入 `context/goal-runs/mainline-mvp.json`：`checkpoint-memory-governance-readiness`。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q --test cli_goal_tests`、`cargo test -q --test tool_runtime_tests`、`cargo test -q --test kernel_status_tests`、`cargo test -q --test cli_doctor_tests`、`cargo test -q --test cli_status_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`，全部通过。
- 子代理报告受理状态本轮已补：
  - 新增 `ReportAdmissionStatus::{Accepted, Rejected}` 和独立 `ReportAdmission`，用于表达主控是否接受子代理提交的报告。
  - `SubagentReport` 继续保持不可变执行快照，只记录 worker 声称的执行状态；主控受理状态不写回报告本体，避免把“执行结果”和“受理结果”混在一起。
  - `SubagentReportValidator::admit_raw()` 会对原始 stdout report 生成 admission：合法报告得到 `Accepted`，缺字段、schema 不支持、时间错误、体积过大等得到 `Rejected`，并尽量保留 report/task/agent id 方便审计。
  - command runner 协议入口已接入 admission：stdout 看起来是完整 `SubagentReport` 时，先生成主控受理判断，受理通过后再 decode 和做 task/agent/parent 身份校验。
  - `docs/subagent-runner-protocol.md` 已补明确定义：`SubagentReport.status` 是 worker 执行状态，`ReportAdmission.status` 是 controller 受理状态；即使 worker 声称 Success，主控也可以因协议或身份问题拒绝。
  - 本轮 checkpoint 已写入 `context/goal-runs/mainline-mvp.json`：`checkpoint-subagent-report-admission`。
  - 已运行 `cargo fmt --all`、`git diff --check`、`cargo test -q --test subagent_report_tests`、`cargo test -q --test subagent_queue_tests`、`cargo test -q --test cli_subagent_dispatch_tests`、`cargo test -q`、`sh scripts/chuang-mvp-smoke.sh`，全部通过。

### 约束
- 进度必须持续写入本文件，避免 new 后丢失上下文
- 最终以本地代码和测试为准，不以网页对话停留状态为准
- BrowserWorker 是并行能力线，不能反客为主抢掉长期记忆/多子代理/上下文管理三大主线
- 本轮补齐本地 `workspace_file_adapter`，新增 `read_file / write_file / list_dir / apply_patch` 四个能力，路径严格限制在 workspace root 内，写入保留审计备份与 diff 预览。
- `tool_runtime` 已接入 `apply_patch`：`ToolCall`、协议字段、治理动作、`ToolSurfaceStatus`、CLI 工具名映射和 `cli_doctor` schema 校验都已同步。
- 新增对应测试并通过 `cargo test -q` 与 `sh scripts/chuang-mvp-smoke.sh`，当前第二个测试版本的文件工作区能力已可验收、可回归、可续接。
# 2026-05-12 runtime event ledger 可查询摘要补齐
- 本轮继续沿 M5/M6/M7 主链接线收口 runtime event ledger：`runtime_event_ledger_json` 现在通过统一的内部结构化解析写入 `runtime_observability_meta`，直接暴露 `runtime_event_count`、`runtime_event_tool_started_count`、`runtime_event_tool_finished_count`、`runtime_event_approval_requested_count`、`runtime_event_approval_resolved_count` 和 `runtime_event_elicitation_requested_count`；`runtime_meta.observability` 文本摘要也把 approval/elicitation 事件显式列出，状态面和报告面不再只靠 artifact 说明反推。
- 回归已补在 `tests/runtime_report_tests.rs`，锁住 observability map 与 runtime event ledger artifact 的计数一致，并确认 `approval_resolved` 不再被漏计。验证已通过 `cargo fmt --all --check` 和 `cargo test -q --test runtime_report_tests`。下一轮入口：继续把 runtime report surface、goal collect/show 和 subagent tree 的只读摘要再对齐一层，优先盯 smoke 和 status 面的最终一致性。

# 2026-05-12 context compaction summary 门禁收口
- 本轮把 M7 的 `context_compaction_summary_json` 从内核产出推进到高层验收门禁：`scripts/chuang-mvp-smoke.sh`、`scripts/chuang-complete-local-smoke.sh`、`tests/cli_smoke_tests.rs`、`tests/cli_status_tests.rs`、`tests/cli_doctor_tests.rs`、`tests/app_server_tests.rs` 和 `tests/kernel_status_tests.rs` 现在都显式断言 `runtime_meta.context_compaction_summary_json` artifact locator 与 `context_compaction_summary_json` observability field 存在。
- 这样 `status` / `doctor` / `app-server health` / complete-local / MVP smoke 都会同时看见 `context_pack_trace`、`context_compaction_events` 和结构化 compaction summary，不再只靠文本预览和 event ledger 反推。验证已通过 `cargo test -q --test runtime_report_tests --test kernel_status_tests --test cli_smoke_tests --test cli_status_tests --test cli_doctor_tests --test app_server_tests`、`sh scripts/chuang-complete-local-smoke.sh`、`cargo fmt --all --check`、`git diff --check`、`cargo test -q` 和 `sh scripts/chuang-mvp-smoke.sh`。
- 下一轮入口：继续沿 M5/M6/M7 主链接线看 final verify / candidate verify 是否还需要再并一层只读门禁，或转去 goal run / subagent tree 的剩余回归边角。

# 2026-05-12 candidate/third-test runtime report surface 并入
- 本轮把 M5/M6/M7 的 `runtime_report_surface` 继续并到 candidate/third-test 共同入口：`scripts/chuang-live-runner-readiness-view.sh` 现在会从 `status --json` / `doctor --json` / `app-server health --diagnostic --json` 的只读源里聚合 `runtime_report_surface`，JSON/text 都直接暴露 `artifact_count=10`、`observability_field_count=20` 和 blocked reason；candidate verify 与 third-test smoke 在 live runner readiness view 阶段显式断言 goal handoff、subagent children、context compaction、tool/approval/elicitation event 这些关键字段。
- 对应回归补在 `tests/live_operator_scripts_tests.rs` 与 `tests/live_runner_readiness_view_tests.rs`，锁住 candidate/third-test wrapper 必须读取 `runtime_report_surface`，并更新 aggregated JSON 顶层键集合。这样 final/complete-local 之外，候选与第三测试门禁也能直接看见 M5 MCP approval/elicitation/tool events、M6 handoff/subagent 摘要和 M7 compaction trace 的同一状态面。
- 验证已通过 `cargo fmt --all --check`、`git diff --check`、`bash scripts/chuang-live-runner-readiness-view.sh --json | python3 -c ...`（确认 `10 20`）、`cargo test -q --test live_operator_scripts_tests`、`cargo test -q --test live_runner_readiness_view_tests`、`cargo test -q --test cli_smoke_tests candidate_verify_wrapper_sequences_dirty_tree_friendly_candidate_gates`、`cargo test -q` 和 `sh scripts/chuang-mvp-smoke.sh`。下一轮入口：可以继续跑 `sh scripts/chuang-complete-local-smoke.sh` / candidate wrapper 复验，或转去 goal run / subagent tree 的查询摘要边角。
# 2026-05-12 live runner readiness view 文本面并齐 runtime surface 列表
- 本轮把 `scripts/chuang-live-runner-readiness-view.sh` 的只读文本输出再补了一层：`status --json` / `doctor --json` / `app-server health --diagnostic --json` 聚合出来的 `runtime_report_surface` 现在不只打印 `artifact_count=10`、`observability_field_count=22`，还会直接列出 `artifact_locators` 和 `observability_fields` 名称列表，方便人工在纯文本模式下直接确认 M5/M6/M7 的 runtime trace、goal handoff、subagent children、context compaction 和 reason-code 分布是否在场。
- `tests/live_runner_readiness_view_tests.rs` 新增文本模式回归，锁住 `runtime_report_surface.artifact_locators` / `runtime_report_surface.observability_fields` 的可见性；已通过 `cargo test -q --test live_runner_readiness_view_tests`、`cargo fmt --all --check`、`git diff --check` 和 `sh scripts/chuang-mvp-smoke.sh`。下一轮入口：继续盯 candidate/third-test/final verify 的只读摘要是否还要再补一层文本可见字段，或回到 goal run / subagent tree 的剩余边角。

# 2026-05-12 app-server/channel 工具协议观测字段高层锁定
- 本轮继续沿 M5/M6/M7 主链接线补高层回归：app-server 的 `turn/start` 响应与 `turn/completed` 事件现在显式断言 `runtimeObservability.tool_protocol_error_count` 和 `toolProtocolErrors` 数组存在；channel simulate JSON 也断言 `runtime_observability.tool_protocol_error_count` 与顶层 `tool_protocol_error_count` 对齐。
- 这轮不新增协议合同、不造新的 fake provider 行为，只锁住已有主链字段形态，避免 Feishu/app-server/channel 状态面回退到只有 count、没有可查询错误数组或 observability 字段。验证已通过 `cargo test -q --test app_server_tests app_server_turn_uses_workspace_provider_config`、`cargo test -q --test cli_channel_tests cli_channel_simulate_runs_workspace_config_without_fake_responder`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：继续查 runtime report surface / goal show / readiness 文本面是否还缺 tool protocol correction 或 M5/M6/M7 reason-code 字段。

# 2026-05-12 status/doctor/app-server 文本面 admission 字段锁定
- 本轮继续补 M6/M7 主链接线的人读状态面回归：`status`、`doctor` 与 `app-server health --diagnostic` 文本输出测试现在不仅断言 `runtime_report_surface` 的 10/25 总数，还显式锁住 `runtime_response.trace`、`runtime_response_trace_chars`、`runtime_meta.goal_handoff_query_summary_json`、`runtime_meta.subagent_children_summary_json`、goal handoff admission ref/reason-code 字段，以及 subagent children admission ref/reason-code 字段。
- 这样 operator 在纯文本状态面也能直接看见 goal handoff / subagent children 的 admission locator 与 reason-code 查询面，不必只依赖 JSON 或总数字段。验证已通过 `cargo test -q --test cli_status_tests cli_status_prints_mvp_health_summary`、`cargo test -q --test cli_doctor_tests cli_doctor_reports_mvp_health_in_text`、`cargo test -q --test app_server_tests app_server_health_text_reports_diagnostic_summary_and_next_actions`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：继续扫 goal collect/show/step 与 readiness/candidate wrapper 是否还缺 tool protocol correction 或 nonzero event 的高层回归。

# 2026-05-12 readiness JSON runtime surface 字段补齐
- 本轮继续把 M5/M6/M7 查询面往 readiness JSON 聚合面补齐：`tests/live_runner_readiness_view_tests.rs` 的 aggregated JSON 回归现在显式断言 `runtime_response.trace`、`runtime_response_trace_chars`、`runtime_meta.goal_handoff_query_summary_json`、`runtime_meta.subagent_children_summary_json`、goal handoff admission refs/reason-codes、subagent children admission refs/reason-codes 和 `context_compaction_summary_json` 都在 `runtime_report_surface` 中。
- 这让 candidate/third-test 依赖的只读聚合 JSON 与文本面同口径，不再只靠 10/25 总数或少数字段抽样。验证已通过 `cargo test -q --test live_runner_readiness_view_tests live_runner_readiness_view_script_outputs_aggregated_json_view`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：继续扫 `complete-local` / candidate wrapper 静态测试与 goal-mode smoke 是否还缺同类字段抽样。

# 2026-05-12 complete-local runtime surface 同口径补齐
- 本轮把 `scripts/chuang-complete-local-smoke.sh` 的 4 个 runtime_report_surface 检查块补到 candidate/third-test 同口径：全部显式断言 `runtime_response.trace`、`runtime_response_trace_chars`、`runtime_meta.context_compaction_summary_json`、goal handoff admission reason codes 和 subagent children reason codes，避免 complete-local 门禁落后于 readiness/candidate 聚合面。
- `tests/cli_smoke_tests.rs` 的 complete-local 静态回归同步锁住这些字段；验证已通过 `cargo test -q --test cli_smoke_tests complete_local_smoke_wrapper_reuses_safe_local_acceptance`、`cargo fmt --all --check`、`git diff --check` 和完整 `sh scripts/chuang-complete-local-smoke.sh`。下一轮入口：继续扫 MVP smoke 与 goal-mode smoke 的字段抽样是否也需要补齐 reason-code/trace chars，或者转去 tool protocol correction 的非零路径测试。

# 2026-05-12 MVP smoke runtime surface 同口径补齐
- 本轮把 `scripts/chuang-mvp-smoke.sh` 的 3 个 runtime_report_surface 检查块补齐到 complete-local/candidate 同口径：显式断言 `runtime_response.trace`、`runtime_response_trace_chars`、goal handoff admission reason codes 和 subagent children reason codes，避免 MVP/second-test 基础门禁落后于后续验证脚本。
- `tests/cli_smoke_tests.rs` 的 second-test wrapper 静态回归同步锁住这些字段；验证已通过 `cargo test -q --test cli_smoke_tests second_test_smoke_wrapper_reuses_safe_mvp_smoke`、`cargo fmt --all --check`、`git diff --check` 和完整 `sh scripts/chuang-mvp-smoke.sh`。下一轮入口：继续查 tool protocol correction 的高层非零路径，或跑 M5/M6/M7 汇总矩阵确认连续 checkpoint 后状态一致。

# 2026-05-12 tool protocol errors 提升到 runtime report surface
- 本轮把 M7/tool protocol 非零错误从 provider meta 可见推进到 runtime report 查询面：`runtime_report` 现在会把 `tool_protocol_errors_json` 提升为 `runtime_meta.tool_protocol_errors_json` artifact，并给出错误数量与 code 摘要；`runtime_report_surface` 同步提升到 11 个 artifact / 26 个 observability 字段，新增 `tool_protocol_error_count`。
- status/doctor/app-server health、readiness view、MVP/complete-local/candidate/third-test 脚本与静态回归已同步到 11/26，并显式断言 `runtime_meta.tool_protocol_errors_json` 与 `tool_protocol_error_count`。验证已通过 runtime/status/doctor/smoke/readiness/operator/app-server 相关 cargo 测试、完整 `sh scripts/chuang-mvp-smoke.sh` 和 `sh scripts/chuang-candidate-verify.sh`。下一轮入口：继续寻找 app-server/channel 能否稳定构造非零 tool protocol 错误路径；若没有脚本 provider 入口，优先加 runtime 层合同测试而不造后门。

# 2026-05-12 tool protocol error artifact 合同回归
- 本轮在 runtime report 层补了非零协议错误合同回归：`runtime_report_promotes_tool_report_metadata_to_artifact` 现在构造 `tool_protocol_errors_json`，并断言 `runtime_meta.tool_protocol_errors_json` artifact 存在，description 含 `count=2`、`invalid_action_json` 与 `plain_text_response`。
- 验证已通过 `cargo test -q --test runtime_report_tests runtime_report_promotes_tool_report_metadata_to_artifact`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：继续看 app-server/channel 是否能通过现有 provider 配置稳定触发非零协议错误；若不能，保持 runtime 层合同优先，不引入测试后门。

# 2026-05-12 app-server 非零 tool protocol error 高层回归
- 本轮用现有 OpenAI-compatible HTTP 本地测试服务补了 app-server 高层非零协议错误路径：第一轮真实 provider 返回缺少 `path` 的 `file_read` ACTION，tool loop 记录 `invalid_action_json` 后把错误反馈给模型，第二轮 provider 返回 FINAL。
- `tests/app_server_tests.rs` 现在断言 `turn/start` 响应和 `turn/completed` 事件都暴露 `toolProtocolErrorCount=1`、`runtimeObservability.tool_protocol_error_count=1`、`toolProtocolErrors[0].code=invalid_action_json`、`toolEvents.kind=protocol_error` 和 provider meta 中的 `tool_protocol_errors_json`；没有新增 scripted provider 后门。验证已通过 `cargo test -q --test app_server_tests app_server_turn`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：把同类非零路径继续评估到 `channel simulate`，如果 channel 只能走单轮 stub，则保留 app-server 覆盖并转向 M5/M6/M7 全矩阵复验。

# 2026-05-12 channel 非零 tool protocol error 高层回归
- 本轮把 app-server 已验证的非零协议错误路径推进到 channel simulate：测试用现有 OpenAI-compatible HTTP 本地服务连续返回缺少 `path` 的 `file_read` ACTION 与修正 FINAL，真实走 `run_with_options` tool loop，不新增 scripted provider 后门。
- `tests/cli_channel_tests.rs` 现在断言 channel JSON 输出包含 `tool_protocol_error_count=1`、`runtime_observability.tool_protocol_error_count=1`、`tool_protocol_errors[0].code=invalid_action_json`、`tool_events.kind=protocol_error`、provider meta 的 `tool_protocol_errors_json`，且 outbound text 为修正后的最终答复。验证已通过 `cargo test -q --test cli_channel_tests`、`cargo fmt --all --check` 和 `git diff --check`。下一轮入口：跑 app-server/channel/runtime 的联合矩阵后，继续查 M5/M6/M7 是否还有状态面或 smoke 面漏字段。

# 2026-05-12 protocol error surface 联合矩阵复验
- 在 app-server/channel 非零 tool protocol error 高层回归补齐后，本轮重跑联合矩阵：`cargo test -q --test app_server_tests --test cli_channel_tests --test runtime_report_tests --test kernel_status_tests` 全部通过，覆盖 11/26 runtime surface、protocol error artifact、app-server/channel 非零错误输出、status surface。
- 同步完整跑通 `sh scripts/chuang-mvp-smoke.sh`，确认 MVP/second-test 基础门禁仍接受 `runtime_meta.tool_protocol_errors_json` 与 `tool_protocol_error_count`。下一轮入口：继续扫 M5/M6/M7 的 goal/subagent 状态面，重点看 goal run/readiness 是否还缺本轮 protocol artifact 的只读摘要。
