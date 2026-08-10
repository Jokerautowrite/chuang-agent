use std::fs;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpListener};
use std::path::PathBuf;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};

use chuang_agent::actuator::{Actuator, ObserveTarget};
use chuang_agent::control_plane::{ControlAction, ControlPlane, ControlRequest, ManagedUnitStatus};
use chuang_agent::governance::{ActionKind, Governance, ProposedAction, RiskDecision};
use chuang_agent::provider_openai_compatible::ProviderTransport;
use chuang_agent::responder::{Responder, ResponderRequest};
use chuang_agent::runtime_config::{
    EvolutionConfig, OpenAICompatibleConfig, ProviderConfig, ProviderFallbackPolicy, RuntimeConfig,
    SubagentConfig, SubagentQueueConfig,
};
use chuang_agent::skill_evolver::{
    EvolutionScope, GovernanceContext, RuntimeEvent, RuntimeEventKind, SkillEvolver,
};
use chuang_agent::slot_registry::{
    build_genesis_actuator, build_provider_responder, build_runtime_slots, summarize_runtime_slots,
    EmotionSlotRuntime, SubagentRuntimeSlot,
};
use chuang_agent::subagent_report::{ExecutionStatus, ResourceUsage, SubagentReport};
use chuang_agent::subagent_spawner::{
    ContextIsolation, RunId, SpawnRequest, SubagentError, SubagentSpawner, SubagentToolPolicy,
    QUEUED_STEER_MESSAGES_METADATA_KEY,
};
use chuang_agent::tool_runtime::ToolCall;
use chuang_agent::{common::AgentId, common::ReportId, common::TaskId, common::Timestamp};

fn temp_queue_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-agent-slot-registry-{name}-{nanos}"))
}

fn spawn_provider_error_server(
    status_line: &'static str,
    body: &'static str,
) -> (SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    let server = thread::spawn(move || {
        // The adapter retries transient HTTP statuses (429/5xx) up to
        // MAX_PROVIDER_ATTEMPTS times. Keep serving the same error on every
        // attempt so the test observes the final classified failure instead
        // of a connection-refused transport error on the second attempt.
        for _ in 0..3 {
            let Ok((mut stream, _)) = listener.accept() else {
                break;
            };
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer);
            let response = format!(
                "HTTP/1.1 {status_line}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = stream.write_all(response.as_bytes());
        }
    });
    (address, server)
}

#[test]
fn slot_registry_builds_all_current_runtime_slots_from_config() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    let mut slots = build_runtime_slots(&config).expect("default slots should build");

    let decision = slots
        .governance
        .classify(&ProposedAction {
            action_id: "action-1".to_string(),
            kind: ActionKind::Observe,
            target: "screen".to_string(),
            summary: "observe screen".to_string(),
        })
        .expect("governance should classify");
    let observation = slots
        .actuator
        .observe(ObserveTarget::Screen)
        .expect("fake actuator should observe");
    let proposals = slots
        .evolution
        .propose(EvolutionScope {
            agent_id: "xiaoce".to_string(),
            task_kind: None,
            max_proposals: 1,
        })
        .expect("noop evolver should accept valid scope");
    let units = slots.control_plane.list_units();
    let execution_mapping = slots
        .execution
        .registry()
        .mapping_for_call(&ToolCall::ReadFile {
            path: "README.md".to_string(),
        });

    assert!(matches!(decision, RiskDecision::Allowed { .. }));
    assert_eq!(execution_mapping.atomic_tool_name, Some("file_read"));
    assert_eq!(observation.summary, "fake observation");
    assert!(proposals.is_empty());
    assert!(units.iter().any(|unit| unit.display_name == "小策"));
}

#[test]
fn slot_registry_builds_subagent_spawner_that_can_spawn_and_collect() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let mut slots = build_runtime_slots(&config).expect("default slots should build");

    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-1".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "验证 slot registry".to_string(),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 512,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("spawn should succeed");
    let report = slots
        .subagent
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .expect("fake report should exist");

    assert_eq!(report.agent_id, receipt.agent_id);
    assert_eq!(report.task_id.0, "task-1");
}

#[test]
fn slot_registry_control_plane_slot_applies_state_and_model_changes() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let mut slots = build_runtime_slots(&config).expect("default slots should build");

    let restart = slots
        .control_plane
        .apply(ControlRequest {
            unit_id: "codex-feishu-bot.service".to_string(),
            action: ControlAction::Restart,
            reason: "slot contract restart".to_string(),
        })
        .expect("control slot should restart services");
    let model_change = slots
        .control_plane
        .apply(ControlRequest {
            unit_id: "codex-xiaoce".to_string(),
            action: ControlAction::ChangeModel {
                model_name: "gpt-5.5".to_string(),
            },
            reason: "slot contract model change".to_string(),
        })
        .expect("control slot should change agent model");

    assert_eq!(restart.next_status, ManagedUnitStatus::Running);
    assert_eq!(model_change.model_name, Some("gpt-5.5".to_string()));
    assert!(slots.control_plane.list_units().iter().any(|unit| {
        unit.unit_id == "codex-xiaoce" && unit.model_name.as_deref() == Some("gpt-5.5")
    }));
}

#[test]
fn slot_registry_summary_matches_runtime_config_slot_kinds() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));

    let summary = summarize_runtime_slots(&config);

    assert_eq!(summary.provider, "fake");
    assert_eq!(summary.governance, "static_rule");
    assert_eq!(summary.execution, "generic_agent_mvp");
    assert_eq!(summary.actuator, "fake");
    assert_eq!(summary.subagent, "fake");
    assert_eq!(summary.evolution, "noop");
    assert_eq!(summary.control_plane, "fake_local");
}

#[test]
fn slot_registry_builds_dry_run_evolution_slot_from_runtime_config() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.evolution = EvolutionConfig::DryRun;

    let mut slots = build_runtime_slots(&config).expect("dry_run evolution slots should build");
    let summary = summarize_runtime_slots(&config);
    assert_eq!(summary.evolution, "dry_run");

    let receipt = slots
        .evolution
        .observe(RuntimeEvent {
            event_id: "event-1".to_string(),
            task_id: "task-1".to_string(),
            kind: RuntimeEventKind::TurnCompleted,
            summary: "completed slot evolution dry run".to_string(),
            metadata: Default::default(),
        })
        .expect("dry-run evolver should accept observed event");
    assert!(receipt.accepted);

    let proposals = slots
        .evolution
        .propose(EvolutionScope {
            agent_id: "xiaoce".to_string(),
            task_kind: None,
            max_proposals: 1,
        })
        .expect("dry-run evolver should propose after observe");

    assert_eq!(proposals.len(), 1);
    assert!(proposals[0].dry_run);
    assert!(!proposals[0].writes_skills);
    assert!(proposals[0].requires_approval);
}

#[test]
fn slot_registry_builds_provider_responder_from_config() {
    let fake = build_provider_responder(&ProviderConfig::Fake {
        provider_id: "fake-runtime".to_string(),
        model_name: "stub-responder".to_string(),
    })
    .expect("fake provider should build");
    let fake_output = fake.generate(&ResponderRequest {
        prompt: "prompt".to_string(),
        user_input: "hello".to_string(),
        recall_hit_count: 0,
    });
    assert_eq!(fake_output.model_name, "stub-responder");
    assert_eq!(fake_output.meta.provider.as_deref(), Some("fake-responder"));

    let openai =
        build_provider_responder(&ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
            provider_id: "custom-openai".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "test-key".to_string(),
            model_name: "gpt-4.1-mini".to_string(),
            transport: ProviderTransport::Stub,
            endpoint: Default::default(),
            reasoning_effort: None,
            request_timeout_ms: None,
            tls_ca_cert_path: None,
        }))
        .expect("openai-compatible provider should build");
    let provider = openai.provider();
    assert_eq!(provider.provider_id, "custom-openai");
    assert_eq!(provider.model_name, "gpt-4.1-mini");
}

#[test]
fn slot_registry_provider_fallback_uses_secondary_on_primary_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    drop(listener);

    let provider = build_provider_responder(&ProviderConfig::Fallback {
        primary: Box::new(ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
            provider_id: "primary-openai".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: "test-key".to_string(),
            model_name: "primary-model".to_string(),
            transport: ProviderTransport::Http,
            endpoint: Default::default(),
            reasoning_effort: None,
            request_timeout_ms: None,
            tls_ca_cert_path: None,
        })),
        fallback: Box::new(ProviderConfig::Fake {
            provider_id: "fallback-fake".to_string(),
            model_name: "fallback-model".to_string(),
        }),
        policy: ProviderFallbackPolicy::default(),
    })
    .expect("fallback provider should build");

    let output = provider.generate(&ResponderRequest {
        prompt: "prompt".to_string(),
        user_input: "fallback smoke".to_string(),
        recall_hit_count: 0,
    });

    assert_eq!(output.model_name, "fallback-model");
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_used")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_from")
            .map(String::as_str),
        Some("primary-openai")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_primary_retryable")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_primary_error_class")
            .map(String::as_str),
        Some("transport")
    );
}

#[test]
fn slot_registry_marks_unconfigured_fallback_on_model_capacity_error() {
    let (address, server) = spawn_provider_error_server(
        "429 Too Many Requests",
        r#"{"error":{"message":"Selected model is at capacity"}}"#,
    );

    let provider =
        build_provider_responder(&ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
            provider_id: "primary-openai".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: "test-key".to_string(),
            model_name: "primary-model".to_string(),
            transport: ProviderTransport::Http,
            endpoint: Default::default(),
            reasoning_effort: None,
            request_timeout_ms: None,
            tls_ca_cert_path: None,
        }))
        .expect("provider should build");

    let output = provider.generate(&ResponderRequest {
        prompt: "prompt".to_string(),
        user_input: "capacity smoke".to_string(),
        recall_hit_count: 0,
    });
    server.join().expect("server thread should finish");

    assert_eq!(output.model_name, "primary-model");
    assert!(output.body.contains("PROVIDER_HTTP_ERROR"));
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_configured")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_used")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_failure_reason_code")
            .map(String::as_str),
        Some("model_capacity")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_failure_category")
            .map(String::as_str),
        Some("capacity")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_retryable")
            .map(String::as_str),
        Some("true")
    );
}

#[test]
fn slot_registry_provider_fallback_preserves_primary_capacity_reason() {
    let (address, server) = spawn_provider_error_server(
        "429 Too Many Requests",
        r#"{"error":{"message":"Selected model is at capacity"}}"#,
    );

    let provider = build_provider_responder(&ProviderConfig::Fallback {
        primary: Box::new(ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
            provider_id: "primary-openai".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: "test-key".to_string(),
            model_name: "primary-model".to_string(),
            transport: ProviderTransport::Http,
            endpoint: Default::default(),
            reasoning_effort: None,
            request_timeout_ms: None,
            tls_ca_cert_path: None,
        })),
        fallback: Box::new(ProviderConfig::Fake {
            provider_id: "fallback-fake".to_string(),
            model_name: "fallback-model".to_string(),
        }),
        policy: ProviderFallbackPolicy::default(),
    })
    .expect("fallback provider should build");

    let output = provider.generate(&ResponderRequest {
        prompt: "prompt".to_string(),
        user_input: "capacity fallback smoke".to_string(),
        recall_hit_count: 0,
    });
    server.join().expect("server thread should finish");

    assert_eq!(output.model_name, "fallback-model");
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_configured")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_used")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_primary_failure_reason_code")
            .map(String::as_str),
        Some("model_capacity")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_primary_failure_category")
            .map(String::as_str),
        Some("capacity")
    );
}

#[test]
fn slot_registry_provider_fallback_does_not_mask_unlisted_error() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("listener should bind");
    let address = listener.local_addr().expect("local addr should exist");
    drop(listener);

    let provider = build_provider_responder(&ProviderConfig::Fallback {
        primary: Box::new(ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
            provider_id: "primary-openai".to_string(),
            base_url: format!("http://{address}/v1"),
            api_key: "test-key".to_string(),
            model_name: "primary-model".to_string(),
            transport: ProviderTransport::Http,
            endpoint: Default::default(),
            reasoning_effort: None,
            request_timeout_ms: None,
            tls_ca_cert_path: None,
        })),
        fallback: Box::new(ProviderConfig::Fake {
            provider_id: "fallback-fake".to_string(),
            model_name: "fallback-model".to_string(),
        }),
        policy: ProviderFallbackPolicy {
            on_retryable: false,
            status_codes: Vec::new(),
            error_classes: Vec::new(),
        },
    })
    .expect("fallback provider should build");

    let output = provider.generate(&ResponderRequest {
        prompt: "prompt".to_string(),
        user_input: "fallback policy smoke".to_string(),
        recall_hit_count: 0,
    });

    assert_eq!(output.model_name, "primary-model");
    assert_ne!(
        output
            .meta
            .extra
            .get("provider_fallback_used")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        output
            .meta
            .extra
            .get("provider_fallback_used")
            .map(String::as_str),
        Some("false")
    );
}

#[test]
fn slot_registry_builds_genesis_actuator_from_config() {
    let actuator = build_genesis_actuator(chuang_agent::genesis_actuator::GenesisConfig {
        program: "printf".to_string(),
        profile_dir: PathBuf::from("/tmp/chuang-genesis-slot-profile"),
        cdp_port: 9333,
        timeout_ms: 12_345,
    });

    let primary = actuator.primary_spec("查询");
    let fallback = actuator.fallback_spec("查询");

    assert_eq!(primary.program, "printf");
    assert_eq!(primary.channel.as_str(), "user_data_dir");
    assert!(primary.args.contains(&"--user-data-dir".to_string()));
    assert!(primary
        .args
        .contains(&"/tmp/chuang-genesis-slot-profile".to_string()));
    assert!(primary.args.contains(&"12345".to_string()));
    assert_eq!(fallback.channel.as_str(), "cdp");
    assert!(fallback.args.contains(&"9333".to_string()));
}

#[test]
fn slot_registry_rejects_invalid_provider_config_before_adapter_use() {
    let fake_err = build_provider_responder(&ProviderConfig::Fake {
        provider_id: "fake-runtime".to_string(),
        model_name: String::new(),
    })
    .expect_err("fake provider with empty model should be rejected");
    assert_eq!(fake_err.field, "provider.model_name");

    let openai_err =
        build_provider_responder(&ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
            provider_id: "custom-openai".to_string(),
            base_url: String::new(),
            api_key: "test-key".to_string(),
            model_name: "gpt-4.1-mini".to_string(),
            transport: ProviderTransport::Stub,
            endpoint: Default::default(),
            reasoning_effort: None,
            request_timeout_ms: None,
            tls_ca_cert_path: None,
        }))
        .expect_err("openai-compatible provider with empty base_url should be rejected");
    assert_eq!(openai_err.field, "provider.base_url");
}

#[test]
fn slot_registry_rejects_runtime_with_invalid_provider_slot() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.provider = ProviderConfig::OpenAICompatible(OpenAICompatibleConfig {
        provider_id: "custom-openai".to_string(),
        base_url: "https://api.example.com/v1".to_string(),
        api_key: String::new(),
        model_name: "gpt-4.1-mini".to_string(),
        transport: ProviderTransport::Stub,
        endpoint: Default::default(),
        reasoning_effort: None,
        request_timeout_ms: None,
        tls_ca_cert_path: None,
    });

    let err = build_runtime_slots(&config).expect_err("invalid provider should reject slots");

    assert_eq!(err.field, "provider.api_key");
}

#[test]
fn slot_registry_can_build_queued_external_subagent_slot() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued"),
    };

    let mut slots = build_runtime_slots(&config).expect("queued slot should build");
    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-queued".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "生成外部派发任务".to_string(),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 512,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("queued spawn should succeed");

    assert_eq!(receipt.run_id.0, "queued-run-1");
    assert!(config
        .subagent_queue
        .root
        .join("dispatch")
        .join("queued-run-1.json")
        .exists());
    assert!(slots
        .subagent
        .collect(&receipt.run_id)
        .expect("collect should succeed")
        .is_none());
    assert_eq!(summarize_runtime_slots(&config).subagent, "queued_external");
}

#[test]
fn slot_registry_rejects_queued_external_subagent_with_invalid_queue_root() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: PathBuf::new(),
    };

    let err = build_runtime_slots(&config).expect_err("empty queue root should reject slots");

    assert_eq!(err.field, "subagent_queue.root");
}

#[test]
fn slot_registry_queued_external_rejects_invalid_spawn_without_dispatch_file() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-invalid-spawn"),
    };

    let mut slots = build_runtime_slots(&config).expect("queued slot should build");
    let err = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-invalid-spawn".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "坏请求不能写入外部队列".to_string(),
            tool_policy: SubagentToolPolicy::Analyze,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 512,
            idle_timeout_ms: 30_000,
            recursive_spawn: true,
            metadata: Default::default(),
        })
        .expect_err("invalid queued spawn should be rejected before dispatch write");

    assert!(matches!(err, SubagentError::InvalidRequest(_)));
    assert!(!config
        .subagent_queue
        .root
        .join("dispatch")
        .join("queued-run-1.json")
        .exists());
}

#[test]
fn slot_registry_queued_external_spawn_write_failure_rolls_back_in_memory_state() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-spawn-rollback"),
    };

    let dispatch_path = config
        .subagent_queue
        .root
        .join("dispatch")
        .join("queued-run-1.json");
    let tmp_path = dispatch_path.with_extension("tmp");
    fs::create_dir_all(&tmp_path).expect("tmp path blocker should be created");

    let mut slots = build_runtime_slots(&config).expect("queued slot should build");
    let error = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-queued-spawn-rollback".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "失败时不要留下 queued ghost 状态".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 768,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect_err("spawn should fail when dispatch artifact write fails");

    assert!(
        matches!(error, SubagentError::InvalidRequest(message) if message.contains("StorageUnavailable"))
    );
    assert!(!dispatch_path.exists());

    match &slots.subagent {
        SubagentRuntimeSlot::QueuedExternal { spawner, queue } => {
            assert!(spawner.pending_dispatches().is_empty());
            assert!(spawner.state(&RunId("queued-run-1".to_string())).is_none());
            assert!(queue
                .read_dispatch(&RunId("queued-run-1".to_string()))
                .expect("queue read should still work")
                .is_none());
        }
        SubagentRuntimeSlot::Fake(_) => panic!("expected queued external subagent slot"),
    }

    fs::remove_dir_all(&tmp_path).expect("tmp path blocker should be removable");
    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-queued-spawn-rollback-retry".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "重试时 run id 不应跳号".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 768,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("retry spawn should succeed");

    assert_eq!(receipt.run_id.0, "queued-run-1");
    assert!(dispatch_path.exists());
}

#[test]
fn slot_registry_queued_external_slot_attaches_report_from_queue_on_collect() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-report"),
    };

    let mut slots = build_runtime_slots(&config).expect("queued slot should build");
    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-queued-report".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "回收外部子代理报告".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 768,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("queued spawn should succeed");
    let report = queued_slot_report(&receipt.run_id.0, &receipt.agent_id);

    match &slots.subagent {
        SubagentRuntimeSlot::QueuedExternal { queue, .. } => {
            queue
                .write_report(&receipt.run_id, &report)
                .expect("report should be written to queue");
        }
        SubagentRuntimeSlot::Fake(_) => panic!("expected queued external subagent slot"),
    }

    let collected = slots
        .subagent
        .collect(&receipt.run_id)
        .expect("queued collect should attach report")
        .expect("queued report should be available");

    assert_eq!(collected, report);
    assert_eq!(collected.summary, "queued slot worker completed");
}

#[test]
fn slot_registry_queued_external_steer_rewrites_dispatch_artifact() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-steer"),
    };

    let mut slots = build_runtime_slots(&config).expect("queued slot should build");
    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-queued-steer".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "把 steer 写回 queue artifact".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 768,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("queued spawn should succeed");

    slots
        .subagent
        .steer(&receipt.run_id, "先补失败路径".to_string())
        .expect("first steer should persist");
    slots
        .subagent
        .steer(&receipt.run_id, "再补回执校验".to_string())
        .expect("second steer should persist");

    let persisted_dispatch = match &slots.subagent {
        SubagentRuntimeSlot::QueuedExternal { queue, .. } => queue
            .read_dispatch(&receipt.run_id)
            .expect("dispatch should read from queue")
            .expect("dispatch should exist on disk"),
        SubagentRuntimeSlot::Fake(_) => panic!("expected queued external subagent slot"),
    };
    let steer_messages = serde_json::from_str::<Vec<String>>(
        persisted_dispatch
            .metadata
            .get(QUEUED_STEER_MESSAGES_METADATA_KEY)
            .expect("persisted dispatch should carry steer messages"),
    )
    .expect("persisted steer metadata should decode");

    assert_eq!(
        steer_messages,
        vec!["先补失败路径".to_string(), "再补回执校验".to_string()]
    );
}

#[test]
fn slot_registry_queued_external_steer_write_failure_rolls_back_in_memory_state() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-steer-rollback"),
    };

    let mut slots = build_runtime_slots(&config).expect("queued slot should build");
    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-queued-steer-rollback".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "失败时不要让 steer 留在内存态".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 768,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("queued spawn should succeed");

    slots
        .subagent
        .steer(&receipt.run_id, "先补失败路径".to_string())
        .expect("first steer should persist");

    let dispatch_path = config
        .subagent_queue
        .root
        .join("dispatch")
        .join(format!("{}.json", receipt.run_id.0));
    let tmp_path = dispatch_path.with_extension("tmp");
    fs::create_dir_all(&tmp_path).expect("tmp path blocker should be created");

    let error = slots
        .subagent
        .steer(&receipt.run_id, "这条不应残留".to_string())
        .expect_err("second steer should fail when queue artifact write fails");
    assert!(
        matches!(error, SubagentError::InvalidRequest(message) if message.contains("StorageUnavailable"))
    );

    let (persisted_dispatch, in_memory_messages, in_memory_dispatch) = match &slots.subagent {
        SubagentRuntimeSlot::QueuedExternal { spawner, queue } => (
            queue
                .read_dispatch(&receipt.run_id)
                .expect("dispatch should remain readable")
                .expect("dispatch should still exist on disk"),
            spawner
                .steer_messages(&receipt.run_id)
                .expect("run should remain in memory")
                .to_vec(),
            spawner
                .dispatch_snapshot(&receipt.run_id)
                .expect("dispatch snapshot should remain available"),
        ),
        SubagentRuntimeSlot::Fake(_) => panic!("expected queued external subagent slot"),
    };

    let persisted_messages = serde_json::from_str::<Vec<String>>(
        persisted_dispatch
            .metadata
            .get(QUEUED_STEER_MESSAGES_METADATA_KEY)
            .expect("persisted dispatch should keep steer messages"),
    )
    .expect("persisted steer metadata should decode");
    let in_memory_dispatch_messages = serde_json::from_str::<Vec<String>>(
        in_memory_dispatch
            .metadata
            .get(QUEUED_STEER_MESSAGES_METADATA_KEY)
            .expect("in-memory dispatch should keep steer messages"),
    )
    .expect("in-memory steer metadata should decode");

    assert_eq!(persisted_messages, vec!["先补失败路径".to_string()]);
    assert_eq!(in_memory_messages, vec!["先补失败路径".to_string()]);
    assert_eq!(
        in_memory_dispatch_messages,
        vec!["先补失败路径".to_string()]
    );
}

#[test]
fn slot_registry_queued_external_restores_pending_dispatches_and_continues_numbering() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-restore-numbering"),
    };

    let dispatch_dir = config.subagent_queue.root.join("dispatch");
    fs::create_dir_all(&dispatch_dir).expect("dispatch dir should be created");
    fs::write(
        dispatch_dir.join("queued-run-7.json"),
        r#"{
  "run_id":"queued-run-7",
  "agent_id":"worker-7",
  "task_id":"task-restored-7",
  "parent_agent_id":"xiaoce",
  "agent_name":"worker",
  "task":"恢复编号 7",
  "tool_policy":"Execute",
  "context_isolation":"Isolated",
  "token_budget":768,
  "idle_timeout_ms":30000,
  "recursive_spawn":false,
  "metadata":{}
}"#,
    )
    .expect("dispatch should be written");
    fs::write(
        dispatch_dir.join("persisted-run-alpha.json"),
        r#"{
  "run_id":"persisted-run-alpha",
  "agent_id":"worker-alpha",
  "task_id":"task-restored-alpha",
  "parent_agent_id":"xiaoce",
  "agent_name":"worker",
  "task":"恢复自定义编号",
  "tool_policy":"Execute",
  "context_isolation":"Isolated",
  "token_budget":768,
  "idle_timeout_ms":30000,
  "recursive_spawn":false,
  "metadata":{}
}"#,
    )
    .expect("dispatch should be written");

    let mut slots = build_runtime_slots(&config).expect("queued slot should rebuild from queue");
    let restored_run_ids = match &slots.subagent {
        SubagentRuntimeSlot::QueuedExternal { spawner, .. } => spawner
            .pending_dispatches()
            .into_iter()
            .map(|dispatch| dispatch.run_id.0)
            .collect::<Vec<_>>(),
        SubagentRuntimeSlot::Fake(_) => panic!("expected queued external subagent slot"),
    };
    assert_eq!(
        restored_run_ids,
        vec![
            "persisted-run-alpha".to_string(),
            "queued-run-7".to_string()
        ]
    );

    let receipt = slots
        .subagent
        .spawn(SpawnRequest {
            task_id: TaskId("task-after-restore".to_string()),
            parent_agent_id: AgentId("xiaoce".to_string()),
            agent_name: "worker".to_string(),
            task: "重建后继续编号".to_string(),
            tool_policy: SubagentToolPolicy::Execute,
            context_isolation: ContextIsolation::Isolated,
            token_budget: 768,
            idle_timeout_ms: 30_000,
            recursive_spawn: false,
            metadata: Default::default(),
        })
        .expect("spawn should continue after restored queue");

    assert_eq!(receipt.run_id.0, "queued-run-8");
    assert!(dispatch_dir.join("queued-run-7.json").exists());
    assert!(dispatch_dir.join("queued-run-8.json").exists());
}

#[test]
fn slot_registry_queued_external_rebuild_fails_on_corrupt_dispatch_artifact() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    config.subagent = SubagentConfig::QueuedExternal;
    config.subagent_queue = SubagentQueueConfig {
        root: temp_queue_root("queued-corrupt-dispatch"),
    };

    let dispatch_dir = config.subagent_queue.root.join("dispatch");
    fs::create_dir_all(&dispatch_dir).expect("dispatch dir should be created");
    fs::write(dispatch_dir.join("queued-run-1.json"), "{not valid json")
        .expect("corrupt dispatch should be written");

    let error = build_runtime_slots(&config).expect_err("corrupt dispatch should fail rebuild");

    assert_eq!(error.field, "subagent_queue.dispatch");
    assert!(error
        .message
        .contains("failed to restore queued dispatches"));
    assert!(error.message.contains("Decode"));
}

fn queued_slot_report(run_id: &str, agent_id: &AgentId) -> SubagentReport {
    SubagentReport {
        schema_version: "1.0".to_string(),
        report_id: ReportId(format!("report-{run_id}")),
        task_id: TaskId("task-queued-report".to_string()),
        agent_id: agent_id.clone(),
        parent_agent_id: Some(AgentId("xiaoce".to_string())),
        status: ExecutionStatus::Success,
        started_at: Timestamp("2026-05-01T00:00:00Z".to_string()),
        finished_at: Timestamp("2026-05-01T00:00:01Z".to_string()),
        summary: "queued slot worker completed".to_string(),
        exit_code: Some(0),
        stdout_preview: Some("queued slot ok".to_string()),
        stderr_preview: None,
        resource_usage: ResourceUsage::default(),
        artifacts: Vec::new(),
        replay_ref: Some(format!("queued-subagent://{run_id}")),
        context_debug: None,
        governance_decision: None,
        truncated: false,
        skill_proposals: vec![],
    }
}

#[test]
fn emotion_slot_runtime_observes_delta_and_resets_connection() {
    let mut emotion =
        EmotionSlotRuntime::Jiwen(chuang_agent::emotion_slot::JiwenEmotionSlot::default());
    assert_eq!(emotion.kind(), "jiwen");

    let snapshot = emotion.snapshot().expect("snapshot should build");
    assert!(snapshot.prompt_context.contains("当前情绪状态"));

    // 正向对话 delta：愉悦度上升。
    emotion
        .observe_delta(&chuang_agent::emotion_slot::EmotionDelta {
            connection: Some(-0.1),
            pride: Some(0.2),
            valence: Some(0.3),
            arousal: Some(0.1),
            immersion: Some(0.2),
        })
        .expect("observe delta should succeed");

    let after = emotion
        .snapshot()
        .expect("snapshot after delta should build");
    assert!(after.axes.valence > 0.2);
    assert!(after.axes.pride > 0.1);
    assert_eq!(after.axes.connection, 0.0, "主人来对话后连接需求应重置");

    // 时间流逝 tick：连接需求增长（jiwen 语义）。
    let triggers = emotion.tick(60.0).expect("tick should succeed");
    let after_tick = emotion
        .snapshot()
        .expect("snapshot after tick should build");
    assert!(after_tick.axes.connection > 0.0);
    // 60 分钟 < 阈值，不应触发主动联系。
    assert!(triggers.is_empty());
}

#[test]
fn build_runtime_slots_installs_jiwen_emotion_slot_by_default() {
    let config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let slots = build_runtime_slots(&config).expect("default slots should build");
    assert_eq!(slots.emotion.kind(), "jiwen");
}

fn temp_skill_root(name: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock should be valid")
        .as_nanos();
    std::env::temp_dir().join(format!("chuang-slot-evolver-{name}-{nanos}"))
}

fn slot_failure_event(id: &str, task: &str, tool: &str) -> RuntimeEvent {
    RuntimeEvent {
        event_id: id.to_string(),
        task_id: task.to_string(),
        kind: RuntimeEventKind::ToolFailed,
        summary: format!("tool {tool} failed"),
        metadata: std::collections::BTreeMap::from([("tool".to_string(), tool.to_string())]),
    }
}

fn canonical_slot_config(root: PathBuf) -> RuntimeConfig {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let mut canonical = chuang_agent::runtime_config::CanonicalEvolutionConfig::default();
    canonical.skill_root = root;
    canonical.detector.min_repeats = 2;
    config.evolution = EvolutionConfig::Canonical(canonical);
    config
}

#[test]
fn slot_registry_builds_canonical_evolution_slot_from_runtime_config() {
    let config = canonical_slot_config(temp_skill_root("build-slot"));

    let mut slots = build_runtime_slots(&config).expect("canonical evolution slots should build");
    let summary = summarize_runtime_slots(&config);
    assert_eq!(summary.evolution, "canonical");

    let receipt = slots
        .evolution
        .observe(RuntimeEvent {
            event_id: "event-1".to_string(),
            task_id: "task-1".to_string(),
            kind: RuntimeEventKind::TurnCompleted,
            summary: "completed slot evolution canonical".to_string(),
            metadata: Default::default(),
        })
        .expect("canonical evolver should accept observed event");
    assert!(receipt.accepted);

    assert!(slots.evolution.rule_change_governance().is_some());
    assert!(slots.evolution.rule_change_governance_context().is_some());
    assert!(slots.evolution.rule_change_detector_config().is_some());
    assert!(slots.evolution.rule_change_journal_path().is_some());
    // 接线层需要的 owned 治理拷贝：与 `&mut evolver` 同帧使用时避免借用冲突。
    let owned_governance = slots
        .evolution
        .cloned_rule_change_governance()
        .expect("canonical slot exposes owned governance");
    assert!(owned_governance
        .evaluate(
            &dummy_rule_change_proposal(),
            &slots
                .evolution
                .rule_change_governance_context()
                .expect("governance context"),
        )
        .is_ok());
}

fn dummy_rule_change_proposal() -> chuang_agent::skill_evolver::RuleChangeProposal {
    use chuang_agent::skill_evolver::{FailureEvidence, RuleChangeKind, SkillProposalProvenance};
    chuang_agent::skill_evolver::RuleChangeProposal {
        proposal_id: "slot-test-proposal".to_string(),
        rule_id: "rule-for-slot-test".to_string(),
        change_kind: RuleChangeKind::CreateRule,
        title: "Slot test rule".to_string(),
        trigger: "slot test trigger".to_string(),
        old_procedure: Vec::new(),
        new_procedure: vec!["apply the slot test rule".to_string()],
        rationale: "slot registry owned governance test".to_string(),
        evidence: vec![FailureEvidence {
            pattern_signature: "tool=slot-test".to_string(),
            count: 2,
            event_ids: vec!["f1".to_string(), "f2".to_string()],
            task_ids: vec!["t1".to_string()],
            summary: "repeated failure observed".to_string(),
        }],
        writes_rules: true,
        requires_governance: true,
        provenance: vec![SkillProposalProvenance {
            source_event_id: "f1".to_string(),
            source_task_id: "t1".to_string(),
            source_kind: RuntimeEventKind::ToolFailed,
            source_summary: "tool slot-test failed".to_string(),
            source_metadata: Default::default(),
        }],
    }
}

#[test]
fn slot_registry_canonical_evolution_runs_full_outer_loop_with_governance() {
    let root = temp_skill_root("outer-loop");
    let config = canonical_slot_config(root.clone());
    let mut slots = build_runtime_slots(&config).expect("canonical evolution slots should build");

    // 1) 运行时事件流进入外环。
    slots
        .evolution
        .observe(slot_failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    slots
        .evolution
        .observe(slot_failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    // 2) detect：从已观察事件流检测重复失败模式。
    let detector_config = slots
        .evolution
        .rule_change_detector_config()
        .expect("canonical slot exposes detector config")
        .clone();
    let patterns = slots
        .evolution
        .detect_repeated_failures(&detector_config)
        .expect("valid config should detect repeated failures");
    assert_eq!(patterns.len(), 1);
    assert_eq!(patterns[0].signature, "tool=build");
    assert_eq!(
        patterns[0].event_ids,
        vec!["f1".to_string(), "f2".to_string()]
    );

    // 3) propose：把模式转成可审计提案，证据必须来自已观察事件流。
    let proposal = slots
        .evolution
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    assert!(proposal.writes_rules);
    assert!(proposal.requires_governance);
    assert_eq!(
        proposal.evidence[0].event_ids,
        vec!["f1".to_string(), "f2".to_string()]
    );

    // 4) 治理批准 + apply：只有批准后才落盘，并写入 JSONL journal。
    let governance = chuang_agent::slot_registry::CanonicalGovernanceSlot::default();
    let context = slots
        .evolution
        .rule_change_governance_context()
        .expect("canonical slot exposes governance context");
    let create_receipt = slots
        .evolution
        .apply_rule_change(proposal.clone(), &governance, &context)
        .expect("policy governance should approve grounded rule change");
    assert!(create_receipt.writes_rules);
    assert!(create_receipt.path.exists());

    let history = slots
        .evolution
        .rule_change_history()
        .expect("journal history should be readable");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].proposal.proposal_id, proposal.proposal_id);
    assert!(history[0].before.is_none());
    assert!(history[0].after.contains("tool=build"));
    let journal_path = slots
        .evolution
        .rule_change_journal_path()
        .expect("canonical slot exposes journal path");
    assert!(journal_path.exists());
    assert!(journal_path.ends_with(".evolver/rule_changes.jsonl"));

    // 5) 同一签名再次失败 → 提案变成 UpdateRule；再次经治理批准落盘。
    slots
        .evolution
        .observe(slot_failure_event("f3", "t3", "build"))
        .expect("failure event should be observed");
    slots
        .evolution
        .observe(slot_failure_event("f4", "t4", "build"))
        .expect("failure event should be observed");
    let patterns = slots
        .evolution
        .detect_repeated_failures(&detector_config)
        .expect("detect should find the grown pattern");
    let update_proposal = slots
        .evolution
        .propose_rule_change(&patterns[0])
        .expect("grounded pattern should propose");
    assert_eq!(
        update_proposal.change_kind,
        chuang_agent::skill_evolver::RuleChangeKind::UpdateRule
    );
    assert!(!update_proposal.old_procedure.is_empty());
    let context = slots
        .evolution
        .rule_change_governance_context()
        .expect("refreshed governance context");
    let update_receipt = slots
        .evolution
        .apply_rule_change(update_proposal.clone(), &governance, &context)
        .expect("update should pass governance and write");
    assert_eq!(update_receipt.version, create_receipt.version + 1);
    assert_eq!(
        slots
            .evolution
            .rule_change_history()
            .expect("history")
            .len(),
        2
    );

    // 6) rollback：回滚 Update 同样必须经过治理门禁，按 journal 恢复旧内容。
    let history = slots.evolution.rule_change_history().expect("history");
    let update_entry = history
        .iter()
        .find(|entry| entry.proposal.proposal_id == update_proposal.proposal_id)
        .expect("update journal entry exists");
    let rollback = slots
        .evolution
        .rollback_rule_change(&update_entry.entry_id, &governance, &context)
        .expect("rollback should pass governance and write");
    assert!(rollback.writes_rules);
    assert_eq!(
        rollback.proposal_id,
        format!("rollback-{}", update_proposal.proposal_id)
    );
    assert_eq!(
        slots
            .evolution
            .rule_change_history()
            .expect("history")
            .len(),
        3
    );
    let after_rollback = fs::read_to_string(&create_receipt.path).expect("rule file readable");
    assert!(after_rollback.contains("tool=build"));
}

#[test]
fn slot_registry_canonical_governance_noop_rejects_and_writes_nothing() {
    let root = temp_skill_root("noop-governance");
    let mut config = canonical_slot_config(root.clone());
    if let EvolutionConfig::Canonical(canonical) = &mut config.evolution {
        canonical.governance = chuang_agent::runtime_config::CanonicalEvolutionGovernance::Noop;
    }
    let mut slots = build_runtime_slots(&config).expect("canonical evolution slots should build");
    slots
        .evolution
        .observe(slot_failure_event("f1", "t1", "build"))
        .expect("failure event should be observed");
    slots
        .evolution
        .observe(slot_failure_event("f2", "t2", "build"))
        .expect("failure event should be observed");

    let detector_config = slots
        .evolution
        .rule_change_detector_config()
        .expect("detector config")
        .clone();
    let patterns = slots
        .evolution
        .detect_repeated_failures(&detector_config)
        .expect("detect should succeed");
    let proposal = slots
        .evolution
        .propose_rule_change(&patterns[0])
        .expect("propose should succeed");

    let governance = chuang_agent::slot_registry::CanonicalGovernanceSlot::Noop(
        chuang_agent::skill_evolver::NoopRuleChangeGovernance,
    );
    let context = slots
        .evolution
        .rule_change_governance_context()
        .expect("governance context");
    let err = slots
        .evolution
        .apply_rule_change(proposal, &governance, &context)
        .expect_err("noop governance must never approve");

    assert!(matches!(
        err,
        chuang_agent::skill_evolver::EvolutionError::ValidationRejected(_)
    ));
    assert!(slots
        .evolution
        .rule_change_history()
        .expect("history")
        .is_empty());
    // 观察流持久化（ac4a074）会创建 `.evolver/observed-events.jsonl`，
    // 因此 `.evolver` 目录存在是预期；Noop 拒绝的语义是「不落任何规则/审计」：
    // rule_changes.jsonl 不得存在，且 skill_root 下不得有规则 .md。
    assert!(!root.join(".evolver").join("rule_changes.jsonl").exists());
    let rule_files: Vec<_> = std::fs::read_dir(&root)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|entry| {
                    entry
                        .file_name()
                        .to_string_lossy()
                        .ends_with(".md")
                })
                .collect()
        })
        .unwrap_or_default();
    assert!(rule_files.is_empty(), "noop governance must not write rule files");
}

#[test]
fn slot_registry_canonical_propose_rejects_unverifiable_evidence() {
    let config = canonical_slot_config(temp_skill_root("propose-invalid"));
    let slots = build_runtime_slots(&config).expect("canonical evolution slots should build");
    let pattern = chuang_agent::skill_evolver::FailurePattern {
        signature: "tool=ghost".to_string(),
        kind: RuntimeEventKind::ToolFailed,
        count: 1,
        window_size: 1,
        event_ids: vec!["ghost-event".to_string()],
        task_ids: vec!["t1".to_string()],
        first_seen_event_id: "ghost-event".to_string(),
        last_seen_event_id: "ghost-event".to_string(),
        summary: "not grounded".to_string(),
    };

    let err = slots
        .evolution
        .propose_rule_change(&pattern)
        .expect_err("proposal citing unknown events must fail");

    assert!(matches!(
        err,
        chuang_agent::skill_evolver::EvolutionError::InvalidProposal(_)
    ));
}

#[test]
fn slot_registry_non_canonical_slots_return_structured_errors_for_outer_loop() {
    let mut config = RuntimeConfig::new(PathBuf::from("./data/chuang-agent.db"));
    let slots = build_runtime_slots(&config).expect("noop evolution slots should build");

    // Noop 槽位：外环方法必须结构化报错，不能 panic，也不提供治理/上下文。
    assert!(slots.evolution.rule_change_governance().is_none());
    assert!(slots.evolution.rule_change_governance_context().is_none());
    assert!(slots.evolution.rule_change_detector_config().is_none());
    assert!(slots.evolution.rule_change_journal_path().is_none());
    assert!(matches!(
        slots
            .evolution
            .detect_repeated_failures(&Default::default()),
        Err(chuang_agent::skill_evolver::EvolutionError::InvalidScope(_))
    ));
    let pattern = chuang_agent::skill_evolver::FailurePattern {
        signature: "tool=build".to_string(),
        kind: RuntimeEventKind::ToolFailed,
        count: 2,
        window_size: 2,
        event_ids: vec!["f1".to_string()],
        task_ids: vec!["t1".to_string()],
        first_seen_event_id: "f1".to_string(),
        last_seen_event_id: "f1".to_string(),
        summary: "x".to_string(),
    };
    assert!(matches!(
        slots.evolution.propose_rule_change(&pattern),
        Err(chuang_agent::skill_evolver::EvolutionError::InvalidRuleChange(_))
    ));

    // DryRun 槽位：同样结构化报错。
    config.evolution = EvolutionConfig::DryRun;
    let mut slots = build_runtime_slots(&config).expect("dry_run evolution slots should build");
    let proposal = chuang_agent::skill_evolver::RuleChangeProposal {
        proposal_id: "p1".to_string(),
        rule_id: "rule-tool-build".to_string(),
        change_kind: chuang_agent::skill_evolver::RuleChangeKind::CreateRule,
        title: "Rule for tool=build".to_string(),
        trigger: "repeated failure".to_string(),
        old_procedure: Vec::new(),
        new_procedure: vec!["step".to_string()],
        rationale: "rationale".to_string(),
        evidence: Vec::new(),
        writes_rules: true,
        requires_governance: true,
        provenance: Vec::new(),
    };
    let context = GovernanceContext {
        observed_events: Vec::new(),
        detector_config: Default::default(),
    };
    assert!(matches!(
        slots.evolution.apply_rule_change(
            proposal,
            &chuang_agent::slot_registry::CanonicalGovernanceSlot::default(),
            &context
        ),
        Err(chuang_agent::skill_evolver::EvolutionError::InvalidRuleChange(_))
    ));
    assert!(matches!(
        slots.evolution.rollback_rule_change(
            "missing",
            &chuang_agent::slot_registry::CanonicalGovernanceSlot::default(),
            &context
        ),
        Err(chuang_agent::skill_evolver::EvolutionError::InvalidRuleChange(_))
    ));
    assert!(slots.evolution.rule_change_history().is_err());
}
