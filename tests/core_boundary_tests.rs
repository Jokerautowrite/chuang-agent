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

#[test]
fn mvp_runtime_entrypoints_do_not_import_browser_worker_adapter_line() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = [
        "src/main.rs",
        "src/agent_runtime.rs",
        "src/chuang_kernel.rs",
        "src/slot_registry.rs",
    ];
    let forbidden = ["crate::browser_worker", "chuang_agent::browser_worker"];
    let mut violations = Vec::new();

    for relative_path in files {
        let content = fs::read_to_string(repo_root.join(relative_path))
            .expect("source file should be readable");
        for token in forbidden {
            if content.contains(token) {
                violations.push(format!("{relative_path} imports {token}"));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "browser_worker is an adapter/plugin line and must not enter MVP runtime entrypoints:\n{}",
        violations.join("\n")
    );
}

#[test]
fn main_entrypoint_stays_thin_and_does_not_own_cli_adapters() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let content =
        fs::read_to_string(repo_root.join("src/main.rs")).expect("main source should be readable");
    let forbidden = [
        "FileSubagentQueue",
        "QueuedSubagentSpawner",
        "run_control_surface_intent",
        "DEFAULT_CONFIG_TEMPLATE",
        "OpenAICompatibleProviderAdapter",
    ];
    let mut violations = Vec::new();

    for token in forbidden {
        if content.contains(token) {
            violations.push(format!("src/main.rs contains {token}"));
        }
    }

    assert!(
        violations.is_empty(),
        "main.rs should remain a thin CLI entrypoint; move concrete command adapters to cli_* modules:\n{}",
        violations.join("\n")
    );
}

#[test]
fn cli_genesis_stays_on_slot_boundary_and_does_not_own_autocli_details() {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let content = fs::read_to_string(repo_root.join("src/cli_genesis.rs"))
        .expect("cli_genesis should be readable");
    let forbidden = ["AutoCliGenesisActuator", "SystemGenesisCommandRunner"];
    let mut violations = Vec::new();

    for token in forbidden {
        if content.contains(token) {
            violations.push(format!("src/cli_genesis.rs contains {token}"));
        }
    }

    assert!(
        violations.is_empty(),
        "cli_genesis should stay on the slot boundary and not own concrete Genesis adapter types:\n{}",
        violations.join("\n")
    );
}
