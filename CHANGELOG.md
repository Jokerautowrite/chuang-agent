# Changelog

本项目遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
版本号遵循 [Semantic Versioning](https://semver.org/lang/zh-CN/)。

## [0.1.0] - 2026-08-15

首个公开版本（source-available）。2026-04-30 第一行代码落地，经过 3 个多月
的打磨与三平台（Linux / Windows / macOS）CI 门禁，正式对外发布。

### 核心能力

- **记忆是本体**：会话自动落盘，断线、重启、换模型不丢记忆；上下文压缩强制回忆
  最近对话；每日日记自动蒸馏为长期经验。
- **模型无关**：OpenAI 兼容接口任意模型即插即用，原生支持 Anthropic Messages
  格式（Claude / Opus 系列）。
- **多子代理并行**：任务自动拆分、并行派发（上限 32），能并发就不排队。
- **自进化**：从会话提炼经验、沉淀 SOP / skill，失败自动复盘修订规则。
- **治理刹车**：审批流 + 风险控制；子代理不能直接改核心记忆，只能提交报告与
  记忆提案。
- **双入口**：终端 REPL（Ratatui）+ 飞书流式卡片（推荐）。

### 平台支持

- Linux / macOS：REPL、运行时、Unix socket app-server
- Windows：原生 REPL、运行时、治理、PowerShell 桌面适配器

### 工程与安全

- 三平台 GitHub Actions CI：gitleaks 密钥扫描、RustSec 漏洞审计、fmt/clippy/test 全绿
- fail-closed 默认配置：首次使用离线 fake provider，不调用模型与外部 worker
- 自定义非商业许可证（详见 LICENSE）

[0.1.0]: https://github.com/Jokerautowrite/chuang-agent/releases/tag/v0.1.0
