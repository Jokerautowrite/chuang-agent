use serde::{Deserialize, Serialize};

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
