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
  const sessionMemoryWriteStatus = normalizeText(
    turn.sessionMemoryWriteStatus ||
      turn.runtimeObservability?.session_memory_write_status ||
      turn.providerMeta?.session_memory_write_status
  );
  const sessionMemoryWriteError = normalizeText(
    turn.sessionMemoryWriteError ||
      turn.runtimeObservability?.session_memory_write_error ||
      turn.providerMeta?.session_memory_write_error
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
    sessionMemoryWriteStatus && sessionMemoryWriteStatus !== "written"
      ? `会话记忆 ${formatSessionMemoryStatus(sessionMemoryWriteStatus)}`
      : "",
    sessionMemoryWriteError && sessionMemoryWriteStatus !== "written"
      ? `会话记忆错误 ${truncateText(sessionMemoryWriteError, 120)}`
      : "",
  ]
    .filter(Boolean)
    .join(" · ");
}

function buildProcessSection(turn) {
  if (!turn || typeof turn !== "object") {
    return "";
  }
  const providerMeta = turn.providerMeta && typeof turn.providerMeta === "object" ? turn.providerMeta : {};
  const observability = turn.runtimeObservability && typeof turn.runtimeObservability === "object" ? turn.runtimeObservability : {};
  const responseKind = normalizeText(providerMeta.response_kind);
  const finishReason = normalizeText(providerMeta.response_finish_reason);
  const toolCallCount = pickNumber(turn.toolCallCount || providerMeta.tool_call_count);
  const unifiedStatus = normalizeText(
    observability.tool_unified_execution_status || providerMeta.tool_unified_execution_status
  );
  const unifiedFailureCount = pickNumber(
    observability.tool_unified_execution_failure_count || providerMeta.tool_unified_execution_failure_count
  );
  const protocolErrorCount = pickNumber(
    turn.toolProtocolErrorCount || observability.tool_protocol_error_count || providerMeta.tool_protocol_error_count
  );
  const liveReadiness = turn.liveReadiness && typeof turn.liveReadiness === "object" ? turn.liveReadiness : null;
  const liveReadinessState = normalizeText(liveReadiness?.overall_state);
  const realExternalPending = liveReadiness?.real_external_acceptance_pending === true;
  const readyDoesNotMeanLive = liveReadiness?.ready_does_not_mean_live === true;
  const toolState = toolCallCount > 0 ? "当前轮已执行本地工具" : "当前轮未触发工具调用";
  const lines = ["过程摘要", `- ${toolState}`];
  if (toolCallCount > 0) {
    lines.push(`- 工具调用 ${toolCallCount} 次`);
  }
  if (unifiedStatus || unifiedFailureCount > 0) {
    lines.push(`- 工具执行 ${unifiedStatus || "unknown"} / 失败 ${unifiedFailureCount}`);
  }
  if (protocolErrorCount > 0) {
    lines.push(`- 工具协议错误 ${protocolErrorCount} 次`);
  }
  if (liveReadinessState) {
    const liveParts = [liveReadinessState];
    if (realExternalPending) {
      liveParts.push("真实验收待完成");
    }
    if (readyDoesNotMeanLive) {
      liveParts.push("ready不等于live");
    }
    lines.push(`- live readiness ${liveParts.join(" / ")}`);
  }
  if (responseKind || finishReason) {
    lines.push(`- provider ${responseKind || "unknown"} / finish ${finishReason || "unknown"}`);
  }
  return lines.join("\n");
}

function normalizeText(value) {
  return typeof value === "string" ? value.trim() : "";
}

function formatSessionMemoryStatus(status) {
  if (status === "compacted") {
    return "已压缩写入";
  }
  if (status === "hard_limit_exceeded") {
    return "超限未写入";
  }
  return status;
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
