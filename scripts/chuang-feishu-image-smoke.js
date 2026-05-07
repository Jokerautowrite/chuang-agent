#!/usr/bin/env node

const assert = require("assert");
const {
  buildImagePrompt,
  buildOcrLanguageCandidates,
  parseFeishuImageContent,
} = require("./chuang-feishu-image");

assert.deepStrictEqual(parseFeishuImageContent('{"image_key":"img_123"}'), {
  imageKey: "img_123",
  imageKeys: [],
});

assert.deepStrictEqual(parseFeishuImageContent('{"image_key_list":["img_a","img_b"]}'), {
  imageKey: "img_a",
  imageKeys: ["img_a", "img_b"],
});

const prompt = buildImagePrompt({
  imageKey: "img_123",
  imagePath: "/tmp/chuang-feishu-images/abc.bin",
  imageBytes: 1024,
  ocrText: "Hello world",
  ocrLanguage: "eng",
  ocrStatus: "ok",
  messageId: "msg-1",
  threadId: "thread-1",
});
assert(prompt.includes("[图片消息]"));
assert(prompt.includes("image_key: img_123"));
assert(prompt.includes("local_path: /tmp/chuang-feishu-images/abc.bin"));
assert(prompt.includes("ocr_status: ok"));
assert(prompt.includes("[OCR 文本]"));
assert(prompt.includes("Hello world"));

assert.deepStrictEqual(
  buildOcrLanguageCandidates({ availableLanguages: ["eng", "osd"] }),
  ["eng"]
);
assert.deepStrictEqual(
  buildOcrLanguageCandidates({ availableLanguages: ["chi_sim", "eng"] }),
  ["chi_sim+eng", "chi_sim", "eng"]
);
assert.deepStrictEqual(
  buildOcrLanguageCandidates({ override: "chi_sim+eng,eng;chi_tra" }),
  ["chi_sim+eng", "eng", "chi_tra"]
);

const emptyPrompt = buildImagePrompt({
  imageKey: "",
  imagePath: "",
  imageBytes: 0,
  ocrText: "",
  ocrLanguage: "eng",
  ocrStatus: "empty",
});
assert(emptyPrompt.includes("<missing>"));
assert(emptyPrompt.includes("未识别到可读文本"));

console.log("chuang_feishu_image_smoke_ok");
