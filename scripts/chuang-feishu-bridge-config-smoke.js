#!/usr/bin/env node

const assert = require("assert");
const { listForbiddenCredentialEnvNames } = require("./chuang-feishu-bridge-config");

const forbidden = listForbiddenCredentialEnvNames({
  FEISHU_APP_ID: "legacy-feishu-app",
  HERMES_FEISHU_APP_SECRET: "legacy-hermes-secret",
  CODEX_FEISHU_BOT_ID: "",
  CHUANG_FEISHU_APP_ID: "cli_a_chuang",
  CHUANG_FEISHU_APP_SECRET: "chuang-secret",
});

assert.deepStrictEqual(forbidden, [
  "FEISHU_APP_ID",
  "HERMES_FEISHU_APP_SECRET",
  "CODEX_FEISHU_BOT_ID",
]);

const forbiddenError = new Error(
  `Forbidden credential env names detected for Chuang Feishu bridge: ${forbidden.join(",")}`
);
assert(!forbiddenError.message.includes("legacy-feishu-app"));
assert(!forbiddenError.message.includes("legacy-hermes-secret"));
assert(!forbiddenError.message.includes("chuang-secret"));

console.log("chuang_feishu_bridge_config_smoke_ok");
