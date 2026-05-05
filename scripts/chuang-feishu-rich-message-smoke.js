#!/usr/bin/env node

const assert = require("assert");
const {
  ChuangFeishuClientAdapter,
  buildChuangReplyPayload,
  buildChuangTextPayload,
} = require("./chuang-feishu-client-adapter");

const rich = buildChuangReplyPayload({
  replyText: "富消息 <ok>",
  modelName: "gpt-test",
  threadId: "thread-1",
  runtimeReportId: "report-1",
});
assert.strictEqual(rich.msgType, "interactive");
const card = JSON.parse(rich.content);
assert.strictEqual(card.config.wide_screen_mode, true);
assert.strictEqual(card.header.title.content, "Chuang");
assert(card.elements.some((element) => element.tag === "markdown" && element.content.includes("&lt;ok&gt;")));
assert(card.elements.some((element) => element.tag === "div"));

const text = buildChuangTextPayload("纯文本回复");
assert.strictEqual(text.msgType, "text");
assert.deepStrictEqual(JSON.parse(text.content), { text: "纯文本回复" });

let captured = null;
const fakeClient = {
  im: {
    v1: {
      message: {
        create(request) {
          captured = request;
          return Promise.resolve({ ok: true });
        },
      },
    },
  },
};

const adapter = new ChuangFeishuClientAdapter(fakeClient);
adapter
  .sendResourceMessage({
    chatId: "chat-1",
    msgType: rich.msgType,
    content: rich.content,
  })
  .then(() => {
    assert.strictEqual(captured.params.receive_id_type, "chat_id");
    assert.strictEqual(captured.data.receive_id, "chat-1");
    assert.strictEqual(captured.data.msg_type, "interactive");
    JSON.parse(captured.data.content);
    console.log("feishu_rich_message_smoke_ok");
  })
  .catch((error) => {
    console.error(error);
    process.exit(1);
  });
