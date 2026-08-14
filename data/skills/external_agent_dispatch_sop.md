---
skill_id: external_agent_dispatch_sop
canonical_id: external_agent_dispatch_sop
title: External Agent Dispatch SOP
name: External Agent Dispatch SOP
status: active
version: 1
trigger: "Use when a subagent needs bounded help from an external AI worker for research, review, comparison, multimodal planning, or stale-knowledge fallback."
aliases:
  - external-ai-dispatch
  - subagent-external-worker-sop
domains:
  - subagent
  - external_ai
  - governance
approval_policy: darwin_style_skill_lifecycle_v1
approval_source: self_policy:canonical_metadata_migration
score_total: 86
approval_threshold: 80
last_review_score: 86
last_reviewed_at: 2026-05-08T20:59:02Z
created_at: 2026-05-08T20:59:02Z
updated_at: 2026-05-08T20:59:02Z
content_hash: sha256:94dab0b9e52c6546009de0e7267be70e7726c6c0171eb9426c0a4a31056a340c
content_hash_scope: body_without_frontmatter_at_migration
source_proposal_ids:
  - manual_metadata_migration_2026_05_08
evidence_event_ids: []
provenance_event_ids: []
supersedes: []
superseded_by: null
duplicate_policy: upsert_canonical_skill_id
retirement_policy: deprecate_or_retire_in_place_never_delete
maintenance_status: canonicalized
---

# ---
# skill_id: external_agent_dispatch_sop
# title: External Agent Dispatch SOP
# trigger: external AI research, architecture review, or fallback search
# status: active
# version: 1
# approval_policy: repo_migration
# approval_source: repo_migration
# approval_note: canonical frontmatter migration for existing repo skill
# provenance_event_ids: []
# ---

# External Agent Dispatch SOP

## Position

This skill belongs to the AgentSlot downstream path.

It does not add a new core slot. It does not let the main process talk to
external AI platforms directly by default.

Target chain:

```text
main process -> subagent -> external AI identity engine -> platform session
```

The main process owns task decomposition, final review, user reporting, and
memory admission. Subagents may call external AI workers, then must review and
compress the result before returning a `SubagentReport`.

## When To Use

Use external AI only when the task benefits from a separate specialist or a
second opinion:

- broad web research or source gathering
- architecture review
- long document comparison
- multimodal generation planning
- independent code review
- fallback search when native provider knowledge may be stale

Do not use external AI for:

- secrets, tokens, private credentials, or personal data
- direct local filesystem access
- direct shell execution
- actions that require the main process governance layer
- simple tasks that the current runtime can handle cheaply

## Platform Map

This table is advisory. It can evolve from audit results and task outcomes.

| task kind | preferred worker | reason |
|---|---|---|
| deep search and architecture analysis | xiaocheng | strong research and design synthesis |
| broad material collection | kimi | long-context gathering and summarization |
| code review or logic cross-check | qianwen | independent reasoning and Chinese code review |
| multimodal draft, image, PPT planning | doubao | multimodal and presentation-oriented work |
| generic fallback | configured-default | use the healthiest enabled platform |

## Task Translation Template

Subagents should send compact, bounded prompts:

```text
Role: external specialist.
Task: <one concrete task>.
Project context: <only the minimum context needed>.
Constraints:
- Do not ask for secrets.
- Do not assume local filesystem access.
- Return concise, structured findings.
Output format:
- conclusion
- evidence or reasoning
- risks
- recommended next action
```

Never paste raw secrets, `.env` content, cookies, private keys, access tokens, or
full private logs into an external AI platform.

## Quality Rating

Every external result must be rated before it reaches the main process:

| rating | meaning |
|---|---|
| excellent | directly useful and well-supported |
| acceptable | useful but needs main-process review |
| retry_needed | incomplete, vague, or likely stale |
| failed | unusable or unsafe |

The subagent report must include:

- `platform`
- `audit_id`
- `quality`
- `result_summary`
- `risk_notes`
- `follow_up_needed`

## Follow-Up Rules

Ask at most two follow-up questions to the same external platform for one
subtask. If the answer remains weak, mark `retry_needed` or `failed` and return
the evidence to the main process.

Use a second platform only when:

- the task is high-impact
- the first answer conflicts with known project facts
- the first answer lacks enough evidence
- the subagent has remaining budget

## Memory Write Boundary

External AI output is never written directly to long-term memory.

Allowed path:

```text
external result -> subagent review -> SubagentReport -> main process review -> memory admission
```

Memory candidates should be short and provenance-rich:

- task kind
- platform
- quality rating
- reusable lesson
- source report id or audit id

## Audit Boundary

Audit logs may record:

- `audit_id`
- timestamp
- platform
- task summary
- duration
- success state
- quality rating
- failure class

Audit logs must not record:

- cookies
- tokens
- passwords
- private keys
- full browser profile paths when they expose account names
- raw private user messages beyond the bounded task summary

## Engine Contract

The unified identity engine adapter should accept:

```json
{
  "platform": "kimi",
  "task": "review the proposed memory architecture",
  "context": "bounded project context",
  "timeout_ms": 60000,
  "audit": true
}
```

It should return:

```json
{
  "success": true,
  "platform": "kimi",
  "audit_id": "uuid",
  "quality": "acceptable",
  "result": "structured response",
  "duration_ms": 12000,
  "failure_class": null
}
```

Adapter failures must be structured and retryable where possible. Login expiry
must return a manual-refresh signal; it must not attempt unattended login or
profile deletion.
