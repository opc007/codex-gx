---
name: pr-review
description: |
  对当前 PR 跑 6 维度 code review（readability / security / performance /
  tests / naming / consistency）。激活时需有未合并的 PR 或 working tree 改动。
triggers:
  - "review this PR"
  - "check my code"
  - "/review-pr"
author: Codex gx
version: "1.7.0"
trust: trusted
---

# PR Review Skill

## When to use
User has uncommitted changes or an open PR and asks for review.

## What to do
1. Run `git diff main...HEAD` (or staged if no commits)
2. For each file, score 0-5 on: readability, security, performance, tests
3. Output in this format: `[file]:line — issue — suggested fix`
4. End with: ✅ LGTM / ⚠️ Has N issues / ❌ Block on N issues

## Tone
- 直接说"这个不行"，不绕弯
- 给可执行 fix（不只是描述问题）
