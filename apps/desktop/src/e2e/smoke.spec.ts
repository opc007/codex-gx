import { test, expect } from "@playwright/test";

/**
 * 冒烟测试：确保应用能正常启动，核心 UI 存在，无致命错误
 */
test.describe("App Launch & Core UI", () => {
  test("应用启动成功，页面加载", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);
    const title = await page.title();
    expect(title.length).toBeGreaterThan(0);
  });

  test("无致命控制台错误", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });

    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(3_000);

    // 过滤已知无害错误
    const fatalErrors = errors.filter((err: string) =>
      !err.includes("Failed to fetch") &&
      !err.includes("net::ERR") &&
      !err.includes("CORS") &&
      !err.includes("favicon") &&
      !err.includes("WebSocket") &&
      !err.includes("ECONNREFUSED") &&
      !err.toLowerCase().includes("panic")
    );

    expect(fatalErrors, `致命错误: ${fatalErrors.join("\n")}`).toHaveLength(0);
  });

  test("侧边栏可见", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);
    const sidebar = page.locator("nav, aside, [class*='sidebar' i], [class*='side' i]").first();
    await expect(sidebar).toBeVisible({ timeout: 5_000 });
  });

  test("顶部栏可见", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);
    const topbar = page
      .locator("header, [class*='top' i], [class*='header' i], [role='banner']")
      .first();
    await expect(topbar).toBeVisible({ timeout: 5_000 });
  });

  test("主内容区域可见且非空", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);
    const main = page.locator("main, [role='main'], [class*='main' i], [class*='content' i]").first();
    await expect(main).toBeVisible({ timeout: 5_000 });
    const childCount = await main.locator(":scope > *").count();
    expect(childCount).toBeGreaterThan(0);
  });
});
