# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: src/e2e/smoke.spec.ts >> App Launch & Core UI >> 应用启动成功，页面加载
- Location: src/e2e/smoke.spec.ts:7:3

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
  4  |  * 冒烟测试：确保应用能正常启动，核心 UI 存在，无致命错误
  5  |  */
  6  | test.describe("App Launch & Core UI", () => {
  7  |   test("应用启动成功，页面加载", async ({ page }) => {
> 8  |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
     |                ^ Error: page.goto: Protocol error (Page.navigate): Cannot navigate to invalid URL
  9  |     await page.waitForTimeout(2_000);
  10 |     const title = await page.title();
  11 |     expect(title.length).toBeGreaterThan(0);
  12 |   });
  13 | 
  14 |   test("无致命控制台错误", async ({ page }) => {
  15 |     const errors: string[] = [];
  16 |     page.on("console", (msg) => {
  17 |       if (msg.type() === "error") errors.push(msg.text());
  18 |     });
  19 | 
  20 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  21 |     await page.waitForTimeout(3_000);
  22 | 
  23 |     // 过滤已知无害错误
  24 |     const fatalErrors = errors.filter((err: string) =>
  25 |       !err.includes("Failed to fetch") &&
  26 |       !err.includes("net::ERR") &&
  27 |       !err.includes("CORS") &&
  28 |       !err.includes("favicon") &&
  29 |       !err.includes("WebSocket") &&
  30 |       !err.includes("ECONNREFUSED") &&
  31 |       !err.toLowerCase().includes("panic")
  32 |     );
  33 | 
  34 |     expect(fatalErrors, `致命错误: ${fatalErrors.join("\n")}`).toHaveLength(0);
  35 |   });
  36 | 
  37 |   test("侧边栏可见", async ({ page }) => {
  38 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  39 |     await page.waitForTimeout(2_000);
  40 |     const sidebar = page.locator("nav, aside, [class*='sidebar' i], [class*='side' i]").first();
  41 |     await expect(sidebar).toBeVisible({ timeout: 5_000 });
  42 |   });
  43 | 
  44 |   test("顶部栏可见", async ({ page }) => {
  45 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  46 |     await page.waitForTimeout(2_000);
  47 |     const topbar = page
  48 |       .locator("header, [class*='top' i], [class*='header' i], [role='banner']")
  49 |       .first();
  50 |     await expect(topbar).toBeVisible({ timeout: 5_000 });
  51 |   });
  52 | 
  53 |   test("主内容区域可见且非空", async ({ page }) => {
  54 |     await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
  55 |     await page.waitForTimeout(2_000);
  56 |     const main = page.locator("main, [role='main'], [class*='main' i], [class*='content' i]").first();
  57 |     await expect(main).toBeVisible({ timeout: 5_000 });
  58 |     const childCount = await main.locator(":scope > *").count();
  59 |     expect(childCount).toBeGreaterThan(0);
  60 |   });
  61 | });
  62 | 
```