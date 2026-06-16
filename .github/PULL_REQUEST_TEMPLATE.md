## 关联 Issue

- Closes #（issue 编号）
- Related to #（issue 编号）

## 📋 改动内容

简要说明改动：

- 新增 X
- 修改 Y
- 删除 Z

## 🎯 影响范围

- 影响的版本（v0.1.0 / v0.2.0 / 全部）
- 影响的模块（agent-core / provider / patch / mcp / UI / 文档）
- 是否破坏向后兼容

## 📸 截图 / 录屏（如适用）

（截图）

## ✅ Checklist

### 代码
- [ ] 代码跑过 `cargo test --workspace`
- [ ] 代码跑过 `cargo clippy --all-targets --all-features -- -D warnings`
- [ ] 前端跑过 `pnpm lint` + `pnpm test`
- [ ] 没有引入新的 `unwrap()` / `panic!()` 在生产代码
- [ ] 没有引入不必要的依赖

### 文档
- [ ] docs/开发文档.md 同步更新（如涉及设计）
- [ ] CHANGELOG.md 加 entry
- [ ] README.md 更新（如涉及用户可见改动）
- [ ] inline 注释解释**为什么**而非**做什么**

### 测试
- [ ] 单元测试覆盖新代码路径
- [ ] 集成测试（如适用）
- [ ] 手动测试：macOS / Windows

### 安全
- [ ] 没有引入新的安全风险
- [ ] API key / 用户数据没有 hardcode

## 🧪 测试步骤

告诉 reviewer 怎么验证：

1. 跑 `pnpm tauri dev`
2. 打开 Settings
3. 点击 X
4. 应该看到 Y

## 📚 参考

- 开发文档章节：§X.Y
- 相关 Codex / Claude Code / Cursor 实现：link
- 性能 / 安全 / 兼容性考量：...
