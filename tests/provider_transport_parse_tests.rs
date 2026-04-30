use chuang_agent::responder::ProviderTransport;

#[test]
fn provider_transport_parses_http_variant() {
    let parsed = "http"
        .parse::<ProviderTransport>()
        .expect("http transport should parse");
    assert_eq!(parsed, ProviderTransport::Http);
    assert_eq!(parsed.as_str(), "http");
}
