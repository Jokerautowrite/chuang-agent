use chuang_agent::browser_worker::{
    BrowserMode, BrowserWorkerError, BrowserWorkerSession, OpenCliCommandResult,
    OpenCliCommandSpec, OpenCliRealBrowserDriver, OpenCliRunner, ProviderKind, RealBrowserCommand,
    RealBrowserDriver, RealBrowserObservation, WorkerState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
struct StubRunner {
    calls: Vec<OpenCliCommandSpec>,
    results: Vec<OpenCliCommandResult>,
}

impl StubRunner {
    fn new(results: Vec<OpenCliCommandResult>) -> Self {
        Self {
            calls: Vec::new(),
            results,
        }
    }
}

impl OpenCliRunner for StubRunner {
    fn run(
        &mut self,
        spec: OpenCliCommandSpec,
    ) -> Result<OpenCliCommandResult, BrowserWorkerError> {
        self.calls.push(spec);
        Ok(self.results.remove(0))
    }
}

fn session() -> BrowserWorkerSession {
    BrowserWorkerSession {
        worker_id: "worker-opencli".to_string(),
        provider: ProviderKind::DeepSeekWeb,
        mode: BrowserMode::Expert,
        page_url: "https://chat.deepseek.com/".to_string(),
        logged_in: true,
        last_prompt: Some("继续推进 chuang".to_string()),
        last_prompt_hash: Some("hash-123".to_string()),
        last_output_hash: None,
        last_dispatch_at: None,
        last_read_at: None,
        state: WorkerState::Ready,
    }
}

#[test]
fn opencli_real_driver_capture_output_keeps_state_evidence_and_anchor() {
    let runner = StubRunner::new(vec![OpenCliCommandResult {
        status_code: 0,
        stdout: "URL: https://chat.deepseek.com/\ninteractive: 7\n[0] textarea".to_string(),
        stderr: String::new(),
    }]);
    let mut driver = OpenCliRealBrowserDriver::with_runner(runner);

    let observation = driver
        .execute(&session(), &RealBrowserCommand::CaptureOutput)
        .expect("capture output should succeed");

    assert_eq!(
        observation,
        RealBrowserObservation::OutputCaptured {
            content: "URL: https://chat.deepseek.com/\ninteractive: 7\n[0] textarea".to_string(),
            snapshot_ref: Some("opencli://state/hash-123".to_string()),
        }
    );
}

#[test]
fn opencli_real_driver_open_page_builds_expected_command() {
    let runner = StubRunner::new(vec![OpenCliCommandResult {
        status_code: 0,
        stdout: String::new(),
        stderr: String::new(),
    }]);
    let mut driver = OpenCliRealBrowserDriver::with_runner(runner);
    let current = session();

    let observation = driver
        .execute(
            &current,
            &RealBrowserCommand::OpenPage {
                url: current.page_url.clone(),
            },
        )
        .expect("open page should succeed");

    assert_eq!(
        observation,
        RealBrowserObservation::PageOpened {
            url: "https://chat.deepseek.com/".to_string(),
        }
    );
}

#[test]
fn opencli_real_driver_surfaces_command_failure() {
    let runner = StubRunner::new(vec![OpenCliCommandResult {
        status_code: 1,
        stdout: String::new(),
        stderr: "boom".to_string(),
    }]);
    let mut driver = OpenCliRealBrowserDriver::with_runner(runner);

    let err = driver
        .execute(
            &session(),
            &RealBrowserCommand::EnsureMode {
                mode: BrowserMode::Expert,
            },
        )
        .expect_err("non-zero opencli exit should fail");

    assert_eq!(
        err,
        BrowserWorkerError::OpenCliCommandFailed {
            command: "opencli browser state".to_string(),
            detail: "boom".to_string(),
        }
    );
}
