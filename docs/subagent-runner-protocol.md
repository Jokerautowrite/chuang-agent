# Subagent Runner Protocol

This document defines the MVP boundary between Chuang core and external subagent workers.

The core owns dispatch, claim, report validation, and collection. A runner owns execution. The runner can be Codex, Hermes, OpenClaw, a script, or a future dedicated worker process, but it must speak this protocol instead of being hard-wired into the runtime.

## Dispatch

Create work with:

```bash
cargo run -- subagent dispatch \
  --task "review memory module" \
  --policy analyze \
  --requires-capability rust \
  --requires-capability filesystem
```

The command writes one dispatch JSON file under:

```text
<subagent_queue_root>/dispatch/<run_id>.json
```

Capability requirements are stored in dispatch metadata:

```json
{
  "metadata": {
    "source": "cli",
    "required_capabilities": "rust,filesystem"
  }
}
```

`required_capabilities` is intentionally metadata in the MVP. This keeps the core schema stable while still allowing the scheduler and control UI to route work.

Capability names are trimmed, lowercased, deduplicated, and must not contain commas.

## Worker Selection

Run one matching task:

```bash
cargo run -- subagent run-once \
  --runner command \
  --runner-command ./runner.sh \
  --approve-exec \
  --capability rust \
  --capability filesystem
```

Run several matching tasks:

```bash
cargo run -- subagent run-loop \
  --runner command \
  --runner-command ./runner.sh \
  --approve-exec \
  --max-runs 5 \
  --max-concurrency 4 \
  --capability rust \
  --capability filesystem
```

The worker only claims a dispatch when all required capabilities are present. Dispatches without requirements match any worker.

Current MVP concurrency is bounded local worker batching: `--max-concurrency` accepts `1..8`. Each worker still claims from the durable queue, matches declared capabilities, and writes a normal `SubagentReport`; command runners still require explicit `--approve-exec`.

## Live Runner Preflight Rehearsal

Before any real external worker runner is enabled, run the read-only rehearsal:

```bash
cargo run -- subagent live-preflight \
  --runner-command scripts/chuang-codex-runner.py \
  --allow-runner-command scripts/chuang-codex-runner.py \
  --requires-capability rust \
  --capability rust \
  --json
```

This command does not claim dispatches, start runner processes, write reports, touch services, or open external sessions. It only reports whether the live subagent adapter would pass the current preflight surface:

- `CHUANG_CODEX_RUNNER_ENABLE` gate status and audit label.
- Gate default state, exact required env value, and live preflight checks.
- Exact runner command allowlist match.
- Dispatch `required_capabilities` satisfied by worker `--capability` values, including matched and missing capability lists.
- `ReportAdmission` boundary present for controller acceptance/rejection evidence, covered CLI commands, and stable reason-code examples.
- Forbidden live subagent capabilities still rejected, including unscoped external worker pools, direct core-memory writes, and platform login/session mutation.
- Approval and audit prerequisites for any later live run, including explicit operator approval, governance evidence, dispatch/worker evidence, and audit receipt requirements.

The stable JSON and text summary fields are:

```text
ready_for_live
readonly
gate_enabled
runner_allowlist_ok
capability_routing_ok
report_admission_ok
forbidden_capabilities_ok
approval_audit_prerequisites_ok
next_action
```

JSON also keeps the detailed nested checks under `gate`, `runner_allowlist`, `capability_routing`, `report_admission`, `forbidden_capabilities`, and `approval_audit_prerequisites`. Text output uses the same stable field names for the summary line and then prints one evidence line per check.

`ok=true` means the read-only rehearsal contract passed. `ready_for_live=true` additionally requires `CHUANG_CODEX_RUNNER_ENABLE=1`, so it must remain `false` when the gate env is unset or set to any non-enabling value. `ready_for_live=true` should only appear after explicit operator approval for the exact runner command and target dispatch.

Minimum live-gate acceptance check:

```bash
CHUANG_CODEX_RUNNER_ENABLE=1 cargo run -- subagent live-preflight \
  --runner-command scripts/chuang-codex-runner.py \
  --allow-runner-command scripts/chuang-codex-runner.py \
  --requires-capability rust \
  --capability rust \
  --json
```

The command should report `ready_for_live=true`, `readonly=true`, `gate_enabled=true`, all `*_ok=true`, and a `next_action` that still requires an approved live runner rehearsal. A disabled or non-enabling gate should report `ready_for_live=false` even if all read-only checks pass.

## Command Runner IO

When `--runner command` is used, Chuang starts the runner process directly. It does not invoke a shell unless the configured command is a shell program such as `sh`.

The runner receives the full dispatch JSON on stdin.

The runner may return either:

- plain stdout/stderr, which Chuang wraps into a `SubagentReport`;
- a full `SubagentReport` JSON on stdout.

Stdout that looks like a protocol report is treated as a protocol report candidate, even when it is incomplete. In the MVP that means a JSON object containing `schema_version` and at least one report identity field such as `report_id`, `task_id`, or `agent_id`. This prevents report-shaped JSON with missing required fields from being accepted as plain successful output.

When a full report is returned, Chuang validates identity before accepting it:

- the JSON must satisfy the `SubagentReport` v1 required-field contract;
- `task_id` must match the dispatch;
- `agent_id` must match the dispatch;
- `parent_agent_id` must match the dispatch parent.

Invalid protocol reports, including reports with missing required fields, bad status values, invalid timestamps, or identity mismatches, are stored as failed reports. They are not treated as success.

Every report produced through the command runner carries controller-side governance evidence in `SubagentReport.governance_decision` unless the worker supplied its own value. The default summary is:

```text
action_id = subagent-command-runner:<run_id>
decision = needs_approval
reason = approved_by_cli_flag: --approve-exec
```

This records why Chuang was allowed to start the external runner process. It does not mean the worker's internal actions were independently approved; worker-side governance must still be reported by the worker or adapter.

The CLI also exposes a separate `ReportAdmission` for the controller side:

- `Accepted` means the controller accepted the report contract.
- `Rejected` means the controller rejected the report contract and stored the failure reason.
- `reason_code` is a stable machine-readable code such as `report_validated`, `missing_required_field`, `empty_required_field`, `invalid_json`, `invalid_utf8`, `unsupported_schema_version`, `invalid_enum_format`, `invalid_timestamp_format`, `invalid_timestamp_order`, `size_limit_exceeded`, or `command_protocol_report_rejected`.
- `upstream_reason_code` is optional. It is set when a command runner emits a protocol-shaped report that must be wrapped as `command_protocol_report_rejected`; the upstream code preserves the lower-level cause such as `missing_required_field`, `invalid_timestamp_order`, or `agent_id_mismatch`.
- `run-once`, `run-loop`, `report`, and `collect` return this admission metadata in their JSON output so the UI can show controller state without parsing report text.

`SubagentReport` remains an immutable execution snapshot. The controller's decision to accept or reject the submitted report is represented separately as `ReportAdmission`:

```text
ReportAdmission.status = Accepted | Rejected
ReportAdmission.reason_code = stable snake_case code
```

This keeps two states distinct:

- execution status: what the worker says happened, stored in `SubagentReport.status`;
- admission status: whether the controller accepted the submitted report contract, stored in `ReportAdmission`.

The controller may reject a report because it is malformed, too large, uses an unsupported schema, or fails identity checks, even when the worker claims `ExecutionStatus::Success`.

If a command runner emits a full protocol report that is rejected, Chuang stores a valid `ExecutionStatus::Failed` report for auditability. The CLI still reports the controller-side admission as `Rejected` with `reason_code=command_protocol_report_rejected` for `run-once`, `run-loop`, and later `report` reads of that stored failure report. When available, `upstream_reason_code` carries the original protocol cause so UI layers do not need to parse human-readable stderr text.

A safe checked-in example is available at:

```bash
sh scripts/chuang-subagent-runner-example.sh
```

It reads dispatch JSON from stdin, returns a valid `SubagentReport`, and does not execute real tools.

A Codex-backed runner scaffold is available at:

```bash
scripts/chuang-codex-runner.py
```

It is disabled by default. It returns a failed protocol report unless `CHUANG_CODEX_RUNNER_ENABLE=1` is set. When enabled, it runs `codex exec <prompt>` in `CHUANG_CODEX_RUNNER_WORKSPACE` and emits a standard `SubagentReport`.

## Claims And Timeouts

Before execution, Chuang creates a claim file:

```text
<subagent_queue_root>/claims/<run_id>.json
```

`run-once` and `run-loop` skip claimed dispatches unless the claim is stale. Staleness uses the dispatch `idle_timeout_ms`.

If a command runner exceeds `idle_timeout_ms`, Chuang terminates that child process and writes a failed report. Existing dispatch, claim, release, and report files are not deleted.

Claim release is append-only: `release-claim` writes a release marker and does not remove the old claim file. `subagent list` reports `is_claimed=false` when a release marker is newer than the claim payload.

## Collection

Read a report directly:

```bash
cargo run -- subagent report --run-id <run_id>
```

Collect through the queued slot:

```bash
cargo run -- subagent collect --run-id <run_id>
```

Collection restores the dispatch identity and verifies the report through the queued subagent slot.

Collected reports must match the restored dispatch `task_id`, `agent_id`, and `parent_agent_id`. A mismatched report fails collection instead of being returned to the caller.

## Safety Rules

- Real runners must be allowlisted outside the core.
- Dangerous execution still requires explicit `--approve-exec`.
- Runners must not depend on Codex or Hermes Feishu bridges.
- Runners must not delete queue files autonomously.
- UI/control layers should display `required_capabilities`, `is_claimed`, `is_claim_stale`, and `has_report` before operators retry or release work.
