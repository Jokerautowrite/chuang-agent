use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

pub const BROWSER_READ_CONTRACT_VERSION: u16 = 1;

pub const BROWSER_READ_CAPABILITIES: &[&str] = &["url", "title", "dom_text"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserReadStatus {
    pub contract_version: u16,
    pub adapter_kind: String,
    pub available: bool,
    pub state: String,
    pub capabilities: Vec<String>,
    pub boundary: String,
    pub reason_code: String,
    pub reason: String,
    pub desktop_read_is_separate: bool,
    pub does_not_use_actuator_observe: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserPageRead {
    pub url: String,
    pub title: String,
    pub dom_text: String,
    pub source: String,
    pub read_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserReadError {
    pub code: String,
    pub message: String,
    pub adapter_kind: String,
    pub retryable: bool,
}

pub trait BrowserReadAdapter {
    fn status(&self) -> BrowserReadStatus;
    fn read_current_page(&self) -> Result<BrowserPageRead, BrowserReadError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeBrowserReadAdapter {
    snapshot: BrowserPageRead,
}

impl FakeBrowserReadAdapter {
    pub fn new(snapshot: BrowserPageRead) -> Self {
        Self { snapshot }
    }
}

impl BrowserReadAdapter for FakeBrowserReadAdapter {
    fn status(&self) -> BrowserReadStatus {
        BrowserReadStatus {
            contract_version: BROWSER_READ_CONTRACT_VERSION,
            adapter_kind: "fake".to_string(),
            available: false,
            state: "fake_contract_only".to_string(),
            capabilities: browser_read_capabilities(),
            boundary: "browser_read_dom_url_title_contract".to_string(),
            reason_code: "fake_contract_only".to_string(),
            reason:
                "fake browser_read adapter returns an injected snapshot for contract tests only"
                    .to_string(),
            desktop_read_is_separate: true,
            does_not_use_actuator_observe: true,
        }
    }

    fn read_current_page(&self) -> Result<BrowserPageRead, BrowserReadError> {
        Ok(self.snapshot.clone())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnavailableBrowserReadAdapter;

impl BrowserReadAdapter for UnavailableBrowserReadAdapter {
    fn status(&self) -> BrowserReadStatus {
        unavailable_browser_read_status("real_adapter_missing")
    }

    fn read_current_page(&self) -> Result<BrowserPageRead, BrowserReadError> {
        Err(BrowserReadError {
            code: "browser_read_unavailable".to_string(),
            message: "browser_read live adapter is not configured; cannot read DOM, URL, or title"
                .to_string(),
            adapter_kind: "unavailable".to_string(),
            retryable: false,
        })
    }
}

pub fn unavailable_browser_read_status(reason_code: &str) -> BrowserReadStatus {
    BrowserReadStatus {
        contract_version: BROWSER_READ_CONTRACT_VERSION,
        adapter_kind: "unavailable".to_string(),
        available: false,
        state: "unavailable".to_string(),
        capabilities: browser_read_capabilities(),
        boundary: "browser_read_dom_url_title_contract".to_string(),
        reason_code: reason_code.to_string(),
        reason: "no audited browser_read adapter is configured; status must not infer URL, title, or DOM from desktop_read observe/screenshot evidence".to_string(),
        desktop_read_is_separate: true,
        does_not_use_actuator_observe: true,
    }
}

fn browser_read_capabilities() -> Vec<String> {
    BROWSER_READ_CAPABILITIES
        .iter()
        .map(|capability| capability.to_string())
        .collect()
}

// -- CDP adapter --

const DOM_TEXT_CHAR_LIMIT: usize = 12_000;

#[derive(Debug, Deserialize)]
struct CdpTabJson {
    #[serde(rename = "type")]
    target_type: Option<String>,
    url: Option<String>,
    title: Option<String>,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

/// Resolve the live CDP adapter (no side effects).
///
/// Order:
/// 1. `CHUANG_CDP_PORT` if set and parseable
/// 2. managed headless port file from `scripts/chuang-headless-chrome.sh`
/// 3. structured unavailable error otherwise
pub fn resolve_cdp_browser_read_adapter() -> Result<CdpBrowserReadAdapter, BrowserReadError> {
    if let Some(port) = resolve_cdp_port() {
        return Ok(CdpBrowserReadAdapter::new(port));
    }
    Err(BrowserReadError {
        code: "browser_read_unavailable".to_string(),
        message: "no headless browser CDP endpoint; start with `chuang browser start` / `scripts/chuang-headless-chrome.sh start` or set CHUANG_CDP_PORT".to_string(),
        adapter_kind: "unavailable".to_string(),
        retryable: true,
    })
}

/// Resolve CDP adapter; when missing/unreachable, auto-start managed headless Chrome unless disabled.
///
/// Disable with `CHUANG_HEADLESS_AUTOSTART=0` (or false/off/no).
pub fn ensure_cdp_browser_read_adapter() -> Result<CdpBrowserReadAdapter, BrowserReadError> {
    if let Some(port) = resolve_cdp_port() {
        let adapter = CdpBrowserReadAdapter::new(port);
        if adapter.is_port_open() {
            return Ok(adapter);
        }
        // Stale CHUANG_CDP_PORT or dead managed instance — try autostart below.
    }
    if headless_autostart_disabled() {
        return resolve_cdp_browser_read_adapter();
    }
    try_start_managed_headless_chrome()?;
    for _ in 0..20 {
        // Prefer managed state file after start; env may still point at dead port.
        if let Some(port) = managed_headless_cdp_port().or_else(cdp_port_from_env) {
            let adapter = CdpBrowserReadAdapter::new(port);
            if adapter.is_port_open() {
                return Ok(adapter);
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    Err(BrowserReadError {
        code: "browser_read_unavailable".to_string(),
        message: "started managed headless Chrome but CDP never became reachable; check `chuang browser status` / scripts/chuang-headless-chrome.sh".to_string(),
        adapter_kind: "unavailable".to_string(),
        retryable: true,
    })
}

pub(crate) fn headless_autostart_disabled() -> bool {
    match std::env::var("CHUANG_HEADLESS_AUTOSTART") {
        Ok(value) => {
            let v = value.trim().to_ascii_lowercase();
            matches!(v.as_str(), "0" | "false" | "off" | "no")
        }
        Err(_) => false,
    }
}

fn try_start_managed_headless_chrome() -> Result<(), BrowserReadError> {
    use std::process::{Command, Stdio};
    let script = find_headless_chrome_script().ok_or_else(|| BrowserReadError {
        code: "browser_headless_script_missing".to_string(),
        message: "cannot find scripts/chuang-headless-chrome.sh (set CHUANG_AGENT_ROOT or CHUANG_HEADLESS_SCRIPT)".to_string(),
        adapter_kind: "unavailable".to_string(),
        retryable: false,
    })?;
    let output = Command::new("bash")
        .arg(&script)
        .arg("start")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|err| BrowserReadError {
            code: "browser_headless_start_failed".to_string(),
            message: format!("failed to spawn headless chrome script: {err}"),
            adapter_kind: "unavailable".to_string(),
            retryable: true,
        })?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(BrowserReadError {
        code: "browser_headless_start_failed".to_string(),
        message: format!(
            "headless chrome start exited {}: stdout={} stderr={}",
            output.status.code().unwrap_or(-1),
            stdout.trim(),
            stderr.trim()
        ),
        adapter_kind: "unavailable".to_string(),
        retryable: true,
    })
}

/// Locate managed headless chrome control script.
pub fn find_headless_chrome_script() -> Option<std::path::PathBuf> {
    if let Ok(path) = std::env::var("CHUANG_HEADLESS_SCRIPT") {
        let p = std::path::PathBuf::from(path);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut candidates = Vec::new();
    if let Ok(root) = std::env::var("CHUANG_AGENT_ROOT") {
        candidates.push(std::path::PathBuf::from(root).join("scripts/chuang-headless-chrome.sh"));
    }
    candidates.push(
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("scripts/chuang-headless-chrome.sh"),
    );
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join("scripts/chuang-headless-chrome.sh"));
        candidates.push(cwd.join("chuang-agent/scripts/chuang-headless-chrome.sh"));
    }
    if let Ok(home) = std::env::var("HOME") {
        candidates.push(
            std::path::PathBuf::from(home)
                .join("projects/chuang-agent/scripts/chuang-headless-chrome.sh"),
        );
    }
    candidates.into_iter().find(|p| p.is_file())
}

pub fn cdp_port_from_env() -> Option<u16> {
    std::env::var("CHUANG_CDP_PORT")
        .ok()
        .and_then(|raw| raw.trim().parse::<u16>().ok())
        .filter(|port| *port > 0)
}

/// Explicit env port, or managed headless chrome state file port.
pub fn resolve_cdp_port() -> Option<u16> {
    if let Some(port) = cdp_port_from_env() {
        return Some(port);
    }
    managed_headless_cdp_port()
}

fn managed_headless_cdp_port() -> Option<u16> {
    let state_dir = std::env::var("CHUANG_HEADLESS_STATE_DIR")
        .ok()
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            let base = std::env::var("XDG_STATE_HOME")
                .map(std::path::PathBuf::from)
                .unwrap_or_else(|_| {
                    std::env::var("HOME")
                        .map(|home| std::path::PathBuf::from(home).join(".local/state"))
                        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"))
                });
            base.join("chuang-agent/headless-chrome")
        });
    let port_path = state_dir.join("cdp.port");
    let raw = std::fs::read_to_string(port_path).ok()?;
    let port = raw.trim().parse::<u16>().ok().filter(|p| *p > 0)?;
    // Only accept if the port is actually open.
    let adapter = CdpBrowserReadAdapter::new(port);
    if adapter.is_port_open() {
        Some(port)
    } else {
        None
    }
}

/// Live browser-read adapter that reads URL, title, and DOM text from a Chrome/Chromium
/// instance running with `--remote-debugging-port=<port>`.
///
/// Enable by:
/// - `scripts/chuang-headless-chrome.sh start` (default port 9222), or
/// - setting `CHUANG_CDP_PORT=9222` against any Chrome with remote debugging.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdpBrowserReadAdapter {
    port: u16,
}

impl CdpBrowserReadAdapter {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    fn cdp_error(code: &str, message: String) -> BrowserReadError {
        BrowserReadError {
            code: code.to_string(),
            message,
            adapter_kind: "cdp".to_string(),
            retryable: true,
        }
    }

    fn is_port_open(&self) -> bool {
        TcpStream::connect_timeout(
            &format!("127.0.0.1:{}", self.port).parse().unwrap(),
            Duration::from_millis(500),
        )
        .is_ok()
    }

    /// Send a plain HTTP/1.1 request over a fresh TCP connection and return the body.
    /// Modern Chrome remote-debugging rejects empty/silent HTTP/1.0 replies.
    fn http_request_body(&self, method: &str, path: &str) -> Result<String, BrowserReadError> {
        let addr = format!("127.0.0.1:{}", self.port);
        let mut stream = TcpStream::connect(&addr).map_err(|e| {
            Self::cdp_error(
                "cdp_connect_failed",
                format!("cannot reach CDP at localhost:{}: {}", self.port, e),
            )
        })?;
        stream.set_read_timeout(Some(Duration::from_secs(5))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let request = format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
            self.port
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| Self::cdp_error("cdp_write_failed", format!("{}", e)))?;

        let mut raw = Vec::with_capacity(4096);
        let mut buf = [0u8; 2048];
        loop {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => raw.extend_from_slice(&buf[..n]),
                Err(e)
                    if e.kind() == std::io::ErrorKind::WouldBlock
                        || e.kind() == std::io::ErrorKind::TimedOut =>
                {
                    // Content-Length may already be complete; stop if we have a full header/body.
                    if raw.windows(4).any(|w| w == b"\r\n\r\n") {
                        break;
                    }
                    return Err(Self::cdp_error(
                        "cdp_read_timeout",
                        format!("timed out reading CDP HTTP response for {method} {path}"),
                    ));
                }
                Err(e) => {
                    return Err(Self::cdp_error("cdp_read_failed", format!("{}", e)));
                }
            }
            // Early exit when Content-Length body is fully buffered.
            if let Some(header_end) = raw.windows(4).position(|w| w == b"\r\n\r\n") {
                let header = String::from_utf8_lossy(&raw[..header_end]);
                if let Some(cl) = header.lines().find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(|v| v.trim().to_string())
                }) {
                    if let Ok(len) = cl.parse::<usize>() {
                        let body_start = header_end + 4;
                        if raw.len() >= body_start + len {
                            break;
                        }
                    }
                }
            }
            if raw.len() > 8 * 1024 * 1024 {
                return Err(Self::cdp_error(
                    "cdp_response_too_large",
                    "CDP HTTP response exceeded 8MiB".to_string(),
                ));
            }
        }

        let response = String::from_utf8_lossy(&raw);
        let status_line = response.lines().next().unwrap_or("");
        if status_line.contains(" 4") || status_line.contains(" 5") {
            return Err(Self::cdp_error(
                "cdp_http_error",
                format!("CDP HTTP error for {method} {path}: {status_line}"),
            ));
        }

        let body = response
            .splitn(2, "\r\n\r\n")
            .nth(1)
            .ok_or_else(|| {
                Self::cdp_error("cdp_no_body", "no HTTP body in CDP response".to_string())
            })?
            .to_string();
        Ok(body)
    }

    fn http_get_body(&self, path: &str) -> Result<String, BrowserReadError> {
        self.http_request_body("GET", path)
    }

    fn http_put_body(&self, path: &str) -> Result<String, BrowserReadError> {
        self.http_request_body("PUT", path)
    }

    fn list_tabs(&self) -> Result<Vec<CdpTabJson>, BrowserReadError> {
        let body = self.http_get_body("/json/list")?;
        serde_json::from_str(&body).map_err(|e| {
            Self::cdp_error("cdp_json_parse", format!("cannot parse /json/list: {}", e))
        })
    }

    fn pick_any_page_tab(tabs: Vec<CdpTabJson>) -> Result<CdpTabJson, BrowserReadError> {
        let ranked = |prefer_content: bool| {
            tabs.iter().find(|t| {
                let is_page = t
                    .target_type
                    .as_deref()
                    .map(|ty| ty == "page")
                    .unwrap_or(true);
                let has_ws = t
                    .web_socket_debugger_url
                    .as_deref()
                    .map(|u| !u.is_empty())
                    .unwrap_or(false);
                if !is_page || !has_ws {
                    return false;
                }
                if !prefer_content {
                    return true;
                }
                t.url
                    .as_deref()
                    .map(|u| {
                        !u.starts_with("chrome-extension://")
                            && !u.starts_with("devtools://")
                            && u != "about:blank"
                    })
                    .unwrap_or(false)
            })
        };
        ranked(true)
            .or_else(|| ranked(false))
            .map(|tab| CdpTabJson {
                target_type: tab.target_type.clone(),
                url: tab.url.clone(),
                title: tab.title.clone(),
                web_socket_debugger_url: tab.web_socket_debugger_url.clone(),
            })
            .ok_or_else(|| {
                Self::cdp_error(
                    "cdp_no_target_tab",
                    "no page target with websocket in /json/list".to_string(),
                )
            })
    }

    /// Navigate the managed browser to `url`, wait briefly for load, then read page state.
    pub fn navigate_and_read(&self, url: &str) -> Result<BrowserPageRead, BrowserReadError> {
        let url = url.trim();
        if url.is_empty() {
            return Err(Self::cdp_error(
                "cdp_navigate_empty_url",
                "browser_navigate requires a non-empty url".to_string(),
            ));
        }
        if !(url.starts_with("http://")
            || url.starts_with("https://")
            || url.starts_with("file://")
            || url.starts_with("about:"))
        {
            return Err(Self::cdp_error(
                "cdp_navigate_unsupported_scheme",
                format!("unsupported url scheme for browser_navigate: {url}"),
            ));
        }

        // Prefer HTTP PUT /json/new?url=… (Chrome ≥ modern headless returns 405 for GET).
        let encoded = url
            .replace('%', "%25")
            .replace(' ', "%20")
            .replace('#', "%23");
        let new_path = format!("/json/new?{encoded}");
        let new_body = self
            .http_put_body(&new_path)
            .or_else(|_| self.http_get_body(&new_path));
        let ws_url = match new_body {
            Ok(body) => {
                if let Ok(tab) = serde_json::from_str::<CdpTabJson>(&body) {
                    tab.web_socket_debugger_url
                } else {
                    None
                }
            }
            Err(_) => None,
        };

        let ws_url = match ws_url {
            Some(ws) if !ws.is_empty() => ws,
            _ => {
                // Fallback: navigate existing page tab via CDP Page.navigate.
                let tab = Self::pick_any_page_tab(self.list_tabs()?)?;
                let ws = tab.web_socket_debugger_url.ok_or_else(|| {
                    Self::cdp_error(
                        "cdp_no_ws",
                        "selected tab has no webSocketDebuggerUrl".to_string(),
                    )
                })?;
                let params = serde_json::json!({ "url": url });
                self.ws_cdp_method(&ws, "Page.navigate", params)?;
                ws
            }
        };

        // A newly-created CDP target can report readyState=complete for about:blank
        // before Chrome applies the requested URL. Wait for both navigation and load.
        std::thread::sleep(Duration::from_millis(400));
        for _ in 0..20 {
            let current_url = self.ws_eval(&ws_url, "location.href").unwrap_or_default();
            let state = self
                .ws_eval(&ws_url, "document.readyState")
                .unwrap_or_default();
            let navigation_visible = url == "about:blank"
                || (!current_url.is_empty()
                    && current_url != "about:blank"
                    && current_url != "data:,");
            if navigation_visible && (state == "complete" || state == "interactive") {
                break;
            }
            std::thread::sleep(Duration::from_millis(200));
        }

        self.read_page_via_ws(&ws_url)
    }

    fn read_page_via_ws(&self, ws_url: &str) -> Result<BrowserPageRead, BrowserReadError> {
        let url = self.ws_eval(ws_url, "location.href").unwrap_or_default();
        let title = self.ws_eval(ws_url, "document.title").unwrap_or_default();
        let mut dom_text = self
            .ws_eval(
                ws_url,
                "document.body ? (document.body.innerText || '') : ''",
            )
            .unwrap_or_default();
        if dom_text.chars().count() > DOM_TEXT_CHAR_LIMIT {
            dom_text = dom_text
                .chars()
                .take(DOM_TEXT_CHAR_LIMIT)
                .collect::<String>()
                + "…";
        }
        Ok(BrowserPageRead {
            url,
            title,
            dom_text,
            source: format!("cdp_localhost_{}", self.port),
            read_at: Utc::now().to_rfc3339(),
        })
    }

    /// Execute a JS expression via CDP WebSocket and return the string result.
    fn ws_eval(&self, ws_url: &str, js_expr: &str) -> Result<String, BrowserReadError> {
        let params = serde_json::json!({
            "expression": js_expr,
            "returnByValue": true,
        });
        let json = self.ws_cdp_method(ws_url, "Runtime.evaluate", params)?;
        if let Some(val) = json.pointer("/result/result/value") {
            return Ok(match val {
                serde_json::Value::String(s) => s.clone(),
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            });
        }
        Ok(String::new())
    }

    /// Send a CDP method over WebSocket and return the response JSON for id=1.
    fn ws_cdp_method(
        &self,
        ws_url: &str,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, BrowserReadError> {
        // Parse ws://host:port/path
        let without_scheme = ws_url
            .trim_start_matches("ws://")
            .trim_start_matches("wss://");
        let slash_pos = without_scheme.find('/').unwrap_or(without_scheme.len());
        let host_port = &without_scheme[..slash_pos];
        let path = if slash_pos < without_scheme.len() {
            &without_scheme[slash_pos..]
        } else {
            "/"
        };

        let mut stream = TcpStream::connect(host_port)
            .map_err(|e| Self::cdp_error("cdp_ws_connect_failed", format!("{}", e)))?;
        stream.set_read_timeout(Some(Duration::from_secs(15))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        let handshake = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
            path, host_port
        );
        stream
            .write_all(handshake.as_bytes())
            .map_err(|e| Self::cdp_error("cdp_ws_upgrade_write", format!("{}", e)))?;

        let mut hdr = Vec::with_capacity(256);
        let mut byte = [0u8; 1];
        loop {
            if stream
                .read(&mut byte)
                .map_err(|e| Self::cdp_error("cdp_ws_upgrade_read", format!("{}", e)))?
                == 0
            {
                break;
            }
            hdr.push(byte[0]);
            if hdr.ends_with(b"\r\n\r\n") {
                break;
            }
            if hdr.len() > 4096 {
                return Err(Self::cdp_error(
                    "cdp_ws_header_overflow",
                    "WebSocket response headers too large".to_string(),
                ));
            }
        }
        if !String::from_utf8_lossy(&hdr).contains("101") {
            return Err(Self::cdp_error(
                "cdp_ws_upgrade_rejected",
                "CDP WebSocket upgrade did not return 101".to_string(),
            ));
        }

        let cmd = serde_json::json!({
            "id": 1,
            "method": method,
            "params": params,
        });
        let payload = serde_json::to_vec(&cmd)
            .map_err(|e| Self::cdp_error("cdp_json_encode", format!("{}", e)))?;
        let mask: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d];

        let mut frame: Vec<u8> = Vec::new();
        frame.push(0x81); // FIN=1, opcode=1 (text)
        let plen = payload.len();
        if plen <= 125 {
            frame.push(0x80 | plen as u8);
        } else if plen <= 65535 {
            frame.push(0x80 | 126);
            frame.push((plen >> 8) as u8);
            frame.push(plen as u8);
        } else {
            frame.push(0x80 | 127);
            for i in (0..8).rev() {
                frame.push((plen >> (i * 8)) as u8);
            }
        }
        frame.extend_from_slice(&mask);
        for (i, b) in payload.iter().enumerate() {
            frame.push(b ^ mask[i % 4]);
        }
        stream
            .write_all(&frame)
            .map_err(|e| Self::cdp_error("cdp_ws_send", format!("{}", e)))?;

        loop {
            let mut h2 = [0u8; 2];
            stream
                .read_exact(&mut h2)
                .map_err(|e| Self::cdp_error("cdp_ws_recv_hdr", format!("{}", e)))?;
            let opcode = h2[0] & 0x0f;
            let masked_flag = (h2[1] & 0x80) != 0;
            let len_byte = (h2[1] & 0x7f) as usize;

            let frame_len = if len_byte == 126 {
                let mut ext = [0u8; 2];
                stream
                    .read_exact(&mut ext)
                    .map_err(|e| Self::cdp_error("cdp_ws_recv_len", format!("{}", e)))?;
                u16::from_be_bytes(ext) as usize
            } else if len_byte == 127 {
                let mut ext = [0u8; 8];
                stream
                    .read_exact(&mut ext)
                    .map_err(|e| Self::cdp_error("cdp_ws_recv_len", format!("{}", e)))?;
                u64::from_be_bytes(ext) as usize
            } else {
                len_byte
            };

            if masked_flag {
                let mut mk = [0u8; 4];
                stream
                    .read_exact(&mut mk)
                    .map_err(|e| Self::cdp_error("cdp_ws_recv_mask", format!("{}", e)))?;
            }

            let mut data = vec![0u8; frame_len];
            stream
                .read_exact(&mut data)
                .map_err(|e| Self::cdp_error("cdp_ws_recv_data", format!("{}", e)))?;

            match opcode {
                0x8 => {
                    return Err(Self::cdp_error(
                        "cdp_ws_closed",
                        "WebSocket closed by server".to_string(),
                    ))
                }
                0x9 => {
                    stream.write_all(&[0x8a, 0x00]).ok();
                    continue;
                }
                0x1 | 0x0 => {}
                _ => continue,
            }

            let text = String::from_utf8_lossy(&data);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if json["id"].as_u64() == Some(1) {
                    if json.get("error").is_some() {
                        return Err(Self::cdp_error(
                            "cdp_method_error",
                            format!("CDP {method} failed: {}", json["error"]),
                        ));
                    }
                    return Ok(json);
                }
            }
        }
    }
}

impl BrowserReadAdapter for CdpBrowserReadAdapter {
    fn status(&self) -> BrowserReadStatus {
        if self.is_port_open() {
            BrowserReadStatus {
                contract_version: BROWSER_READ_CONTRACT_VERSION,
                adapter_kind: "cdp".to_string(),
                available: true,
                state: "cdp_connected".to_string(),
                capabilities: browser_read_capabilities(),
                boundary: "browser_read_dom_url_title_contract".to_string(),
                reason_code: "cdp_port_reachable".to_string(),
                reason: format!(
                    "CDP adapter connected to localhost:{}; url/title/dom_text available from active tab",
                    self.port
                ),
                desktop_read_is_separate: true,
                does_not_use_actuator_observe: true,
            }
        } else {
            BrowserReadStatus {
                contract_version: BROWSER_READ_CONTRACT_VERSION,
                adapter_kind: "cdp".to_string(),
                available: false,
                state: "cdp_port_unreachable".to_string(),
                capabilities: browser_read_capabilities(),
                boundary: "browser_read_dom_url_title_contract".to_string(),
                reason_code: "cdp_port_unreachable".to_string(),
                reason: format!(
                    "CHUANG_CDP_PORT is set to {} but the port is not reachable; \
                     start Chrome/Chromium with --remote-debugging-port={}",
                    self.port, self.port
                ),
                desktop_read_is_separate: true,
                does_not_use_actuator_observe: true,
            }
        }
    }

    fn read_current_page(&self) -> Result<BrowserPageRead, BrowserReadError> {
        let tab = Self::pick_any_page_tab(self.list_tabs()?)?;
        if let Some(ws) = tab.web_socket_debugger_url.as_deref() {
            if let Ok(page) = self.read_page_via_ws(ws) {
                return Ok(page);
            }
        }
        // Fallback to HTTP metadata only when websocket evaluate fails.
        Ok(BrowserPageRead {
            url: tab.url.unwrap_or_default(),
            title: tab.title.unwrap_or_default(),
            dom_text: String::new(),
            source: format!("cdp_localhost_{}", self.port),
            read_at: Utc::now().to_rfc3339(),
        })
    }
}

#[cfg(test)]
mod ensure_tests {
    use super::{find_headless_chrome_script, headless_autostart_disabled};

    #[test]
    fn finds_headless_script_from_manifest() {
        let script = find_headless_chrome_script().expect("script should resolve in repo");
        assert!(script.ends_with("chuang-headless-chrome.sh"));
        assert!(script.is_file());
    }

    #[test]
    fn autostart_disabled_flag_parses() {
        // default is enabled (function returns false)
        std::env::remove_var("CHUANG_HEADLESS_AUTOSTART");
        assert!(!headless_autostart_disabled());
        std::env::set_var("CHUANG_HEADLESS_AUTOSTART", "0");
        assert!(headless_autostart_disabled());
        std::env::remove_var("CHUANG_HEADLESS_AUTOSTART");
    }
}
