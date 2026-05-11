const FORBIDDEN_CREDENTIAL_ENV_NAMES = [
  "FEISHU_APP_ID",
  "FEISHU_APP_SECRET",
  "FEISHU_BOT_ID",
  "FEISHU_VERIFICATION_TOKEN",
  "FEISHU_ENCRYPT_KEY",
  "HERMES_FEISHU_APP_ID",
  "HERMES_FEISHU_APP_SECRET",
  "HERMES_FEISHU_BOT_ID",
  "HERMES_FEISHU_VERIFICATION_TOKEN",
  "HERMES_FEISHU_ENCRYPT_KEY",
  "CODEX_FEISHU_APP_ID",
  "CODEX_FEISHU_APP_SECRET",
  "CODEX_FEISHU_BOT_ID",
  "CODEX_FEISHU_VERIFICATION_TOKEN",
  "CODEX_FEISHU_ENCRYPT_KEY",
];

function listForbiddenCredentialEnvNames(env = process.env) {
  return FORBIDDEN_CREDENTIAL_ENV_NAMES.filter((name) =>
    Object.prototype.hasOwnProperty.call(env, name)
  );
}

function listDisallowedProviderEnvNames(values = {}) {
  const names = Object.keys(values || {});
  return names.filter(
    (name) =>
      name.startsWith("CHUANG_FEISHU_") || FORBIDDEN_CREDENTIAL_ENV_NAMES.includes(name)
  );
}

module.exports = {
  FORBIDDEN_CREDENTIAL_ENV_NAMES,
  listDisallowedProviderEnvNames,
  listForbiddenCredentialEnvNames,
};
