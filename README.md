# Codex GX

<p align="center">
  <img src="https://img.shields.io/badge/Codex-GX-v2.0-blue?style=for-the-badge&logo=github" alt="Version">
  <img src="https://img.shields.io/badge/License-MIT-green?style=for-the-badge" alt="License">
  <img src="https://img.shields.io/badge/Rust-Tauri-orange?style=for-the-badge&logo=rust" alt="Tech">
  <img src="https://img.shields.io/badge/Price-100%25%20Free-red?style=for-the-badge" alt="Price">
</p>

<p align="center">
  <a href="https://github.com/opc007/codex-gx/stargazers"><img src="https://img.shields.io/github/stars/opc007/codex-gx?style=social" alt="Stars"></a>
  <a href="https://github.com/opc007/codex-gx/network/members"><img src="https://img.shields.io/github/forks/opc007/codex-gx?style=social" alt="Forks"></a>
  <a href="https://github.com/opc007/codex-gx/issues"><img src="https://img.shields.io/github/issues/opc007/codex-gx?style=social" alt="Issues"></a>
</p>


> **全功能免费开源 AI Agent 桌面客户端** — Tauri 2 + Rust，macOS / Windows 双端
>
> v2.0 永久免费 · MIT 开源协议 · 社区共建

## ✨ 特性

- 🧠 **多 Provider** — M3 / DeepSeek / OpenAI / Claude / Ollama / Llama.cpp
- 🛠 **15+ 内置工具** — Read / Edit / Write / Bash / Grep / Glob / WebFetch / Git / PR Review …
- 📚 **Skills 库** — 12 个内置模板（shell / prompt / chain 三种执行模式）
- 🔊 **TTS** — 跨平台语音播报
- 🕸️ **Agent Flow Graph** — Plan 任务可视化 DAG + Mermaid 导出
- ☁️ **Session Sync** — 本地 bundle 缓存 / 导入导出
- 🧩 **Plugin 热加载** — 5 个内置插件 + DSL 文本转换
- 🤝 **开源 MIT** — 完全免费，共同维护

## 📦 安装

### macOS

1. 下载 [GitHub Releases](https://github.com/opc007/codex-gx/releases) 的 `Codex gx_*.dmg`
2. 双击 `.dmg`，拖到 Applications
3. 打开 Applications，右键 → 打开 → 仍要打开

### Windows

1. 下载 `Codex gx_*_x64-setup.exe`
2. 双击运行

## 🛠 从源码 Build

```bash
git clone https://github.com/opc007/codex-gx.git
cd codex-gx

# 1. 安装 Rust 工具链（≥ 1.85）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 安装 Node.js（≥ 20）
brew install node@20

# 3. 安装前端依赖（使用 npm）
cd apps/desktop && npm install && cd ..

# 4. 安装 Tauri CLI
cargo install tauri-cli --version "^2"

# 5. 开发模式
cargo tauri dev

# 6. 生产 build
cargo tauri build
```

## 🤝 参与贡献

Codex GX 是开源项目，欢迎所有人参与！

- 🐛 报告 Bug：[Issues](https://github.com/opc007/codex-gx/issues)
- 💡 提出功能建议
- 🔧 提交 PR 修复问题或添加功能
- 📖 完善文档

## 📁 目录结构

```
codex-gx/
├── apps/desktop/           # Tauri 2 桌面端
│   ├── src/                # React + TypeScript 前端
│   └── src-tauri/          # Rust 后端
├── crates/                 # 共享 Rust 库
│   ├── agent-core/         # Agent 核心
│   ├── provider/           # LLM provider 抽象
│   ├── patch/              # apply_patch 引擎
│   ├── context/            # 上下文压缩
│   ├── mcp/                # MCP 协议
│   └── ...                 # 更多模块
└── .github/workflows/       # 自动化工作流
    ├── self-iterate.yml    # 🧠 每日自动优化迭代
    ├── daily-upgrade.yml   # 📦 每日构建发布
    └── ci.yml              # CI 检查
```

## 🧠 每日自动迭代

项目配置了全自动迭代系统，每天自动运行：

1. **检查** — 扫描 Rust/TypeScript 代码问题
2. **修复** — 自动修复 clippy 建议、unused imports、依赖问题
3. **验证** — 运行测试确保修复有效
4. **提交** — 通过则自动提交推送；失败则创建 Issue 通知

## 📜 License

MIT License — 完全免费，可自由使用、修改、分发。

## 🔗 链接

- [GitHub](https://github.com/opc007/codex-gx)
- [Releases](https://github.com/opc007/codex-gx/releases)
- [Issues](https://github.com/opc007/codex-gx/issues)

---

## 🤝 如何参与贡献

### 方式一：提交 Pull Request（推荐）

```bash
# 1. Fork 仓库到你的 GitHub 账号
# 2. Clone 你的 fork
git clone https://github.com/YOUR_USERNAME/codex-gx.git
cd codex-gx

# 3. 从 main 创建功能分支
git checkout -b feat/你的功能名

# 4. 开发
git commit -m "feat: 添加新功能"

# 5. Push 到你的 fork
git push origin feat/你的功能名

# 6. 在 GitHub 上发起 Pull Request
```

### 方式二：提交 Issue

- 🐛 发现 Bug？ → [新建 Bug Report](https://github.com/opc007/codex-gx/issues/new?template=bug.md)
- 💡 有功能建议？ → [新建 Feature Request](https://github.com/opc007/codex-gx/issues/new?template=feature.md)
- 📖 文档改进？ → [新建 Documentation Issue](https://github.com/opc007/codex-gx/issues/new?template=documentation.md)

### 协作流程

```
贡献者                 管理员 (@ahs)
   │                        │
   ├─── Fork + Clone ──────→│
   │                        │
   │─── 提交 PR ────────────→ 自动通知
   │                        │
   │                        ├─── Review 代码
   │                        │
   │←─── Review 反馈 ───────┤
   │                        │
   ├─── 修复问题 ───────────→│
   │                        │
   │                        ├─── Approve + Merge
   │                        │
   └─── 🚀 贡献完成！      │
```

### 自动化工具体系

- **Dependabot**：自动更新依赖版本（每周）
- **Self-Iterate**：每日自动检查-修复-验证代码问题
- **Playwright E2E**：每次 PR 自动运行端到端测试
- **Greeting**：新 PR/Issue 自动感谢回复

### 贡献者荣誉

查看 [CONTRIBUTORS.md](CONTRIBUTORS.md)，所有贡献者都会被记录！
