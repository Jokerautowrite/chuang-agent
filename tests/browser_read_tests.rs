use chuang_agent::browser_read::{
    BrowserPageRead, BrowserReadAdapter, CdpBrowserReadAdapter, FakeBrowserReadAdapter,
    UnavailableBrowserReadAdapter,
};

#[test]
fn fake_browser_read_adapter_returns_injected_snapshot() {
    let adapter = FakeBrowserReadAdapter::new(BrowserPageRead {
        url: "https://example.test/page".to_string(),
        title: "Example Page".to_string(),
        dom_text: "visible DOM text".to_string(),
        source: "fake_snapshot".to_string(),
        read_at: "2026-05-10T00:00:00Z".to_string(),
    });

    let status = adapter.status();
    assert!(!status.available);
    assert_eq!(status.adapter_kind, "fake");
    assert_eq!(status.state, "fake_contract_only");
    assert_eq!(status.reason_code, "fake_contract_only");
    assert_eq!(status.capabilities, vec!["url", "title", "dom_text"]);
    assert!(status.desktop_read_is_separate);
    assert!(status.does_not_use_actuator_observe);
    assert!(status.reason.contains("contract tests only"));

    let page = adapter
        .read_current_page()
        .expect("fake adapter should return injected snapshot");
    assert_eq!(page.url, "https://example.test/page");
    assert_eq!(page.title, "Example Page");
    assert_eq!(page.dom_text, "visible DOM text");
}

#[test]
fn unavailable_browser_read_adapter_never_claims_dom_url_or_title() {
    let adapter = UnavailableBrowserReadAdapter;

    let status = adapter.status();
    assert!(!status.available);
    assert_eq!(status.adapter_kind, "unavailable");
    assert_eq!(status.state, "unavailable");
    assert_eq!(status.reason_code, "real_adapter_missing");
    assert!(status.reason.contains("must not infer URL, title, or DOM"));
    assert!(status.desktop_read_is_separate);
    assert!(status.does_not_use_actuator_observe);

    let error = adapter
        .read_current_page()
        .expect_err("missing real adapter must be structured unavailable");
    assert_eq!(error.code, "browser_read_unavailable");
    assert_eq!(error.adapter_kind, "unavailable");
    assert!(!error.retryable);
    assert!(error.message.contains("cannot read DOM, URL, or title"));
}

#[test]
fn cdp_adapter_unreachable_port_returns_unavailable_status() {
    // Port 1 is never open; adapter must not panic and must report not available.
    let adapter = CdpBrowserReadAdapter::new(1);
    let status = adapter.status();
    assert!(!status.available);
    assert_eq!(status.adapter_kind, "cdp");
    assert_eq!(status.reason_code, "cdp_port_unreachable");
    assert!(status.desktop_read_is_separate);
    assert!(status.does_not_use_actuator_observe);
}

#[test]
fn cdp_adapter_unreachable_port_read_returns_structured_error() {
    let adapter = CdpBrowserReadAdapter::new(1);
    let err = adapter
        .read_current_page()
        .expect_err("unreachable CDP port must return error");
    assert_eq!(err.adapter_kind, "cdp");
    assert!(err.retryable);
    assert!(err.code.starts_with("cdp_"));
}
