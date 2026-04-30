use std::process::Command;

#[test]
fn cli_repl_accepts_provider_transport_flag() {
    let output = Command::new("bash")
        .arg("-lc")
        .arg(
            "printf '继续推进 transport\\nexit\\n' | cargo run --quiet -- repl --provider-base-url https://api.example.com/v1 --provider-api-key test-key --provider-model gpt-4.1-mini --provider-id custom-openai --provider-transport stub",
        )
        .output()
        .expect("cargo repl should execute");

    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("transport_mode: stub"), "stdout={stdout}");
}
