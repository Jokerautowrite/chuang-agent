//! `cli_browser` 模块。内部实现模块（无公开顶层项）。

use std::process::{Command, Stdio};

use chuang_agent::browser_read::{find_headless_chrome_script, resolve_cdp_port};

use crate::cli_output::usage;

pub(crate) fn browser_command(args: &[String]) -> Result<(), String> {
    let action = args
        .first()
        .map(String::as_str)
        .ok_or_else(|| "usage: chuang browser <start|stop|restart|status|env>".to_string())?;
    match action {
        "start" | "stop" | "restart" | "status" | "env" => run_headless_script(action),
        "help" | "-h" | "--help" => {
            println!(
                "chuang browser <start|stop|restart|status|env>\n\
Manage managed headless Chrome for browser_read / browser_navigate.\n\
Auto-start on tool use is default; disable with CHUANG_HEADLESS_AUTOSTART=0."
            );
            Ok(())
        }
        other => Err(format!("unsupported browser action: {other}\n{}", usage())),
    }
}

fn run_headless_script(action: &str) -> Result<(), String> {
    let script = find_headless_chrome_script().ok_or_else(|| {
        "cannot find scripts/chuang-headless-chrome.sh (set CHUANG_AGENT_ROOT or CHUANG_HEADLESS_SCRIPT)"
            .to_string()
    })?;
    let status = Command::new("bash")
        .arg(&script)
        .arg(action)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .map_err(|err| {
            format!(
                "browser script spawn failed path={} error={err}",
                script.display()
            )
        })?;
    if !status.success() {
        return Err(format!(
            "browser {action} failed exit={}",
            status.code().unwrap_or(-1)
        ));
    }
    if action == "status" {
        if let Some(port) = resolve_cdp_port() {
            println!("resolved_cdp_port={port}");
        } else {
            println!("resolved_cdp_port=none");
        }
    }
    Ok(())
}
