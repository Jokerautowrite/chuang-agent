# 底部常驻输入 + 去重 · 2026-07-18

## 截图问题
1. `❯ 哈喽小创` 与下方 `你 │ 哈喽小创` **同一句显示两遍**（终端回显 + 程序再渲染）
2. 输入区随滚动上飘，不像 Grok 钉在底部

## 修复
1. **去重**：提交后先 `clear_prompt_strip`（清掉刚回显的那行），再写紧凑 `你  文本`
2. **钉底**：DEC scroll region 保留底部 3 行；对话滚上面；`pin_prompt` 重绘底栏
3. 运行中不每 200ms 重绘底栏（避免打字被擦掉）

## 重启
```bash
/exit
cd ~/projects/chuang-agent && ./scripts/launch-chuang-agent-repl.sh
```
