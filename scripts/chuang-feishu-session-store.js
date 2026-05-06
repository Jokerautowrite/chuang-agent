const fs = require("fs");
const path = require("path");

class FeishuSessionStore {
  constructor(filePath) {
    this.filePath = filePath;
    this.state = this.load();
  }

  load() {
    try {
      if (!fs.existsSync(this.filePath)) {
        return { version: 1, bindings: {} };
      }
      const raw = fs.readFileSync(this.filePath, "utf8");
      const parsed = JSON.parse(raw);
      if (!parsed || typeof parsed !== "object") {
        return { version: 1, bindings: {} };
      }
      const bindings = parsed.bindings && typeof parsed.bindings === "object" ? parsed.bindings : {};
      return { version: 1, bindings };
    } catch {
      return { version: 1, bindings: {} };
    }
  }

  getBinding(chatId) {
    const key = normalizeKey(chatId);
    if (!key) {
      return null;
    }
    const binding = this.state.bindings[key];
    if (!binding || typeof binding !== "object") {
      return null;
    }
    return {
      chatId: key,
      threadId: normalizeKey(binding.threadId),
      workspaceRoot: normalizeKey(binding.workspaceRoot),
      createdAt: normalizeKey(binding.createdAt),
      updatedAt: normalizeKey(binding.updatedAt),
    };
  }

  getThreadId(chatId) {
    return this.getBinding(chatId)?.threadId || "";
  }

  bind(chatId, threadId, workspaceRoot) {
    const key = normalizeKey(chatId);
    const nextThreadId = normalizeKey(threadId);
    if (!key || !nextThreadId) {
      return null;
    }
    const now = new Date().toISOString();
    const existing = this.state.bindings[key];
    this.state.bindings[key] = {
      chatId: key,
      threadId: nextThreadId,
      workspaceRoot: normalizeKey(workspaceRoot),
      createdAt: existing && typeof existing === "object" && existing.createdAt ? existing.createdAt : now,
      updatedAt: now,
    };
    this.save();
    return this.getBinding(key);
  }

  clear(chatId) {
    const key = normalizeKey(chatId);
    if (!key || !this.state.bindings[key]) {
      return false;
    }
    delete this.state.bindings[key];
    this.save();
    return true;
  }

  save() {
    ensureParentDir(this.filePath);
    const nextState = JSON.stringify(this.state, null, 2);
    const tempPath = `${this.filePath}.tmp`;
    fs.writeFileSync(tempPath, nextState, "utf8");
    fs.renameSync(tempPath, this.filePath);
  }
}

function ensureParentDir(filePath) {
  fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

function normalizeKey(value) {
  return typeof value === "string" ? value.trim() : "";
}

module.exports = { FeishuSessionStore };
