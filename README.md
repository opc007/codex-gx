# Codex gx

> **Codex 平替桌面 Agent 客户端** — Tauri 2 + Rust，macOS / Windows 双端
>
> v1.6 商业化版本

## ✨ 特性

- 🧠 **多 Provider** — M3 / DeepSeek / OpenAI / Claude / Ollama / Llama.cpp
- 🛠 **15+ 内置工具** — Read / Edit / Write / APPLY_PATCH / Bash / Grep / Glob / WebFetch / Git / PR Review …
- 📚 **Skills 库** — 12 个内置模板（shell / prompt / chain 三种执行模式）
- 🔊 **TTS** — 跨平台语音播报（macOS `say` / Windows PowerShell / Linux `espeak`）
- 🕸️ **Agent Flow Graph** — Plan 任务可视化 DAG + Mermaid 导出
- ☁️ **Session Sync** — 本地 bundle 缓存 / 导入导出
- 🧩 **Plugin 热加载** — 5 个内置插件 + DSL 文本转换
- 🔐 **License 商业化** — 4 档激活码（v1.6）

## 📦 安装

### macOS

1. 在 [GitHub Releases](https://github.com/opc007/codex-gx/releases) 下载 `Codex gx_1.6.0_aarch64.dmg`
2. 双击 `.dmg`，把 **Codex gx.app** 拖到 **Applications** 文件夹
3. 打开 Applications，右键 **Codex gx.app** → 打开（首次需要确认未签名应用）
4. 第一次启动可能提示"无法打开，因为它来自身份不明的开发者"，点 **系统设置 → 隐私与安全性 → 仍要打开**

> **Apple Silicon (M1/M2/M3) 用户**：下载 `aarch64` 版本
> **Intel Mac 用户**：下载 `x64` 版本

### Windows

1. 在 [GitHub Releases](https://github.com/opc007/codex-gx/releases) 下载 `Codex gx_1.6.0_x64-setup.exe`
2. 双击运行安装器
3. 如果 SmartScreen 拦截，点 **更多信息 → 仍要运行**

### Linux

暂未提供安装包。可从源码 build（见下）。

## 🛒 License 激活

启动后默认进入 License 页面（v1.6 强制激活），4 档可选：

| 档位 | 价格 | 设备数 | 适合 |
|------|------|--------|------|
| 月卡 | ¥9.9 | 1 台 | 试用 1 个月 |
| 季卡 | ¥29.9 | 1 台 | 想试一季度再决定 |
| **年卡**（推荐） | **¥99** | 1 台 | 重度个人开发者 |
| 终身 | ¥299 | 1 台 | 一次买断 |

### 购买渠道

- 主渠道：Lemon Squeezy（国际，支持微信/支付宝/信用卡）
- 备选：微信小商店
- 备选：Gumroad（仅国际）

购买后你会收到一封邮件，里面有 Base64 字符串的激活码。

### 激活步骤

1. 启动 Codex gx → 自动弹出 License 页
2. 把激活码粘到「已有激活码？」输入框
3. 点 **立即激活** → 3 秒内完成

### 重要规则

- **一个码 = 一段时间**（月 30 天 / 季 90 天 / 年 365 天 / 终身 = 永不到期）
- **从输入激活码的那一刻起算**，到期就失效
- **到期后重新输入新码**，从新输入时间重新累计时长
- **旧码剩余时间不可合并到新码**
- **终身档用户永久免费升级软件版本**（v0.1 → v1.0 全部免费）
- **不退款、不自动续费、不升级折价**

## 🛠 从源码 Build

```bash
git clone https://github.com/opc007/codex-gx.git
cd codex-gx

# 1. 安装 Rust 工具链（≥ 1.85）
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. 安装 Node.js（≥ 20）
brew install node@20  # macOS
# 或 nvm install 20

# 3. 安装 pnpm
npm install -g pnpm

# 4. 安装 Tauri CLI
cargo install tauri-cli --version "^2"

# 5. 安装前端依赖
cd apps/desktop
pnpm install

# 6. 开发模式
cargo tauri dev

# 7. 生产 build
cargo tauri build
```

## 🧪 生成测试 License（开发用）

```bash
# 跑 license-gen 生成 demo 激活码（仅 dev 用）
cargo run --bin license-gen -- yearly
```

> ⚠️ 内部工具 — 仅用于开发 / 自测。生产 license 由服务端签发。

## 📁 目录结构

```
codex-gx/
├── apps/desktop/           # Tauri 2 桌面端
│   ├── src/                # React + TypeScript 前端
│   ├── src-tauri/          # Rust 后端
│   └── src-tauri/src/bin/  # 内部 CLI 工具
├── crates/                 # 共享 Rust 库
│   ├── agent-core/         # Agent 核心
│   ├── provider/           # LLM provider 抽象
│   ├── patch/              # apply_patch 引擎
│   ├── context/            # 上下文压缩
│   ├── mcp/                # MCP 协议
│   ├── queue/              # 任务队列
│   ├── p2p/                # 设备间通信
│   ├── license/            # License 系统（v1.6）
│   ├── memory/             # 跨 session 记忆
│   ├── voice/              # 语音
│   ├── marketplace/        # 插件市场
│   ├── vault/              # 凭证保险库
│   ├── lint/               # 代码 lint
│   ├── learning/           # 用户行为学习
│   └── plugin/             # 插件运行时
├── docs/                   # 设计文档
└── README.md
```

## ⌨️ 常用快捷键

- `Cmd/Ctrl + K` — 快速命令面板
- `Cmd/Ctrl + N` — 新建 thread
- `Cmd/Ctrl + /` — Slash 命令
- `Cmd/Ctrl + Enter` — 发送消息

## 🆘 常见问题

### 安装后打不开（macOS）

- 右键 `Codex gx.app` → **打开**（不是双击）
- 提示 "无法打开" 时，去 **系统设置 → 隐私与安全性** → 找到 Codex gx → **仍要打开**

### 激活码提示"签名验证失败"

- 码可能被截断。重新复制整行（包括末尾的 `==`）
- 码来源不对（仅支持 Codex gx 官方码）

### 激活码提示"设备不匹配"

- 一个码只能在首次激活的设备使用
- 换设备？联系客服 / 重新购买

### License 已过期

- 到期后软件进入只读模式
- 购买新码粘到 License 页激活，从新输入时间重新累计

## 📜 License

本仓库源码为 MIT License（开发用）。
发行版为商业软件 — 见 [License 商业化策略](docs/开发文档.md#136-商业化策略激活码--4-档-sku)。

## 🤝 贡献

欢迎 PR！但请先读 [开发文档](docs/开发文档.md) 了解架构。

## 🔗 链接

- [GitHub Releases](https://github.com/opc007/codex-gx/releases)
- [开发文档](docs/开发文档.md)
- [Issues](https://github.com/opc007/codex-gx/issues)
