#!/usr/bin/env node

const fs = require("fs");
const os = require("os");
const path = require("path");
const { spawnSync } = require("child_process");
const { FORBIDDEN_CREDENTIAL_ENV_NAMES } = require("./chuang-feishu-bridge-config");

const ROOT = process.env.CHUANG_AGENT_ROOT || path.resolve(__dirname, "..");
const DEFAULT_ENV_FILE =
  process.env.CHUANG_FEISHU_ENV_FILE || path.join(os.homedir(), ".codex-im/chuang-feishu-bridge.env");

function main() {
  const request = parseArgs(process.argv.slice(2));
  const envFile = path.resolve(request.envFile || DEFAULT_ENV_FILE);
  const envValues = readEnvFile(envFile);
  const workspaceRoot = path.resolve(
    request.workspaceRoot ||
      envValues.values.CHUANG_AGENT_WORKSPACE_ROOT ||
      process.env.CHUANG_AGENT_WORKSPACE_ROOT ||
      ROOT
  );
  const stateFile = path.resolve(
    request.stateFile ||
      process.env.CHUANG_FEISHU_STATE_FILE ||
      path.join(workspaceRoot, "context", "feishu-session-state.json")
  );

  const checks = [];
  checks.push(checkEnvFile(envFile, envValues));
  checks.push(checkEnvIsolation(envFile, envValues.values));
  checks.push(runFeishuCheck(envFile));
  checks.push(checkWorkspace(workspaceRoot));
  checks.push(runAppServerDiagnostic(workspaceRoot));
  checks.push(runNodeSmoke("bridge_command_smoke", "chuang-feishu-command-smoke.js"));
  checks.push(checkSessionStoreAccess(stateFile));
  checks.push(checkProviderEnv(envValues.values.CHUANG_PROVIDER_ENV_FILE));

  const nextActions = checks.flatMap((check) => check.next_actions || []);
  const ok = checks.every((check) => check.status !== "fail");
  const result = {
    ok,
    status: ok ? (checks.some((check) => check.status === "warn") ? "warning" : "ready") : "blocked",
    readonly: true,
    connects_real_feishu: false,
    env_file: envFile,
    workspace_root: workspaceRoot,
    session_state_file: stateFile,
    evidence: {
      schema_version: 1,
      purpose: "chuang_feishu_live_readiness_without_live_feishu_connection",
      operation_mode: "local_readonly_preflight",
      live_feishu_connection_attempted: false,
      live_feishu_message_send_attempted: false,
      session_store_write_attempted: false,
      service_modify_attempted: false,
      secret_values_redacted: true,
      local_commands: checks.flatMap((check) => check.local_commands || []),
      filesystem_methods: checks.flatMap((check) => check.filesystem_methods || []),
    },
    checks,
    next_actions: nextActions,
    boundaries: {
      reads_env_file: true,
      connects_real_feishu: false,
      sends_feishu_messages: false,
      prints_secret_values: false,
      writes_session_store: false,
      modifies_services: false,
      reuses_codex_or_hermes_credentials: false,
    },
  };

  if (request.json) {
    process.stdout.write(`${JSON.stringify(result, null, 2)}\n`);
  } else {
    printText(result);
  }
  process.exit(ok ? 0 : 1);
}

function parseArgs(args) {
  const request = { envFile: "", workspaceRoot: "", stateFile: "", json: false };
  for (let index = 0; index < args.length; index += 1) {
    const arg = args[index];
    if (arg === "--env-file") {
      request.envFile = takeValue(args, index, arg);
      index += 1;
    } else if (arg === "--workspace-root") {
      request.workspaceRoot = takeValue(args, index, arg);
      index += 1;
    } else if (arg === "--state-file") {
      request.stateFile = takeValue(args, index, arg);
      index += 1;
    } else if (arg === "--json") {
      request.json = true;
    } else {
      usage();
    }
  }
  return request;
}

function takeValue(args, index, flag) {
  const value = args[index + 1];
  if (!value) {
    throw new Error(`missing value after ${flag}`);
  }
  return value;
}

function usage() {
  throw new Error(
    "usage: node scripts/chuang-feishu-live-preflight.js [--env-file PATH] [--workspace-root PATH] [--state-file PATH] [--json]"
  );
}

function checkEnvFile(envFile, envValues) {
  const nextActions = [];
  if (!fs.existsSync(envFile)) {
    return fail("env_file", "env file is missing", {
      path: envFile,
      next_actions: ["create_chuang_dedicated_feishu_env_file"],
    });
  }
  if (envValues.error) {
    return fail("env_file", "env file could not be parsed", {
      path: envFile,
      error: sanitize(envValues.error),
      next_actions: ["fix_chuang_feishu_env_file_syntax"],
    });
  }
  const missing = ["CHUANG_FEISHU_APP_ID", "CHUANG_FEISHU_APP_SECRET", "CHUANG_AGENT_WORKSPACE_ROOT"].filter(
    (name) => !String(envValues.values[name] || "").trim()
  );
  if (missing.length) {
    nextActions.push(`set_missing_chuang_env_vars:${missing.join(",")}`);
  }
  const legacy = Object.keys(envValues.values).filter((name) =>
    FORBIDDEN_CREDENTIAL_ENV_NAMES.includes(name)
  );
  if (legacy.length) {
    nextActions.push("remove_legacy_feishu_env_names");
  }
  return {
    name: "env_file",
    status: nextActions.length ? "fail" : "pass",
    summary: nextActions.length
      ? "Chuang Feishu env needs local fixes before live startup"
      : "Chuang Feishu env exists and required values are present",
    required_vars: Object.fromEntries(
      ["CHUANG_FEISHU_APP_ID", "CHUANG_FEISHU_APP_SECRET", "CHUANG_AGENT_WORKSPACE_ROOT"].map((name) => [
        name,
        envValues.values[name] ? "<set>" : "<missing>",
      ])
    ),
    legacy_var_names: legacy,
    filesystem_methods: [`readFileSync:${envFile}`],
    next_actions: nextActions,
  };
}

function checkEnvIsolation(envFile, values) {
  const forbiddenInFile = Object.keys(values).filter((name) =>
    FORBIDDEN_CREDENTIAL_ENV_NAMES.includes(name)
  );
  const inheritedForbiddenStates = Object.fromEntries(
    FORBIDDEN_CREDENTIAL_ENV_NAMES.map((name) => [
      name,
      Object.prototype.hasOwnProperty.call(process.env, name) ? "<set_ignored>" : "<unset>",
    ])
  );
  const pathWarnings = envFileScopeWarnings(envFile);
  const nextActions = [];
  if (forbiddenInFile.length) {
    nextActions.push("remove_codex_or_hermes_feishu_env_names_from_chuang_env");
  }
  if (pathWarnings.length) {
    nextActions.push("move_env_file_to_chuang_dedicated_path");
  }
  return {
    name: "env_source_isolation",
    status: nextActions.length ? "fail" : "pass",
    summary: nextActions.length
      ? "env source isolation is not clean"
      : "preflight uses only Chuang-scoped Feishu env names and ignores inherited Codex/Hermes Feishu credentials",
    env_file: envFile,
    env_file_scope_warnings: pathWarnings,
    accepted_feishu_env_names: [
      "CHUANG_FEISHU_APP_ID",
      "CHUANG_FEISHU_APP_SECRET",
      "CHUANG_FEISHU_BOT_ID",
      "CHUANG_FEISHU_VERIFICATION_TOKEN",
      "CHUANG_FEISHU_ENCRYPT_KEY",
      "CHUANG_FEISHU_CONNECTION_MODE",
    ],
    forbidden_credential_env_names_in_file: forbiddenInFile,
    inherited_forbidden_credential_env_states: inheritedForbiddenStates,
    inherited_forbidden_credentials_used: false,
    codex_feishu_bridge_env_used: false,
    hermes_feishu_env_used: false,
    next_actions: nextActions,
  };
}

function runFeishuCheck(envFile) {
  if (!fs.existsSync(envFile)) {
    return fail("channel_feishu_check", "skipped because env file is missing", {
      next_actions: ["create_chuang_dedicated_feishu_env_file"],
    });
  }
  const result = spawnSync(
    "cargo",
    ["run", "--quiet", "--manifest-path", path.join(ROOT, "Cargo.toml"), "--", "channel", "feishu-check", "--env-file", envFile, "--json"],
    { cwd: ROOT, encoding: "utf8" }
  );
  if (result.status !== 0) {
    return fail("channel_feishu_check", "cargo channel feishu-check failed", {
      stderr: sanitize(result.stderr || result.stdout || ""),
      next_actions: ["run_channel_feishu_check_manually"],
    });
  }
  const parsed = parseJson(result.stdout);
  if (!parsed.ok) {
    return fail("channel_feishu_check", parsed.diagnostic_summary || "feishu-check blocked startup", {
      diagnostic_status: parsed.diagnostic_status || "blocked",
      env_file_is_chuang_scoped: Boolean(parsed.env_file_is_chuang_scoped),
      workspace_root_exists: Boolean(parsed.workspace_root_exists),
      workspace_config_exists: Boolean(parsed.workspace_config_exists),
      connection_mode_ok: Boolean(parsed.connection_mode_ok),
      has_legacy_names: Boolean(parsed.has_legacy_names),
      next_actions: parsed.next_actions || [],
    });
  }
  return {
    name: "channel_feishu_check",
    status: "pass",
    summary: parsed.diagnostic_summary || "channel feishu-check passed without a live Feishu call",
    diagnostic_status: parsed.diagnostic_status || "ready",
    env_file_is_chuang_scoped: Boolean(parsed.env_file_is_chuang_scoped),
    workspace_root_exists: Boolean(parsed.workspace_root_exists),
    workspace_config_exists: Boolean(parsed.workspace_config_exists),
    connection_mode: parsed.connection_mode || "",
    connection_mode_ok: Boolean(parsed.connection_mode_ok),
    live_feishu_call_made: false,
    local_commands: ["cargo channel feishu-check --json"],
    next_actions: [],
  };
}

function checkWorkspace(workspaceRoot) {
  const exists = fs.existsSync(workspaceRoot) && fs.statSync(workspaceRoot).isDirectory();
  const configPath = path.join(workspaceRoot, "config.toml");
  const configExists = exists && fs.existsSync(configPath) && fs.statSync(configPath).isFile();
  const nextActions = [];
  if (!exists) {
    nextActions.push("fix_chuang_agent_workspace_root");
  } else if (!configExists) {
    nextActions.push("add_or_fix_workspace_config_toml");
  }
  return {
    name: "workspace_config",
    status: nextActions.length ? "fail" : "pass",
    summary: nextActions.length ? "workspace/config is not ready" : "workspace and config.toml are present",
    workspace_root: workspaceRoot,
    workspace_root_exists: exists,
    workspace_root_realpath: exists ? safeRealpath(workspaceRoot) : "",
    config_path: configPath,
    workspace_config_exists: configExists,
    config_realpath: configExists ? safeRealpath(configPath) : "",
    filesystem_methods: [`statSync:${workspaceRoot}`, `statSync:${configPath}`],
    next_actions: nextActions,
  };
}

function runAppServerDiagnostic(workspaceRoot) {
  const configPath = path.join(workspaceRoot, "config.toml");
  if (!fs.existsSync(configPath)) {
    return fail("app_server_diagnostic", "skipped because workspace config.toml is missing", {
      next_actions: ["add_or_fix_workspace_config_toml"],
    });
  }
  const result = spawnSync(
    "cargo",
    [
      "run",
      "--quiet",
      "--manifest-path",
      path.join(ROOT, "Cargo.toml"),
      "--",
      "app-server",
      "health",
      "--workspace-root",
      workspaceRoot,
      "--diagnostic",
      "--json",
    ],
    { cwd: ROOT, encoding: "utf8" }
  );
  if (result.status !== 0) {
    return fail("app_server_diagnostic", "app-server diagnostic health failed", {
      stderr: sanitize(result.stderr || result.stdout || ""),
      next_actions: ["fix_app_server_workspace_diagnostics"],
    });
  }
  const parsed = parseJson(result.stdout);
  return {
    name: "app_server_diagnostic",
    status: parsed.ok ? "pass" : "fail",
    summary: parsed.ok
      ? "app-server diagnostic health passed without provider live calls"
      : "app-server diagnostic health reported not ok",
    diagnostic_mode: Boolean(parsed.diagnostic_mode),
    api_key_state: parsed.api_key_state || "unknown",
    release_readiness_state: parsed.release_readiness?.overall_state || "unknown",
    channel_readiness_state: parsed.channel_readiness?.overall_state || "unknown",
    provider_kind: parsed.config?.provider || parsed.provider || "unknown",
    provider_transport: parsed.config?.transport || parsed.transport || "unknown",
    live_provider_call_made: false,
    local_commands: ["cargo app-server health --diagnostic --json"],
    next_actions: parsed.ok ? [] : ["fix_app_server_workspace_diagnostics"],
  };
}

function runNodeSmoke(name, scriptName) {
  const result = spawnSync("node", [path.join(ROOT, "scripts", scriptName)], {
    cwd: ROOT,
    encoding: "utf8",
  });
  if (result.status !== 0) {
    return fail(name, `${scriptName} failed`, {
      stderr: sanitize(result.stderr || result.stdout || ""),
      next_actions: [`fix_${name}`],
    });
  }
  return {
    name,
    status: "pass",
    summary: `${scriptName} passed locally without a live Feishu call`,
    marker: sanitize((result.stdout || "").trim()),
    live_feishu_call_made: false,
    local_commands: [`node scripts/${scriptName}`],
    next_actions: [],
  };
}

function checkSessionStoreAccess(stateFile) {
  const parent = path.dirname(stateFile);
  const parentExists = fs.existsSync(parent);
  const nearest = nearestExistingParent(parent);
  const parentWritable = parentExists ? canAccess(parent, fs.constants.R_OK | fs.constants.W_OK) : false;
  const nearestWritable = nearest ? canAccess(nearest, fs.constants.R_OK | fs.constants.W_OK) : false;
  const fileExists = fs.existsSync(stateFile);
  const fileWritable = fileExists ? canAccess(stateFile, fs.constants.R_OK | fs.constants.W_OK) : null;
  const stateSummary = summarizeSessionStore(stateFile);
  const pass = fileExists ? fileWritable : parentExists ? parentWritable : nearestWritable;
  return {
    name: "session_store_access",
    status: pass ? (parentExists ? "pass" : "warn") : "fail",
    summary: pass
      ? parentExists
        ? "session store path is accessible"
        : "session store parent is absent, but nearest existing parent is writable"
      : "session store path is not writable",
    method: "fs_access_only_no_write",
    state_file: stateFile,
    parent_exists: parentExists,
    parent_writable: parentWritable,
    nearest_existing_parent: nearest || "",
    nearest_existing_parent_writable: nearestWritable,
    file_exists: fileExists,
    file_writable: fileWritable,
    parsed_state: stateSummary,
    filesystem_methods: [
      `existsSync:${stateFile}`,
      `accessSync:${fileExists ? stateFile : parentExists ? parent : nearest || parent}`,
      ...(fileExists ? [`readFileSync:${stateFile}`] : []),
    ],
    writes_attempted: false,
    next_actions: pass ? [] : ["fix_chuang_feishu_session_state_path_permissions"],
  };
}

function checkProviderEnv(providerEnvFile) {
  const file = String(providerEnvFile || process.env.CHUANG_PROVIDER_ENV_FILE || "").trim();
  if (!file) {
    return {
      name: "provider_env_file",
      status: "warn",
      summary: "CHUANG_PROVIDER_ENV_FILE is not set in the Chuang Feishu env",
      provider_env_file_state: "<unset>",
      provider_env_file: "",
      provider_secret_var_states: {},
      forbidden_feishu_credential_names_in_provider_env: [],
      next_actions: ["set_chuang_provider_env_file_if_live_provider_is_required"],
    };
  }
  const exists = fs.existsSync(file);
  const parsed = exists ? readEnvFile(file) : { values: {}, error: "" };
  const forbiddenFeishuNames = Object.keys(parsed.values).filter((name) => name.startsWith("CHUANG_FEISHU_"));
  const nextActions = [];
  if (!exists) {
    nextActions.push("create_or_fix_chuang_provider_env_file");
  }
  if (parsed.error) {
    nextActions.push("fix_chuang_provider_env_file_syntax");
  }
  if (forbiddenFeishuNames.length) {
    nextActions.push("remove_feishu_credentials_from_provider_env_file");
  }
  const status = !exists ? "warn" : nextActions.length ? "fail" : "pass";
  return {
    name: "provider_env_file",
    status,
    summary: exists
      ? parsed.error
        ? "provider env file exists but could not be parsed"
        : forbiddenFeishuNames.length
          ? "provider env file exists but contains Feishu credential names"
          : "provider env file path exists and secret values are redacted"
      : "provider env file path is set but missing",
    provider_env_file: file,
    provider_env_file_state: exists ? "<set>" : "<missing_file>",
    provider_env_parse_status: parsed.error ? `parse_error:${sanitize(parsed.error)}` : exists ? "ok" : "missing",
    provider_secret_var_states: exists
      ? Object.fromEntries(Object.keys(parsed.values).map((name) => [name, parsed.values[name] ? "<set>" : "<missing>"]))
      : {},
    forbidden_feishu_credential_names_in_provider_env: forbiddenFeishuNames,
    filesystem_methods: exists ? [`readFileSync:${file}`] : [`existsSync:${file}`],
    next_actions: nextActions,
  };
}

function summarizeSessionStore(stateFile) {
  if (!fs.existsSync(stateFile)) {
    return {
      readable: false,
      parse_status: "missing",
      version: null,
      binding_count: 0,
      has_workspace_roots: false,
    };
  }
  try {
    const parsed = JSON.parse(fs.readFileSync(stateFile, "utf8"));
    const bindings = parsed && typeof parsed.bindings === "object" && parsed.bindings ? parsed.bindings : {};
    const bindingValues = Object.values(bindings).filter((value) => value && typeof value === "object");
    return {
      readable: true,
      parse_status: "ok",
      version: parsed.version || null,
      binding_count: bindingValues.length,
      has_workspace_roots: bindingValues.some((binding) => Boolean(binding.workspaceRoot)),
    };
  } catch (error) {
    return {
      readable: false,
      parse_status: `parse_error:${sanitize(error.message)}`,
      version: null,
      binding_count: 0,
      has_workspace_roots: false,
    };
  }
}

function readEnvFile(envFile) {
  try {
    const content = fs.readFileSync(envFile, "utf8");
    const values = {};
    for (const rawLine of content.split(/\r?\n/)) {
      const line = rawLine.trim();
      if (!line || line.startsWith("#")) {
        continue;
      }
      const equalIndex = line.indexOf("=");
      if (equalIndex < 1) {
        return { values: {}, error: "invalid env line" };
      }
      const key = line.slice(0, equalIndex).trim();
      const value = line.slice(equalIndex + 1).trim().replace(/^['"]|['"]$/g, "");
      values[key] = value;
    }
    return { values, error: "" };
  } catch (error) {
    return { values: {}, error: error.message };
  }
}

function nearestExistingParent(target) {
  let current = target;
  while (current && current !== path.dirname(current)) {
    if (fs.existsSync(current)) {
      return current;
    }
    current = path.dirname(current);
  }
  return fs.existsSync(current) ? current : "";
}

function canAccess(target, mode) {
  try {
    fs.accessSync(target, mode);
    return true;
  } catch {
    return false;
  }
}

function safeRealpath(target) {
  try {
    return fs.realpathSync(target);
  } catch {
    return "";
  }
}

function envFileScopeWarnings(envFile) {
  const normalized = envFile.split(path.sep).join("/");
  const basename = path.basename(envFile);
  const warnings = [];
  if (basename === ".env") {
    warnings.push("generic_dotenv_path");
  }
  if (/hermes/i.test(normalized)) {
    warnings.push("hermes_path_segment");
  }
  if (/codex-feishu/i.test(normalized)) {
    warnings.push("codex_feishu_path_segment");
  }
  if (!/chuang/i.test(normalized)) {
    warnings.push("missing_chuang_path_marker");
  }
  return warnings;
}

function parseJson(text) {
  try {
    return JSON.parse(text);
  } catch (error) {
    return { ok: false, diagnostic_summary: `json_parse_failed: ${sanitize(error.message)}` };
  }
}

function fail(name, summary, extra = {}) {
  return {
    name,
    status: "fail",
    summary,
    ...extra,
    next_actions: extra.next_actions || [],
  };
}

function sanitize(value) {
  return String(value || "")
    .replace(/(app[_-]?secret|token|api[_-]?key|authorization|password)=\S+/gi, "$1=<redacted>")
    .replace(/Bearer\s+[A-Za-z0-9._~+/=-]+/gi, "Bearer <redacted>")
    .slice(0, 800);
}

function printText(result) {
  console.log(`chuang_feishu_live_preflight_ok: ${result.ok}`);
  console.log(`status: ${result.status}`);
  console.log(`readonly: ${result.readonly}`);
  console.log(`connects_real_feishu: ${result.connects_real_feishu}`);
  console.log(`evidence.operation_mode: ${result.evidence.operation_mode}`);
  console.log(`evidence.live_feishu_connection_attempted: ${result.evidence.live_feishu_connection_attempted}`);
  console.log(`evidence.session_store_write_attempted: ${result.evidence.session_store_write_attempted}`);
  console.log(`env_file: ${result.env_file}`);
  console.log(`workspace_root: ${result.workspace_root}`);
  console.log(`session_state_file: ${result.session_state_file}`);
  for (const check of result.checks) {
    console.log(`check.${check.name}: ${check.status} - ${check.summary}`);
  }
  console.log(`next_actions: ${result.next_actions.length ? result.next_actions.join(";") : "none"}`);
}

try {
  main();
} catch (error) {
  console.error(sanitize(error.message));
  process.exit(2);
}
