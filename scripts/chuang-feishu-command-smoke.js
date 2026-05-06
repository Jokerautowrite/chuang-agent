#!/usr/bin/env node

const assert = require("assert");
const {
  buildHelpCommandReply,
  buildNewSessionCommandReply,
  parseBridgeCommand,
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

assert.strictEqual(parseBridgeCommand("继续推进"), null);
assert.strictEqual(buildNewSessionCommandReply().commandName, "new");

const help = parseBridgeCommand("/help");
assert(help, "/help should be handled as a bridge command");
assert.strictEqual(help.commandName, "help");
assert(help.replyText.includes("/new"));
assert(help.replyText.includes("开新窗口/新上下文入口"));
assert(help.replyText.includes("普通文本会转发到 Chuang app-server"));
assert.strictEqual(buildHelpCommandReply().commandName, "help");

console.log("chuang_feishu_command_smoke_ok");
