// Minimal Feishu SDK adapter for the dedicated Chuang bridge.
// This stays repo-local and only wraps the SDK surface needed by the bridge.

class ChuangFeishuClientAdapter {
  constructor(client) {
    this.client = client;
  }

  async sendResourceMessage({ chatId, replyToMessageId = "", replyInThread = false, msgType, content }) {
    if (replyToMessageId) {
      const replyMessage = resolveReplyMessageMethod(this.client);
      return replyMessage.call(this.client.im?.v1?.message || this.client.im?.message || this.client, {
        path: {
          message_id: normalizeMessageId(replyToMessageId),
        },
        data: {
          msg_type: msgType,
          content,
          reply_in_thread: replyInThread,
        },
      });
    }

    const createMessage = resolveCreateMessageMethod(this.client);
    return createMessage.call(this.client.im?.v1?.message || this.client.im?.message || this.client, {
      params: {
        receive_id_type: "chat_id",
      },
      data: {
        receive_id: chatId,
        msg_type: msgType,
        content,
      },
    });
  }
}

function buildChuangReplyPayload({
  replyText,
  modelName = "unknown",
  threadId = "",
  runtimeReportId = "",
  channelMessageId = "",
}) {
  const text = normalizeCardText(replyText) || "已收到。";
  const model = normalizeCardText(modelName) || "unknown";
  const thread = normalizeCardText(threadId);
  const report = normalizeCardText(runtimeReportId);
  const message = normalizeCardText(channelMessageId);
  const fields = [
    { label: "模型", value: model },
    thread ? { label: "线程", value: thread } : null,
    report ? { label: "报告", value: report } : null,
    message ? { label: "消息", value: message } : null,
  ].filter(Boolean);

  return {
    msgType: "interactive",
    content: JSON.stringify({
      config: {
        wide_screen_mode: true,
      },
      header: {
        template: "blue",
        title: {
          tag: "plain_text",
          content: "Chuang",
        },
      },
      elements: [
        {
          tag: "markdown",
          content: escapeFeishuMarkdown(text),
        },
        {
          tag: "hr",
        },
        {
          tag: "div",
          fields: fields.map((field) => ({
            is_short: true,
            text: {
              tag: "lark_md",
              content: `**${field.label}**\n${escapeFeishuMarkdown(field.value)}`,
            },
          })),
        },
      ],
    }),
  };
}

function buildChuangTextPayload(replyText) {
  return {
    msgType: "text",
    content: JSON.stringify({ text: normalizeCardText(replyText) || "已收到。" }),
  };
}

function resolveCreateMessageMethod(client) {
  const fn = client?.im?.v1?.message?.create || client?.im?.message?.create;
  if (typeof fn !== "function") {
    throw new Error("Unsupported Feishu SDK shape: missing message.create");
  }
  return fn;
}

function resolveReplyMessageMethod(client) {
  const fn = client?.im?.v1?.message?.reply || client?.im?.message?.reply;
  if (typeof fn !== "function") {
    throw new Error("Unsupported Feishu SDK shape: missing message.reply");
  }
  return fn;
}

function normalizeMessageId(messageId) {
  const normalized = typeof messageId === "string" ? messageId.trim() : "";
  if (!normalized) {
    return "";
  }
  return normalized.split(":")[0];
}

function normalizeCardText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function escapeFeishuMarkdown(value) {
  return normalizeCardText(value).replace(/</g, "&lt;").replace(/>/g, "&gt;");
}

function patchWsClientForCardCallbacks(wsClient) {
  if (!wsClient || typeof wsClient.handleEventData !== "function") {
    return;
  }

  const originalHandleEventData = wsClient.handleEventData.bind(wsClient);
  wsClient.handleEventData = (data) => {
    const headers = Array.isArray(data?.headers) ? data.headers : [];
    const messageType = headers.find((header) => header?.key === "type")?.value;
    if (messageType === "card") {
      const patchedData = {
        ...data,
        headers: headers.map((header) => (
          header?.key === "type" ? { ...header, value: "event" } : header
        )),
      };
      return originalHandleEventData(patchedData);
    }
    return originalHandleEventData(data);
  };
}

module.exports = {
  ChuangFeishuClientAdapter,
  buildChuangReplyPayload,
  buildChuangTextPayload,
  patchWsClientForCardCallbacks,
};
