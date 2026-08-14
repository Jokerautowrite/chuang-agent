# Project provenance

This page records verifiable repository history. It is evidence of chuang-agent's independent development, not a claim that another project copied it and not a claim about any other project's private development timeline.

## Repository anchors

| Date | Commit or artifact | What it establishes |
|---|---|---|
| 2026-04-30 | `80be7453` — `feat: bootstrap chuang-agent prototype` | The first source prototype existed in this repository. |
| 2026-05-01 | `docs/blueprint-v1.md` | Memory-as-body, Rust event kernel, subagents, governance, and evolution were documented. |
| 2026-05-01 | `docs/pluggable-architecture-v1.md` | Provider, memory, context, subagent, actuator, governance, and evolver interfaces were documented. |
| 2026-05-01 | `docs/source-project-audit-v1.md` | Referenced projects and borrowed ideas were explicitly attributed. |
| 2026-07-18 | Dispatcher principle in the blueprint | The runtime was explicitly scoped as an orchestrator rather than the strongest worker. |
| 2026-08-14 | `2a202847bd74dd35399014f9c9aca4a17ab4cbbd` | Baseline immediately before the public-readiness repair branch. |

The Git history contains continuous implementation work across these dates. Preserve the original private repository and commit timestamps. Before a public release, create a signed tag and an offline full-history bundle with a published SHA-256 checksum; do not squash or rewrite the evidence chain.

## DeepSeek Harness context

DeepSeek publicly created the official [`deepseek-ai/deepseek-harness`](https://github.com/deepseek-ai/deepseek-harness) repository on 2026-08-13. Its release increased public interest in modular agent harnesses and is a useful moment to publish chuang-agent. That public date does not establish when DeepSeek began internal work.

The projects share common runtime patterns, including provider/tool/subagent seams, structured events, persistence, and governance. Chuang Agent's distinguishing thesis is cross-runtime long-term memory and identity continuity. Public comparisons should stay factual, cite first-party material, and avoid copying allegations in either direction.
