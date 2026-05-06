#!/usr/bin/env bash
set -eu

work_dir="$(mktemp -d "${TMPDIR:-/tmp}/chuang-provider-fallback-smoke.XXXXXX")"
config_without_fallback="$work_dir/no-fallback.toml"
config_with_fallback="$work_dir/with-fallback.toml"

export CHUANG_AGENT_PROVIDER_SMOKE_API_KEY="<set>"

cat >"$config_without_fallback" <<EOF_CONFIG
db_path = "$work_dir/no-fallback.db"
identity_memory_root = "$work_dir/no-fallback-identity"
identity_root = "./identity"
soul_path = "./identity/SOUL.md"
story_path = "./identity/STORY.md"
first_wake_path = "./identity/FIRST_WAKE.md"
agents_registry_path = "./identity/agents.toml"
rules_root = "./rules"
rules_core_path = "./rules/core.md"
provider = "openai_compatible"
provider_id = "primary-unavailable"
base_url = "http://127.0.0.1:1/v1"
model = "primary-fixture-model"
api_key_env = "CHUANG_AGENT_PROVIDER_SMOKE_API_KEY"
transport = "http"
provider_timeout_ms = 1000
EOF_CONFIG

cat >"$config_with_fallback" <<EOF_CONFIG
db_path = "$work_dir/with-fallback.db"
identity_memory_root = "$work_dir/with-fallback-identity"
identity_root = "./identity"
soul_path = "./identity/SOUL.md"
story_path = "./identity/STORY.md"
first_wake_path = "./identity/FIRST_WAKE.md"
agents_registry_path = "./identity/agents.toml"
rules_root = "./rules"
rules_core_path = "./rules/core.md"
provider = "openai_compatible"
provider_id = "primary-unavailable"
base_url = "http://127.0.0.1:1/v1"
model = "primary-fixture-model"
api_key_env = "CHUANG_AGENT_PROVIDER_SMOKE_API_KEY"
transport = "http"
provider_timeout_ms = 1000
fallback_provider = "fake"
fallback_provider_id = "fallback-fixture"
fallback_model = "fallback-fixture-model"
fallback_on_retryable = "true"
fallback_status_codes = "429,500,502,503,504"
fallback_error_classes = "transport,tls"
EOF_CONFIG

without_fallback_output="$(cargo run --quiet -- run --config "$config_without_fallback" --input "provider fallback fixture without fallback")"
printf '%s\n' "$without_fallback_output" | grep -F 'model_name: primary-fixture-model' >/dev/null
printf '%s\n' "$without_fallback_output" | grep -F 'provider_fallback_configured: false' >/dev/null
printf '%s\n' "$without_fallback_output" | grep -F 'provider_fallback_used: false' >/dev/null
printf '%s\n' "$without_fallback_output" | grep -F 'provider_error_class: transport' >/dev/null

with_fallback_output="$(cargo run --quiet -- run --config "$config_with_fallback" --input "provider fallback fixture with fallback")"
printf '%s\n' "$with_fallback_output" | grep -F 'model_name: fallback-fixture-model' >/dev/null
printf '%s\n' "$with_fallback_output" | grep -F 'provider_fallback_configured: true' >/dev/null
printf '%s\n' "$with_fallback_output" | grep -F 'provider_fallback_used: true' >/dev/null
printf '%s\n' "$with_fallback_output" | grep -F 'provider_fallback_from: primary-unavailable' >/dev/null
printf '%s\n' "$with_fallback_output" | grep -F 'provider_fallback_primary_error_class: transport' >/dev/null

printf '%s\n' 'provider_fallback_smoke_ok'
