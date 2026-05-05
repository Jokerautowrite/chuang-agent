# External Knowledge Adapter

更新时间：2026-05-04

## 目标

把 wiki / GBrain 作为外脑知识库层接入 Chuang，但只走只读 adapter 边界，不把外脑直接写进核心主链。

## 当前边界

```text
status / doctor
memory knowledge status
memory knowledge search --root PATH --query TEXT [--limit N]
```

当前只接本地 markdown/text 根目录的只读检索，不连接真实 wiki/GBrain，不做自动写回，不注入 runtime。

`memory knowledge search` 输出 `source/path/line/score/preview`，用于验证 provenance-bearing search contract。它会跳过隐藏路径和疑似 secret/token/password/private/credential 文件，只作为外脑检索入口的本地 contract，不代表真实外部知识库已接通。

## 约束

- 外脑是知识库，不是 prompt 记忆。
- 先读后写，先检索后结论。
- 只读 adapter 可以存在，自动同步和自动注入都要后置。
- 不把 wiki/GBrain 直接塞进 runtime 主干。

## 下一步

1. 扩展本地 wiki 文件层的格式和 provenance 字段。
2. 再接索引召回，但继续保持 runtime 注入显式可见。
3. 最后评估 provenance 回写；自动同步和自动写核心记忆继续后置。
