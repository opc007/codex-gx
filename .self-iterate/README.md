# 🧠 自我迭代系统

本目录由 `self-iterate.yml` workflow 自动管理，记录 Codex GX 的自我进化历程。

## 目录结构

```
.self-iterate/
├── README.md              # 本文件
├── health_history.jsonl   # 每日健康度快照（趋势分析用）
└── plans/                # 每日改进计划存档
    └── plan-YYYYMMDD-HHMM.md
```

## 工作原理

### 7 阶段每日循环

```
每日 UTC 3:00 (北京时间 11:00) 自动触发
│
├─ 1️⃣ 分析 (analyze)
│   └─ 扫描代码健康度：warning 数、代码行数、依赖状态
│
├─ 2️⃣ 学习 (learn-from-history)
│   └─ 读取历史记录，分析趋势是"改善/稳定/需关注"
│
├─ 3️⃣ 规划 (plan)
│   └─ 生成改进计划，分高/中/低优先级
│
├─ 4️⃣ 实施 (auto-improve)
│   ├─ cargo fix → 清理 unused imports
│   ├─ cargo clippy --fix → 应用建议
│   ├─ cargo fmt → 格式化
│   ├─ npm audit fix → 前端依赖
│   └─ 提交改动
│
├─ 5️⃣ 验证 (verify)
│   └─ build + clippy + type-check 确保没破坏功能
│
├─ 6️⃣ 学习保存 (learn-and-persist)
│   └─ 将本次迭代写入 health_history.jsonl
│
└─ 7️⃣ 通知 (notify)
    └─ 生成 GitHub Actions 摘要报告
```

## 自动修复的能力边界

### ✅ 能自动做的
- 清理 unused imports
- 应用 clippy 建议（简单 ones）
- `cargo fmt` 格式化
- `npm audit fix`
- 补全缺失的文档注释
- `cargo update` 更新依赖版本

### ❌ 不会自动做的
- 架构重构（风险太高，需人工判断）
- 业务逻辑改动
- 性能优化（需要 profiling 数据）
- 测试覆盖率提升（需要理解业务）
- 破坏性 breaking change

## 手动触发

在 GitHub Actions 页面手动触发：
- 仓库 → Actions → **Self-Iterate** → Run workflow

或本地运行模拟：
```bash
# 本地检查当前健康度
cargo clippy --workspace --all-targets 2>&1 | grep "^warning" | wc -l

# 查看改进建议
cargo clippy --workspace --fix --dry-run

# 查看历史趋势
cat .self-iterate/health_history.jsonl
```

## 回滚

如果自动改动有问题：
```bash
git revert HEAD          # 回滚最后一次自动提交
git push origin main
```
