#!/usr/bin/env node

const fs = require("fs");
const os = require("os");
const path = require("path");
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
const {
  listDisallowedProviderEnvNames,
  listForbiddenCredentialEnvNames,
} = require("./chuang-feishu-bridge-config");
const {
  AppServerClient,
} = require("./chuang-app-server-client");

const DEFAULT_ROOT = path.resolve(__dirname, "..");
let ROOT = process.env.CHUANG_AGENT_ROOT || DEFAULT_ROOT;
let ENV_FILE =
  process.env.CHUANG_FEISHU_ENV_FILE || path.join(ROOT, "ops/systemd/chuang-feishu-bridge.env");
let WORKSPACE_ROOT = "";
let PROVIDER_ENV_FILE = "";
let SESSION_STATE_FILE = "";
let FEISHU_SDK_MODULES = "";
let EVENT_LOG_FILE = "";
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
  ROOT = process.env.CHUANG_AGENT_ROOT || ROOT;
  ENV_FILE = process.env.CHUANG_FEISHU_ENV_FILE || ENV_FILE;
  WORKSPACE_ROOT = normalizeWorkspaceRoot(
    process.env.CHUANG_AGENT_WORKSPACE_ROOT || process.env.CHUANG_FEISHU_WORKSPACE_ROOT || ROOT
  );
  PROVIDER_ENV_FILE =
    process.env.CHUANG_PROVIDER_ENV_FILE || path.join(os.homedir(), ".config/chuang-agent/provider.env");
  loadProviderEnvReadonly(PROVIDER_ENV_FILE);
  SESSION_STATE_FILE =
    process.env.CHUANG_FEISHU_STATE_FILE || path.join(ROOT, "context", "feishu-session-state.json");
  FEISHU_SDK_MODULES =
    process.env.CHUANG_FEISHU_SDK_NODE_MODULES ||
    path.join(os.homedir(), "agent-hub", "plugins", "agent-bridge", "node_modules");
  EVENT_LOG_FILE =
    process.env.CHUANG_FEISHU_EVENT_LOG_FILE || "/tmp/chuang-feishu-bridge-events.log";
  process.env.NODE_PATH = `${FEISHU_SDK_MODULES}${process.env.NODE_PATH ? `:${process.env.NODE_PATH}` : ""}`;
  require("module").Module._initPaths();
}

function loadProviderEnvReadonly(providerEnvPath) {
  if (!normalizeText(providerEnvPath) || !fs.existsSync(providerEnvPath)) {
    return;
  }
  const parsed = dotenv.parse(fs.readFileSync(providerEnvPath, "utf8"));
  const disallowed = listDisallowedProviderEnvNames(parsed);
  if (disallowed.length) {
    throw new Error(
      `Provider env file contains forbidden Feishu config names: ${disallowed.join(",")}`
    );
  }
  for (const [name, value] of Object.entries(parsed)) {
    if (!Object.prototype.hasOwnProperty.call(process.env, name)) {
      process.env[name] = value;
    }
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
    this.startProactivePoller();
    console.log(`[chuang-feishu] bridge ready for app ${maskSecret(process.env.CHUANG_FEISHU_APP_ID)}`);
  }

  // ===== 情感主动联系（心跳）投递 =====
  // CLI `emotion heartbeat` 把主动联系提案写入发件箱（context/proactive-outbox/），
  // 桥轮询读取并发送到绑定会话；投递成功后归档，失败保留待下轮重试。
  startProactivePoller() {
    const rawSeconds = parseInt(process.env.CHUANG_PROACTIVE_POLL_SECONDS || "60", 10);
    const seconds = Number.isFinite(rawSeconds) && rawSeconds > 0 ? rawSeconds : 60;
    this.proactivePollSeconds = seconds;
    this.proactiveTimer = setInterval(() => {
      this.pollProactiveOutbox().catch((error) => {
        console.error(`[chuang-feishu] proactive poll failed: ${error.message}`);
      });
    }, seconds * 1000);
    // 启动后稍等立即查一次（不等一个周期）。
    setTimeout(() => {
      this.pollProactiveOutbox().catch((error) => {
        console.error(`[chuang-feishu] proactive initial poll failed: ${error.message}`);
      });
    }, 5 * 1000);
    console.log(`[chuang-feishu] proactive poller every ${seconds}s`);
  }

  proactiveOutboxDir() {
    return (
      process.env.CHUANG_PROACTIVE_OUTBOX_DIR ||
      path.join(ROOT, "context", "proactive-outbox")
    );
  }

  async pollProactiveOutbox() {
    const dir = this.proactiveOutboxDir();
    const dryRun = process.env.CHUANG_PROACTIVE_DRY_RUN === "1";
    let fileNames;
    try {
      fileNames = fs.readdirSync(dir).filter((name) => name.endsWith(".json"));
    } catch {
      return;
    }
    for (const fileName of fileNames) {
      const fullPath = path.join(dir, fileName);
      let entry;
      try {
        if (!fs.statSync(fullPath).isFile()) {
          continue;
        }
        entry = JSON.parse(fs.readFileSync(fullPath, "utf8"));
      } catch {
        continue;
      }
      if (!entry || typeof entry.text !== "string" || !entry.text.trim()) {
        continue;
      }
      const chatId = this.resolveProactiveChatId(entry);
      if (!chatId) {
        appendEventLog("proactive_skip", {
          id: entry.id || "unknown",
          reason: "no_bound_chat",
        });
        continue;
      }
      if (dryRun) {
        appendEventLog("proactive_dry_run", {
          id: entry.id || "unknown",
          chatId,
          text: truncateText(entry.text, 120),
        });
        this.archiveProactiveEntry(fullPath);
        continue;
      }
      try {
        await this.adapter.sendResourceMessage({
          chatId,
          replyToMessageId: "",
          replyInThread: false,
          msgType: "text",
          content: JSON.stringify({ text: entry.text }),
        });
        appendEventLog("proactive_sent", {
          id: entry.id || "unknown",
          chatId,
          text: truncateText(entry.text, 120),
        });
        this.archiveProactiveEntry(fullPath);
      } catch (error) {
        appendEventLog("proactive_failed", {
          id: entry.id || "unknown",
          reason: truncateText(error.message, 240),
        });
      }
    }
  }

  resolveProactiveChatId(entry) {
    const bindings = this.sessionStore.state.bindings || {};
    const keys = Object.keys(bindings);
    if (!keys.length) {
      return "";
    }
    const workspaceRoot = String(entry.workspaceRoot || "").trim();
    if (workspaceRoot) {
      const match = keys.find(
        (key) => String(bindings[key].workspaceRoot || "").trim() === workspaceRoot
      );
      if (match) {
        return match;
      }
    }
    // 单用户/单工作区场景：无精确匹配时投递到任一绑定会话。
    return keys[0];
  }

  archiveProactiveEntry(fullPath) {
    try {
      const archiveDir = path.join(path.dirname(fullPath), "archive");
      fs.mkdirSync(archiveDir, { recursive: true });
      fs.renameSync(fullPath, path.join(archiveDir, path.basename(fullPath)));
    } catch (error) {
      console.error(`[chuang-feishu] proactive archive failed: ${error.message}`);
    }
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
    const forbidden = listForbiddenCredentialEnvNames(process.env);
    if (forbidden.length) {
      throw new Error(
        `Forbidden credential env names detected for Chuang Feishu bridge: ${forbidden.join(",")}`
      );
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
        appendEventLog("ws_event", probeFeishuInboundEnvelope(data));
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
      appendEventLog("inbound_dropped", probeFeishuInboundEnvelope(data));
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
  if (messageType !== "text" && messageType !== "image" && messageType !== "post") {
    return null;
  }
  const text = messageType === "text"
    ? parseFeishuTextContent(message.content)
    : messageType === "post"
      ? parseFeishuPostContent(message.content)
      : "";
  const { imageKey } = messageType === "image" ? parseFeishuImageContent(message.content) : { imageKey: "" };
  if ((messageType === "text" || messageType === "post") && !text) {
    return null;
  }
  if (messageType === "image" && !imageKey) {
    return null;
  }
  const chatId = normalizeText(message.chat_id);
  const messageId = normalizeText(message.message_id);
  const senderId = normalizeText(
    sender?.sender_id?.open_id || sender?.sender_id?.user_id || sender?.sender_id?.union_id
  );
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

function parseFeishuPostContent(rawContent) {
  try {
    const parsed = JSON.parse(rawContent || "{}");
    return extractFeishuPostText(parsed).trim();
  } catch {
    return "";
  }
}

function extractFeishuPostText(value, fragments = []) {
  if (!value) {
    return fragments.join("").replace(/\n{3,}/g, "\n\n");
  }
  if (Array.isArray(value)) {
    for (const item of value) {
      extractFeishuPostText(item, fragments);
    }
    return fragments.join("").replace(/\n{3,}/g, "\n\n");
  }
  if (typeof value !== "object") {
    return fragments.join("").replace(/\n{3,}/g, "\n\n");
  }

  const tag = normalizeText(value.tag).toLowerCase();
  if ((tag === "text" || tag === "a" || tag === "at") && typeof value.text === "string") {
    fragments.push(value.text);
  } else if (tag === "br") {
    fragments.push("\n");
  }

  for (const child of Object.values(value)) {
    if (child && typeof child === "object") {
      extractFeishuPostText(child, fragments);
    }
  }
  return fragments.join("").replace(/\n{3,}/g, "\n\n");
}

function probeFeishuInboundEnvelope(data) {
  const event = data?.event || data || {};
  const message = event?.message || {};
  const sender = event?.sender || {};
  return {
    eventId: normalizeText(data?.header?.event_id || data?.event_id),
    eventType: normalizeText(data?.header?.event_type || data?.event_type || data?.type),
    messageType: normalizeText(message.message_type),
    chatId: normalizeText(message.chat_id),
    messageId: normalizeText(message.message_id),
    rootId: normalizeText(message.root_id),
    threadId: normalizeText(message.thread_id),
    senderOpenId: normalizeText(sender?.sender_id?.open_id),
    senderUserId: normalizeText(sender?.sender_id?.user_id),
    senderUnionId: normalizeText(sender?.sender_id?.union_id),
    hasContent: Boolean(normalizeText(message.content)),
  };
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
  ChuangFeishuBridge,
  buildProcessSection,
  buildStatusFooter,
  listForbiddenCredentialEnvNames,
  parseBridgeCommand,
};
