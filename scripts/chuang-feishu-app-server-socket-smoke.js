#!/usr/bin/env node

const assert = require("assert");
const fs = require("fs");
const net = require("net");
const os = require("os");
const path = require("path");
const {
  AppServerClient,
  resolveAppServerSocket,
} = require("./chuang-app-server-client");

const socketPath = path.join(
  os.tmpdir(),
  `chuang-feishu-app-server-socket-${process.pid}-${Date.now()}.sock`
);
const missingSocketPath = `${socketPath}.missing`;
const originalSocket = process.env.CHUANG_APP_SERVER_SOCKET;

async function main() {
  let finalResponseSent = false;
  const server = net.createServer((socket) => {
    let buffer = "";
    socket.on("data", (chunk) => {
      buffer += chunk.toString();
      const newlineIndex = buffer.indexOf("\n");
      if (newlineIndex < 0) {
        return;
      }
      const request = JSON.parse(buffer.slice(0, newlineIndex));
      if (request.method === "turn/start") {
        socket.write(`${JSON.stringify({
          method: "turn/started",
          params: { threadId: "chuang-thread-1", turn: { id: "chuang-turn-1" } },
        })}\n`);
        socket.write(`${JSON.stringify({
          method: "turn/progress",
          params: { threadId: "chuang-thread-1", turnId: "chuang-turn-1", event: { event: "working" } },
        })}\n`);
        setTimeout(() => {
          finalResponseSent = true;
          socket.write(`${JSON.stringify({
            id: request.id,
            result: {
              thread: {
                id: "chuang-thread-1",
                turns: [{
                  items: [{ type: "agentMessage", text: "socket answer", model: "socket-model" }],
                }],
              },
              turn: { id: "chuang-turn-1", status: "completed" },
            },
          })}\n`);
        }, 20);
        return;
      }
      assert.strictEqual(request.method, "thread/start");
      socket.write(`${JSON.stringify({
        id: request.id,
        result: { thread: { id: "chuang-thread-2" } },
      })}\n`);
    });
  });

  try {
    await listen(server, socketPath);
    process.env.CHUANG_APP_SERVER_SOCKET = socketPath;
    assert.strictEqual(resolveAppServerSocket(), socketPath);

    const client = new AppServerClient(process.cwd());
    const result = await client.turnStart({
      threadId: "",
      workspaceRoot: process.cwd(),
      text: "socket smoke",
      channel: "smoke",
      messageId: "message-1",
      senderId: "sender-1",
    });
    assert(finalResponseSent, "request resolved before the final JSON-RPC response");
    assert.strictEqual(result.threadId, "chuang-thread-1");
    assert(result.replyText.startsWith("socket answer\n\n"));
    assert(result.replyText.includes("已完成"));
    assert.strictEqual(result.modelName, "socket-model");
    assert.strictEqual(
      (await client.startThread(process.cwd(), "socket smoke thread")).id,
      "chuang-thread-2"
    );
    assert.strictEqual(client.status().transport, "unix_socket");
    assert.strictEqual(client.status().socketPath, socketPath);

    const unavailable = new AppServerClient(process.cwd(), missingSocketPath);
    await assert.rejects(
      unavailable.request("thread/start", {}),
      new RegExp(`app-server unavailable: socket=${escapeRegex(missingSocketPath)}`)
    );

    console.log("chuang_feishu_app_server_socket_smoke_ok");
  } finally {
    if (originalSocket === undefined) {
      delete process.env.CHUANG_APP_SERVER_SOCKET;
    } else {
      process.env.CHUANG_APP_SERVER_SOCKET = originalSocket;
    }
    await close(server);
    try {
      fs.unlinkSync(socketPath);
    } catch {
      // Socket cleanup is best effort after server close.
    }
  }
}

function listen(server, socket) {
  return new Promise((resolve, reject) => {
    server.once("error", reject);
    server.listen(socket, () => {
      server.removeListener("error", reject);
      resolve();
    });
  });
}

function close(server) {
  return new Promise((resolve) => server.close(resolve));
}

function escapeRegex(value) {
  return value.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

main().catch((error) => {
  console.error(error.stack || error.message);
  process.exit(1);
});
