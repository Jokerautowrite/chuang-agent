use std::fs;
use std::path::Path;

const CORE_FILES: &[&str] = &[
    "src/agent_runtime.rs",
    "src/chuang_kernel.rs",
    "src/context_engine.rs",
    "src/governance.rs",
    "src/memory_recall.rs",
    "src/memory_store.rs",
    "src/memory_admission.rs",
    "src/runtime_report.rs",
];

const FORBIDDEN_CORE_TOKENS: &[&str] = &[
    "OpenAICompatible",
    "ProviderTransport",
    "FakeResponder",
    "FakeActuator",
    "FakeControlPlane",
    "BrowserWorker",
    "DeepSeekWeb",
    "FileSubagentQueue",
    "QueuedSubagentSpawner",
    "FakeSubagentSpawner",
    "SqliteMemoryStore",
    "systemd",
    "FEISHU_",
];

#[test]
fn core_files_do_not_import_or_construct_specific_adapters() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut violations = Vec::new();

    for relative_path in CORE_FILES {
        let path = repo_root.join(relative_path);
        let content = fs::read_to_string(&path).expect("core file should be readable");
        for token in FORBIDDEN_CORE_TOKENS {
            if content.contains(token) {
                violations.push(format!("{relative_path} contains {token}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "core boundary violations:\n{}",
        violations.join("\n")
    );
}

#[test]
fn runtime_config_describes_provider_but_does_not_construct_adapters() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let content = fs::read_to_string(repo_root.join("src/runtime_config.rs"))
        .expect("runtime_config should be readable");

    assert!(
        !content.contains("OpenAICompatibleProviderAdapter"),
        "runtime_config must not construct provider adapters; use slot_registry composition instead"
    );
}
