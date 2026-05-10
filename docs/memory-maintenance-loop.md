# Memory Maintenance Loop

更新时间：2026-05-09

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

它只生成 `identity_health`、`lim_candidates`、`decay_candidates`、`recommendations`、批量 `batches` 和结构化 `boundary`；`dry_run=true`、`writes_automatically=false`、`explicit_writeback_required=true`。

`boundary` 是维护闭环的固定分层口径：

```text
archive_layer=history_session_archive
archive_source=sqlite turn_summary records
archive_read_only=true
archive_mutation_allowed=false
maintenance_layer=maintenance_runtime
maintenance_mode=dry_run_report_then_explicit_apply
decay_boundary=review_only_not_writeback_candidate
decay_writeback_allowed=false
writeback_target=experiences.md
lim_writeback_requires_approval=true
core_memory_rewrite_allowed=false
automatic_writeback=false
```

人工批准写回入口：

```bash
cargo run -- memory maintenance apply --query TEXT [--session-id ID] [--limit N] [--candidate-id ID] --dry-run [--json]
cargo run -- memory maintenance apply --query TEXT [--session-id ID] [--limit N] [--candidate-id ID] --approve-writeback [--approval-note TEXT] [--json]
```

`apply --dry-run` 只选择并预览 LIM 候选，不写 `experiences.md`。真实写回必须显式传 `--approve-writeback`，输出 `approval` 回执和 `selected_candidates`；写入 `experiences.md` 时保留原始 LIM provenance，并附带 `writeback=memory_maintenance_apply`、批准来源、批准时间和可选 `approval_note`。

`decay_candidates` 只用于人工审查 `MEMORY.md` / `USER.md` 是否需要手动整理；它们不是写回候选，传给 `apply` 会被拒绝。

## 分层边界

维护闭环只跨三类边界工作：

- Archive：读取历史会话归档，也就是 SQLite `turn_summary` 记录；维护命令不能修改、删除、压缩 archive。
- Maintenance：生成 dry-run 报告、候选、回执和人工 apply 结果；它是维护运行时，不是核心记忆层。
- Decay：只提出 `MEMORY.md` / `USER.md` 人工 review 建议；decay 候选不能被 `apply` 写回，也不能自动改写核心文件。

唯一允许的维护写回路径是：

```text
LIM candidate -> explicit --approve-writeback -> experiences.md
```

这条路径必须保留 source record、批准来源、批准时间和 provenance。`MEMORY.md` / `USER.md` 只能通过显式 identity write 命令和 overwrite approval 处理，不能由 maintenance report 自动重写。

## 维护对象

- identity / MEMORY.md：核心热记忆，只能人工 review/显式覆盖。
- USER.md：用户事实，只能人工 review/显式覆盖。
- experiences.md：批准后的经验写回目标。
- session summary / turn_summary：历史 archive，只读来源。
- LIM 候选：从 archive 中抽取的写回候选，默认 dry-run。
- 外脑知识库索引：只读 knowledge 来源，不参与 maintenance 自动写回。

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
