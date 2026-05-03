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
  --max-concurrency 1 \
  --capability rust \
  --capability filesystem
```

The worker only claims a dispatch when all required capabilities are present. Dispatches without requirements match any worker.

Current MVP concurrency is explicit single-worker sequencing: `--max-concurrency 1`. Values above `1` are rejected until real parallel scheduling is implemented.

## Command Runner IO

When `--runner command` is used, Chuang starts the runner process directly. It does not invoke a shell unless the configured command is a shell program such as `sh`.

The runner receives the full dispatch JSON on stdin.

The runner may return either:

- plain stdout/stderr, which Chuang wraps into a `SubagentReport`;
- a full `SubagentReport` JSON on stdout.

When a full report is returned, Chuang validates identity before accepting it:

- the JSON must satisfy the `SubagentReport` v1 required-field contract;
- `task_id` must match the dispatch;
- `agent_id` must match the dispatch;
- `parent_agent_id` must match the dispatch parent.

Invalid protocol reports, including reports with missing required fields, bad status values, invalid timestamps, or identity mismatches, are stored as failed reports. They are not treated as success.

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
