# Grok 式底部固定输入 · 2026-07-18

## 为什么之前「不敢钉底」
用终端 **DECSTBM + 自带回显** 钉底时，Konsole 中文输入法会叠字/半格退格。

## 正确做法（与 Grok 同类）
1. **raw mode**：关闭终端自带回显  
2. **应用自己的输入缓冲区** `draft`：按键进缓冲，Backspace 删一个 Unicode 字符  
3. **底栏 2 行固定重绘**：状态 + `❯ {draft}`  
4. **上方滚动区**只出对话 transcript  
5. 中文由输入法组字后以 `Char` 事件交给我们（crossterm）

依赖：`crossterm = "0.28"`

## 操作
重启 `chuang` / `launch-chuang-agent-repl.sh`  
Ctrl+C 清空草稿；空草稿再 Ctrl+C 退出；Ctrl+U 清空一行。
