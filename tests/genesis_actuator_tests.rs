use std::collections::VecDeque;
#[cfg(unix)]
use std::time::Instant;

#[cfg(unix)]
use chuang_agent::genesis_actuator::SystemGenesisCommandRunner;
use chuang_agent::genesis_actuator::{
    AutoCliGenesisActuator, FakeGenesisActuator, GenesisActuator, GenesisAskRequest,
    GenesisChannel, GenesisCommandOutput, GenesisCommandRunner, GenesisCommandSpec, GenesisConfig,
    GenesisError,
};

#[derive(Debug, Default)]
struct ScriptedRunner {
    calls: Vec<GenesisCommandSpec>,
    outputs: VecDeque<Result<GenesisCommandOutput, GenesisError>>,
}

impl ScriptedRunner {
    fn with_outputs(outputs: Vec<Result<GenesisCommandOutput, GenesisError>>) -> Self {
        Self {
            calls: Vec::new(),
            outputs: outputs.into(),
        }
    }
}

impl GenesisCommandRunner for ScriptedRunner {
    fn run(&mut self, spec: &GenesisCommandSpec) -> Result<GenesisCommandOutput, GenesisError> {
        self.calls.push(spec.clone());
        self.outputs
            .pop_front()
            .expect("scripted output should exist")
    }
}

#[test]
fn fake_genesis_actuator_records_prompt_without_browser_worker() {
    let mut actuator = FakeGenesisActuator::new("fake answer");

    let response = actuator
        .ask(GenesisAskRequest {
            prompt: "第一性原理是什么".to_string(),
        })
        .expect("fake should answer");

    assert_eq!(response.answer, "fake answer");
    assert_eq!(response.channel, GenesisChannel::UserDataDir);
    assert_eq!(actuator.calls(), &["第一性原理是什么".to_string()]);
}

#[test]
fn autocli_genesis_builds_primary_and_fallback_command_shapes() {
    let actuator = AutoCliGenesisActuator::with_runner(
        GenesisConfig::new("/tmp/chuang-genesis-profile"),
        ScriptedRunner::default(),
    );

    let primary = actuator.primary_spec("测试");
    assert_eq!(primary.program, "autocli");
    assert_eq!(primary.channel, GenesisChannel::UserDataDir);
    assert_eq!(primary.timeout_ms, 30_000);
    assert_eq!(
        primary.args,
        vec![
            "deepseek",
            "chat",
            "测试",
            "--headless",
            "--user-data-dir",
            "/tmp/chuang-genesis-profile",
            "--timeout",
            "30000",
        ]
    );

    let fallback = actuator.fallback_spec("测试");
    assert_eq!(fallback.channel, GenesisChannel::Cdp);
    assert_eq!(fallback.timeout_ms, 30_000);
    assert_eq!(
        fallback.args,
        vec![
            "deepseek",
            "chat",
            "测试",
            "--cdp-port",
            "9222",
            "--timeout",
            "30000"
        ]
    );
}

#[test]
#[cfg(unix)]
fn system_genesis_runner_times_out_stuck_command() {
    let mut runner = SystemGenesisCommandRunner;
    let spec = GenesisCommandSpec {
        program: "sleep".to_string(),
        args: vec!["1".to_string()],
        channel: GenesisChannel::UserDataDir,
        timeout_ms: 20,
    };
    let started = Instant::now();

    let output = runner
        .run(&spec)
        .expect("runner should return timeout output");

    assert!(started.elapsed().as_millis() < 500);
    assert_ne!(output.status_code, Some(0));
    assert!(output.stderr.contains("timed out after 20ms"));
}

#[test]
fn autocli_genesis_returns_primary_answer_when_user_data_dir_works() {
    let runner = ScriptedRunner::with_outputs(vec![Ok(GenesisCommandOutput {
        status_code: Some(0),
        stdout: "primary answer".to_string(),
        stderr: String::new(),
    })]);
    let mut actuator =
        AutoCliGenesisActuator::with_runner(GenesisConfig::new("/tmp/genesis-profile"), runner);

    let response = actuator
        .ask(GenesisAskRequest {
            prompt: "Rust async runtime".to_string(),
        })
        .expect("primary channel should answer");

    assert_eq!(response.answer, "primary answer");
    assert_eq!(response.channel, GenesisChannel::UserDataDir);
    assert!(response.primary_repair.is_none());
}

#[test]
fn autocli_genesis_falls_back_to_cdp_and_returns_repair_plan_without_deleting_profile() {
    let runner = ScriptedRunner::with_outputs(vec![
        Ok(GenesisCommandOutput {
            status_code: Some(0),
            stdout: "请登录后查看".to_string(),
            stderr: String::new(),
        }),
        Ok(GenesisCommandOutput {
            status_code: Some(0),
            stdout: "fallback answer".to_string(),
            stderr: String::new(),
        }),
    ]);
    let mut actuator =
        AutoCliGenesisActuator::with_runner(GenesisConfig::new("/tmp/genesis-profile"), runner);

    let response = actuator
        .ask(GenesisAskRequest {
            prompt: "查一下最新资料".to_string(),
        })
        .expect("fallback channel should answer");

    assert_eq!(response.answer, "fallback answer");
    assert_eq!(response.channel, GenesisChannel::Cdp);
    let repair = response
        .primary_repair
        .expect("fallback success should carry repair plan");
    assert!(repair.requires_approval);
    assert!(repair.recommended_action.contains("do not delete profile"));
}

#[test]
fn autocli_genesis_reports_all_channels_down_when_both_fail() {
    let runner = ScriptedRunner::with_outputs(vec![
        Ok(GenesisCommandOutput {
            status_code: Some(1),
            stdout: String::new(),
            stderr: "primary failed".to_string(),
        }),
        Ok(GenesisCommandOutput {
            status_code: Some(1),
            stdout: String::new(),
            stderr: "fallback failed".to_string(),
        }),
    ]);
    let mut actuator =
        AutoCliGenesisActuator::with_runner(GenesisConfig::new("/tmp/genesis-profile"), runner);

    let error = actuator
        .ask(GenesisAskRequest {
            prompt: "查询".to_string(),
        })
        .expect_err("both channels should fail");

    assert!(matches!(error, GenesisError::AllChannelsDown { .. }));
}
