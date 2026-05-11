#!/usr/bin/env node

const assert = require("assert");
const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");

const root = path.resolve(__dirname, "..");
const workDir = fs.mkdtempSync(path.join(os.tmpdir(), "chuang-feishu-live-preflight-"));
const workspace = path.join(workDir, "workspace");
fs.mkdirSync(path.join(workspace, "identity"), { recursive: true });
fs.mkdirSync(path.join(workspace, "rules"), { recursive: true });
fs.writeFileSync(path.join(workspace, "identity", "SOUL.md"), "Feishu preflight soul\n", "utf8");
fs.writeFileSync(path.join(workspace, "identity", "STORY.md"), "Feishu preflight story\n", "utf8");
fs.writeFileSync(path.join(workspace, "identity", "FIRST_WAKE.md"), "Feishu preflight first wake\n", "utf8");
fs.writeFileSync(path.join(workspace, "identity", "agents.toml"), "[agents]\n", "utf8");
fs.writeFileSync(path.join(workspace, "rules", "core.md"), "- Keep Feishu preflight local.\n", "utf8");
fs.writeFileSync(
  path.join(workspace, "config.toml"),
  `
db_path = "${path.join(workDir, "chuang-agent.db")}"
identity_memory_root = "${path.join(workDir, "hermes-memory")}"
identity_root = "${path.join(workspace, "identity")}"
soul_path = "${path.join(workspace, "identity", "SOUL.md")}"
story_path = "${path.join(workspace, "identity", "STORY.md")}"
first_wake_path = "${path.join(workspace, "identity", "FIRST_WAKE.md")}"
agents_registry_path = "${path.join(workspace, "identity", "agents.toml")}"
rules_root = "${path.join(workspace, "rules")}"
rules_core_path = "${path.join(workspace, "rules", "core.md")}"

provider = "openai_compatible"
provider_id = "feishu-preflight-smoke"
base_url = "https://api.example.com/v1"
model = "gpt-feishu-preflight-smoke"
api_key_env = "CHUANG_FEISHU_PREFLIGHT_SMOKE_API_KEY"
transport = "stub"
`,
  "utf8"
);

const envFile = path.join(workDir, "chuang-feishu-bridge.env");
const providerEnvFile = path.join(workDir, "provider.env");
fs.writeFileSync(providerEnvFile, "CODEX_PPTOKEN_API_KEY=<set>\n", "utf8");
fs.writeFileSync(
  envFile,
  [
    `CHUANG_AGENT_ROOT=${root}`,
    `CHUANG_AGENT_WORKSPACE_ROOT=${workspace}`,
    `CHUANG_PROVIDER_ENV_FILE=${providerEnvFile}`,
    "CHUANG_FEISHU_APP_ID=cli_a_chuang_preflight_smoke",
    "CHUANG_FEISHU_APP_SECRET=smoke-secret-value",
    "CHUANG_FEISHU_CONNECTION_MODE=websocket",
    "",
  ].join("\n"),
  "utf8"
);

const result = spawnSync(
  "node",
  [
    path.join(root, "scripts", "chuang-feishu-live-preflight.js"),
    "--env-file",
    envFile,
    "--workspace-root",
    workspace,
    "--state-file",
    path.join(workspace, "context", "feishu-session-state.json"),
    "--json",
  ],
  {
    cwd: root,
    env: {
      ...process.env,
      CHUANG_FEISHU_PREFLIGHT_SMOKE_API_KEY: "test-key",
    },
    encoding: "utf8",
  }
);

assert.strictEqual(result.status, 0, result.stderr || result.stdout);
assert(!result.stdout.includes("smoke-secret-value"), "preflight output must not leak app secret");
const parsed = JSON.parse(result.stdout);
assert.strictEqual(parsed.ok, true);
assert.strictEqual(parsed.readonly, true);
assert.strictEqual(parsed.connects_real_feishu, false);
assert.strictEqual(parsed.evidence.operation_mode, "local_readonly_preflight");
assert.strictEqual(parsed.evidence.live_feishu_connection_attempted, false);
assert.strictEqual(parsed.evidence.live_feishu_message_send_attempted, false);
assert.strictEqual(parsed.evidence.session_store_write_attempted, false);
assert.strictEqual(parsed.evidence.service_modify_attempted, false);
assert.strictEqual(parsed.evidence.secret_values_redacted, true);
assert(parsed.evidence.local_commands.includes("cargo channel feishu-check --json"));
assert(parsed.evidence.local_commands.includes("cargo app-server health --diagnostic --json"));
assert.strictEqual(parsed.boundaries.prints_secret_values, false);
assert.strictEqual(parsed.boundaries.writes_session_store, false);
assert.strictEqual(parsed.boundaries.modifies_services, false);
assert.strictEqual(parsed.boundaries.reuses_codex_or_hermes_credentials, false);

const checks = Object.fromEntries(parsed.checks.map((check) => [check.name, check]));
assert.strictEqual(checks.env_file.status, "pass");
assert.strictEqual(checks.env_source_isolation.status, "pass");
assert.strictEqual(checks.env_source_isolation.inherited_forbidden_credentials_used, false);
assert.strictEqual(checks.env_source_isolation.codex_feishu_bridge_env_used, false);
assert.strictEqual(checks.env_source_isolation.hermes_feishu_env_used, false);
assert.strictEqual(
  checks.env_source_isolation.inherited_forbidden_credential_env_states.HERMES_FEISHU_ENCRYPT_KEY,
  "<unset>"
);
assert.strictEqual(
  checks.env_source_isolation.inherited_forbidden_credential_env_states.CODEX_FEISHU_BOT_ID,
  "<unset>"
);
assert.deepStrictEqual(checks.env_source_isolation.forbidden_credential_env_names_in_file, []);
assert.strictEqual(checks.channel_feishu_check.status, "pass");
assert.strictEqual(checks.channel_feishu_check.live_feishu_call_made, false);
assert.strictEqual(checks.workspace_config.status, "pass");
assert.strictEqual(checks.workspace_config.config_path, path.join(workspace, "config.toml"));
assert.strictEqual(checks.app_server_diagnostic.status, "pass");
assert.strictEqual(checks.app_server_diagnostic.live_provider_call_made, false);
assert.strictEqual(checks.bridge_command_smoke.status, "pass");
assert.strictEqual(checks.bridge_command_smoke.live_feishu_call_made, false);
assert(["pass", "warn"].includes(checks.session_store_access.status));
assert.strictEqual(checks.session_store_access.method, "fs_access_only_no_write");
assert.strictEqual(checks.session_store_access.writes_attempted, false);
assert.strictEqual(checks.session_store_access.parsed_state.parse_status, "missing");
assert.strictEqual(checks.provider_env_file.status, "pass");
assert.strictEqual(checks.provider_env_file.provider_env_parse_status, "ok");
assert.strictEqual(checks.provider_env_file.provider_secret_var_states.CODEX_PPTOKEN_API_KEY, "<set>");
assert.deepStrictEqual(checks.provider_env_file.forbidden_feishu_credential_names_in_provider_env, []);

const legacyEnvFile = path.join(workDir, "chuang-feishu-bridge-legacy.env");
fs.writeFileSync(
  legacyEnvFile,
  [
    `CHUANG_AGENT_ROOT=${root}`,
    `CHUANG_AGENT_WORKSPACE_ROOT=${workspace}`,
    `CHUANG_PROVIDER_ENV_FILE=${providerEnvFile}`,
    "CHUANG_FEISHU_APP_ID=cli_a_chuang_preflight_smoke",
    "CHUANG_FEISHU_APP_SECRET=smoke-secret-value",
    "HERMES_FEISHU_ENCRYPT_KEY=legacy-hermes-encrypt",
    "CODEX_FEISHU_BOT_ID=legacy-codex-bot",
    "",
  ].join("\n"),
  "utf8"
);
const legacyResult = spawnSync(
  "node",
  [
    path.join(root, "scripts", "chuang-feishu-live-preflight.js"),
    "--env-file",
    legacyEnvFile,
    "--workspace-root",
    workspace,
    "--state-file",
    path.join(workspace, "context", "feishu-session-state.json"),
    "--json",
  ],
  {
    cwd: root,
    env: {
      ...process.env,
      CHUANG_FEISHU_PREFLIGHT_SMOKE_API_KEY: "test-key",
    },
    encoding: "utf8",
  }
);
assert.notStrictEqual(legacyResult.status, 0, "legacy Feishu env names should block preflight");
assert(!legacyResult.stdout.includes("legacy-hermes-encrypt"), "preflight output must not leak legacy secret");
const legacyParsed = JSON.parse(legacyResult.stdout);
const legacyChecks = Object.fromEntries(legacyParsed.checks.map((check) => [check.name, check]));
assert.strictEqual(legacyParsed.ok, false);
assert.strictEqual(legacyChecks.env_source_isolation.status, "fail");
assert.deepStrictEqual(legacyChecks.env_source_isolation.forbidden_credential_env_names_in_file, [
  "HERMES_FEISHU_ENCRYPT_KEY",
  "CODEX_FEISHU_BOT_ID",
]);

console.log(`chuang_feishu_live_preflight_smoke_ok work_dir=${workDir}`);
