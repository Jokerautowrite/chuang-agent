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
}) {
  const text = normalizeCardText(stripProcessFooter(replyText)) || "已收到。";

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
      ],
    }),
  };
}

function buildChuangTextPayload(replyText) {
  return {
    msgType: "text",
    content: JSON.stringify({
      text: normalizeCardText(stripProcessFooter(replyText)) || "已收到。",
    }),
  };
}

// 只保留回复正文：在"过程摘要 / 已完成 · 耗时"等过程尾注处截断，
// 避免把模型附带的执行元信息（耗时/摘要/模型/线程/报告/消息）发给主人。
function stripProcessFooter(text) {
  const raw = String(text || "");
  const lines = raw.split("\n");
  const kept = [];
  for (const line of lines) {
    const trimmed = line.trim();
    if (
      trimmed.startsWith("过程摘要") ||
      trimmed.startsWith("已完成 ·") ||
      trimmed === "已完成" ||
      trimmed === "模型" ||
      trimmed === "线程" ||
      trimmed === "报告" ||
      trimmed === "消息"
    ) {
      break;
    }
    kept.push(line);
  }
  return kept.join("\n").trimEnd();
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
