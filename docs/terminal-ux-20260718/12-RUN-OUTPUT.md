# run 子命令输出 · 第四刀 · 2026-07-18

## 问题
`chuang run --input …` 默认打 `model_name/body/trace/context_*` 字段墙，和 REPL 的「答复优先」不一致。

## 改动
| 模式 | 行为 |
|------|------|
| 默认 | `小创  {model}` → 正文 → `模型 · provider · 引擎 · 召回`；短 operational `key: value` 保留；`*_json` 大块隐藏 |
| `--verbose` | 完整字段墙（旧行为，脚本/排障） |

入口：
- `cli_output::print_runtime_result` / `print_runtime_result_verbose`
- `run_command` 解析 `--verbose`（`split_run_verbosity`）
- REPL `/verbose` 与非 TTY `repl --verbose` 仍走完整字段墙

## 验收
```bash
chuang run --input "哈喽"
chuang run --verbose --input "哈喽"
```
