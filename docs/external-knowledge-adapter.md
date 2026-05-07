# External Knowledge Adapter

更新时间：2026-05-04

## 目标

把 wiki / GBrain 作为外脑知识库层接入 Chuang，但只走只读 adapter 边界，不把外脑直接写进核心主链。

## 当前边界

```text
status / doctor
memory knowledge status
memory knowledge search --root PATH --query TEXT [--limit N]
memory knowledge preview-context --root PATH --query TEXT [--limit N]
memory knowledge source-contract --source wiki|gbrain
```

当前只接本地 markdown/text 根目录的只读检索，不连接真实 wiki/GBrain，不做自动写回，不注入 runtime。

`memory knowledge search` 输出 `source/path/line/score/preview`，并在每条 hit 上附带稳定的 `provenance` 与 `evidence` 对象，用于验证 provenance-bearing search contract。当前 evidence 固定来自本地文件行匹配，字段包含 `local_file`、`line`、`score`、`query`、`read_only=true`、`connects_real_service=false`；hit provenance 也固定声明 `source=local_file`、`adapter=local_external_knowledge`、`writes_automatically=false`。它会跳过隐藏路径和疑似 secret/token/password/private/credential 文件，只作为外脑检索入口的本地 contract，不代表真实外部知识库已接通。

`memory knowledge preview-context` 复用同一批 search hit，但把它们包装成未来 runtime 注入前的 context segment candidates。它会显式标记 `read_only=true`、`connects_real_service=false`、`writes_automatically=false`、`runtime_injection_applied=false`、`runtime_retrieval_wired=false`，并为每个 segment 附带 `source/provenance/evidence/preview/score/token_estimate`。这个入口只做 preview，不代表 runtime 注入已经接线，也不会把外脑内容自动写入核心记忆。

`run --enable-knowledge-context-preview --knowledge-context-root PATH --knowledge-context-query TEXT` 是显式的本地只读注入开关：默认关闭，只有操作者同时给出 root/query 和 enable flag 时，才会把 preview segment 放入本轮 runtime context，并在 metadata 中标记 `knowledge_context_read_only=true`、`knowledge_context_connects_real_service=false`、`knowledge_context_writes_automatically=false`、`knowledge_context_runtime_retrieval_wired=false`。这不是 live wiki/GBrain adapter，也不自动写核心记忆。

`memory knowledge source-contract --source wiki|gbrain` 只输出未来 wiki/GBrain 只读 adapter 的 request/response/boundary 合同。它固定声明 `live_adapter_configured=false`、`connects_real_service=false`、`writes_automatically=false`、`runtime_retrieval_wired=false`，并要求后续真实 adapter 保留 provenance/evidence，凭 operator credentials 只读访问，禁止把 secret 写入仓库。

## 约束

- 外脑是知识库，不是 prompt 记忆。
- 先读后写，先检索后结论。
- 只读 adapter 可以存在，自动同步和自动注入都要后置。
- 不把 wiki/GBrain 直接塞进 runtime 主干。

## 下一步

1. 扩展本地 wiki 文件层的格式和 provenance 字段。
2. 再接索引召回，但继续保持 runtime 注入显式可见。
3. 最后评估 provenance 回写；自动同步和自动写核心记忆继续后置。
