| 模块 | 文件数 | 主要入口 | 职责 |
|---|---|---|---|
| actuator | 2 | struct CommandActuator, struct FakeActuator | （无 doc，2 文件，见文件表） |
| actuator.rs | 1 | enum ObserveTarget | （无 doc，1 文件，见文件表） |
| agent_runtime.rs | 1 | struct RuntimeRequest | （无 doc，1 文件，见文件表） |
| app_server.rs | 1 | - | （无 doc，1 文件，见文件表） |
| app_server_service.rs | 1 | const APP_SERVER_SERVICE_NAME | （无 doc，1 文件，见文件表） |
| atomic_tool.rs | 1 | enum AtomicToolKind | Short always-on catalog (progressive disclosure layer 1). |
| benchmark.rs | 1 | enum BenchmarkCapability | Chuang capability benchmark prototype (Penguin methodology, Phase A). Minimal benchmark loop: definition -> isolated statement/rubric -> scoreboard. The Target agent sees only the statement; the rubric is private (0600) so the scored agent cannot game the rubric. This module is intentionally deterministic and model-free for the prototype: an external Evaluator (later a subagent / DeepSeek) produces per-case scores; this module records, aggregates, and version-rolls the scoreboard. |
| benchmark_evaluator.rs | 1 | struct CaseAnswer | Model-backed automatic evaluator for Chuang capability benchmarks. Phase A evaluator: reads the private rubric (0600), asks a provider model to score the Target agent's answer against it, and parses the structured JSON score. The Target never sees the rubric; the Evaluator does. |
| brand_theme.rs | 1 | const BG | 创 · 产品视觉主色（雷蛇绿） **已定稿：主基调 = Razer Green。** 这是产品特色，不是临时装饰。 终端 TUI、后续 GUI/文档配色一律以本文件 token 为准。 色值来源：Razer 品牌绿常用值 RGB(68, 214, 44) / 近似 #44D62C。 老爸若用 Figma 出终稿色板，只改这里的常量，不要在各处硬编码散落 RGB。 |
| browser_read.rs | 1 | const BROWSER_READ_CONTRACT_VERSION | Resolve the live CDP adapter (no side effects). Order: 1. `CHUANG_CDP_PORT` if set and parseable 2. managed headless port file from `scripts/chuang-headless-chrome.sh` 3. structured unavailable error otherwise |
| browser_worker | 10 | struct DeepSeekWebAdapter, mod deepseek_web, struct BrowserWorkerCoordinator, fn stable_content_hash, mod adapters, struct OpenCliCommandSpec | Browser-backed external worker adapter line. `browser_worker` belongs to the adapter/plugin side of Chuang Agent. It can drive browser surfaces such as DeepSeek Web through fake, injected, or opencli-backed drivers, but the core runtime must continue to depend on generic provider/subagent/actuator ports instead of this module directly. |
| capability_primer.rs | 1 | const DEFAULT_CAPABILITY_PRIMER_ID | （无 doc，1 文件，见文件表） |
| channel_adapter.rs | 1 | struct ChannelInboundMessage | （无 doc，1 文件，见文件表） |
| chuang_kernel.rs | 1 | struct ChuangKernelConfig | 记忆本轮并附额外元数据标签（如情感状态 emotion_axes/emotion_state）。 标签进入 MemoryRecord.metadata，供未来检索按情绪维度召回。 |
| cli_approval.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_args.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_benchmark.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_browser.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_channel.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_config.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_console.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_control.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_doctor.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_emotion.rs | 1 | fn emotion_command | `chuang emotion` 子命令：心跳主动联系 + 状态查看。 |
| cli_experiment.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_external_ai.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_genesis.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_goal.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_memory.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_output.rs | 1 | enum ControlOutputFormat | Print a single-shot `run` (or REPL `--verbose`) result. Default is conversational: answer first, then a short meta line. Full field-wall dump (model_name/body/trace/context_*) only when `verbose`. |
| cli_plugin.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_repl_transport.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_repl_tui.rs | 1 | fn run_ratatui_repl | Ratatui REPL shell — calm chat on **chuang brand green** (Razer green). Product look (定稿): - 主基调雷蛇绿，见 `brand_theme`；禁止再散落其它主色 - 启动字模仅空会话展示，首条用户消息后清掉 - **用户发送后：本轮用户话钉在对话区顶部**（Grok 式）；本轮内容超出视口才跟到底 - thinking / 用量在输入框上方（外），模型名在右下角；框内只有 `>` + 光标 - 助手正文左缩进 2 格；用户消息右侧显示时间 - 输入 `/` 弹出 slash 命令菜单（筛选 / ↑↓ / Tab 补全 / Enter 执行） - 输入光标可左右移动 - Runtime stays in existing chuang paths; this module only paints. 改 TUI 时勿破坏上述行为；优先加字段/分支，禁止 silently 改回 scroll=MAX 跟底。 |
| cli_runtime.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_skill.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_subagent.rs | 1 | - | （无 doc，1 文件，见文件表） |
| cli_types.rs | 1 | - | （无 doc，1 文件，见文件表） |
| common | 4 | struct AuditRecord, struct TaskId, struct Timestamp | （无 doc，4 文件，见文件表） |
| context_engine | 2 | struct DeterministicContextEngine, struct SummaryCompressionContextEngine | （无 doc，2 文件，见文件表） |
| context_engine.rs | 1 | struct ContextSegment | （无 doc，1 文件，见文件表） |
| control_intent.rs | 1 | struct ControlIntentInput | （无 doc，1 文件，见文件表） |
| control_plane | 2 | struct ControlPlaneCommandResult, struct FakeControlPlane | （无 doc，2 文件，见文件表） |
| control_plane.rs | 1 | enum ManagedUnitKind | （无 doc，1 文件，见文件表） |
| control_surface.rs | 1 | struct ControlSurfaceRequest | （无 doc，1 文件，见文件表） |
| control_workflow.rs | 1 | struct ControlWorkflowRequest | （无 doc，1 文件，见文件表） |
| display_projector.rs | 1 | struct DisplayEvent | Conversational REPL (default): tools visible, no step theater. Fast path = only final answer. Slow path = tools / optional thinking when enabled. Protocol self-corrections stay off-transcript (bottom status /trace only). |
| emotion_brain.rs | 1 | struct BrainHit | EmotionBrain：情感外脑桥接（EmotionSlot ↔ GBrain）。 原则： - 情感核心（emotion_slot）不依赖外脑；外脑只做「更懂主人」的可选增强。 - GBrain 是脱敏共享脑图（只读口径），私人情绪记忆走创自己的 MemoryStore， 不写共享 wiki。这里只查询主人相关的历史偏好/上下文，摘要点 + slug 进 prompt。 只读调用本机 CLI：`agent-hub-brain-query semantic <query> <limit>`（JSON 输出）。 |
| emotion_delta.rs | 1 | trait EmotionDeltaExtractor | EmotionDeltaExtractor：对话 → 五轴情感修正量（可拔插）。 铁律：接口先行；先规则版（零模型成本、确定性强），后模型版（复用现有 provider 通道，只回几个 delta 值，不引入额外 fallback 复杂度）。 规则版是默认接入；模型版是可插增强，测试用 ScriptedResponder 即可。 |
| emotion_heartbeat.rs | 1 | struct HeartbeatPolicy | EmotionHeartbeat：情感主动联系（心跳）。 原则（贴合创可拔插/解耦铁律）： - 核心只产出「主动联系提案」写入发件箱（目录式 outbox），不直接发消息； 投递层（Chuang Feishu 桥轮询）负责真正发送到绑定会话。 - 触发门槛 / 频率 / 每日上限全部参数化（metadata.heartbeat_*），可配可调。 - 只对 `Contact` 触发发消息；Observation/FindActivity 是内部念头不打扰主人。 |
| emotion_slot.rs | 1 | fn now_rfc3339 | EmotionSlot：可拔插的情感槽位。 设计来源：jiwen（积温）五轴连续状态模型（MIT）。 - connection：连接需求（0..=1，多久没听到主人） - pride：骄傲（-1..=1） - valence：愉悦度（-1..=1，Russell 环状模型） - arousal：唤醒度（-1..=1，Russell 环状模型） - immersion：沉浸度（0..=1） 铁律：接口先行，实现第二；每个 slot 先 Fake 后真实；不依赖具体存储/模型。 |
| emotion_store.rs | 1 | struct PersistedEmotionState | EmotionStore：情感状态跨轮持久化（可拔插）。 原则：情感核心不依赖具体存储；这里提供最简 JSON 文件实现。 - 位置：与 db_path 同目录的 `emotion-state.json`（主人/身份维度全局，不按 session 分片）。 - 保存：每轮 turn 结束后 snapshot() → PersistedEmotionState。 - 恢复：启动时读回 axes + 上次心跳时间，用真实流逝分钟数 tick（jiwen 连接增长）。 - 失败永远静默（load 失败用默认状态继续，save 失败只记日志，不阻断主流程）。 |
| evolution_loop.rs | 1 | enum EvolutionBridgeError | evolver 外环的运行时接线：把 ledger 运行时事件桥接进 evolver 观察流， 并在每个 agent turn 结束后自动驱动 detect → propose → governance → apply。 设计约束： - 桥接映射是纯函数、可单测；每条 ledger 事件要么映射成一条 evolver 事件，要么被忽略。 - 外环驱动不 panic：任何阶段失败都收集进结构化的 `OuterLoopReport`。 - 治理是强制门禁：`apply_rule_change` 只有在治理批准后才被调用，且 apply 内部 仍会再次执行治理门禁（双保险，绝不绕过治理直接落盘）。 |
| external_ai_dispatch.rs | 1 | struct ExternalAiDispatchRequest | （无 doc，1 文件，见文件表） |
| external_knowledge.rs | 1 | enum ExternalKnowledgeSource | （无 doc，1 文件，见文件表） |
| genesis_actuator | 2 | struct GenesisCommandSpec, struct FakeGenesisActuator | （无 doc，2 文件，见文件表） |
| genesis_actuator.rs | 1 | enum GenesisChannel | （无 doc，1 文件，见文件表） |
| goal_dispatch.rs | 1 | struct GoalDispatchReceipt | （无 doc，1 文件，见文件表） |
| goal_mode.rs | 1 | struct GoalSpec | 验收证据定义：目标完成时磁盘上必须出现的内容。 - `path`：证据文件路径（相对 goal root 解析）。 - `min_lines`：文件内容至少多少行（非空壳检查；None 表示不检查行数）。 - `min_content`：文件内容必须包含的子串（如 `RESULT=PASS`；None 表示不检查内容）。 - `description`：人类可读说明（show / diagnostics 展示用）。 |
| goal_run.rs | 1 | const ACCEPTANCE_COMMAND_TIMEOUT_SECS | 命令类验收检查的执行超时（秒）。`goal verify` 显式执行验收命令时兜底， 避免声明错误的命令把 operator 卡死。 |
| governance | 2 | struct MarkdownRuleSet, struct StaticRuleGovernance | （无 doc，2 文件，见文件表） |
| governance.rs | 1 | enum ActionKind | （无 doc，1 文件，见文件表） |
| hermes_memory | 1 | struct FileDualFileMemoryStore | （无 doc，1 文件，见文件表） |
| hermes_memory.rs | 1 | const DEFAULT_USER_MEMORY_MAX_CHARS | （无 doc，1 文件，见文件表） |
| identity_registry.rs | 1 | struct AgentIdentity | （无 doc，1 文件，见文件表） |
| kernel_status.rs | 1 | struct ChuangMvpStatus | （无 doc，1 文件，见文件表） |
| knowledge_read.rs | 1 | const KNOWLEDGE_READ_CONTRACT_VERSION | （无 doc，1 文件，见文件表） |
| lib.rs | 1 | mod actuator | Adapter/plugin line for browser-backed external workers. This module is intentionally exported for experiments and future plugins, but it must not become a dependency of the core runtime chain. |
| lifecycle | 5 | const RUNTIME_CHECKPOINT_SCHEMA_VERSION, trait LifecycleStateMachine, enum LifecycleState, struct LifecycleTransitionTable | （无 doc，5 文件，见文件表） |
| live_adapter_gate.rs | 1 | enum LiveAdapterSlot | （无 doc，1 文件，见文件表） |
| live_subagent_rehearsal.rs | 1 | struct LiveSubagentRehearsalInput | （无 doc，1 文件，见文件表） |
| main.rs | 1 | - | （无 doc，1 文件，见文件表） |
| mcp_fake_adapter.rs | 1 | struct McpToolSpec | （无 doc，1 文件，见文件表） |
| memory_admission.rs | 1 | const DEFAULT_MEMORY_WRITE_MAX_CHARS | （无 doc，1 文件，见文件表） |
| memory_policy | 5 | enum BudgetMode, struct FreedBytes, struct EvictionPlan, struct ReservationToken | （无 doc，5 文件，见文件表） |
| memory_recall.rs | 1 | struct RecallRequest | （无 doc，1 文件，见文件表） |
| memory_store | 1 | struct InMemoryMemoryStore | （无 doc，1 文件，见文件表） |
| memory_store.rs | 1 | struct MemoryRecord | 文本匹配模式。 - `Token`（默认）：整句精确包含优先，否则按 token 滑窗召回（自然语言改述也能命中）。 用于 agent 自动记忆召回（recall 注入 context），容忍改述。 - `ExactPhrase`：仅整句精确包含才命中，行为确定可预期。 用于 CLI 诊断搜索（memory session search / maintenance / lim extract）， 避免近似的相似内容（如"锚点A" vs "锚点B"）被误判为命中。 |
| memory_store_sqlite.rs | 1 | struct SqliteMemoryStore | （无 doc，1 文件，见文件表） |
| norm_layer.rs | 1 | const DOCTRINE_CARD_ID | Prompt / norm layering: thick on disk, thin in context. Includes distilled CC harness discipline plus dad's operating theorems: Occam (dev subtract) / Murphy (accept add) / Coase (delegate) / grill-clarify / no optional commentary. Plus thin closed-loop control (engineering cybernetics) as on-demand skill only. |
| operator_approval.rs | 1 | const OPERATOR_APPROVAL_TICKET_SCHEMA_VERSION | （无 doc，1 文件，见文件表） |
| path_utils.rs | 1 | fn normalize_path_lexically | （无 doc，1 文件，见文件表） |
| permission_profile_slot.rs | 1 | enum PermissionProfileId | （无 doc，1 文件，见文件表） |
| plugin_registry.rs | 1 | struct PluginRegistry | （无 doc，1 文件，见文件表） |
| provider_openai_compatible.rs | 1 | struct ProviderConfigError | （无 doc，1 文件，见文件表） |
| responder | 2 | struct FakeResponder, struct ScriptedResponder | （无 doc，2 文件，见文件表） |
| responder.rs | 1 | struct ResponderRequest | （无 doc，1 文件，见文件表） |
| runtime_config.rs | 1 | const DEFAULT_WORKSPACE_ROOT | 默认工作区根（仅作为 RuntimeConfig::new 的初始哨兵值； 实际生效值由 app_server 按 base_dir / 环境变量归一化）。 |
| runtime_config_file.rs | 1 | enum RuntimeConfigFileError | （无 doc，1 文件，见文件表） |
| runtime_event_ledger.rs | 1 | const RUNTIME_EVENT_SCHEMA_VERSION | （无 doc，1 文件，见文件表） |
| runtime_report.rs | 1 | fn build_runtime_report | （无 doc，1 文件，见文件表） |
| secret_redaction.rs | 1 | struct RedactedText | （无 doc，1 文件，见文件表） |
| self_experiment.rs | 1 | struct ExperimentRequest | （无 doc，1 文件，见文件表） |
| session_archive.rs | 1 | struct SessionTurnArchive | （无 doc，1 文件，见文件表） |
| skill_evolver | 5 | enum SkillLifecycleStatus, struct DryRunProposalEvolver, struct FailureDetectorConfig, struct NoopEvolver, enum RuleChangeKind | 观察流持久化文件：`<skill_root>/.evolver/observed-events.jsonl`（与规则 修改审计 journal 同一目录）。跨 turn 累积失败证据依赖它：每个 CLI 进程 结束时观察流不会丢失，下次启动恢复，`min_repeats>=2` 才可能在多 turn 上真实触发。 |
| skill_evolver.rs | 1 | struct RuntimeEvent | （无 doc，1 文件，见文件表） |
| slot_registry.rs | 1 | struct RuntimeSlots | canonical 外环的治理门禁槽。治理是强制门禁：规则修改只有经 `RuleChangeGovernance` 批准后才允许走写路径，本槽位本身从不写盘。 |
| subagent_queue.rs | 1 | struct FileSubagentQueueConfig | （无 doc，1 文件，见文件表） |
| subagent_report | 4 | fn governance_metadata, enum ExecutionStatus, const DEFAULT_REPORT_SIZE_LIMIT_BYTES, enum ReportRejectReason | （无 doc，4 文件，见文件表） |
| subagent_spawner | 2 | struct FakeSubagentSpawner, struct QueuedSubagentSpawner | （无 doc，2 文件，见文件表） |
| subagent_spawner.rs | 1 | const QUEUED_STEER_MESSAGES_METADATA_KEY | （无 doc，1 文件，见文件表） |
| subagent_tree_events.rs | 1 | struct SubagentTreeEventBuilder | （无 doc，1 文件，见文件表） |
| subagent_tree_ledger.rs | 1 | struct SubagentTreePolicy | （无 doc，1 文件，见文件表） |
| terminal_event.rs | 1 | enum TerminalEvent | （无 doc，1 文件，见文件表） |
| tool_loop_meta.rs | 1 | struct ToolLoopMeta | （无 doc，1 文件，见文件表） |
| tool_registry_slot.rs | 1 | struct ToolDescriptor | （无 doc，1 文件，见文件表） |
| tool_runtime.rs | 1 | enum ToolCall | Default worker model for queued subagent Codex runners. Aligned with the example-provider/zen-sub2 account group's actually supported model (deepseek-v4-flash); gpt-5.6-luna/terra both return 404 there. Pluggable: override with `CHUANG_CODEX_RUNNER_MODEL`. |
| turn_context.rs | 1 | struct TurnContextSnapshot | （无 doc，1 文件，见文件表） |
| unified_execution_slot.rs | 1 | const UNIFIED_EXECUTION_SCHEMA_VERSION | （无 doc，1 文件，见文件表） |
| workspace_file_adapter.rs | 1 | struct WorkspaceListResult | （无 doc，1 文件，见文件表） |