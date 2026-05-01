use chuang_agent::provider_openai_compatible::ProviderTransport;

#[test]
fn provider_transport_parses_http_variant() {
    let parsed = "http"
        .parse::<ProviderTransport>()
        .expect("http transport should parse");
    assert_eq!(parsed, ProviderTransport::Http);
    assert_eq!(parsed.as_str(), "http");
}

#[test]
fn provider_transport_parses_curl_variant() {
    let parsed = "curl"
        .parse::<ProviderTransport>()
        .expect("curl transport should parse");
    assert_eq!(parsed, ProviderTransport::Curl);
    assert_eq!(parsed.as_str(), "curl");
}
