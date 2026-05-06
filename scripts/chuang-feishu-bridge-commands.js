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
  buildNewSessionCommandReply,
  buildSessionCommandReply,
  parseBridgeCommand,
  sanitizeErrorMessage,
};
