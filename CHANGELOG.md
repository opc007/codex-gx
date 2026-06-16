# Changelog

所有 **显著的** 变更都会记录在此文件。格式基于 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，
本项目遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

### 计划（v0.1.0-alpha 目标）
- 多 provider 适配（M3 / Claude / GPT / DeepSeek）
- Computer Use 浏览器层（Playwright JS REPL）
- 5.36 主题系统（白天/夜晚/跟随）
- 5.37A Unified Mentions（文件 picker）
- 5.40 Thinking 折叠
- 5.5.8 三种审批模式（Auto/Read-only/Full Access）
- 5.4.1 API key 加密存储（框架先建）
- 13.6 License 激活码（4 档 SKU）
- Slash 命令 17+（P1 子集）

---

## [文档变更历史]

代码未上线前，文档版本与代码版本**分离**。下面是开发文档的版本历史（与代码版本**不**绑定）：

### v1.9.4 — 2026-06-16
**Codex 0.141.0-alpha.1 + M3 官方公告深挖**
- 1 项重大事实修正（R39）：5.18/5.19 图像/视频生成是 MiniMax 平台独立 API（不是 M3 模型能力）
- 附录 A.0 M3 完整事实档案（MSA / 价格 $0.60/$2.40 / Token Plan / Open-weights / 9 项 benchmark）
- Codex 0.141 6 大新功能：5.37A Unified Mentions / 5.22A Goal Automations / 5.32 /usage 三视图 / 5.11 /delete / 5.11 /import / 5.4.1 Managed Bedrock
- 6.14 桌面独有 4 节：Voice Input Ctrl+M / Appshots / Floating pop-out / GUI 调度器
- 附录 C 21 维 5 竞品对比表
- 风险表扩到 R42
- 7700+ 行

### v1.9.3 — 2026-06-16
**Codex UX 小功能大补齐**
- 8 大新章节：5.36 主题 / 5.37 @ 搜索 / 5.38 Tab 队列 / 5.39 ! shell / 5.40 Thinking 折叠 / 5.41 Vim / 5.42 IDE / 5.43 /diff
- 5.5.8 三种审批模式
- 2 个新页面：/appearance /keymap
- 风险 R38
- 7100+ 行

### v1.9.2 — 2026-06-16
**Codex 0.137.0 / 26.527 全功能补齐**
- 7 大新章节：5.29 Pocket / 5.30 Mobile Remote / 5.31 Plugin Marketplace / 5.32 Profiles / 5.33 App-Server / 5.34 Execpolicy / 5.35 Sandbox CLI
- 3 个新页面：/pocket /profile /plugins/audit
- 风险 R32-R37
- 6700+ 行

### v1.9.1 — 2026-06-16
**v1.9 关键事实校正 8 处**
- M3 协议 = OpenAI Chat Completions + tool_calls
- M3 坐标 = 0.0-1.0 float（不是 0-1000 整数）
- M3 工具 4 域 60+（不是 8 个）
- Windows 截图 = WGC（不是 DXGI）
- macOS 权限 = CGPreflightScreenCaptureAccess

### v1.9 — 2026-06-16
**Computer Use 桌面 CUA 完整设计**
- 5.26 桌面 CUA 4 域 60+ tool
- 5.27 截图 + 相对坐标协议
- 5.28 App 白名单 + 权限系统
- 6.13 Desktop Task 面板
- 5900+ 行

### v1.8 — 2026-06-16
**GUI 自动化能力边界明确**
- 5.10.7 7 个 "不做" 场景
- R31 半成品报告
- 4800+ 行

### v1.7 — 2026-06-16
**Codex 2026 全功能对齐**
- 18 个新 slash 命令
- 5.20 Personality / 5.21 Skills / 5.22 Goal
- 5.23 Headless / 5.24 Background / 5.25 Fork+Side
- 9.5 GDPR / 9.6 a11y
- 风险 R25-R30
- 4500+ 行

### v1.6 — 2026-06-16
**商业化：激活码（4 档 SKU）**
- 13.6 License 商业化
- 6.12 License 管理页
- 月卡 ¥9.9 / 季卡 ¥29.9 / 年卡 ¥99 / 终身 ¥299
- 风险 R23-R24

### v1.5 — 2026-06-16
**M3 独家能力 — 多模态生成**
- 5.18 generate_image（MiniMax 平台 API）
- 5.19 generate_video（MiniMax 平台 API）
- 6.10 Media Generator 面板
- 6.11 Gallery

### v1.4 — 2026-06-16
**Codex 功能对齐补全（10 个新功能）**
- 5.9.2 Memory / 5.11 Slash / 5.12 Statusline / 5.13 PR Review
- 5.14 Hook / 5.15 Service Tier / 5.16 Quick Chat / 5.17 App 锁
- 6.2 页面从 4 扩到 13 / 6.7 浮动窗口 / 6.8 内置浏览器 / 6.9 多文件预览

### v1.3 — 2026-06-16
**Computer Use 极简化重构**
- 5.10 Computer Use（Playwright JS REPL）
- 6.5 WindowShot / 6.6 CLI ↔ Desktop Handoff

### v1.2 — 2026-06-16
**加速 Computer Use 路径**

### v1.1 — 2026-06-16
**重写 5.5 权限 / 5.6 会话 / 5.7 Plan / 5.8 撤销重做 / 5.9 多 Agent**
- 8.5 Context Compaction / 8.6 Progressive Disclosure / 8.7 Tool Schema 成本控制
- 16 项风险 R1-R16 / 附录 D 分档 backlog

### v1.0 — 2026-06-15
**初始版本**
- 产品定位 / 对标 Codex 功能清单 / 3 大产品特性
- 技术栈 / Monorepo 结构 / 配置文件结构
- 11 个 Phase 任务清单

---

完整 7700+ 行设计文档见 [docs/开发文档.md](docs/开发文档.md)。
