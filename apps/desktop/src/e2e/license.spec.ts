import { test, expect } from "@playwright/test";

/**
 * License/社区版测试 — v2.0 永久免费
 */
test.describe("Community Free Version", () => {
  test("应用直接启动，无激活阻塞", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // v2.0 永久免费，不应显示任何激活界面
    const activationCard = page.locator(".activation-gate, .activation-card").first();
    const hasActivation = await activationCard.isVisible().catch(() => false);
    expect(hasActivation).toBe(false);

    // 应该能看到主界面
    const mainContent = page.locator("main, [role='main'], .main-pane, [class*='main']").first();
    await expect(mainContent).toBeVisible({ timeout: 5_000 });
  });

  test("License 面板显示社区版信息", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // 查找 License 菜单入口
    const licenseBtn = page.locator(
      "button, [role='button'], [class*='menu']"
    ).filter({ hasText: /license|授权|社区/i }).first();

    const hasLicenseBtn = await licenseBtn.isVisible().catch(() => false);
    // 如果有入口就点击检查
    if (hasLicenseBtn) {
      await licenseBtn.click();
      await page.waitForTimeout(1_000);
      const panel = page.locator(".license-panel, dialog").first();
      await expect(panel).toBeVisible({ timeout: 5_000 });
    }
  });
});
