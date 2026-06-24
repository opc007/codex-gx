# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: src/e2e/license.spec.ts >> Community Free Version >> License 面板显示社区版信息
- Location: src/e2e/license.spec.ts:21:3

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
  4  |  * License/社区版测试 — v2.0 永久免费
  5  |  */
  6  | test.describe("Community Free Version", () => {
  7  |   test("应用直接启动，无激活阻塞", async ({ page }) => {
  8  |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  9  |     await page.waitForTimeout(2_000);
  10 | 
  11 |     // v2.0 永久免费，不应显示任何激活界面
  12 |     const activationCard = page.locator(".activation-gate, .activation-card").first();
  13 |     const hasActivation = await activationCard.isVisible().catch(() => false);
  14 |     expect(hasActivation).toBe(false);
  15 | 
  16 |     // 应该能看到主界面
  17 |     const mainContent = page.locator("main, [role='main'], .main-pane, [class*='main']").first();
  18 |     await expect(mainContent).toBeVisible({ timeout: 5_000 });
  19 |   });
  20 | 
  21 |   test("License 面板显示社区版信息", async ({ page }) => {
> 22 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
     |                ^ Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
  23 |     await page.waitForTimeout(2_000);
  24 | 
  25 |     // 查找 License 菜单入口
  26 |     const licenseBtn = page.locator(
  27 |       "button, [role='button'], [class*='menu']"
  28 |     ).filter({ hasText: /license|授权|社区/i }).first();
  29 | 
  30 |     const hasLicenseBtn = await licenseBtn.isVisible().catch(() => false);
  31 |     // 如果有入口就点击检查
  32 |     if (hasLicenseBtn) {
  33 |       await licenseBtn.click();
  34 |       await page.waitForTimeout(1_000);
  35 |       const panel = page.locator(".license-panel, dialog").first();
  36 |       await expect(panel).toBeVisible({ timeout: 5_000 });
  37 |     }
  38 |   });
  39 | });
  40 | 
```