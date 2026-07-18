# REPL App-Server Transport

Interactive `chuang repl` uses the canonical Unix socket by default:

```text
${XDG_RUNTIME_DIR}/chuang-agent/app-server.sock
```

`CHUANG_APP_SERVER_MODE=local` and `CHUANG_REPL_STUB=1` keep the direct local runtime path.
Socket failures are returned as `app_server_unavailable`; the REPL does not fall back locally.
The launchers preserve the caller's directory in `CHUANG_REPL_WORKSPACE_ROOT`, so socket turns,
local compatibility turns, terminal metadata, and approval handling use the same workspace even
though the launcher changes into the project root to load Chuang's config.

The current `turn/start` protocol is request/response. It preserves final answer, model,
packed-context usage, provider metadata, and tool/approval metadata, but it does not expose
streamed progress or an actionable interrupt while a synchronous turn is running. In socket mode,
the REPL states this explicitly for `/stop` and mid-turn guidance instead of claiming either action
was delivered. The generic `turn/interrupt` RPC also returns an explicit unsupported error.

Thread state is daemon-local. A caller must omit `threadId` to start a new thread; an unknown or
stale id is rejected instead of silently creating a replacement. Completed thread snapshots retain
`providerMeta`, including pending-approval metadata, and approval turns use
`status=human_input_required`.
