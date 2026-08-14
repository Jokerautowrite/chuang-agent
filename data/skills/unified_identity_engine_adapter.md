---
skill_id: unified_identity_engine_adapter
canonical_id: unified_identity_engine_adapter
title: Unified Identity Engine Adapter
name: Unified Identity Engine Adapter
status: active
version: 1
trigger: "Use when implementing or reviewing the lower external-AI identity adapter boundary for platform/session selection, request execution, audit logging, and structured failures."
aliases:
  - identity-engine-adapter
  - external-ai-identity-adapter
domains:
  - external_ai
  - identity
  - adapter_contract
approval_policy: darwin_style_skill_lifecycle_v1
approval_source: self_policy:canonical_metadata_migration
score_total: 84
approval_threshold: 80
last_review_score: 84
last_reviewed_at: 2026-05-08T20:59:02Z
created_at: 2026-05-08T20:59:02Z
updated_at: 2026-05-08T20:59:02Z
content_hash: sha256:71d5741a5c9ecfd7fba0dc69fade3d6fa6e1132cbaf4b6d4a8ab78d04238b819
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
# skill_id: unified_identity_engine_adapter
# title: Unified Identity Engine Adapter
# trigger: external-AI platform/session execution or audit boundary
# status: active
# version: 1
# approval_policy: repo_migration
# approval_source: repo_migration
# approval_note: canonical frontmatter migration for existing repo skill
# provenance_event_ids: []
# ---

# Unified Identity Engine Adapter

## Purpose

This adapter is the lower external-AI boundary.

It is responsible for:

- platform/session selection
- login-state reuse
- request execution
- response capture
- audit logging
- circuit breaking

It does not belong in the core slot count.
It does not own the external-AI policy.
It does not write long-term memory directly.

## Input Contract

The adapter accepts a bounded request:

```json
{
  "platform": "kimi",
  "task": "collect sources for the new memory layer",
  "context": "bounded project context",
  "session_hint": "optional-session-id",
  "timeout_ms": 60000,
  "audit": true
}
```

Required fields:

- `platform`
- `task`
- `context`

Optional fields:

- `session_hint`
- `timeout_ms`
- `audit`

## Output Contract

The adapter returns a bounded result:

```json
{
  "success": true,
  "platform": "kimi",
  "audit_id": "uuid",
  "quality": "acceptable",
  "result": {
    "summary": "structured answer",
    "evidence": [],
    "risks": [],
    "follow_up_needed": false
  },
  "duration_ms": 12000,
  "failure_class": null
}
```

## Boundary Rules

- No cookie, token, password, or private-key data in logs.
- No unattended login repair.
- No automatic profile deletion.
- No direct long-term memory writes.
- No silent platform fallback across accounts.
- All session reuse must be explicit and auditable.

## Failure Classes

Use structured failures only:

- `session_expired`
- `login_required`
- `timeout`
- `platform_unavailable`
- `unexpected_response`
- `audit_failed`

Failures should be retryable only when they do not imply credential or session
repair.

## Expected Evolution

This adapter can later be backed by:

- browser profile reuse
- CDP session attachment
- HTTP-compatible provider bridges
- platform-specific session managers

The contract stays stable even if the backing implementation changes.
