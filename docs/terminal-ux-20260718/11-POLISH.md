# chuang 终端抛光 · 2026-07-18

## 做了啥
1. **底栏 Grok/OpenCode 感**：状态行更短；输入前缀 `›`；idle 显示 `/help`
2. **启动更安静**：meta 压成一行 + 短路径；可选 `CHUANG_QUIET_BANNER=1` 极简 banner
3. **过程行**：去掉 `1. 2. 3.` 编号，只留 `▸/✓/✗` + 人话
4. **你/小创气泡**：用户侧只留模型名；答复前细分隔线

## 验收
```bash
cd ~/projects/chuang-agent
./scripts/launch-chuang-agent-repl.sh
# 或安静启动：
CHUANG_QUIET_BANNER=1 ./scripts/launch-chuang-agent-repl.sh
```
