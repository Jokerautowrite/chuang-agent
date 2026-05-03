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

Rules:

- Do not put secrets in logs or response text. `SecretOrPlainText::Secret` carries only a label.
- Do not implement broad arbitrary shell passthrough. The adapter must own an explicit allowlist of tools/actions.
- Real click/type/browser/desktop control belongs in the adapter, not in core runtime.
- Non-zero exit, timeout, malformed JSON, or missing required response fields are treated as actuator errors.
- The checked-in example script is a safe fixture. It does not operate the real desktop.

## Checked-In Allowlist Scaffold

The repository includes a dry-run real adapter scaffold:

```bash
scripts/chuang-real-actuator-adapter.py --json --allowlist config/actuator-allowlist.example.json
```

Live `open_app` is disabled by default and only runs when `CHUANG_REAL_ACTUATOR_ENABLE=1` is set. Click, input, and screenshot must be explicitly enabled in the allowlist. The example allowlist keeps them disabled.
