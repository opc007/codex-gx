---
name: openai-docs
description: |
  检索 OpenAI 官方文档（platform.openai.com/docs）。
  用户问 OpenAI API 用法时自动激活。
triggers:
  - "openai docs say"
  - "openai documentation"
  - "openai API"
author: Codex gx
version: "1.7.0"
trust: trusted
---

# OpenAI Docs Skill

## When to use
User asks about OpenAI platform APIs, models, or features.

## What to do
1. Use `webfetch` tool to fetch https://platform.openai.com/docs/...
2. For Chat Completions: https://platform.openai.com/docs/api-reference/chat
3. For Assistants: https://platform.openai.com/docs/api-reference/assistants
4. For Realtime: https://platform.openai.com/docs/api-reference/realtime
5. For new features, check the Changelog: https://platform.openai.com/docs/changelog
6. Quote the exact API name and parameter; do not paraphrase

## Tone
- 严格按官方文档回答
- 引用具体的 URL 路径
- 不确定时明确说"未在官方文档中确认"
