#!/usr/bin/env node

const assert = require("assert");
const {
  listDisallowedProviderEnvNames,
  listForbiddenCredentialEnvNames,
} = require("./chuang-feishu-bridge-config");

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

const disallowedInProvider = listDisallowedProviderEnvNames({
  OPENAI_API_KEY: "provider-key",
  CHUANG_FEISHU_APP_ID: "bad-feishu-app",
  FEISHU_ENCRYPT_KEY: "legacy-encrypt",
});
assert.deepStrictEqual(disallowedInProvider, [
  "CHUANG_FEISHU_APP_ID",
  "FEISHU_ENCRYPT_KEY",
]);
const providerError = new Error(
  `Provider env file contains forbidden Feishu config names: ${disallowedInProvider.join(",")}`
);
assert(!providerError.message.includes("provider-key"));
assert(!providerError.message.includes("bad-feishu-app"));
assert(!providerError.message.includes("legacy-encrypt"));

console.log("chuang_feishu_bridge_config_smoke_ok");
