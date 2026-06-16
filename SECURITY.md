# 🔒 安全策略

## 报告漏洞

**请不要**在公开 GitHub Issues 报告安全漏洞。

请通过以下方式私下报告：

- **邮箱**：`security@agentshell.dev`（即将开通）
- **GitHub Security Advisories**：[新建 advisory](https://github.com/ahs/agentshell/security/advisories/new)

**报告内容请包含**：
- 漏洞描述 + 复现步骤
- 影响范围（哪些 v0.x 版本受影响）
- 严重程度评估（critical / high / medium / low）
- 概念验证（PoC）代码 / 截图

## 响应时间

| 严重程度 | 首次响应 | 修复目标 |
|----------|----------|----------|
| **Critical** | 24h | 7 天 |
| **High** | 48h | 30 天 |
| **Medium** | 7 天 | 90 天 |
| **Low** | 14 天 | 下一个 minor 版本 |

## 支持的版本

| 版本 | 支持状态 |
|------|----------|
| 最新 minor | ✅ 完整支持 |
| 前一个 minor | ⚠️ 仅安全更新 |
| 更早 | ❌ 不支持 |

## 安全最佳实践（给使用者）

- **不要**把 API key 提交到 Git 仓库
- **不要**分享你的 License 激活码（13.6 商业化，1 设备 1 码）
- **使用** `~/.agentshell/` 目录（已 gitignored）
- **使用** macOS Keychain / Windows Credential Manager 加密 API key（v0.2.0+ 默认开启）
- **谨慎** 给 AgentShell `Full Access` 权限（5.5.8 三种模式）
- **谨慎** 启用 untrusted plugin（5.31.4 自动安全扫描）

## 已知风险

详见 [开发文档 §16 风险表](docs/开发文档.md#16-风险) — 当前 42 条已识别风险。

## 安全审计

- **v0.1.0 GA 前**会做 1 次完整安全审计（外部公司）
- **v1.0 GA 前**会做 1 次渗透测试

## 致谢

报告安全漏洞并协助修复的贡献者会在 [CHANGELOG.md](CHANGELOG.md) 致谢（除非要求匿名）。

---

**保护用户数据是 AgentShell 的第一优先级。**
