# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: src/e2e/navigation.spec.ts >> Navigation & Interaction >> 快捷键 Cmd/Ctrl+N 新建会话
- Location: src/e2e/navigation.spec.ts:22:3

# Error details

```
Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
Call log:
  - navigating to "/", waiting until "networkidle"

```

# Test source

```ts
  1  | import { test, expect } from "@playwright/test";
  2  | 
  3  | /**
  4  |  * 导航和交互测试
  5  |  */
  6  | test.describe("Navigation & Interaction", () => {
  7  |   test("新会话按钮可点击", async ({ page }) => {
  8  |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  9  |     await page.waitForTimeout(2_000);
  10 | 
  11 |     // 查找"新会话"/"新建"/"New"相关按钮
  12 |     const newBtn = page.locator("button").filter({ hasText: /新|new|plus|add/i }).first();
  13 |     
  14 |     if (await newBtn.isVisible()) {
  15 |       await newBtn.click();
  16 |       await page.waitForTimeout(500);
  17 |       // 点击后页面不应崩溃
  18 |       await expect(page.locator("body")).toBeVisible();
  19 |     }
  20 |   });
  21 | 
  22 |   test("快捷键 Cmd/Ctrl+N 新建会话", async ({ page }) => {
> 23 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
     |                ^ Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
  24 |     await page.waitForTimeout(2_000);
  25 | 
  26 |     // macOS: Cmd+N, Windows: Ctrl+N
  27 |     await page.keyboard.press("Meta+n");
  28 |     await page.waitForTimeout(500);
  29 |     
  30 |     // 页面仍正常
  31 |     await expect(page.locator("body")).toBeVisible();
  32 |   });
  33 | 
  34 |   test("主题切换按钮存在（如果有）", async ({ page }) => {
  35 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  36 |     await page.waitForTimeout(2_000);
  37 | 
  38 |     // 查找主题切换相关按钮
  39 |     const themeBtn = page.locator(
  40 |       "button, [role='button']"
  41 |     ).filter({ hasText: /theme|dark|light|深|浅|主题/i }).first();
  42 |     
  43 |     if (await themeBtn.isVisible()) {
  44 |       await themeBtn.click();
  45 |       await page.waitForTimeout(500);
  46 |       // 主题切换不应崩溃
  47 |       await expect(page.locator("body")).toBeVisible();
  48 |     }
  49 |   });
  50 | 
  51 |   test("设置入口存在（如果有侧边栏图标）", async ({ page }) => {
  52 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  53 |     await page.waitForTimeout(2_000);
  54 | 
  55 |     // 查找设置按钮（齿轮图标或文本）
  56 |     const settingsBtn = page.locator(
  57 |       "button, a, [role='button'], [class*='setting' i], [class*='gear' i], svg"
  58 |     ).filter({ hasText: /setting|preference|设置|配置/i }).first();
  59 |     
  60 |     const hasSettings = await settingsBtn.isVisible().catch(() => false);
  61 |     if (hasSettings) {
  62 |       await settingsBtn.click();
  63 |       await page.waitForTimeout(1_000);
  64 |       // 设置面板/对话框应打开
  65 |       const dialog = page.locator("dialog, [role='dialog'], [class*='modal' i], [class*='dialog' i]").first();
  66 |       await dialog.isVisible().catch(() => false);
  67 |       // 不崩溃是唯一要求
  68 |       expect(true).toBeTruthy();
  69 |     }
  70 |   });
  71 | 
  72 |   test("键盘导航：Tab 可以在元素间移动", async ({ page }) => {
  73 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  74 |     await page.waitForTimeout(2_000);
  75 | 
  76 |     // Tab 应该能在可聚焦元素间移动
  77 |     await page.keyboard.press("Tab");
  78 |     await page.keyboard.press("Tab");
  79 |     await page.keyboard.press("Tab");
  80 |     
  81 |     // 页面仍然正常
  82 |     await expect(page.locator("body")).toBeVisible();
  83 |   });
  84 | });
  85 | 
```