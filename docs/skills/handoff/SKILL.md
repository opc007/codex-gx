---
name: handoff
description: |
  把当前 session 压缩成可交接的 markdown 摘要。
  适合：换人接手、长期停工后回顾、CI 失败时给同事的 brief。
triggers:
  - "summarize this session"
  - "handoff"
  - "/handoff"
  - "write a brief"
author: Codex gx
version: "1.7.0"
trust: trusted
---

# Handoff Skill

## When to use
User wants a clean summary of the current session for handoff to another person or future-self.

## What to do
1. Walk through messages in reverse chronological order
2. Identify the main goal / task
3. List decisions made + why
4. List open questions / unresolved issues
5. List files changed (with paths)
6. List commands run that worked
7. List commands that failed + why
8. Output as markdown with sections:
   - TL;DR (1-3 sentences)
   - Goal
   - What was done
   - What's open
   - Key files
   - Next steps for the receiver

## Tone
- 简洁、技术、不带情绪
- 用第三人称（"the user" / "the assistant"）
- 不超过 1 屏
