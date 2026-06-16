# AgentShell

> **Codex 式极简 Agent 桌面壳 + 多模型 Provider（默认 [MiniMax M3](https://www.minimax.io/models/text/m3)）**
> **macOS / Windows 双端桌面版** · **Tauri 2 + Rust**

[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2-blue)](https://tauri.app/)
[![Rust](https://img.shields.io/badge/Rust-1.85%2B-orange)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/Platform-macOS%20%7C%20Windows-lightgrey)](#-下载)
[![Status](https://img.shields.io/badge/Status-Planning-yellow)](#-路线图)

---

## ✨ 这是什么

AgentShell 是一个**轻量级桌面 Agent**，让 AI 模型（M3 / Claude / GPT / DeepSeek）直接在你的电脑上干活：

- 🪟 **极简** — 三栏布局，对标 Codex CLI / Cursor，开箱即用
- 🤖 **多模型** — 默认 MiniMax M3（**$0.60 / M tokens，比 Opus 4.8 便宜 8×**），可切 Claude / GPT / DeepSeek
- 🖥️ **Computer Use** — 浏览器层（Playwright）+ 桌面层（macOS/Windows 原生 CUA）
- 🖼️ **多模态生成** — 集成 MiniMax 图像/视频生成 API（独立平台能力）
- 🛠️ **完整工具集** — 60+ tools（文件 / Bash / 浏览器 / 桌面 / 网络 / 截图）
- 💬 **远程触发** — 飞书 / 企微 / Slack / Mobile Web（5.29 Pocket）
- 🔌 **插件市场** — 6 种 plugin（tool / skill / hook / provider / theme / workflow）
- 🔐 **本地优先** — Memory / Session / Profile 全部本地加密，**不上云**

## 📸 截图

（v0.1.0 上线后补）

## 🚀 快速开始

### 下载

| 系统 | 下载 | 状态 |
|------|------|------|
| macOS (Apple Silicon) | [Releases](https://github.com/ahs/agentshell/releases) | v0.1.0-alpha 计划中 |
| macOS (Intel) | [Releases](https://github.com/ahs/agentshell/releases) | v0.1.0-alpha 计划中 |
| Windows (x64) | [Releases](https://github.com/ahs/agentshell/releases) | v0.1.0-alpha 计划中 |
| Windows (ARM64) | [Releases](https://github.com/ahs/agentshell/releases) | v0.1.0-alpha 计划中 |

### 从源码编译

```bash
# 前置：Rust 1.85+, Node.js 20+, pnpm 9+
git clone https://github.com/ahs/agentshell.git
cd agentshell
pnpm install
pnpm tauri build
```

### 配置

```bash
# 复制配置模板
cp configs/config.example.toml ~/.agentshell/config.toml

# 设置 M3 API Key（也可走 MiniMax Token Plan）
export MINIMAX_API_KEY=sk-...
```

详见 [配置示例](configs/config.example.toml) 和 [开发文档](docs/开发文档.md#42-配置文件结构)。

## 📚 文档

- [**开发文档**（7700+ 行完整设计）](docs/开发文档.md) — 产品定位 / 架构 / 功能 / 路线图 / 风险
- [配置示例](configs/config.example.toml) — 4.2 配置文件结构
- [CHANGELOG](CHANGELOG.md) — 版本变更
- [CONTRIBUTING](CONTRIBUTING.md) — 贡献指南
- [CODE_OF_CONDUCT](CODE_OF_CONDUCT.md) — 社区公约

## 🗺️ 路线图

| 版本 | 时间 | 关键能力 |
|------|------|----------|
| **v0.1.0-alpha** | 2026-08 | 多 provider + Computer Use (Web) + License 激活码 + 5.36 主题 + 5.37A Unified Mentions + 5.40 Thinking 折叠 |
| v0.2.0 | 2026-10 | Computer Use (Web 增强) + Personality + Skills + Vim 模式 + MCP 集成 |
| v0.3.0 | 2026-12 | Automations 定时任务 + Voice Input + Appshots + Floating pop-out + 5.22A Goal |
| v0.4.0 | 2027-02 | Computer Use 桌面 CUA 双端 + Pocket 消息触发 + Plugin Marketplace + Profile lifetime token + Mobile Remote |
| v0.5.0 | 2027-04 | Open-weights M3 私有化部署（To B 路线）+ Headless 模式 + GDPR |
| **v1.0.0** | 2027-06 | GA 正式版 + 终身免费升级路径 |

完整 14 个 Phase 任务清单见 [开发文档 §10](docs/开发文档.md#10-开发阶段与里程碑)。

## 🛠️ 技术栈

- **壳层**：Tauri 2（Rust + WebView）
- **核心**：Rust（`crates/agent-core`, `provider`, `patch`, `context`, `mcp`, `sandbox`）
- **前端**：React 19 + TypeScript + Vite
- **MCP 协议**：stdio / WebSocket / Unix Domain Socket
- **目标系统**：macOS 12+ / Windows 10+

## 🤝 贡献

我们**欢迎所有形式的贡献**！详情见 [CONTRIBUTING.md](CONTRIBUTING.md)：

- 🐛 **Bug 报告**：[GitHub Issues](https://github.com/ahs/agentshell/issues/new?template=bug.md)
- 💡 **功能请求**：[GitHub Issues](https://github.com/ahs/agent-agent/issues/new?template=feature.md)
- 🔧 **Pull Request**：参考 [CONTRIBUTING.md](CONTRIBUTING.md) 的 PR 流程
- 🌐 **翻译**：[docs/i18n](docs/i18n) （v0.4+ 启动）
- 📖 **文档改进**：所有 docs/ 下文件都欢迎 PR

**Code of Conduct**：所有参与者请遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。

## 💬 社区

- **GitHub Discussions**：[讨论区](https://github.com/ahs/agentshell/discussions)（问答 / 分享 / 提案）
- **GitHub Issues**：[bug / feature](https://github.com/ahs/agentshell/issues)
- **微信公众号**：搜索 `AgentShell 社区`（即将开通）

## 📊 项目状态

- 🚧 **v0.1.0-alpha**：规划中（设计已完成 100%，代码 0%）
- 📋 **7700+ 行设计文档**：[docs/开发文档.md](docs/开发文档.md)
- ⭐ **Star History**：[![](https://img.shields.io/github/stars/ahs/agentshell?style=social)](https://github.com/ahs/agentshell/stargazers)

## 📜 许可证

本项目采用 **MIT 许可证** — 详见 [LICENSE](LICENSE) 文件。

```
MIT License

Copyright (c) 2026 AgentShell Contributors

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## 🙏 致谢

- [OpenAI Codex](https://github.com/openai/codex) — 架构与命令系统的主要参考
- [MiniMax M3](https://www.minimax.io/models/text/m3) — 默认模型，MSA 架构 + 1M context + 多模态
- [Anthropic Claude Code](https://github.com/anthropics/claude-code) — 权限模型与 Skill 系统
- [Tauri](https://tauri.app/) — 跨平台桌面壳
- 所有 [Contributors](https://github.com/ahs/agentshell/graphs/contributors) — 谢谢你们的 PR！

---

**⭐ 如果这个项目对你有帮助，请给我们一个 Star！**
