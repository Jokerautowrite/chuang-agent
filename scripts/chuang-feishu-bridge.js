#!/usr/bin/env node

const fs = require("fs");
const os = require("os");
const path = require("path");
const readline = require("readline");
const { spawn } = require("child_process");
const { spawnSync } = require("child_process");

const dotenv = require("dotenv");
const {
  AppType,
  Client,
  Domain,
  EventDispatcher,
  LoggerLevel,
  WSClient,
} = require("@larksuiteoapi/node-sdk");
const { FeishuSessionStore } = require("./chuang-feishu-session-store");
const {
  ChuangFeishuClientAdapter,
  buildChuangReplyPayload,
  buildChuangTextPayload,
  patchWsClientForCardCallbacks,
} = require("./chuang-feishu-client-adapter");
const {
  buildBridgeErrorReply,
  buildHealthCommandReply,
  buildNewSessionCommandReply,
  buildSessionCommandReply,
  parseBridgeCommand,
} = require("./chuang-feishu-bridge-commands");
const {
  buildImagePrompt,
  buildOcrLanguageCandidates,
  parseFeishuImageContent,
} = require("./chuang-feishu-image");
const {
  buildProcessSection,
  buildStatusFooter,
} = require("./chuang-feishu-turn-summary");

const ROOT = process.env.CHUANG_AGENT_ROOT || path.resolve(__dirname, "..");
const ENV_FILE =
  process.env.CHUANG_FEISHU_ENV_FILE || path.join(ROOT, "ops/systemd/chuang-feishu-bridge.env");
const WORKSPACE_ROOT =
  normalizeWorkspaceRoot(process.env.CHUANG_AGENT_WORKSPACE_ROOT || process.env.CHUANG_FEISHU_WORKSPACE_ROOT || ROOT);
const PROVIDER_ENV_FILE =
  process.env.CHUANG_PROVIDER_ENV_FILE || path.join(os.homedir(), ".config/chuang-agent/provider.env");
const SESSION_STATE_FILE =
  process.env.CHUANG_FEISHU_STATE_FILE || path.join(ROOT, "context", "feishu-session-state.json");
const FEISHU_SDK_MODULES =
  process.env.CHUANG_FEISHU_SDK_NODE_MODULES ||
  "/home/user/.codex/codex-feishu-bridge/node_modules";
const EVENT_LOG_FILE =
  process.env.CHUANG_FEISHU_EVENT_LOG_FILE || "/tmp/chuang-feishu-bridge-events.log";
let cachedTesseractLanguages = null;

loadEnv();

function normalizeWorkspaceRoot(raw) {
  const trimmed = String(raw || "").trim();
  return path.resolve(trimmed || ".");
}

function loadEnv() {
  const envPaths = [
    ENV_FILE,
    path.join(ROOT, ".env"),
  ];
  for (const envPath of envPaths) {
    if (fs.existsSync(envPath)) {
      dotenv.config({ path: envPath });
    }
  }
  process.env.NODE_PATH = `${FEISHU_SDK_MODULES}${process.env.NODE_PATH ? `:${process.env.NODE_PATH}` : ""}`;
  require("module").Module._initPaths();
}

class AppServerClient {
  constructor(rootDir) {
    this.rootDir = normalizeWorkspaceRoot(rootDir);
    this.nextId = 1;
    this.pending = new Map();
    this.buffer = "";
    this.startedAt = "";
    this.lastError = "";
    this.restart();
  }

  restart() {
    if (this.child) {
      this.child.kill();
    }
    const childEnv = {
      ...process.env,
      CHUANG_AGENT_WORKSPACE_ROOT: this.rootDir,
      CHUANG_FEISHU_WORKSPACE_ROOT: this.rootDir,
    };
    this.child = spawn("cargo", ["run", "--quiet", "--manifest-path", path.join(ROOT, "Cargo.toml"), "--", "app-server"], {
      cwd: this.rootDir,
      env: childEnv,
      stdio: ["pipe", "pipe", "pipe"],
    });
    this.startedAt = new Date().toISOString();
    this.lastError = "";
    this.child.stdout.on("data", (chunk) => this.handleStdout(chunk));
    this.child.stderr.on("data", (chunk) => {
      const text = chunk.toString().trimEnd();
      if (text) {
        console.error(`[chuang-feishu] app-server: ${text}`);
        this.lastError = truncateText(text, 240);
      }
    });
    this.child.on("exit", (code, signal) => {
      const error = new Error(`app-server exited: code=${code} signal=${signal || ""}`.trim());
      this.lastError = error.message;
      this.child = null;
      for (const [, pending] of this.pending.entries()) {
        pending.reject(error);
      }
      this.pending.clear();
    });
  }

  handleStdout(chunk) {
    this.buffer += chunk.toString();
    let newlineIndex = this.buffer.indexOf("\n");
    while (newlineIndex >= 0) {
      const rawLine = this.buffer.slice(0, newlineIndex).trim();
      this.buffer = this.buffer.slice(newlineIndex + 1);
      newlineIndex = this.buffer.indexOf("\n");
      if (!rawLine) {
        continue;
      }
      let payload;
      try {
        payload = JSON.parse(rawLine);
      } catch {
        continue;
      }
      if (payload && Object.prototype.hasOwnProperty.call(payload, "id")) {
        const pending = this.pending.get(String(payload.id));
        if (!pending) {
          continue;
        }
        this.pending.delete(String(payload.id));
        if (payload.error) {
          pending.reject(new Error(payload.error.message || "app-server request failed"));
          continue;
        }
        pending.resolve(payload.result || {});
      }
    }
  }

  request(method, params) {
    const id = String(this.nextId++);
    const payload = JSON.stringify({ id, method, params });
    return new Promise((resolve, reject) => {
      if (!this.child || this.child.exitCode !== null || this.child.killed) {
        try {
          this.restart();
        } catch (error) {
          reject(error);
          return;
        }
      }
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(`${payload}\n`);
    });
  }

  status() {
    return {
      running: Boolean(this.child && this.child.exitCode === null && !this.child.killed),
      startedAt: this.startedAt,
      workspaceRoot: this.rootDir,
      childWorkspaceRoot: this.rootDir,
      configuredWorkspaceRoot: WORKSPACE_ROOT,
      workspaceRootMatchesConfig: this.rootDir === WORKSPACE_ROOT,
      pendingCount: this.pending.size,
      lastError: this.lastError,
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
    const fullText = parts.join("\n\n");
    return {
      threadId: thread.id || inbound.threadId || inbound.messageId,
      replyText: fullText,
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

class ChuangFeishuBridge {
  constructor() {
    this.lark = null;
    this.client = null;
    this.wsClient = null;
    this.adapter = null;
    this.queue = Promise.resolve();
    this.appServer = new AppServerClient(WORKSPACE_ROOT);
    this.sessionStore = new FeishuSessionStore(SESSION_STATE_FILE);
  }

  async start() {
    this.validateConfig();
    this.initializeSdk();
    this.startLongConnection();
    console.log(`[chuang-feishu] bridge ready for app ${maskSecret(process.env.CHUANG_FEISHU_APP_ID)}`);
  }

  validateConfig() {
    const required = [
      "CHUANG_FEISHU_APP_ID",
      "CHUANG_FEISHU_APP_SECRET",
      "CHUANG_AGENT_WORKSPACE_ROOT",
    ];
    const missing = required.filter((name) => !String(process.env[name] || "").trim());
    if (missing.length) {
      throw new Error(`Missing required env: ${missing.join(",")}`);
    }
  }

  initializeSdk() {
    this.lark = { Client, WSClient, AppType, Domain, LoggerLevel, EventDispatcher };
    this.client = new Client({
      appId: process.env.CHUANG_FEISHU_APP_ID,
      appSecret: process.env.CHUANG_FEISHU_APP_SECRET,
      appType: AppType.SelfBuild,
      domain: Domain.Feishu,
      loggerLevel: LoggerLevel.info,
    });
    this.wsClient = new WSClient({
      appId: process.env.CHUANG_FEISHU_APP_ID,
      appSecret: process.env.CHUANG_FEISHU_APP_SECRET,
      appType: AppType.SelfBuild,
      domain: Domain.Feishu,
      loggerLevel: LoggerLevel.info,
      wsConfig: {
        PingInterval: 30,
        PingTimeout: 5,
      },
    });
    this.adapter = new ChuangFeishuClientAdapter(this.client);
    patchWsClientForCardCallbacks(this.wsClient);
  }

  startLongConnection() {
    const dispatcher = new EventDispatcher({}).register({
      "im.message.receive_v1": (data) => {
        this.enqueue(() => this.handleInboundEvent(data)).catch((error) => {
          console.error(`[chuang-feishu] failed to handle message: ${error.message}`);
        });
      },
    });
    this.wsClient.start({ eventDispatcher: dispatcher });
    console.log("[chuang-feishu] Feishu long connection started");
  }

  enqueue(task) {
    this.queue = this.queue.then(task, task);
    return this.queue;
  }

  async handleInboundEvent(data) {
    const inbound = normalizeFeishuInboundEvent(data);
    if (!inbound) {
      return;
    }
    const effectiveThreadId = this.sessionStore.getThreadId(inbound.chatId) || inbound.threadId;
    appendEventLog("inbound", {
      chatId: inbound.chatId,
      messageId: inbound.messageId,
      threadId: effectiveThreadId,
      senderId: inbound.senderId,
      messageType: inbound.messageType,
      text: truncateText(inbound.text, 240),
      imageKey: inbound.imageKey ? truncateText(inbound.imageKey, 80) : "",
    });

    if (inbound.messageType === "text") {
      const command = parseBridgeCommand(inbound.text);
      if (command) {
        if (command.commandName === "new") {
          try {
            const thread = await this.appServer.startThread(
              inbound.workspaceRoot,
              buildNewThreadDisplayName(inbound)
            );
            if (thread && thread.id) {
              this.sessionStore.bind(inbound.chatId, thread.id, inbound.workspaceRoot);
            }
            await this.sendReply(inbound, {
              ...buildNewSessionCommandReply(thread.id || ""),
              threadId: thread.id || "",
            });
          } catch (error) {
            await this.sendReply(inbound, buildBridgeErrorReply({
              operation: "/new thread/start",
              error,
              threadId: effectiveThreadId,
            }));
          }
        } else if (command.commandName === "session") {
          await this.sendReply(
            inbound,
            buildSessionCommandReply(this.sessionStore.getBinding(inbound.chatId))
          );
        } else if (command.commandName === "health") {
          await this.sendReply(
            inbound,
            buildHealthCommandReply(this.buildHealthDiagnostics(inbound, effectiveThreadId))
          );
        } else {
          await this.sendReply(inbound, command);
        }
        appendEventLog("command", {
          chatId: inbound.chatId,
          messageId: inbound.messageId,
          command: command.commandName,
          threadId: this.sessionStore.getThreadId(inbound.chatId) || effectiveThreadId,
        });
        return;
      }
    }

    let promptText = inbound.text;
    if (inbound.messageType === "image") {
      try {
        const imageContext = await this.prepareImageContext(inbound);
        promptText = buildImagePrompt({
          imageKey: imageContext.imageKey,
          imagePath: imageContext.imagePath,
          imageBytes: imageContext.imageBytes,
          ocrText: imageContext.ocrText,
          ocrLanguage: imageContext.ocrLanguage,
          ocrStatus: imageContext.ocrStatus,
          messageId: inbound.messageId,
          threadId: effectiveThreadId,
        });
        appendEventLog("inbound_image", {
          chatId: inbound.chatId,
          messageId: inbound.messageId,
          threadId: effectiveThreadId,
          imageKey: truncateText(imageContext.imageKey, 80),
          imagePath: imageContext.imagePath,
          imageBytes: imageContext.imageBytes,
          ocrChars: imageContext.ocrText.length,
          ocrLanguage: imageContext.ocrLanguage,
          ocrStatus: imageContext.ocrStatus,
        });
      } catch (error) {
        const errorReply = buildBridgeErrorReply({
          operation: "image/download",
          error,
          threadId: effectiveThreadId,
        });
        await this.sendReply(inbound, errorReply);
        appendEventLog("image_error", {
          chatId: inbound.chatId,
          messageId: inbound.messageId,
          threadId: effectiveThreadId,
          reason: truncateText(error.message, 240),
        });
        return;
      }
    }

    try {
      const turn = await this.appServer.turnStart({
        ...inbound,
        threadId: effectiveThreadId,
        text: promptText,
      });
      if (turn.threadId) {
        this.sessionStore.bind(inbound.chatId, turn.threadId, inbound.workspaceRoot);
      }
      await this.sendReply(inbound, turn);
      appendEventLog("outbound", {
        chatId: inbound.chatId,
        messageId: inbound.messageId,
        threadId: turn.threadId,
        modelName: turn.modelName,
        runtimeReportId: turn.runtimeReportId,
        reply: truncateText(turn.replyText, 360),
      });
    } catch (error) {
      const errorReply = buildBridgeErrorReply({
        operation: "turn/start",
        error,
        threadId: effectiveThreadId,
      });
      await this.sendReply(inbound, errorReply);
      appendEventLog("turn_error", {
        chatId: inbound.chatId,
        messageId: inbound.messageId,
        threadId: effectiveThreadId,
        reason: truncateText(error.message, 240),
      });
    }
  }

  buildHealthDiagnostics(inbound, effectiveThreadId = "") {
    const binding = inbound?.chatId ? this.sessionStore.getBinding(inbound.chatId) : null;
    const appServerStatus = this.appServer.status();
    return {
      bridgeReady: true,
      workspaceRoot: WORKSPACE_ROOT,
      bridgeWorkspaceRoot: WORKSPACE_ROOT,
      appServer: appServerStatus,
      workspace: {
        bridgeRoot: WORKSPACE_ROOT,
        appServerRoot: appServerStatus.workspaceRoot,
        appServerChildRoot: appServerStatus.childWorkspaceRoot,
        configuredRoot: WORKSPACE_ROOT,
        inboundRoot: inbound?.workspaceRoot || WORKSPACE_ROOT,
        rootsMatch: appServerStatus.workspaceRootMatchesConfig,
        appServerMatchesConfig: appServerStatus.workspaceRootMatchesConfig,
      },
      session: binding
        ? { ...binding, bound: true }
        : {
            bound: false,
            chatId: inbound?.chatId || "",
            threadId: normalizeText(effectiveThreadId),
            workspaceRoot: inbound?.workspaceRoot || WORKSPACE_ROOT,
          },
      env: {
        appIdState: envState("CHUANG_FEISHU_APP_ID"),
        appSecretState: envState("CHUANG_FEISHU_APP_SECRET"),
        providerEnvState: providerEnvState(),
      },
    };
  }

  async sendReply(inbound, turn) {
    const richPayload = buildChuangReplyPayload({
      replyText: turn.replyText,
      modelName: turn.modelName,
      threadId: turn.threadId,
      runtimeReportId: turn.runtimeReportId,
      channelMessageId: inbound.messageId,
    });
    try {
      await this.adapter.sendResourceMessage({
        chatId: inbound.chatId,
        msgType: richPayload.msgType,
        content: richPayload.content,
      });
      appendEventLog("outbound_format", {
        chatId: inbound.chatId,
        messageId: inbound.messageId,
        msgType: richPayload.msgType,
      });
      return;
    } catch (error) {
      console.error(`[chuang-feishu] rich reply failed, falling back to text: ${error.message}`);
      appendEventLog("outbound_fallback", {
        chatId: inbound.chatId,
        messageId: inbound.messageId,
        reason: truncateText(error.message, 180),
      });
    }

    const textPayload = buildChuangTextPayload(turn.replyText);
    await this.adapter.sendResourceMessage({
      chatId: inbound.chatId,
      msgType: textPayload.msgType,
      content: textPayload.content,
    });
  }

  async prepareImageContext(inbound) {
    if (!inbound.imageKey) {
      throw new Error("image message missing image_key");
    }
    const imageBuffer = await this.downloadImageBuffer(inbound.imageKey);
    const imagePath = await this.writeImageTempFile(inbound, imageBuffer);
    const ocrResult = this.runImageOcr(imagePath);
    return {
      imageKey: inbound.imageKey,
      imagePath,
      imageBytes: imageBuffer.length,
      ocrText: ocrResult.text,
      ocrLanguage: ocrResult.language,
      ocrStatus: ocrResult.status,
    };
  }

  async downloadImageBuffer(imageKey) {
    const response = await this.client.im.v1.image.get({
      path: {
        image_key: imageKey,
      },
    });
    return responseToBuffer(response);
  }

  async writeImageTempFile(inbound, imageBuffer) {
    const imageDir = path.join(os.tmpdir(), "chuang-feishu-images");
    await fs.promises.mkdir(imageDir, { recursive: true });
    const safeMessageId = sanitizePathSegment(inbound.messageId);
    const safeImageKey = sanitizePathSegment(inbound.imageKey);
    const rawPath = path.join(imageDir, `${Date.now()}-${safeMessageId}-${safeImageKey}.bin`);
    await fs.promises.writeFile(rawPath, imageBuffer);
    return rawPath;
  }

  runImageOcr(imagePath) {
    const preprocessedPath = `${imagePath}.ocr.png`;
    const convertResult = spawnSync("convert", [imagePath, "-auto-orient", "-colorspace", "Gray", "-resize", "200%", preprocessedPath], {
      encoding: "utf8",
      maxBuffer: 4 * 1024 * 1024,
    });
    const ocrInputPath = convertResult.status === 0 ? preprocessedPath : imagePath;
    const languageCandidates = buildOcrLanguageCandidates({
      availableLanguages: listAvailableTesseractLanguages(),
      override: process.env.CHUANG_FEISHU_OCR_LANGS || "",
    });
    for (const language of languageCandidates) {
      const result = spawnSync("tesseract", [ocrInputPath, "stdout", "-l", language], {
        encoding: "utf8",
        maxBuffer: 8 * 1024 * 1024,
      });
      if (result.status === 0) {
        return {
          text: normalizeText(result.stdout),
          language,
          status: normalizeText(result.stdout) ? "ok" : "empty",
        };
      }
    }
    return {
      text: "",
      language: "eng",
      status: "failed",
    };
  }
}

function listAvailableTesseractLanguages() {
  if (Array.isArray(cachedTesseractLanguages)) {
    return cachedTesseractLanguages;
  }
  const result = spawnSync("tesseract", ["--list-langs"], {
    encoding: "utf8",
    maxBuffer: 2 * 1024 * 1024,
  });
  if (result.status !== 0) {
    cachedTesseractLanguages = [];
    return cachedTesseractLanguages;
  }
  cachedTesseractLanguages = result.stdout
    .split(/\r?\n/)
    .map((line) => normalizeText(line))
    .filter((line) => line && !line.startsWith("List of available languages"));
  return cachedTesseractLanguages;
}

function normalizeFeishuInboundEvent(data) {
  const event = data?.event || data || {};
  const message = event?.message || {};
  const sender = event?.sender || {};
  const messageType = normalizeText(message.message_type);
  if (messageType !== "text" && messageType !== "image") {
    return null;
  }
  const text = messageType === "text" ? parseFeishuTextContent(message.content) : "";
  const { imageKey } = messageType === "image" ? parseFeishuImageContent(message.content) : { imageKey: "" };
  if (messageType === "text" && !text) {
    return null;
  }
  if (messageType === "image" && !imageKey) {
    return null;
  }
  const chatId = normalizeText(message.chat_id);
  const messageId = normalizeText(message.message_id);
  const senderId = normalizeText(sender?.sender_id?.open_id || sender?.sender_id?.user_id);
  const threadId = normalizeText(message.root_id || message.thread_id || message.message_id);
  if (!chatId || !messageId || !senderId) {
    return null;
  }
  return {
    channel: "feishu-dedicated-chuang",
    chatId,
    messageId,
    senderId,
    threadId,
    workspaceRoot: WORKSPACE_ROOT,
    messageType,
    text,
    imageKey,
  };
}

function parseFeishuTextContent(rawContent) {
  try {
    const parsed = JSON.parse(rawContent || "{}");
    return normalizeText(parsed.text);
  } catch {
    return "";
  }
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function buildNewThreadDisplayName(inbound) {
  const chat = normalizeText(inbound.chatId) || "feishu-chat";
  return `feishu:${chat}`;
}

function formatDuration(ms) {
  const totalMs = Number.isFinite(Number(ms)) ? Math.max(0, Number(ms)) : 0;
  if (totalMs < 1000) {
    return `${totalMs}ms`;
  }
  const seconds = (totalMs / 1000).toFixed(totalMs >= 10_000 ? 0 : 1);
  return `${seconds}s`;
}

function formatThousands(value) {
  const num = Number.isFinite(Number(value)) ? Number(value) : 0;
  return num.toLocaleString("en-US");
}

function pickNumber(value) {
  const num = Number(value);
  return Number.isFinite(num) && num >= 0 ? num : 0;
}

function truncateText(value, maxLen) {
  const text = normalizeText(value);
  if (text.length <= maxLen) {
    return text;
  }
  return `${text.slice(0, Math.max(0, maxLen - 1))}…`;
}

function appendEventLog(kind, payload) {
  try {
    const line = JSON.stringify({
      at: new Date().toISOString(),
      kind,
      ...payload,
    });
    fs.appendFileSync(EVENT_LOG_FILE, `${line}\n`, "utf8");
  } catch (error) {
    console.error(`[chuang-feishu] failed to write event log: ${error.message}`);
  }
}

function envState(name) {
  return normalizeText(process.env[name]) ? "<set>" : "<missing>";
}

function providerEnvState() {
  const providerFileState = fs.existsSync(PROVIDER_ENV_FILE) ? "<set>" : "<missing>";
  return `CHUANG_PROVIDER_ENV_FILE=${providerFileState} CODEX_PPTOKEN_API_KEY=${envState("CODEX_PPTOKEN_API_KEY")}`;
}

function responseToBuffer(response) {
  if (Buffer.isBuffer(response)) {
    return response;
  }
  if (response instanceof Uint8Array) {
    return Buffer.from(response);
  }
  if (response && typeof response.getReadableStream === "function") {
    return new Promise((resolve, reject) => {
      const stream = response.getReadableStream();
      const chunks = [];
      stream.on("data", (chunk) => {
        chunks.push(Buffer.isBuffer(chunk) ? chunk : Buffer.from(chunk));
      });
      stream.on("end", () => resolve(Buffer.concat(chunks)));
      stream.on("error", reject);
    });
  }
  if (response && response.data) {
    if (Buffer.isBuffer(response.data)) {
      return response.data;
    }
    if (response.data instanceof Uint8Array) {
      return Buffer.from(response.data);
    }
  }
  throw new Error("unexpected image download response type");
}

function sanitizePathSegment(value) {
  const text = normalizeText(value);
  if (!text) {
    return "unknown";
  }
  return text.replace(/[^a-zA-Z0-9._-]+/g, "_").slice(0, 64);
}

function maskSecret(value) {
  if (!value) {
    return "";
  }
  if (value.length <= 6) {
    return "***";
  }
  return `${value.slice(0, 3)}***${value.slice(-3)}`;
}

async function main() {
  const bridge = new ChuangFeishuBridge();
  await bridge.start();
}

if (require.main === module) {
  main().catch((error) => {
    console.error(`[chuang-feishu] ${error.message}`);
    process.exit(1);
  });
}

module.exports = {
  buildProcessSection,
  buildStatusFooter,
  parseBridgeCommand,
};
