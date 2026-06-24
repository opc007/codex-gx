# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: src/e2e/settings.spec.ts >> Settings & API Keys >> API Key 设置页面可打开
- Location: src/e2e/settings.spec.ts:7:3

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
  4  |  * 设置/API Key 配置测试
  5  |  */
  6  | test.describe("Settings & API Keys", () => {
  7  |   test("API Key 设置页面可打开", async ({ page }) => {
> 8  |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
     |                ^ Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
  9  |     await page.waitForTimeout(2_000);
  10 | 
  11 |     const apiKeyLink = page
  12 |       .locator("button, a, [role='button']")
  13 |       .filter({ hasText: /api.key|apikey|key.*设置|provider|模型/i })
  14 |       .first();
  15 | 
  16 |     const hasLink = await apiKeyLink.isVisible().catch(() => false);
  17 | 
  18 |     if (!hasLink) {
  19 |       test.skip();
  20 |       return;
  21 |     }
  22 | 
  23 |     await apiKeyLink.click();
  24 |     await page.waitForTimeout(1_000);
  25 | 
  26 |     const panel = page
  27 |       .locator("dialog, [role='dialog'], [class*='panel' i], [class*='drawer' i]")
  28 |       .first();
  29 | 
  30 |     await expect(panel).toBeVisible({ timeout: 5_000 });
  31 |   });
  32 | 
  33 |   test("Provider 选择器存在", async ({ page }) => {
  34 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  35 |     await page.waitForTimeout(2_000);
  36 | 
  37 |     const providerSelect = page
  38 |       .locator("select, [role='combobox'], [class*='provider' i], [class*='model' i]")
  39 |       .first();
  40 | 
  41 |     const hasSelect = await providerSelect.isVisible().catch(() => false);
  42 |     if (!hasSelect) {
  43 |       test.skip();
  44 |       return;
  45 |     }
  46 | 
  47 |     await expect(providerSelect).toBeVisible();
  48 |   });
  49 | 
  50 |   test("输入无效 API Key 不崩溃", async ({ page }) => {
  51 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  52 |     await page.waitForTimeout(2_000);
  53 | 
  54 |     const inputs = page.locator("input[type='text'], input[type='password']");
  55 |     const count = await inputs.count();
  56 | 
  57 |     if (count === 0) {
  58 |       test.skip();
  59 |       return;
  60 |     }
  61 | 
  62 |     await inputs.first().fill("sk-invalid-test-key");
  63 |     await page.waitForTimeout(500);
  64 | 
  65 |     await expect(page.locator("body")).toBeVisible();
  66 |   });
  67 | });
  68 | 
```