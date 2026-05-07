# Provider Fallback Diagnostics

Chuang provider fallback is explicit only. A fallback provider runs only when configuration declares `fallback_provider` or `[fallback.provider]`; otherwise provider failures are returned as diagnosable runtime output and are not silently rerouted.

## Runtime Fields

Provider failures expose stable metadata:

- `provider_response_ok`: `true` only when the primary provider returned usable
  assistant content; `false` for structured HTTP failures and successful HTTP
  responses that still lack assistant content.
- `provider_retryable`: whether the primary failure is retryable.
- `provider_error_class`: coarse source such as `http_status`, `transport`, `tls`, `protocol`, `config`, or `missing_content`.
- `provider_failure_reason_code`: stable reason such as `model_capacity`, `rate_limited`, `quota_or_billing`, `auth_failed`, `upstream_unavailable`, or `transport_failure`.
- `provider_failure_category`: broader group such as `capacity`, `rate_limit`, `quota`, `auth`, `upstream`, `transport`, `protocol`, `config`, or `response`.
- `provider_fallback_configured`: `true` only when fallback was explicitly configured.
- `provider_fallback_used`: `true` only when the fallback provider actually answered this turn.

When fallback is used, the fallback response also preserves primary failure context:

- `provider_fallback_from`
- `provider_fallback_reason`
- `provider_fallback_primary_retryable`
- `provider_fallback_primary_status_code`
- `provider_fallback_primary_error_class`
- `provider_fallback_primary_failure_reason_code`
- `provider_fallback_primary_failure_category`

## Capacity Boundary

Upstream messages like `Selected model is at capacity` are classified as:

```text
provider_failure_reason_code=model_capacity
provider_failure_category=capacity
provider_retryable=true
```

If no fallback is configured, the same turn must also say:

```text
provider_fallback_configured=false
provider_fallback_used=false
```

That is the intentional boundary: visible failure, no silent fallback.

## Acceptance Examples

Successful provider response:

```text
provider_response_ok=true
```

The same successful turn should not include
`provider_failure_reason_code` or `provider_failure_category`.

HTTP failure:

```text
provider_response_ok=false
provider_error_class=http_status
provider_failure_reason_code=rate_limited
provider_failure_category=rate_limit
```

Successful HTTP status with no usable assistant content:

```text
PROVIDER_MISSING_CONTENT
provider_response_ok=false
provider_error_class=missing_content
provider_failure_reason_code=missing_content
provider_failure_category=response
```

Fallback hit after an explicit fallback configuration:

```text
provider_fallback_configured=true
provider_fallback_used=true
provider_fallback_primary_failure_reason_code=model_capacity
provider_fallback_primary_failure_category=capacity
```

## Operator Setup

Start from `config.example-provider-fallback.toml` when an operator wants a real
primary and backup provider:

```bash
cp config.example-provider-fallback.toml config.provider-fallback.local.toml
export CHUANG_AGENT_PRIMARY_API_KEY='<set>'
export CHUANG_AGENT_FALLBACK_API_KEY='<set>'
cargo run --quiet -- config check --config config.provider-fallback.local.toml
cargo run --quiet -- run --config config.provider-fallback.local.toml --input 'provider fallback check'
```

Only the environment variable names belong in config. Reports and docs should
describe secret state as `<set>` or `<missing>`, never the actual value.

For a local fixture that does not call any real provider:

```bash
sh scripts/chuang-provider-fallback-smoke.sh
```

The fixture runs two turns:

- Primary unavailable without fallback: `provider_fallback_configured=false`
  and `provider_fallback_used=false`.
- Same primary with explicit fallback: `provider_fallback_configured=true`,
  `provider_fallback_used=true`, and primary error context is preserved.
