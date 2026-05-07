function parseFeishuImageContent(rawContent) {
  const parsed = parseJsonLike(rawContent);
  if (!parsed) {
    return { imageKey: "", imageKeys: [] };
  }
  const imageKey = normalizeText(
    parsed.image_key || parsed.imageKey || parsed.file_key || parsed.fileKey || parsed.key || ""
  );
  const imageKeys = Array.isArray(parsed.image_key_list)
    ? parsed.image_key_list.map(normalizeText).filter(Boolean)
    : [];
  return {
    imageKey: imageKey || imageKeys[0] || "",
    imageKeys,
  };
}

function buildImagePrompt({
  imageKey = "",
  imagePath = "",
  imageBytes = 0,
  ocrText = "",
  ocrLanguage = "eng",
  ocrStatus = "ok",
  messageId = "",
  threadId = "",
}) {
  const lines = [
    "[图片消息]",
    imageKey ? `- image_key: ${normalizeText(imageKey)}` : "- image_key: <missing>",
    imagePath ? `- local_path: ${normalizeText(imagePath)}` : "- local_path: <missing>",
    `- image_bytes: ${Number.isFinite(Number(imageBytes)) ? Number(imageBytes) : 0}`,
    `- ocr_status: ${normalizeText(ocrStatus) || "unknown"}`,
    `- ocr_language: ${normalizeText(ocrLanguage) || "unknown"}`,
  ];
  if (threadId) {
    lines.push(`- thread_id: ${normalizeText(threadId)}`);
  }
  if (messageId) {
    lines.push(`- message_id: ${normalizeText(messageId)}`);
  }
  const text = normalizeText(ocrText);
  if (text) {
    lines.push("", "[OCR 文本]", text);
  } else {
    lines.push("", "[OCR 文本]", "未识别到可读文本。请根据图片可见内容、布局和上下文尽量判断。");
  }
  return lines.join("\n");
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function parseJsonLike(rawContent) {
  if (!rawContent) {
    return null;
  }
  if (typeof rawContent === "object") {
    return rawContent;
  }
  try {
    return JSON.parse(rawContent);
  } catch {
    return null;
  }
}

module.exports = {
  buildImagePrompt,
  parseFeishuImageContent,
};
