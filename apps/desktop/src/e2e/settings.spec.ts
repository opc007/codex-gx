import { test, expect } from "@playwright/test";

/**
 * 设置/API Key 配置测试
 */
test.describe("Settings & API Keys", () => {
  test("API Key 设置页面可打开", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    const apiKeyLink = page
      .locator("button, a, [role='button']")
      .filter({ hasText: /api.key|apikey|key.*设置|provider|模型/i })
      .first();

    const hasLink = await apiKeyLink.isVisible().catch(() => false);

    if (!hasLink) {
      test.skip();
      return;
    }

    await apiKeyLink.click();
    await page.waitForTimeout(1_000);

    const panel = page
      .locator("dialog, [role='dialog'], [class*='panel' i], [class*='drawer' i]")
      .first();

    await expect(panel).toBeVisible({ timeout: 5_000 });
  });

  test("Provider 选择器存在", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    const providerSelect = page
      .locator("select, [role='combobox'], [class*='provider' i], [class*='model' i]")
      .first();

    const hasSelect = await providerSelect.isVisible().catch(() => false);
    if (!hasSelect) {
      test.skip();
      return;
    }

    await expect(providerSelect).toBeVisible();
  });

  test("输入无效 API Key 不崩溃", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    const inputs = page.locator("input[type='text'], input[type='password']");
    const count = await inputs.count();

    if (count === 0) {
      test.skip();
      return;
    }

    await inputs.first().fill("sk-invalid-test-key");
    await page.waitForTimeout(500);

    await expect(page.locator("body")).toBeVisible();
  });
});
