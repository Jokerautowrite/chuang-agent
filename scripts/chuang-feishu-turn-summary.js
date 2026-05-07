function buildStatusFooter(turn) {
  if (!turn || typeof turn !== "object") {
    return "";
  }
  const status = turn.status === "completed" ? "已完成" : normalizeText(turn.status) || "处理中";
  const elapsed = formatDuration(turn.elapsedMs);
  const model = normalizeText(turn.modelName) || "unknown";
  const runtimeReportId = normalizeText(
    turn.runtimeReportId ||
      turn.runtime_report_id ||
      turn.runtimeObservability?.runtime_report_id ||
      turn.providerMeta?.runtime_report_id
  );
  const recallHits = Number.isFinite(Number(turn.recallHitCount)) ? Number(turn.recallHitCount) : 0;
  const packedTokens = Number.isFinite(Number(turn.packedTokenCount)) ? Number(turn.packedTokenCount) : 0;
  const contextMaxTokens = Number.isFinite(Number(turn.contextMaxTokens)) ? Number(turn.contextMaxTokens) : 0;
  const providerMeta = turn.providerMeta && typeof turn.providerMeta === "object" ? turn.providerMeta : {};
  const promptTokens = pickNumber(providerMeta.prompt_tokens);
  const completionTokens = pickNumber(providerMeta.completion_tokens);
  const apiCallCount = pickNumber(turn.apiCallCount) || 1;
  const contextText =
    contextMaxTokens > 0
      ? `上下文 ${formatThousands(packedTokens)}/${formatThousands(contextMaxTokens)}`
      : `上下文 ${formatThousands(packedTokens)}`;
  const tokenText = promptTokens || completionTokens ? `↑ ${formatThousands(promptTokens)} · ↓ ${formatThousands(completionTokens)}` : "";
  return [
    status,
    `耗时 ${elapsed}`,
    model,
    tokenText,
    contextText,
    `回忆 ${recallHits}`,
    `API ${apiCallCount} 次`,
    runtimeReportId ? `报告 ${runtimeReportId}` : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

function buildProcessSection(turn) {
  if (!turn || typeof turn !== "object") {
    return "";
  }
  const providerMeta = turn.providerMeta && typeof turn.providerMeta === "object" ? turn.providerMeta : {};
  const responseKind = normalizeText(providerMeta.response_kind);
  const finishReason = normalizeText(providerMeta.response_finish_reason);
  const toolCallCount = pickNumber(turn.toolCallCount || providerMeta.tool_call_count);
  const toolTrace = truncateText(normalizeText(turn.toolTrace || providerMeta.tool_trace), 240);
  const toolState = toolCallCount > 0 ? "当前轮已执行本地工具" : "当前轮未触发工具调用";
  const lines = ["过程摘要", `- ${toolState}`];
  if (toolCallCount > 0) {
    lines.push(`- 工具调用 ${toolCallCount} 次`);
    if (toolTrace) {
      lines.push(`- 工具轨迹：${toolTrace}`);
    }
  }
  if (responseKind || finishReason) {
    lines.push(`- provider ${responseKind || "unknown"} / finish ${finishReason || "unknown"}`);
  }
  return lines.join("\n");
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
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

module.exports = {
  buildProcessSection,
  buildStatusFooter,
};
