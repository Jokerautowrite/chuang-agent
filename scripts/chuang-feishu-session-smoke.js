#!/usr/bin/env node

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { buildNewSessionCommandReply } = require("./chuang-feishu-bridge-commands");
const { FeishuSessionStore } = require("./chuang-feishu-session-store");

const stateFile = path.join(
  os.tmpdir(),
  `chuang-feishu-session-smoke-${process.pid}-${Date.now()}.json`
);

try {
  const store = new FeishuSessionStore(stateFile);
  assert.strictEqual(store.getThreadId("chat-a"), "");
  assert.strictEqual(store.bind("chat-a", "chuang-thread-7", "/home/user/projects/chuang-agent").threadId, "chuang-thread-7");
  assert.strictEqual(store.getThreadId("chat-a"), "chuang-thread-7");

  const reloaded = new FeishuSessionStore(stateFile);
  assert.strictEqual(reloaded.getThreadId("chat-a"), "chuang-thread-7");
  assert(reloaded.getBinding("chat-a").workspaceRoot.includes("chuang-agent"));

  const reply = buildNewSessionCommandReply("chuang-thread-7");
  assert(reply.replyText.includes("当前 Feishu 聊天已切到新的 Chuang 会话"));
  assert(reply.replyText.includes("chuang-thread-7"));

  assert(store.clear("chat-a"));
  assert.strictEqual(store.getThreadId("chat-a"), "");

  console.log("chuang_feishu_session_smoke_ok");
} finally {
  try {
    fs.unlinkSync(stateFile);
  } catch {
    // ignore
  }
}
