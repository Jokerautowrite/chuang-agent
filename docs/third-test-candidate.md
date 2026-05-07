# Third Test Candidate Quick Entry

更新时间：2026-05-07

这一页只回答四件事：现在哪些已经是 `local-ready`，哪些必须人工 live check，怎么跑第三测试版候选，和 100% 前最后的硬门槛是什么。

## 一句话结论

第三测试版候选不是“所有 live adapter 全开”，而是先用最小真实链路证明：老爸能通过 Chuang 专用 Feishu live 通道发起请求，主控能拿到 provider/env 状态、operator receipt、单个子代理 live rehearsal 证据，然后再回到本地 `final verify` 绿。

## 现在的分层

### local-ready

- `sh scripts/chuang-final-verify.sh`
- `sh scripts/chuang-live-readonly-preflight.sh`

这两项都属于本地可复验门禁，不要求真实 Feishu、不读 secret、不控制服务。

### 必须人工 live check

- Chuang 专用 Feishu live 通道真实收发一次，并拿到可审计 receipt。
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
```

2. 再跑只读 operator 预检与回执模板：

```bash
scripts/chuang-live-operator-checklist.sh --json
scripts/chuang-live-operator-receipt.sh --json
```

3. 按 runbook 做人工 live check：

```text
/live-check
/health
/new
/session
```

4. 只允许单个子代理 live rehearsal，不扩成 runner 池。
5. rehearsal 后再跑一次 `sh scripts/chuang-final-verify.sh`。
6. 最后做 `git diff --check -- docs/third-test-candidate.md docs/acceptance-next-matrix.md`。

## 边界

- 不碰 Codex Feishu。
- 不碰 Hermes。
- 不打印 token 或 secret。
- 不做 deletion、cleanup、reset、purge、uninstall。
- 不把 local-ready smoke 解释成 100%。

## 快速参考

- [Acceptance Next Matrix](./acceptance-next-matrix.md)
- [Live Operator Test Runbook](./live-operator-test-runbook.md)

