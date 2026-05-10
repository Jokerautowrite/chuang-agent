# Browser Read Contract

`browser_read` is a separate live-read interface for browser page state. It is not the same thing as `desktop_read`.

## Boundaries

- `desktop_read`: actuator observation through `locate` / `screenshot`. It can produce read-only screen or window evidence.
- `browser_read`: browser page read through an audited adapter. It may expose URL, title, and DOM text when a real adapter exists.

Current state:

- Contract version: `1`.
- Fake implementation: `FakeBrowserReadAdapter`, for injected snapshot tests only.
- Default real implementation: `UnavailableBrowserReadAdapter`.
- Missing real adapter returns structured `browser_read_unavailable`.
- Status exposes `browser_readiness.overall_state=desktop_read_ready_browser_read_unavailable`.

Until a real CDP/Playwright/browser adapter is added, Chuang must not claim it has read the current browser URL, title, or DOM. Desktop observation evidence must not be re-labeled as browser DOM reads.
