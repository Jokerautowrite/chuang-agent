#!/usr/bin/env node

const assert = require("assert");
const {
  buildBridgeErrorReply,
  buildHelpCommandReply,
  buildHealthCommandReply,
  buildLiveCheckCommandReply,
  buildReceiptCommandReply,
  buildToolsCommandReply,
  buildNewSessionCommandReply,
  buildSessionCommandReply,
  loadCapabilityPrimerText,
  parseBridgeCommand,
  sanitizeErrorMessage,
} = require("./chuang-feishu-bridge-commands");

const reply = parseBridgeCommand(" /new ");
assert(reply, "/new should be handled as a bridge command");
assert.strictEqual(reply.commandName, "new");
assert.strictEqual(reply.modelName, "chuang-feishu-bridge");
assert(reply.replyText.includes("开新窗口/新上下文命令"));
assert(reply.replyText.includes("不会进入 Agent 主链"));
assert(reply.replyText.includes("不会消耗一轮任务"));
assert(reply.replyText.includes("飞书机器人不能直接替你创建客户端窗口"));
assert(reply.replyText.includes("/codex bind /home/user/projects/chuang-agent"));
assert(buildNewSessionCommandReply("chuang-thread-1").replyText.includes("当前 Feishu 聊天已切到新的 Chuang 会话"));

assert.strictEqual(parseBridgeCommand("继续推进"), null);
assert.strictEqual(buildNewSessionCommandReply().commandName, "new");

const help = parseBridgeCommand("/help");
assert(help, "/help should be handled as a bridge command");
assert.strictEqual(help.commandName, "help");
assert(help.replyText.includes("/new"));
assert(help.replyText.includes("/session"));
assert(help.replyText.includes("/health"));
assert(help.replyText.includes("图片消息：会先下载、OCR，再进入 Chuang 主链。"));
assert(help.replyText.includes("/receipt"));
assert(help.replyText.includes("/live-check"));
assert(help.replyText.includes("/tools"));
assert(help.replyText.includes("开新窗口/新上下文入口"));
assert(help.replyText.includes("普通文本会转发到 Chuang app-server"));
assert.strictEqual(buildHelpCommandReply().commandName, "help");

const receipt = parseBridgeCommand("/receipt");
assert(receipt, "/receipt should be handled as a bridge command");
assert.strictEqual(receipt.commandName, "receipt");
assert(receipt.replyText.includes("chuang-live-operator-receipt.sh --json"));
assert(receipt.replyText.includes("tested_at"));
assert(receipt.replyText.includes("runtime_report_id"));
assert(receipt.replyText.includes("CHUANG_LIVE_OPERATOR"));
assert(receipt.replyText.includes("不要把 secret"));
assert.strictEqual(buildReceiptCommandReply().commandName, "receipt");

const liveReceipt = parseBridgeCommand("/live-receipt");
assert(liveReceipt, "/live-receipt should be handled as a bridge command");
assert.strictEqual(liveReceipt.commandName, "receipt");
assert(liveReceipt.replyText.includes("静态模板"));

const liveCheck = parseBridgeCommand("/live-check");
assert(liveCheck, "/live-check should be handled as a bridge command");
assert.strictEqual(liveCheck.commandName, "live-check");
assert(liveCheck.replyText.includes("chuang-live-operator-checklist.sh --json"));
assert(liveCheck.replyText.includes("chuang-live-readonly-preflight.sh"));
assert(liveCheck.replyText.includes("不执行任何本地命令或 checklist"));
assert(liveCheck.replyText.includes("不连接外部服务"));
assert(liveCheck.replyText.includes("ready"));
assert(liveCheck.replyText.includes("blocked"));
assert(liveCheck.replyText.includes("warning"));
assert(liveCheck.replyText.includes("不要把 secret"));
assert.strictEqual(buildLiveCheckCommandReply().commandName, "live-check");

const tools = parseBridgeCommand("/tools");
assert(tools, "/tools should be handled as a bridge command");
assert.strictEqual(tools.commandName, "tools");
assert(tools.replyText.includes("当前可见能力与边界"));
assert(tools.replyText.includes("/capabilities"));
assert(tools.replyText.includes("主链工具能力"));
assert(tools.replyText.includes(loadCapabilityPrimerText()));
assert(tools.replyText.includes("file_read"));
assert(tools.replyText.includes("file_write"));
assert(tools.replyText.includes("code_execute"));
assert(tools.replyText.includes("list_dir"));
assert(tools.replyText.includes("会话记忆召回"));
assert(tools.replyText.includes("provider/runtime"));
assert(tools.replyText.includes("goal/subagent 派活入口"));
assert(tools.replyText.includes("goal plan/show/dispatch/step/collect/checkpoint"));
assert(tools.replyText.includes("subagent dispatch/list/run-once/run-loop/report/collect"));
assert(tools.replyText.includes("live runner 当前仍是 preflight-only / rehearsal-only"));
assert(tools.replyText.includes("不启用真实 runner 池"));
assert(tools.replyText.includes("单 worker、allowlist、bounded、带 receipt"));
assert(tools.replyText.includes("不复用 Hermes bridge"));
assert(tools.replyText.includes("不复用 Codex bridge"));
assert(tools.replyText.includes("图片消息：先下载并 OCR"));
assert(tools.replyText.includes("不打印 secret"));
assert(tools.replyText.includes("不执行本地检查"));
assert(tools.replyText.includes("不启动服务"));
assert.strictEqual(buildToolsCommandReply().commandName, "tools");

const capabilities = parseBridgeCommand(" /capabilities ");
assert(capabilities, "/capabilities should be handled as a bridge command");
assert.strictEqual(capabilities.commandName, "tools");

const session = parseBridgeCommand("/session");
assert(session, "/session should be handled as a bridge command");
assert.strictEqual(session.commandName, "session");
assert(session.replyText.includes("当前会话"));
const boundSession = buildSessionCommandReply({
  chatId: "chat-1",
  threadId: "chuang-thread-9",
  workspaceRoot: "/home/user/projects/chuang-agent",
  updatedAt: "2026-05-07T00:00:00.000Z",
});
assert(boundSession.replyText.includes("已绑定当前飞书聊天"));
assert(boundSession.replyText.includes("chuang-thread-9"));
assert(boundSession.replyText.includes("工作区：/home/user/projects/chuang-agent"));

const health = parseBridgeCommand("/health");
assert(health, "/health should be handled as a bridge command");
assert.strictEqual(health.commandName, "health");
const healthReply = buildHealthCommandReply({
  bridgeReady: true,
  workspaceRoot: "/home/user/projects/chuang-agent",
  appServer: { running: true, lastError: "token=secret-value app_secret=hidden" },
  session: { bound: true, threadId: "chuang-thread-9" },
  env: {
    appIdState: "<set>",
    appSecretState: "<set>",
    providerEnvState: "CHUANG_PROVIDER_ENV_FILE=<set> CODEX_PPTOKEN_API_KEY=<set>",
  },
});
assert(healthReply.replyText.includes("健康诊断"));
assert(healthReply.replyText.includes("app-server：running"));
assert(healthReply.replyText.includes("session：bound chuang-thread-9"));
assert(healthReply.replyText.includes("CODEX_PPTOKEN_API_KEY=<set>"));
assert(!healthReply.replyText.includes("secret-value"));
assert(!healthReply.replyText.includes("hidden"));

const errorReply = buildBridgeErrorReply({
  operation: "turn/start",
  error: new Error("provider failed api_key=secret-value Authorization Bearer abc.def"),
  threadId: "chuang-thread-9",
});
assert(errorReply.replyText.includes("本轮没有完成"));
assert(errorReply.replyText.includes("turn/start"));
assert(!errorReply.replyText.includes("secret-value"));
assert(!errorReply.replyText.includes("abc.def"));
assert.strictEqual(sanitizeErrorMessage("token=abc"), "token=<redacted>");

console.log("chuang_feishu_command_smoke_ok");
