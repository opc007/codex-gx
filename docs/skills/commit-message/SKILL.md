---
name: commit-message
description: |
  根据 git diff 写 conventional commit message。
  type(scope): subject + body + footer。
triggers:
  - "write commit msg"
  - "commit message"
  - "/commit-msg"
author: Codex gx
version: "1.7.0"
trust: trusted
---

# Commit Message Skill

## When to use
User has staged or unstaged changes and asks to write a commit message.

## What to do
1. Run `git diff --staged` (or `git diff` if no staged)
2. Identify type: feat | fix | refactor | docs | test | chore | perf | style
3. Identify scope: file or feature area
4. Write subject line ≤ 72 chars, lowercase, no period
5. Add body if change is non-trivial (why, not what)
6. Add `BREAKING CHANGE:` footer if applicable

## Format
```
<type>(<scope>): <subject>

<body>

<footer>
```

## Example
```
feat(chat): add streaming SSE parser

- handle chunked event-source
- support cancel token via AbortController

Closes #123
```
