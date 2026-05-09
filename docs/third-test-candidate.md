# Third Test Candidate Quick Entry

更新时间：2026-05-09

这一页只回答四件事：现在哪些已经是 `local-ready`，哪些必须人工 live check，怎么跑第三测试版候选，和 100% 前最后的硬门槛是什么。

## 一句话结论

第三测试版候选不是“所有 live adapter 全开”，而是先用最小真实链路证明：老爸能通过 Chuang 专用 Feishu live 通道发起请求，主控能拿到 provider/env 状态、operator receipt、单个子代理 live rehearsal 证据，然后再回到本地 `final verify` 绿。

2026-05-09 更新：Chuang 专用 Feishu bridge 已由 systemd 长连接保持 active，`channel feishu-check` 和 bridge command smoke 已通过；老爸已确认能在 Feishu 联系上 Chuang。`/tools` / `/capabilities` 已能展示当前可见命令能力与边界。下一步不再卡“桥是否挂上”，而是收集 live 侧可审计 receipt、provider `<set>` 状态和单 worker rehearsal 证据。

当前固定状态词：

- `ga_local_mapped_only`：GA 9 tools 已在本地命令面和诊断面映射，但只代表 slot/route 可见。
- `desktop_browser_live_gated`：真实桌面和浏览器动作仍需要 live gate、allowlist、治理和 operator receipt；当前不能写成 desktop/browser live ready。
- `live_worker_available=false`：当前 subagent preflight/rehearsal 不启动、不附着真实 worker。
- `real_external_acceptance_pending`：Feishu/provider/desktop/browser/wiki/GBrain 的真实外部验收仍 pending，不能由本地 smoke、`<set>` 或 `/tools` 代替。

## 现在的分层

### local-ready

- `sh scripts/chuang-final-verify.sh`
- `sh scripts/chuang-live-readonly-preflight.sh`
- `bash scripts/chuang-live-gaps-check.sh`
- `sh scripts/chuang-candidate-verify.sh`
- `scripts/chuang-provider-readiness-check.sh`
- `cargo run --quiet -- channel feishu-check --env-file /home/user/.codex-im/chuang-feishu-bridge.env --json`
- `node scripts/chuang-feishu-command-smoke.js`

这些都属于本地可复验门禁，不要求真实 Feishu、不读 secret、不控制服务。`chuang-live-gaps-check.sh` 会输出三段矩阵：`local_contract=ready`、`preflight=ready_but_no_start`、`real_live=pending`，用于防止把本地合同或 ready-but-no-start 预检误写成真实 live。`chuang-candidate-verify.sh` 会把 live-gaps 和 provider readiness check 纳入候选门禁；provider readiness check 只读取 `status --json` 的 `provider_readiness`，输出 `<set>/<missing>`，不连接真实 provider。

### 必须人工 live check

- Chuang 专用 Feishu live 通道真实收发一次，并拿到可审计 receipt。
- 在 Chuang 专用 Feishu 会话发 `/tools`，确认可见能力包含 `/new`、`/session`、`/health`、`/receipt`、`/live-check`、普通文本和图片 OCR 边界。
- Chuang provider env 对齐，输出只允许 `变量名=<set>`。
- 生成 live operator receipt，保留 request_id、operator、时间、允许范围、回退条件和结果摘要。
- 单个子代理 live rehearsal 通过 gate + allowlist + capability routing + report admission。
- live rehearsal 之后再次跑 `sh scripts/chuang-final-verify.sh`，确认本地合同没坏。

## 100% 前唯一硬门槛

唯一硬门槛不是“再多跑几个 smoke”，而是把上面的人工 live 链闭环做实：

1. Chuang 专用 Feishu live 通道拿到真实 receipt。
2. provider/env 对齐完成，secret 只显示为 `<set>`。
3. operator receipt 完整。
4. 单个子代理 live rehearsal 完整。
5. rehearsal 后 `final verify` 仍然通过。

这条链没有闭环前，不算 100%。

## 建议执行顺序

1. 先跑本地门禁：

```bash
sh scripts/chuang-final-verify.sh
sh scripts/chuang-live-readonly-preflight.sh
bash scripts/chuang-live-gaps-check.sh
sh scripts/chuang-candidate-verify.sh
scripts/chuang-provider-readiness-check.sh
```

2. 再跑只读 operator 预检与回执模板：

```bash
scripts/chuang-live-operator-checklist.sh --json
scripts/chuang-live-operator-receipt.sh --json
```

3. 按 runbook 做人工 live check：

```text
/live-check
/tools
/health
/new
/session
```

已联系上的 Feishu 会话优先用 `/tools`、`/health` 和 `/session` 留证据；需要新上下文时再发 `/new`，避免把“能联系上”误判成还缺本地绑定。

4. 只允许单个子代理 live rehearsal，不扩成 runner 池。
5. rehearsal 后再跑一次 `sh scripts/chuang-final-verify.sh`。
6. 最后做 `git diff --check -- docs/third-test-candidate.md docs/acceptance-next-matrix.md docs/live-operator-test-runbook.md`。

## 边界

- 不碰 Codex Feishu。
- 不碰 Hermes。
- 不打印 token 或 secret。
- 不做 deletion、cleanup、reset、purge、uninstall。
- 不把 local-ready smoke、provider readiness check、GA 本地映射或 `/tools` 可见能力解释成 100%。
- 不把 `desktop_browser_live_gated`、`live_worker_available=false` 或 `real_external_acceptance_pending` 写成真实 live 完成。

## 快速参考

- [Acceptance Next Matrix](./acceptance-next-matrix.md)
- [Live Operator Test Runbook](./live-operator-test-runbook.md)
