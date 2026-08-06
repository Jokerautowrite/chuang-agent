#!/usr/bin/env node
// 主动联系投递冒烟（dry-run 模式）：
// 在临时发件箱放一条提案 → 桥轮询逻辑应读取、记录 dry_run、归档。
// 用法：CHUANG_PROACTIVE_DRY_RUN=1 node scripts/chuang-feishu-proactive-smoke.js

const fs = require("fs");
const os = require("os");
const path = require("path");

const ROOT = path.resolve(__dirname, "..");
const SMOKE_ROOT = fs.mkdtempSync(path.join(os.tmpdir(), "chuang-proactive-smoke-"));
const OUTBOX_DIR = path.join(SMOKE_ROOT, "proactive-outbox");
const STATE_FILE = path.join(SMOKE_ROOT, "feishu-session-state.json");
const EVENT_LOG = path.join(SMOKE_ROOT, "events.log");

// 让桥的 loadEnv 读到这些路径。
process.env.CHUANG_AGENT_ROOT = ROOT;
process.env.CHUANG_AGENT_WORKSPACE_ROOT = ROOT;
process.env.CHUANG_FEISHU_STATE_FILE = STATE_FILE;
process.env.CHUANG_PROACTIVE_OUTBOX_DIR = OUTBOX_DIR;
process.env.CHUANG_PROACTIVE_DRY_RUN = "1";
process.env.CHUANG_FEISHU_EVENT_LOG_FILE = EVENT_LOG;
process.env.CHUANG_FEISHU_APP_ID = "cli_test_dummy";
process.env.CHUANG_FEISHU_APP_SECRET = "cli_test_dummy_secret";

fs.mkdirSync(OUTBOX_DIR, { recursive: true });
fs.writeFileSync(
  STATE_FILE,
  JSON.stringify({
    version: 1,
    bindings: {
      oc_smoke_chat: {
        chatId: "oc_smoke_chat",
        threadId: "chuang-thread-smoke",
        workspaceRoot: ROOT,
        createdAt: new Date().toISOString(),
        updatedAt: new Date().toISOString(),
      },
    },
  })
);

const { ChuangFeishuBridge } = require("./chuang-feishu-bridge");

async function main() {
  const bridge = new ChuangFeishuBridge();
  fs.writeFileSync(
    path.join(OUTBOX_DIR, "proactive-smoke-1.json"),
    JSON.stringify(
      {
        id: "smoke-1",
        created_at: new Date().toISOString(),
        workspace_root: ROOT,
        reason: "contact",
        urgency: "0.7",
        text: "冒烟测试消息：主人，想你了。",
        source: "emotion-heartbeat",
      },
      null,
      2
    )
  );

  await bridge.pollProactiveOutbox();

  const pending = fs.existsSync(OUTBOX_DIR)
    ? fs.readdirSync(OUTBOX_DIR).filter((name) => name.endsWith(".json"))
    : [];
  const archived = fs.existsSync(path.join(OUTBOX_DIR, "archive"))
    ? fs.readdirSync(path.join(OUTBOX_DIR, "archive"))
    : [];
  const events = fs.existsSync(EVENT_LOG) ? fs.readFileSync(EVENT_LOG, "utf8") : "";

  if (pending.length === 0 && archived.length === 1 && events.includes("proactive_dry_run")) {
    console.log("PASS proactive smoke: dry_run logged, entry archived");
    process.exit(0);
  }
  console.error(
    JSON.stringify({ pending, archived, events: events.split("\n").filter(Boolean).slice(-5) }, null, 2)
  );
  process.exit(1);
}

main().catch((error) => {
  console.error(error.message);
  process.exit(1);
});
