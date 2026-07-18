# spawn_subagent 真派工压测 · 2026-07-18

## 怎么跑

```bash
set -a; . ~/.config/chuang-agent/provider.env; set +a
export CHUANG_CODEX_RUNNER_ENABLE=1
export CHUANG_BIN=/home/user/projects/chuang-agent/target/debug/chuang-agent
# 或先 cargo build
bash scripts/chuang-spawn-stress.sh
```

可选：`SKIP_CASE=2` 跳过某例；日志与 verbose 输出在 `/tmp/chuang-spawn-stress-*`。

## 用例

| # | 场景 | 期望 |
|---|------|------|
| 1 | 单工人 analyze 读 `Cargo.toml` package.name | spawn + admission=accepted + body `chuang-agent` |
| 2 | `tasks[]` + `max_concurrency=2` 并行两问 | workers=2、队列 2 份 report Success |
| 3 | 工人查不存在路径 | 不崩；FINAL 说明不存在/失败 |

## 本轮结果（2026-07-18 16:31–16:34）

```text
PASS=3 FAIL=0  chuang_spawn_stress_ok
workdir=/tmp/chuang-spawn-stress-phVn7Y
```

| 例 | 证据要点 |
|----|----------|
| 1 | `admission=accepted`，`worker_model=gpt-5.6-luna`，~7s，body=`chuang-agent` |
| 2 | queue `…/184277-…` **reports=2** 均为 Success，stdout 各 `是`；body `tool_runtime=是` / `cli_runtime=是` |
| 3 | 工人 `test -e` 退出 1，结论不存在；parent 人话带上 |

全程无 `subagent_runtime_unavailable`。

## 摩擦与处理

| 摩擦 | 处理 |
|------|------|
| 默认 `run` 人话几乎看不出派工 | 人话 meta 增一行 `派工 N工人·accepted·luna`（`cli_output`） |
| 用例依赖「强制 spawn」提示词 | 压测脚本故意强制；日常是否自动派工另题 |
| Case2 父模型 completion 偏高 | 可接受；未当 P0 |

## 边界

- 仍要 `CHUANG_CODEX_RUNNER_ENABLE=1` + 本机 codex + runner 脚本  
- 非 live_worker adapter；queued_external + codex  
- 压测会打真实 provider / Luna，有费用与耗时  
