# Browser Read Contract

`browser_read` is a separate live interface for browser page state. It is not the same thing as `desktop_read`.

## Boundaries

- `desktop_read`: actuator observation through `locate` / `screenshot`. It can produce read-only screen or window evidence.
- `browser_read` / `browser_navigate`: audited CDP access to a **managed headless Chrome**. Exposes URL, title, and DOM text.

## Current state (2026-07-18)

| 层 | 状态 |
|----|------|
| Contract version | `1` |
| Adapter | `CdpBrowserReadAdapter`（HTTP `/json/*` + WebSocket `Runtime.evaluate` / `Page.navigate`） |
| Fake | `FakeBrowserReadAdapter`（单测注入） |
| Unavailable | 无 CDP 时结构化 `browser_read_unavailable` |
| Managed browser | `scripts/chuang-headless-chrome.sh`（默认端口 9222） |
| Model tools | `browser_read`、`browser_navigate` |

## Enable

```bash
# 1) 启动托管无头 Chrome
./scripts/chuang-headless-chrome.sh start
export CHUANG_CDP_PORT=9222   # 可选；9222 可达时也会自动探测

# 2) 状态应显示 browser_read live
chuang status --json | jq .browser_readiness

# 3) 模型工具
# ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"browser_navigate","url":"https://example.com"}}
# ACTION: {"schema_version":1,"type":"tool_call","call":{"tool":"browser_read"}}
```

## Rules

- Desktop observation evidence must not be re-labeled as browser DOM reads.
- Navigate only allows `http(s)://`, `file://`, `about:`.
- DOM text is truncated (~12k chars) for tool outputs.
- Until CDP is reachable, tools return structured errors; status keeps `browser_read_unavailable`.
