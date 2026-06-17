---
name: imagegen
description: |
  调用 Codex gx 集成的 generate_image 工具生成图像。
  改写 prompt → 调 API → 内联到 thread。
triggers:
  - "make a poster"
  - "generate an image"
  - "/image"
  - "draw me"
author: Codex gx
version: "1.7.0"
trust: trusted
---

# ImageGen Skill

## When to use
User asks to create an image, poster, illustration, logo, or any visual content.

## What to do
1. Rewrite the user's casual description into a detailed English prompt:
   - subject + action + environment
   - style (photorealistic, anime, oil painting, pixel art, etc.)
   - lighting + composition + aspect ratio
2. Call `generate_image` tool with the rewritten prompt
3. Specify size: 1024x1024 (square) / 1024x1792 (portrait) / 1792x1024 (landscape)
4. If n>1, pick the best or show all
5. If user wants i2i (image-to-image), set reference_image_url

## Tone
- 创意描述要具体（"a cat" 不好，"an orange tabby cat sitting on a windowsill at golden hour" 好）
- 默认 vivid style
