# Memory Maintenance Loop

更新时间：2026-05-04

## 目标

把记忆维护做成可审计的 dry-run 闭环，再决定是否写回。

## 当前边界

```text
dry-run + manual apply
```

当前已有只读维护报告入口：

```bash
cargo run -- memory maintenance report --query TEXT [--session-id ID] [--limit N] [--json]
```

它只生成 `identity_health`、`lim_candidates`、`recommendations`，并提供显式 `memory maintenance apply` 人工确认写回入口；仍不启自动维护，不自动改写记忆文件。

## 维护对象

- identity / MEMORY.md
- experiences.md
- session summary
- LIM 候选
- 外脑知识库索引

## 约束

- 先生成健康报告，再谈写回。
- 先人工确认，再谈自动化。
- 不做 silent rewrite。
- 不把 maintenance loop 放进主运行链。

## 下一步

1. 再补 decay / extractor 的批量 dry-run。
2. 最后再考虑有限自动写回。
