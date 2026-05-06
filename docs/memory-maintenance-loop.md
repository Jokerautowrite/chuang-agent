# Memory Maintenance Loop

更新时间：2026-05-07

## 目标

把记忆维护做成可审计的 dry-run -> 人工批准 -> provenance 写回闭环。

## 当前边界

```text
dry-run + manual apply + approval receipt
```

当前已有只读维护报告入口：

```bash
cargo run -- memory maintenance report --query TEXT [--session-id ID] [--limit N] [--json]
```

它只生成 `identity_health`、`lim_candidates`、`decay_candidates`、`recommendations` 和批量 `batches`；`dry_run=true`、`writes_automatically=false`、`explicit_writeback_required=true`。

人工批准写回入口：

```bash
cargo run -- memory maintenance apply --query TEXT [--session-id ID] [--limit N] [--candidate-id ID] --dry-run [--json]
cargo run -- memory maintenance apply --query TEXT [--session-id ID] [--limit N] [--candidate-id ID] --approve-writeback [--approval-note TEXT] [--json]
```

`apply --dry-run` 只选择并预览 LIM 候选，不写 `experiences.md`。真实写回必须显式传 `--approve-writeback`，输出 `approval` 回执和 `selected_candidates`；写入 `experiences.md` 时保留原始 LIM provenance，并附带 `writeback=memory_maintenance_apply`、批准来源、批准时间和可选 `approval_note`。

`decay_candidates` 只用于人工审查 `MEMORY.md` / `USER.md` 是否需要手动整理；它们不是写回候选，传给 `apply` 会被拒绝。

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
- 不自动写长期记忆；只有 `apply --approve-writeback` 会把被选中的 LIM 候选追加到 `experiences.md`。
- 不自动重写 `MEMORY.md` / `USER.md`；decay 只给人工 review 建议。

## 下一步

1. 把 approval receipt 暴露给未来只读控制台或飞书报告面。
2. 给候选 review 增加更清晰的人工选择 UX。
3. 最后再评估是否需要有限自动建议；默认仍不自动写回。
