# External AI Downstream

更新时间：2026-05-04

## 目标

把外部智能体放在子代理下游，作为可审计的下游 adapter，而不是主进程直接调用面。

## 当前边界

```text
dry-run adapter contract
```

当前已提供本地 dry-run 契约入口：

```bash
cargo run -- external-ai dispatch \
  --platform kimi \
  --task "review one bounded task" \
  --context "bounded project context" \
  --dry-run \
  --json
```

它只生成统一身份引擎请求、`audit_id`、质量字段和结构化结果；`connects_real_service=false`、`writes_memory=false`。真实 browser/HTTP adapter 仍必须在审计、登录态和 profile 隔离都明确后再接入。

## 约束

- 主进程不直接碰外部智能体。
- 子代理先审核，再决定是否下发。
- 下游结果必须可回传、可评分、可追溯。
- 任何平台接入都要先过 adapter contract。

## 下一步

1. 再补真实平台/session adapter 的 allowlist 和审计。
2. 再补 live adapter 的质量评分和追问上限回传。
3. 最后才接真实浏览器或 HTTP 会话。
