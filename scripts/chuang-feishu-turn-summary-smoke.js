#!/usr/bin/env node

const assert = require("assert");
const {
  buildProcessSection,
  buildStatusFooter,
} = require("./chuang-feishu-turn-summary");

const footer = buildStatusFooter({
  status: "completed",
  elapsedMs: 2200,
  modelName: "gpt-5.5",
  prompt_tokens: 462,
  completion_tokens: 44,
  packedTokenCount: 335,
  contextMaxTokens: 512,
  recallHitCount: 0,
  apiCallCount: 1,
  runtimeReportId: "report-turn-1",
});
assert(footer.includes("已完成"));
assert(footer.includes("耗时 2.2s"));
assert(footer.includes("gpt-5.5"));
assert(footer.includes("上下文 335/512"));
assert(footer.includes("API 1 次"));
assert(footer.includes("报告 report-turn-1"));

const compactedFooter = buildStatusFooter({
  status: "completed",
  elapsedMs: 1200,
  modelName: "gpt-5.5",
  packedTokenCount: 335,
  contextMaxTokens: 512,
  runtimeObservability: {
    session_memory_write_status: "compacted",
  },
});
assert(compactedFooter.includes("会话记忆 已压缩写入"));
assert(!compactedFooter.includes("会话记忆错误"));

const process = buildProcessSection({
  status: "completed",
  providerMeta: {
    response_kind: "response",
    response_finish_reason: "stop",
    tool_call_count: 0,
    tool_trace: "trace transport=openai-compatible provider=local-openai-compatible model=gpt-5.5 base_url=https://api.pptoken.org/v1 api_key=len:67",
  },
  runtimeObservability: {
    tool_unified_execution_status: "ok",
    tool_unified_execution_failure_count: "0",
    tool_protocol_error_count: "0",
  },
  liveReadiness: {
    overall_state: "local_ready_live_pending",
    real_external_acceptance_pending: true,
    ready_does_not_mean_live: true,
    current: "raw current text should stay out of Feishu summary",
    next_action: "raw next action should stay out of Feishu summary",
  },
  toolCallCount: 0,
});
assert(process.startsWith("过程摘要"));
assert(process.includes("当前轮未触发工具调用"));
assert(process.includes("工具执行 ok / 失败 0"));
assert(process.includes("live readiness local_ready_live_pending / 真实验收待完成 / ready不等于live"));
assert(process.includes("provider response / finish stop"));
assert(!process.includes("工具协议错误"));
assert(!process.includes("raw current text"));
assert(!process.includes("raw next action"));
assert(!process.includes("trace transport="));
assert(!process.includes("api_key=len:67"));

const noLiveReadinessProcess = buildProcessSection({
  status: "completed",
  providerMeta: {
    response_kind: "response",
    response_finish_reason: "stop",
    tool_call_count: "0",
  },
});
assert(noLiveReadinessProcess.includes("当前轮未触发工具调用"));
assert(!noLiveReadinessProcess.includes("live readiness"));

const toolTraceProcess = buildProcessSection({
  status: "completed",
  providerMeta: {
    response_kind: "response",
    response_finish_reason: "stop",
    tool_call_count: "1",
    tool_trace: "trace transport=openai-compatible base_url=https://api.pptoken.org/v1 api_key=len:67",
    tool_unified_execution_status: "ok",
    tool_unified_execution_failure_count: "0",
    tool_protocol_error_count: "0",
  },
});
assert(toolTraceProcess.includes("工具调用 1 次"));
assert(toolTraceProcess.includes("工具执行 ok / 失败 0"));
assert(!toolTraceProcess.includes("工具轨迹"));
assert(!toolTraceProcess.includes("trace transport="));
assert(!toolTraceProcess.includes("api_key=len:67"));

const toolProblemProcess = buildProcessSection({
  status: "completed",
  providerMeta: {
    response_kind: "response",
    response_finish_reason: "stop",
    tool_trace: "ACTION: {bad json}",
  },
  runtimeObservability: {
    tool_unified_execution_status: "failed",
    tool_unified_execution_failure_count: "2",
    tool_protocol_error_count: "1",
  },
  toolCallCount: 1,
});
assert(toolProblemProcess.includes("工具调用 1 次"));
assert(toolProblemProcess.includes("工具执行 failed / 失败 2"));
assert(toolProblemProcess.includes("工具协议错误 1 次"));
assert(!toolProblemProcess.includes("ACTION: {bad json}"));

const providerFallbackProcess = buildProcessSection({
  status: "completed",
  providerMeta: {
    response_kind: "response",
    response_finish_reason: "stop",
    tool_call_count: "1",
    tool_trace: "ACTION: {provider meta raw payload}",
    tool_unified_execution_status: "ok",
    tool_unified_execution_failure_count: "0",
    tool_protocol_error_count: "2",
  },
});
assert(providerFallbackProcess.includes("工具调用 1 次"));
assert(providerFallbackProcess.includes("工具执行 ok / 失败 0"));
assert(providerFallbackProcess.includes("工具协议错误 2 次"));
assert(providerFallbackProcess.includes("provider response / finish stop"));
assert(!providerFallbackProcess.includes("ACTION: {provider meta raw payload}"));

console.log("chuang_feishu_turn_summary_smoke_ok");
