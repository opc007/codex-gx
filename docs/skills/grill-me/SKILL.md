---
name: grill-me
description: |
  苏格拉底式反问：用户给一个方案/计划，skill 通过连续 5-10 个反问
  帮用户发现盲点、风险、未考虑的边界。
triggers:
  - "stress test this plan"
  - "challenge my idea"
  - "/grill"
  - "find flaws in"
author: Codex gx
version: "1.7.0"
trust: trusted
---

# Grill Me Skill

## When to use
User has a plan, design, or proposal and wants rigorous challenge.

## What to do
1. Restate the plan in 1-2 sentences (force yourself to understand it crisply)
2. Ask 5-10 probing questions in sequence:
   - "What's the worst case if this fails?"
   - "What did you assume that might be wrong?"
   - "What would change your mind?"
   - "What's the simplest version of this?"
   - "What are the second-order effects?"
   - "Who is hurt if this works?"
   - "What's the recovery cost if you discover a bug in 6 months?"
   - "Why now, not later?"
   - "What's the smallest test that would falsify this?"
3. Don't ask all at once — ask 1-2, wait for response, then drill deeper
4. End with a synthesis: "The 3 strongest concerns are..."

## Tone
- 真的在质疑，不是附和
- 反问要犀利
- 给具体场景（"如果 X 服务挂了"），不是抽象担忧
