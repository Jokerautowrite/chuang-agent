# Acceptance Next Matrix

更新时间：2026-05-09

快速入口：见 [第三测试版候选一页入口](./third-test-candidate.md) 和 [Live Operator Test Runbook](./live-operator-test-runbook.md)。

## 结论

当前基线已经达到第二测试版 `local-ready`：

```text
final verify 本地闭环通过
+ live-readiness 只读证据通过
+ Feishu / provider / subagent / console 诊断面可复验
+ long-run observability 有只读状态入口
+ Chuang 专用 Feishu bridge 已 active，老爸已确认可在 Feishu 联系上 Chuang
+ Feishu `/tools` 可见当前命令能力与边界
+ provider readiness check 已纳入本地候选门禁
```

第三测试版候选新增本地 wrapper：`sh scripts/chuang-third-test-smoke.sh`。它只串联 clean worktree gate、final verify、live-readiness 只读预检、operator checklist 只读摘要和 goal run status 只读摘要；operator env blocked 只作为状态输出，不作为本地合同失败。该 wrapper 不连接真实 Feishu、不读 secret、不启动服务，最终 marker 为 `third_test_candidate_smoke_ok`。

今晚候选验收另有 dirty-tree friendly 入口：`sh scripts/chuang-candidate-verify.sh`。它串联 complete-local smoke、live runner rehearsal smoke 和 provider readiness check；provider readiness check 只读取 `status --json` 的 `provider_readiness`，输出 `<set>/<missing>` 状态，不连接真实 provider、不打印 secret。缺 provider env 时按候选现场状态报告 blocker，但不会伪装成本地合同失败。

第三测试版候选不是“所有 live adapter 全开”，而是 100% 前最后一跳：用最小真实链路证明老爸可以通过 Chuang 专用 Feishu live 通道发起请求，主控能拿到 provider/env 状态、operator receipt、单个子代理 live rehearsal 证据，并最终回到本地 verify 绿。真实 runner 池、桌面 mutation、服务控制、wiki/GBrain live 仍后置，不纳入第三测试版必须项。

## 第三测试版候选 Acceptance

| 项目 | 判定 | 验收方式 | 100% 前是否必须人工验证 | 边界 |
| --- | --- | --- | --- | --- |
| third-test candidate wrapper | ready | `sh scripts/chuang-third-test-smoke.sh` -> `third_test_candidate_smoke_ok` | 否，自动复验即可 | 只串本地门禁和只读摘要；operator env blocked 可见但不让本地合同失败 |
| candidate verify wrapper | ready | `sh scripts/chuang-candidate-verify.sh` -> `chuang_candidate_verify_ok`，或明确报告 provider non-live block | 否，自动复验即可 | dirty-tree friendly；不连接真实 provider/Feishu；provider env 缺失只作为候选现场 blocker |
| final verify 本地门禁 | ready | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 否，自动复验即可 | 证明本地合同闭环，不证明 live Feishu 或真实 runner |
| live-readiness 只读预检 | ready | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | 否，自动复验即可 | 只读预检，不连接真实 Feishu、不读 secret、不控制服务 |
| Feishu `/tools` 可见能力 | ready | 在 Chuang 专用 Feishu 会话发送 `/tools` 或 `/capabilities`，可见 `/new`、`/session`、`/health`、`/receipt`、`/live-check`、普通文本和图片 OCR 边界 | 否，已作为 bridge 命令面可复验；live 侧可继续截图/receipt 留证 | 只展示当前能力与边界，不执行本地检查、不修改服务、不打印 secret |
| 人工 Feishu live check | candidate | 老爸用 Chuang 专用 Feishu 通道发 `/health`、`/session` 和一条普通测试消息，确认 app-server/session/channel 有真实 receipt | 是 | 只用 Chuang 专用 bot 和 env；不碰 Codex Feishu、不碰 Hermes、不打印 token |
| provider env 对齐 | candidate | `scripts/chuang-provider-readiness-check.sh` 读取 `status --json`，人工确认 Chuang provider env 变量存在且配置名一致；输出只允许 `<set>/<missing>` | 是 | 不连接真实 provider；不在聊天、日志、文档或 patch 中泄露 secret；无 fallback 时必须显式报错 |
| live operator receipt | candidate | 人工执行 live cutover checklist，保存 request_id、operator、时间、允许范围、回退条件和结果摘要 | 是 | receipt 只记录审计元数据，不记录凭证、验证码或私密正文 |
| single subagent live rehearsal | candidate | 在 live gate + allowlist 下只跑一个子代理 rehearsal，确认 report/proposal 被主控接收 | 是 | 单 worker、bounded、可停止；子代理不能直接写核心记忆，不能扩大成 runner 池 |
| final verify after live rehearsal | candidate | live rehearsal 后再次运行 `sh scripts/chuang-final-verify.sh` 和本文档 diff check | 是 | live 尝试不能破坏本地合同；失败时先停在诊断，不做 cleanup/reset |

## 100% 前必须人工验证

这些项目是从第三测试版候选走向 100% 前的真实证据，不应由本地 smoke 冒充：

1. Chuang 专用 Feishu live 通道真实收发一次，并拿到可审计 receipt。
2. Chuang provider env 与运行配置对齐，所有 secret 只显示为 `<set>`。
3. live operator receipt 完整记录审批范围、执行人、时间、request_id、结果和回退条件。
4. 单个子代理 live rehearsal 通过 gate、allowlist、capability routing 和 report admission。
5. live rehearsal 后本地 final verify 仍通过。

## 仍然后置

| 后置能力 | 后置原因 | 最早进入条件 | 验收边界 |
| --- | --- | --- | --- |
| 真实 runner 池 | 多 worker 并发会放大外部进程、登录态和任务副作用 | 单 subagent live rehearsal 有 receipt，stop/timeout/report admission 都可审计 | 先单 worker，再 bounded 并发；不把 rehearsal 结果解释成 runner 池 ready |
| 桌面 mutation | 涉及真实 UI、登录态、验证码和不可逆操作 | observe-only、action allowlist、验证码规则和人工 receipt 稳定 | observe 可先行；点击/提交/修改类动作后置 |
| 服务控制 apply | 涉及 start/stop/restart/change_model 等服务扰动 | Chuang-only allowlist、dry-run receipt、人工审批范围明确 | 不允许任意 systemd；不含 Codex Feishu 或 Hermes |
| wiki/GBrain live | 涉及外部知识库权限、检索质量和写入策略 | 本地 knowledge search provenance/evidence 稳定，live 只读账号和审计面确认 | 先只读检索；不自动写外脑 |

## 当前证据状态

| 证据面 | 最新状态 | 已验证命令 | 第三测试版含义 |
| --- | --- | --- | --- |
| final verify | 已完成 | `sh scripts/chuang-final-verify.sh` -> `chuang_final_verify_ok` | 本地门禁可作为 live 前后对照 |
| live-readiness preflight | 已完成 | `sh scripts/chuang-live-readonly-preflight.sh` -> `live_readiness_preflight_ok` | live 前只读排查入口已收口 |
| candidate verify | 已完成 | `sh scripts/chuang-candidate-verify.sh`，并包含 `scripts/chuang-provider-readiness-check.sh` | 本地候选门禁已经覆盖 provider readiness 只读状态；缺 env 会显式报告 blocker |
| Feishu evidence | 已完成 | `node --check scripts/chuang-feishu-live-preflight.js && node scripts/chuang-feishu-live-preflight-smoke.js && node scripts/chuang-feishu-command-smoke.js` | 本地命令和诊断链已可复验；`/tools` 已列出当前可见能力与边界 |
| Feishu live contact | 已完成 | 老爸在 Chuang 专用 Feishu 会话中确认已联系上；本地 `chuang-feishu-bot.service` active | 说明 bridge 已挂上；后续重点是 `/health`、`/session`、普通任务 runtime report 和 receipt 证据 |
| provider evidence | 已完成 | `cargo test -q --test slot_registry_tests --test runtime_report_tests`；`scripts/chuang-provider-readiness-check.sh` | fallback/capacity/retryable 诊断合同已存在；provider readiness check 已进入候选门禁；live 前仍要人工确认 env 对齐 |
| subagent evidence | 已完成 | `cargo test -q --test cli_subagent_live_preflight_tests` | gate/allowlist/capability/report admission rehearsal 已有；下一步只允许单 worker live rehearsal |
| console/watchdog evidence | 已完成 | `cargo test -q --test cli_console_tests` 和 `./scripts/chuang-goal-watchdog.sh --once` | 长跑状态有只读入口；不派活、不重启、不提交 |
| memory evidence | 已完成 | `cargo test -q --test memory_maintenance_cli_tests` | 写回仍需 `--approve-writeback`；live rehearsal 不得让子代理直写核心记忆 |

## 第三测试版执行顺序

1. 先看 [第三测试版候选一页入口](./third-test-candidate.md)。
2. 在干净工作树上复跑 `sh scripts/chuang-third-test-smoke.sh`，确认本地候选 wrapper 输出 `third_test_candidate_smoke_ok`。
3. 复跑 `sh scripts/chuang-candidate-verify.sh`，确认 complete-local、live runner rehearsal 和 provider readiness check 口径一致。
4. 复跑 `sh scripts/chuang-final-verify.sh`，确认本地门禁绿。
5. 复跑 `sh scripts/chuang-live-readonly-preflight.sh`，确认 live 只读预检绿。
6. 人工确认 provider env 对齐，只报告变量名和 `<set>`。
7. 在 Chuang 专用 Feishu 会话发 `/tools`，确认可见能力与边界符合本文档。
8. 人工执行 Chuang 专用 Feishu live check，优先发 `/health`、`/session` 和一条普通任务，采集 request/session/channel/runtime report receipt。
9. 人工执行 single subagent live rehearsal，采集 gate/allowlist/report receipt。
10. 复跑 `sh scripts/chuang-final-verify.sh`，确认 live rehearsal 未破坏本地合同。
11. 跑 `git diff --check -- docs/acceptance-next-matrix.md`，确认本文档格式干净。

## 非目标

- 不启用真实 runner 池。
- 不做桌面 mutation。
- 不做真实服务控制 apply。
- 不接入 wiki/GBrain live 写入。
- 不修改 Codex Feishu 或 Hermes。
- 不删除、cleanup、reset、purge、uninstall 任何目标。
- 不把本地 readiness 证据表述成 live 100% 完成。
