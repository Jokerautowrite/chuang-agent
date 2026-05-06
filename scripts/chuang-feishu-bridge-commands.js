function parseBridgeCommand(text) {
  const normalized = normalizeText(text).toLowerCase();
  if (normalized === "/new") {
    return buildNewSessionCommandReply();
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

function buildHelpCommandReply() {
  return {
    commandName: "help",
    threadId: "",
    modelName: "chuang-feishu-bridge",
    replyText: [
      "Chuang Feishu bridge 本地命令：",
      "",
      "- `/new`：开新窗口/新上下文入口；不会进入 Agent 主链。",
      "- `/help`：显示这条帮助；不会进入 Agent 主链。",
      "",
      "普通文本会转发到 Chuang app-server，由 Agent runtime 处理。",
    ].join("\n"),
  };
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
}

module.exports = {
  buildHelpCommandReply,
  buildNewSessionCommandReply,
  parseBridgeCommand,
};
