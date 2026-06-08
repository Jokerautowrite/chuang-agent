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

#[derive(Debug, Deserialize)]
struct CdpTabJson {
    url: Option<String>,
    title: Option<String>,
    #[serde(rename = "webSocketDebuggerUrl")]
    web_socket_debugger_url: Option<String>,
}

/// Live browser-read adapter that reads URL, title, and DOM text from a Chrome/Chromium
/// instance running with `--remote-debugging-port=<port>`.
///
/// Enable by setting `CHUANG_CDP_PORT=9222` (or another port).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CdpBrowserReadAdapter {
    port: u16,
}

impl CdpBrowserReadAdapter {
    pub fn new(port: u16) -> Self {
        Self { port }
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

    /// Send a plain HTTP/1.0 GET over a fresh TCP connection and return the body.
    fn http_get_body(&self, path: &str) -> Result<String, BrowserReadError> {
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
            "GET {} HTTP/1.0\r\nHost: localhost:{}\r\nConnection: close\r\n\r\n",
            path, self.port
        );
        stream
            .write_all(request.as_bytes())
            .map_err(|e| Self::cdp_error("cdp_write_failed", format!("{}", e)))?;

        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .map_err(|e| Self::cdp_error("cdp_read_failed", format!("{}", e)))?;

        let body = response
            .splitn(2, "\r\n\r\n")
            .nth(1)
            .ok_or_else(|| {
                Self::cdp_error("cdp_no_body", "no HTTP body in CDP response".to_string())
            })?
            .to_string();
        Ok(body)
    }

    /// Execute a JS expression via CDP WebSocket and return the string result.
    /// Uses a minimal hand-rolled WebSocket client (no extra crate).
    fn ws_eval(&self, ws_url: &str, js_expr: &str) -> Result<String, BrowserReadError> {
        // Parse ws://host:port/path
        let without_scheme = ws_url.trim_start_matches("ws://");
        let slash_pos = without_scheme.find('/').unwrap_or(without_scheme.len());
        let host_port = &without_scheme[..slash_pos];
        let path = if slash_pos < without_scheme.len() {
            &without_scheme[slash_pos..]
        } else {
            "/"
        };

        let mut stream = TcpStream::connect(host_port)
            .map_err(|e| Self::cdp_error("cdp_ws_connect_failed", format!("{}", e)))?;
        stream.set_read_timeout(Some(Duration::from_secs(10))).ok();
        stream.set_write_timeout(Some(Duration::from_secs(5))).ok();

        // WebSocket HTTP upgrade
        let handshake = format!(
            "GET {} HTTP/1.1\r\nHost: {}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
             Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n",
            path, host_port
        );
        stream
            .write_all(handshake.as_bytes())
            .map_err(|e| Self::cdp_error("cdp_ws_upgrade_write", format!("{}", e)))?;

        // Read until \r\n\r\n
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

        // Build Runtime.evaluate command JSON
        let cmd = format!(
            "{{\"id\":1,\"method\":\"Runtime.evaluate\",\
             \"params\":{{\"expression\":{},\"returnByValue\":true}}}}",
            serde_json::to_string(js_expr)
                .map_err(|e| Self::cdp_error("cdp_json_encode", format!("{}", e)))?
        );
        let payload = cmd.as_bytes();
        let mask: [u8; 4] = [0x37, 0xfa, 0x21, 0x3d];

        // Encode WebSocket frame (client must mask)
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

        // Read frames until we find id=1
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
                    // Ping — send pong
                    stream.write_all(&[0x8a, 0x00]).ok();
                    continue;
                }
                0x1 | 0x0 => {} // text / continuation
                _ => continue,
            }

            let text = String::from_utf8_lossy(&data);
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&text) {
                if json["id"].as_u64() == Some(1) {
                    if let Some(val) = json.pointer("/result/result/value") {
                        return Ok(val.as_str().unwrap_or("").to_string());
                    }
                    return Ok(String::new());
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
        let body = self.http_get_body("/json/list")?;
        let tabs: Vec<CdpTabJson> = serde_json::from_str(&body).map_err(|e| {
            Self::cdp_error("cdp_json_parse", format!("cannot parse /json/list: {}", e))
        })?;

        // Pick first non-extension, non-blank tab
        let tab = tabs
            .into_iter()
            .find(|t| {
                t.url
                    .as_deref()
                    .map(|u| !u.starts_with("chrome-extension://") && u != "about:blank")
                    .unwrap_or(false)
            })
            .ok_or_else(|| {
                Self::cdp_error(
                    "cdp_no_target_tab",
                    "no suitable tab found in /json/list".to_string(),
                )
            })?;

        let url = tab.url.unwrap_or_default();
        let title = tab.title.unwrap_or_default();
        let dom_text = tab
            .web_socket_debugger_url
            .as_deref()
            .and_then(|ws| {
                self.ws_eval(ws, "document.body ? document.body.innerText : ''")
                    .ok()
            })
            .unwrap_or_default();

        let now = Utc::now().to_rfc3339();
        Ok(BrowserPageRead {
            url,
            title,
            dom_text,
            source: format!("cdp_localhost_{}", self.port),
            read_at: now,
        })
    }
}
