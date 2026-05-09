const fs = require("fs");
const path = require("path");

const CAPABILITY_PRIMER_PATH = path.join(__dirname, "..", "assets", "capability_primer.txt");

function parseBridgeCommand(text) {
  const normalized = normalizeText(text).toLowerCase();
  if (normalized === "/new") {
    return buildNewSessionCommandReply();
  }
  if (normalized === "/session") {
    return buildSessionCommandReply();
  }
  if (normalized === "/health" || normalized === "/status") {
    return buildHealthCommandReply();
  }
  if (normalized === "/receipt" || normalized === "/live-receipt") {
    return buildReceiptCommandReply();
  }
  if (normalized === "/live-check" || normalized === "/live") {
    return buildLiveCheckCommandReply();
  }
  if (normalized === "/tools" || normalized === "/capabilities") {
    return buildToolsCommandReply();
  }
  if (normalized === "/help" || normalized === "help") {
    return buildHelpCommandReply();
  }
  return null;
}

function buildNewSessionCommandReply(threadId = "") {
  const hasThreadId = typeof threadId === "string" && threadId.trim();
  return {
    commandName: "new",
    threadId: "",
    modelName: "chuang-feishu-bridge",
    replyText: [
      "已收到 /new：开新窗口/新上下文命令。",
      "",
      "这条命令不会进入 Agent 主链，也不会消耗一轮任务。",
      "",
      hasThreadId
        ? `当前 Feishu 聊天已切到新的 Chuang 会话：${threadId.trim()}`
        : "飞书机器人不能直接替你创建客户端窗口。",
      "",
      hasThreadId
        ? "后续同一聊天里的普通文本会路由到这个新会话。"
        : "请在飞书里新开一个聊天、话题或消息线程，然后直接发任务；如果新窗口提示未绑定，就发送：",
      "",
      hasThreadId ? "" : "`/codex bind /home/user/projects/chuang-agent`",
      "",
      hasThreadId
        ? "如果你还想再起一个全新的会话，继续发 /new 即可。"
        : "如果只是想在当前窗口按新任务开始，可以直接发：",
      "",
      hasThreadId
        ? ""
        : "从现在开始按新任务处理，忽略前面上下文，工作目录仍是 /home/user/projects/chuang-agent",
    ].join("\n"),
  };
}

function buildSessionCommandReply(binding = null) {
  const threadId = normalizeText(binding?.threadId);
  const workspaceRoot = normalizeText(binding?.workspaceRoot);
  const updatedAt = normalizeText(binding?.updatedAt);
  const bound = Boolean(binding?.chatId && threadId);
  const lines = [
    "Chuang 当前会话：",
    "",
    `- 绑定：${bound ? "已绑定当前飞书聊天" : "未绑定"}`,
    threadId ? `- 线程：${threadId}` : "- 线程：未绑定，下一条普通消息会使用当前飞书消息线程。",
  ];
  if (workspaceRoot) {
    lines.push(`- 工作区：${workspaceRoot}`);
  }
  if (updatedAt) {
    lines.push(`- 更新时间：${updatedAt}`);
  }
  lines.push("", "需要新上下文时发送 `/new`。");
  return {
    commandName: "session",
    threadId,
    modelName: "chuang-feishu-bridge",
    replyText: lines.join("\n"),
  };
}

function buildHealthCommandReply(diagnostics = {}) {
  const appServer = diagnostics.appServer || {};
  const env = diagnostics.env || {};
  const session = diagnostics.session || {};
  const sessionBound = Boolean(session.bound || (session.chatId && session.threadId));
  const lines = [
    "Chuang Feishu 通道健康诊断：",
    "",
    `- bridge：${diagnostics.bridgeReady === false ? "not_ready" : "ready"}`,
    `- app-server：${appServer.running === false ? "not_running" : "running"}`,
    `- workspace：${normalizeText(diagnostics.workspaceRoot) || "unknown"}`,
    `- session：${session.threadId ? `${sessionBound ? "bound" : "default"} ${session.threadId}` : "unbound"}`,
    `- Feishu env：app_id=${env.appIdState || "unknown"} app_secret=${env.appSecretState || "unknown"}`,
    `- provider env：${env.providerEnvState || "unknown"}`,
  ];
  const lastError = sanitizeErrorMessage(appServer.lastError || diagnostics.lastError || "");
  if (lastError) {
    lines.push(`- 最近错误：${lastError}`);
  }
  lines.push("", "这条诊断只读本地状态，不连接真实飞书、不打印密钥。");
  return {
    commandName: "health",
    threadId: session.threadId || "",
    modelName: "chuang-feishu-bridge",
    replyText: lines.join("\n"),
  };
}

function buildReceiptCommandReply() {
  return {
    commandName: "receipt",
    threadId: "",
    modelName: "chuang-feishu-bridge",
    replyText: [
      "Chuang 人工 live 回执模板入口：",
      "",
      "这条命令只显示静态模板，不执行脚本、不读取 secret、不进入 Agent 主链。",
      "",
      "实际回执脚本：",
      "",
      "`scripts/chuang-live-operator-receipt.sh --json`",
      "",
      "需要填写的字段：",
      "",
      "- `tested_at`：现场回执时间。",
      "- `operator`：人工操作人。",
      "- `env_file`：本次检查使用的 Chuang Feishu env 文件。",
      "- `workspace_root`：Chuang 仓库根目录。",
      "- `preflight_status`：`ready` / `blocked` / `warning` / 现场结论。",
      "- `health_status`：`ready` / `blocked` / `warning` / 现场结论。",
      "- `new_thread_status`：`ready` / `blocked` / `warning` / 现场结论。",
      "- `session_status`：`ready` / `blocked` / `warning` / 现场结论。",
      "- `runtime_report_id`：对应本次 live 回复的 runtime report id。",
      "- `provider_status`：provider 侧现场结论。",
      "- `codex_hermes_isolation`：保持 Codex/Hermes 隔离的简短说明。",
      "- `notes`：人工备注。",
      "- `blockers`：阻塞项列表。",
      "- `boundaries`：保密和只读边界。",
      "",
      "环境变量只用于本地写模板时补默认值：`CHUANG_LIVE_OPERATOR`、`CHUANG_AGENT_ROOT`、`CHUANG_LIVE_ENV_FILE`。",
      "",
      "保密边界：不要把 secret、token、app_secret、api_key、完整 env 内容或 Hermes/Codex 凭据写进回执；只记录 `<set>/<missing>`、现场状态和必要的脱敏说明。",
    ].join("\n"),
  };
}

function buildBridgeErrorReply({ operation = "turn", error = null, threadId = "", runtimeReportId = "" } = {}) {
  const safeMessage = sanitizeErrorMessage(error?.message || String(error || ""));
  const lines = [
    "Chuang 本轮没有完成。",
    "",
    `- 阶段：${normalizeText(operation) || "turn"}`,
    threadId ? `- 线程：${threadId}` : "",
    runtimeReportId ? `- 报告：${runtimeReportId}` : "",
    safeMessage ? `- 错误：${safeMessage}` : "- 错误：unknown",
    "",
    "可以先发送 `/health` 查看通道状态，或发送 `/new` 开新会话后重试。",
  ].filter(Boolean);
  return {
    commandName: "error",
    threadId: normalizeText(threadId),
    modelName: "chuang-feishu-bridge",
    runtimeReportId: normalizeText(runtimeReportId),
    replyText: lines.join("\n"),
  };
}

function buildLiveCheckCommandReply() {
  return {
    commandName: "live-check",
    threadId: "",
    modelName: "chuang-feishu-bridge",
    replyText: [
      "Chuang 人工 live 检查入口：",
      "",
      "先在本机终端跑：",
      "",
      "`scripts/chuang-live-operator-checklist.sh --json`",
      "`node scripts/chuang-feishu-live-preflight.js --env-file /home/user/.codex-im/chuang-feishu-bridge.env --workspace-root /home/user/projects/chuang-agent --json`",
      "`sh scripts/chuang-live-readonly-preflight.sh`",
      "`scripts/chuang-goal-run-status.sh --json`",
      "",
      "然后在当前 Chuang bot 里依次发：",
      "",
      "`/health`",
      "`/new`",
      "`晚上人工 live check：请回复当前 thread、runtime report id 和 provider 状态`",
      "`/session`",
      "",
      "边界：这条命令只显示静态步骤，不执行任何本地命令或 checklist，不连接外部服务、不读取密钥、不启动服务、不修改仓库。",
      "",
      "结果判断：",
      "",
      "- ready：所有本地检查通过，Feishu/provider env 只显示 `<set>`，没有 blocker。",
      "- blocked：必需 env、workspace/config、app-server、session state 或只读检查失败；先修复再 live。",
      "- warning：可继续人工判断的非阻断项，例如可选 env 缺失、已有 session state 只读元数据异常或需要复核的隔离提示。",
      "",
      "不要把 secret、token、app_secret、api_key 或完整 env 内容发回聊天；只回传 `<set>/<missing>` 状态和必要的错误摘要。",
    ].join("\n"),
  };
}

function buildToolsCommandReply() {
  const runtimeCapabilityPrimer = loadCapabilityPrimerText();
  return {
    commandName: "tools",
    threadId: "",
    modelName: "chuang-feishu-bridge",
    replyText: [
      "Chuang 当前可见能力与边界：",
      "",
      "Feishu 本地命令：",
      "",
      "- `/new`：创建新的 Chuang 会话上下文。",
      "- `/session`：查看当前飞书聊天绑定的会话与工作区。",
      "- `/health`：查看本地 bridge、app-server 与 provider env 状态。",
      "- `/receipt` / `/live-receipt`：输出人工 live 回执模板，不执行脚本。",
      "- `/live-check`：输出人工 live 检查步骤，不执行本地检查。",
      "- `/tools` / `/capabilities`：查看当前本地能力与边界。",
      "- 普通文本：转发到 Chuang app-server 处理。",
      "- 图片消息：先下载并 OCR，再进入 Chuang 主链。",
      "",
      "主链工具能力：",
      "",
      runtimeCapabilityPrimer,
      "",
      "- governed file tools：`file_read` / `file_write`。",
      "- governed code tool：`code_execute`。",
      "- auxiliary listing：`list_dir`。",
      "- memory/session：会话记忆召回与显式写回诊断。",
      "- provider/runtime：OpenAI-compatible provider、治理回执、runtime report。",
      "",
      "goal/subagent 派活入口：",
      "",
      "- goal：`goal plan/show/dispatch/step/collect/checkpoint`。",
      "- subagent：`subagent dispatch/list/run-once/run-loop/report/collect`。",
      "- Feishu 普通文本可以发起主链任务；真正派活仍由 Chuang runtime/CLI 在治理和队列边界内执行。",
      "",
      "live runner 边界：",
      "",
      "- live runner 当前仍是 preflight-only / rehearsal-only。",
      "- 不启用真实 runner 池，不做桌面 mutation，不做服务控制 apply。",
      "- 子代理 live rehearsal 只能单 worker、allowlist、bounded、带 receipt。",
      "",
      "边界：",
      "",
      "- 不复用 Hermes bridge。",
      "- 不复用 Codex bridge。",
      "- 不打印 secret、token、app_secret、api_key 或完整 env 内容。",
      "- `/tools` 只做静态展示，不读取 secret、不执行本地检查、不启动服务、不修改仓库。",
      "- App-server 或 bridge 的失败会以脱敏运维消息返回。",
    ].join("\n"),
  };
}

function loadCapabilityPrimerText() {
  try {
    return fs.readFileSync(CAPABILITY_PRIMER_PATH, "utf8").trim();
  } catch (_error) {
    return "默认能力：file_read/file_write/code_execute/list_dir；memory/session；goal/subagent 派活。live runner 仅 preflight/rehearsal；桌面/浏览器真实动作仍需治理与 live gate。";
  }
}

function buildHelpCommandReply() {
  return {
    commandName: "help",
    threadId: "",
    modelName: "chuang-feishu-bridge",
    replyText: [
      "Chuang Feishu bridge 本地命令：",
      "",
      "- `/new`：开新窗口/新上下文入口；不会进入 Agent 主链。",
      "- `/session`：查看当前飞书聊天绑定的 Chuang 会话。",
      "- `/health`：查看本地 bridge/app-server/provider env 诊断。",
      "- 图片消息：会先下载、OCR，再进入 Chuang 主链。",
      "- `/receipt` / `/live-receipt`：显示人工 live 回执模板入口；不会进入 Agent 主链。",
      "- `/live-check`：显示人工 live 检查步骤；不会进入 Agent 主链。",
      "- `/tools` / `/capabilities`：显示当前本地能力与边界。",
      "- `/help`：显示这条帮助；不会进入 Agent 主链。",
      "",
      "普通文本会转发到 Chuang app-server，由 Agent runtime 处理。",
    ].join("\n"),
  };
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function sanitizeErrorMessage(value) {
  const text = normalizeText(value)
    .replace(/(app[_-]?secret|token|api[_-]?key|authorization|password)=\S+/gi, "$1=<redacted>")
    .replace(/Bearer\s+[A-Za-z0-9._~+/=-]+/gi, "Bearer <redacted>");
  if (!text) {
    return "";
  }
  return text.length > 240 ? `${text.slice(0, 239)}…` : text;
}

module.exports = {
  buildBridgeErrorReply,
  buildHelpCommandReply,
  buildHealthCommandReply,
  buildLiveCheckCommandReply,
  buildToolsCommandReply,
  buildReceiptCommandReply,
  buildNewSessionCommandReply,
  buildSessionCommandReply,
  loadCapabilityPrimerText,
  parseBridgeCommand,
  sanitizeErrorMessage,
};
