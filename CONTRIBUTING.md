# 🤝 贡献指南

欢迎参与 **AgentShell** 开发！无论是 bug 报告、功能请求、代码 PR、文档改进还是翻译，都非常感谢。

## 📋 目录

- [行为准则](#-行为准则)
- [我该贡献什么？](#-我该贡献什么)
- [Bug 报告](#-bug-报告)
- [功能请求](#-功能请求)
- [Pull Request 流程](#-pull-request-流程)
- [开发环境](#-开发环境)
- [代码规范](#-代码规范)
- [提交规范](#-提交规范)
- [社区](#-社区)

## 🛡️ 行为准则

所有参与者请遵守 [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md)。简而言之：
- 友善、包容、专业
- 尊重不同观点
- 接受建设性批评
- 关注对社区最有利的事

## 💡 我该贡献什么？

**任何形式的贡献都欢迎**！当前最需要的：

| 优先级 | 类别 | 说明 |
|--------|------|------|
| 🔴 高 | 代码 PR | v0.1.0-alpha 阶段，需要 Rust + Tauri 开发者 |
| 🔴 高 | Bug 验证 | 复现并标注 [Confirmed] 标签 |
| 🟡 中 | 文档改进 | 错别字 / 表述不清 / 缺图 |
| 🟡 中 | i18n 翻译 | 英文 README / 文档翻译（v0.4+ 启动）|
| 🟢 低 | 分享案例 | 在 Discussions 分享你的使用场景 |

**新手友好的任务**：标 `good first issue` 的 issue 适合第一次贡献者。

## 🐛 Bug 报告

**使用 [Bug 报告模板](https://github.com/ahs/agentshell/issues/new?template=bug.md)**，包含：

- **环境**：OS 版本 / AgentShell 版本 / MiniMax M3 还是其他 provider
- **复现步骤**：1/2/3...
- **预期 vs 实际**
- **截图 / 日志**（**不要**贴 API key）
- **严重程度**：崩溃 / 功能失效 / 体验差

## ✨ 功能请求

**使用 [功能请求模板](https://github.com/ahs/agentshell/issues/new?template=feature.md)**，包含：

- **场景**：你用 AgentShell 做什么时遇到这个问题
- **建议方案**：你希望怎么解决
- **替代方案**：考虑过的其他方案
- **影响范围**：影响哪些 v0.x 版本的 milestone

> **重要**：AgentShell 严格按 [开发文档](docs/开发文档.md) 路线图开发。**未经设计的功能**会先在 [Discussions](https://github.com/ahs/agentshell/discussions) 讨论，再决定是否合并到路线图。

## 🔧 Pull Request 流程

### 1. Fork & Clone

```bash
# Fork 本仓库（GitHub 网页上点 Fork 按钮）
git clone https://github.com/<你的用户名>/agentshell.git
cd agentshell
git remote add upstream https://github.com/ahs/agentshell.git
```

### 2. 创建分支

```bash
git checkout -b feat/your-feature-name
# 或
git checkout -b fix/issue-123
```

**命名规范**：
- `feat/...` — 新功能
- `fix/...` — Bug 修复
- `docs/...` — 文档
- `refactor/...` — 重构
- `test/...` — 测试
- `chore/...` — 杂项

### 3. 提交代码

```bash
# 写代码
# ...

# 跑测试 + lint
cargo test --workspace
cargo clippy --all-targets --all-features -- -D warnings
npm run lint

# 提交
git add .
git commit -m "feat: add 5.36 theme picker"
```

### 4. 推送 + 创建 PR

```bash
git push origin feat/your-feature-name
# 然后到 GitHub 网页点 "Compare & pull request"
```

### 5. PR 描述模板

```markdown
## 关联 Issue
Closes #123

## 改动内容
- 新增 X
- 修改 Y
- 删除 Z

## 测试
- [ ] 单元测试
- [ ] 集成测试
- [ ] 手动测试（macOS / Windows）

## 截图（如适用）
（截图）

## Checklist
- [ ] 代码跑过 `cargo test`
- [ ] 代码跑过 `cargo clippy`
- [ ] 文档同步更新
- [ ] CHANGELOG.md 加 entry
```

### 6. Code Review

- 维护者会在 1-3 个工作日内 review
- 必要时反复讨论修改
- 合并需要 **至少 1 个** LGTM（looks good to me）

## 🛠️ 开发环境

### 前置依赖

- **Rust**：1.85+（`rustup install stable`）
- **Node.js**：20+（推荐用 nvm）
- **npm**：内置（推荐直接使用）
- **Tauri 前置**：
  - macOS：`xcode-select --install`
  - Windows：Microsoft Visual Studio C++ Build Tools + WebView2

### 初始化

```bash
git clone https://github.com/<你的用户名>/agentshell.git
cd agentshell
npm install
cargo build --workspace
```

### 跑起来

```bash
# 桌面端（开发模式）
npm run tauri dev

# 跑测试
cargo test --workspace
npm test

# 跑 lint
cargo clippy --all-targets --all-features -- -D warnings
npm run lint

# 构建 release
npm run tauri build
```

## 📐 代码规范

### Rust

- **Lints**：`cargo clippy --all-targets --all-features -- -D warnings` 必须通过
- **Format**：`cargo fmt --all` 必须通过（CI 会卡）
- **错误处理**：用 `thiserror` 定义错误类型，避免 `unwrap()` 在生产代码
- **Async**：优先用 `tokio`，避免阻塞主 thread
- **依赖**：新增依赖前在 PR 描述里说明理由

### TypeScript / React

- **ESLint + Prettier**：`npm run lint` 必须通过
- **组件**：函数式 + Hooks，避免 class component
- **TypeScript**：`strict: true`，避免 `any`
- **命名**：组件 PascalCase / 函数 camelCase / 常量 UPPER_SNAKE_CASE

### 文档

- 中文为主（国内用户为主），英文 README + 关键章节双语
- 章节加锚点（`## §1.2 标题 {#anchor}`），方便 cross-reference
- 表格、代码块、emoji 适度用，提高可读性

## 📝 提交规范

用 [Conventional Commits](https://www.conventionalcommits.org/)：

```
<type>(<scope>): <subject>

<body>

<footer>
```

**Type**：
- `feat` — 新功能
- `fix` — Bug 修复
- `docs` — 文档
- `style` — 格式（不影响代码运行）
- `refactor` — 重构
- `test` — 测试
- `chore` — 杂项
- `perf` — 性能优化

**示例**：

```
feat(computer-use): add macOS AXUIElement element index

- Implement desktop_get_element_index tool
- Support element_index format "1.2.5" (dot-separated path)
- Regenerate index per turn
- Reference: docs/开发文档.md §5.26.5

Closes #45
```

## 💬 社区

- **GitHub Discussions**：[问答 / 分享 / 提案](https://github.com/ahs/agentshell/discussions)
- **GitHub Issues**：[bug / feature](https://github.com/ahs/agentshell/issues)
- **微信公众号**：搜索 `AgentShell 社区`（即将开通）

## 📜 许可证

贡献的代码采用 **MIT 许可证**（与本项目一致）。提交 PR 即表示同意。

---

**再次感谢你的贡献！** 🎉
