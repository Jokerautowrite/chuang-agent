const fs = require("fs");
const net = require("net");
const path = require("path");
const {
  buildProcessSection,
  buildStatusFooter,
} = require("./chuang-feishu-turn-summary");

function normalizeWorkspaceRoot(raw) {
  const trimmed = String(raw || "").trim();
  return path.resolve(trimmed || ".");
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function resolveAppServerSocket() {
  const configured = normalizeText(process.env.CHUANG_APP_SERVER_SOCKET);
  if (configured) {
    return configured;
  }
  const runtimeDir = normalizeText(process.env.XDG_RUNTIME_DIR) || `/run/user/${process.getuid()}`;
  return path.join(runtimeDir, "chuang-agent", "app-server.sock");
}

class AppServerClient {
  constructor(rootDir, socketPath = resolveAppServerSocket()) {
    this.rootDir = normalizeWorkspaceRoot(rootDir);
    this.socketPath = socketPath;
    this.nextId = 1;
    this.startedAt = "";
    this.lastError = "";
  }

  request(method, params) {
    const id = String(this.nextId++);
    const payload = JSON.stringify({ id, method, params });
    return new Promise((resolve, reject) => {
      let buffer = "";
      let settled = false;
      const socket = net.createConnection({ path: this.socketPath });
      const settle = (callback, value) => {
        if (settled) {
          return;
        }
        settled = true;
        socket.destroy();
        callback(value);
      };
      const unavailable = (error) => {
        if (settled) {
          return;
        }
        const message = `app-server unavailable: socket=${this.socketPath} error=${error.message}`;
        this.lastError = message;
        settle(reject, new Error(message));
      };

      socket.on("connect", () => {
        this.startedAt = new Date().toISOString();
        this.lastError = "";
        socket.write(`${payload}\n`);
      });
      socket.on("data", (chunk) => {
        buffer += chunk.toString();
        let newlineIndex = buffer.indexOf("\n");
        while (newlineIndex >= 0) {
          const rawLine = buffer.slice(0, newlineIndex).trim();
          buffer = buffer.slice(newlineIndex + 1);
          newlineIndex = buffer.indexOf("\n");
          if (!rawLine) {
            continue;
          }

          let frame;
          try {
            frame = JSON.parse(rawLine);
          } catch (error) {
            const message = `app-server invalid JSONL frame: socket=${this.socketPath} error=${error.message}`;
            this.lastError = message;
            settle(reject, new Error(message));
            return;
          }
          if (String(frame?.id) !== id) {
            continue;
          }
          if (frame.error) {
            const message = frame.error.message || "app-server request failed";
            this.lastError = message;
            settle(reject, new Error(message));
            return;
          }
          if (Object.prototype.hasOwnProperty.call(frame, "result")) {
            settle(resolve, frame.result || {});
            return;
          }
        }
      });
      socket.on("error", unavailable);
      socket.on("end", () => {
        if (!settled) {
          unavailable(new Error("connection_closed_before_response"));
        }
      });
    });
  }

  status() {
    return {
      running: fs.existsSync(this.socketPath),
      startedAt: this.startedAt,
      workspaceRoot: this.rootDir,
      childWorkspaceRoot: this.rootDir,
      configuredWorkspaceRoot: this.rootDir,
      workspaceRootMatchesConfig: true,
      pendingCount: 0,
      lastError: this.lastError,
      transport: "unix_socket",
      socketPath: this.socketPath,
    };
  }

  async turnStart(inbound) {
    const result = await this.request("turn/start", {
      threadId: inbound.threadId || "",
      workspaceRoot: inbound.workspaceRoot,
      text: inbound.text,
      channel: inbound.channel,
      channelMessageId: inbound.messageId,
      senderId: inbound.senderId,
    });
    const thread = result.thread || {};
    const turn = result.turn || {};
    const turns = Array.isArray(thread.turns) ? thread.turns : [];
    const lastTurn = turns[turns.length - 1] || {};
    const items = Array.isArray(lastTurn.items) ? lastTurn.items : [];
    const assistant = items.find((item) => item && item.type === "agentMessage") || {};
    const replyText = normalizeText(assistant.text || assistant.content?.[0]?.text || lastTurn.preview || "");
    const footer = buildStatusFooter(turn);
    const process = buildProcessSection(turn);
    const parts = [replyText || "已收到。"];
    if (footer) {
      parts.push(footer);
    }
    if (process) {
      parts.push(process);
    }
    return {
      threadId: thread.id || inbound.threadId || inbound.messageId,
      replyText: parts.join("\n\n"),
      modelName: assistant.model || result.model || "unknown",
      runtimeReportId: normalizeText(turn.runtimeReportId || turn.runtime_report_id || turn.runtimeObservability?.runtime_report_id || turn.providerMeta?.runtime_report_id),
      sessionMemoryWriteStatus: normalizeText(
        turn.sessionMemoryWriteStatus ||
          turn.runtimeObservability?.session_memory_write_status ||
          turn.providerMeta?.session_memory_write_status
      ),
      sessionMemoryWriteError: normalizeText(
        turn.sessionMemoryWriteError ||
          turn.runtimeObservability?.session_memory_write_error ||
          turn.providerMeta?.session_memory_write_error
      ),
    };
  }

  async startThread(workspaceRoot, displayName) {
    const result = await this.request("thread/start", {
      cwd: workspaceRoot,
      displayName,
    });
    return result.thread || {};
  }
}

module.exports = {
  AppServerClient,
  resolveAppServerSocket,
};
