# Actuator Command Protocol

`actuator = "command"` lets Chuang use a real operation adapter without linking desktop, browser, WeChat, ADB, or OS automation code into the core.

The core starts:

```toml
actuator = "command"
actuator_program = "sh"
actuator_args = "./scripts/chuang-actuator-adapter-example.sh --json"
actuator_timeout_ms = 30000
```

The adapter receives one JSON request on stdin and must return one JSON response on stdout.

Request shape:

```json
{
  "action": "observe|open_app|focus|click|input_text|screenshot",
  "observe_target": null,
  "open_app": null,
  "focus_target": null,
  "click_target": null,
  "input_target": null,
  "text": null,
  "screenshot_target": null
}
```

Response shape:

```json
{
  "observation": null,
  "app_handle": null,
  "evidence_ref": null,
  "message": "optional adapter detail"
}
```

Unknown top-level response fields are rejected. Adapter-specific detail should go in
`message` or in the typed result object for the requested action.

When the live gate is closed, the adapter message should still make the evidence
explicit, for example:

```text
allowed=true dry_run=true real_execution=false audit_label=actuator.operation.live required_env=CHUANG_REAL_ACTUATOR_ENABLE
```

Rules:

- Do not put secrets in logs or response text. `SecretOrPlainText::Secret` carries only a label.
- Do not implement broad arbitrary shell passthrough. The adapter must own an explicit allowlist of tools/actions.
- Real click/type/browser/desktop control belongs in the adapter, not in core runtime.
- GA 原子工具里的 `mouse` / `keyboard` / `screenshot` / `locate` 已映射到 actuator port，`wait` / `human_suspend` 已映射到本地 runtime port；核心 `status` / `doctor` 会把 9 个 GA 工具列为 mapped。真实桌面/浏览器动作必须经过 adapter、live gate、allowlist 和审计，但普通打开应用、点击和输入不需要额外人工审批；只有删除/清理/重置/卸载/支付/验证码/服务或网络变更/密钥访问等高危操作才询问或拒绝。
- Malformed quoting or trailing escapes in `actuator_args` are rejected before spawn instead of being normalized.
- Non-zero exit, timeout, malformed JSON, or missing required response fields are treated as actuator errors.
- The checked-in example script is a safe fixture. It does not operate the real desktop.
- Windows installations use `scripts/chuang-real-actuator-adapter.ps1` and the Windows allowlist by default; screenshots and foreground-window reads use native Windows APIs and do not require Python.
- macOS launcher installations use `scripts/chuang-real-actuator-adapter-macos.js` through the system `osascript`; Screen Recording and Accessibility permissions remain enforced by macOS.

## Checked-In Allowlist Scaffold

The repository includes a dry-run real adapter scaffold:

```bash
scripts/chuang-real-actuator-adapter.py --json --allowlist config/actuator-allowlist.example.json
```

`locate` and `screenshot` are read-only evidence actions. `open_app`, `mouse`,
and non-secret `keyboard` are allowlisted in the checked-in scaffold so the GA
atomic tool line is usable by default, but they execute real desktop actions
only when `CHUANG_REAL_ACTUATOR_ENABLE=1` is set in the service environment.
With the gate closed they return dry-run audit evidence instead of opening,
clicking, or typing.
