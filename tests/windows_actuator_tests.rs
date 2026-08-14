#![cfg(windows)]

use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, Stdio};

#[test]
fn checked_in_windows_actuator_returns_real_observation_boundary() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let script = root
        .join("scripts")
        .join("chuang-real-actuator-adapter.ps1");
    let allowlist = root.join("config").join("actuator-allowlist.windows.json");
    let mut child = Command::new("powershell.exe")
        .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-File"])
        .arg(script)
        .arg("-Allowlist")
        .arg(allowlist)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Windows actuator should spawn");
    child
        .stdin
        .as_mut()
        .expect("stdin should exist")
        .write_all(br#"{"action":"observe","observe_target":"Screen"}"#)
        .expect("request should write");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("actuator should finish");
    assert!(
        output.status.success(),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("response should be JSON");
    assert_eq!(value["observation"]["target"], "Screen");
    assert!(value["observation"]["summary"]
        .as_str()
        .expect("summary should exist")
        .contains("platform=windows"));
    assert!(value["message"]
        .as_str()
        .expect("message should exist")
        .contains("read_only=true"));
}
