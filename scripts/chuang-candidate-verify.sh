#!/usr/bin/env bash
set -euo pipefail

root_dir="$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)"
provider_readiness_check="$root_dir/scripts/chuang-provider-readiness-check.sh"

cd "$root_dir"

# Candidate verification is intentionally dirty-tree friendly. The clean-tree
# release gate remains scripts/chuang-final-verify.sh.

# Keep this wrapper local-only even when the operator shell has live gates set.
unset CHUANG_CODEX_RUNNER_ENABLE
unset CHUANG_REAL_CONTROL_ENABLE
unset CHUANG_REAL_ACTUATOR_ENABLE

printf '%s\n' "[candidate-verify] complete local smoke"
sh scripts/chuang-complete-local-smoke.sh

printf '%s\n' "[candidate-verify] live runner rehearsal smoke"
bash scripts/chuang-live-runner-rehearsal-smoke.sh

printf '%s\n' "[candidate-verify] provider readiness check"
if [ -f "$provider_readiness_check" ]; then
    if bash "$provider_readiness_check"; then
        printf '%s\n' "[candidate-verify] provider readiness check passed"
    else
        provider_status=$?
        if [ "$provider_status" -eq 1 ]; then
            printf '%s\n' "[candidate-verify] provider readiness check reported a non-live block; continuing candidate-only gate"
        else
            printf '%s\n' "[candidate-verify] provider readiness check failed unexpectedly with status $provider_status"
            exit "$provider_status"
        fi
    fi
else
    printf '%s\n' "[candidate-verify] provider readiness check script not found: scripts/chuang-provider-readiness-check.sh"
    printf '%s\n' "[candidate-verify] provider readiness remains covered by complete-local status/doctor/app-server stub checks; no real provider call is attempted"
fi

printf '%s\n' "chuang_candidate_verify_ok"
