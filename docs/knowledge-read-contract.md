# Knowledge Read Contract

`knowledge_read` is the live-read interface for real wiki/GBrain queries.

It is separate from local external knowledge preview:

- Local preview: `memory knowledge search` / `preview-context` over local files.
- Live read: audited adapter query against real wiki/GBrain services.

Current state:

- Contract version: `1`.
- Fake implementation: `FakeKnowledgeReadAdapter`, for injected hit tests only.
- Default real implementation: `UnavailableKnowledgeReadAdapter`.
- Missing real adapter returns structured `knowledge_read_unavailable`.
- Status must keep local preview separate from real wiki/GBrain live reads.

Until a real read-only wiki/GBrain adapter is configured with endpoint, credential loading, provenance, and receipts, Chuang must not claim it has queried real wiki/GBrain.
