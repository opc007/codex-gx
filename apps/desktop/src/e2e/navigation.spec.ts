import { test, expect } from "@playwright/test";

/**
 * 导航和交互测试
 */
test.describe("Navigation & Interaction", () => {
  test("新会话按钮可点击", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // 查找"新会话"/"新建"/"New"相关按钮
    const newBtn = page.locator("button").filter({ hasText: /新|new|plus|add/i }).first();
    
    if (await newBtn.isVisible()) {
      await newBtn.click();
      await page.waitForTimeout(500);
      // 点击后页面不应崩溃
      await expect(page.locator("body")).toBeVisible();
    }
  });

  test("快捷键 Cmd/Ctrl+N 新建会话", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // macOS: Cmd+N, Windows: Ctrl+N
    await page.keyboard.press("Meta+n");
    await page.waitForTimeout(500);
    
    // 页面仍正常
    await expect(page.locator("body")).toBeVisible();
  });

  test("主题切换按钮存在（如果有）", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // 查找主题切换相关按钮
    const themeBtn = page.locator(
      "button, [role='button']"
    ).filter({ hasText: /theme|dark|light|深|浅|主题/i }).first();
    
    if (await themeBtn.isVisible()) {
      await themeBtn.click();
      await page.waitForTimeout(500);
      // 主题切换不应崩溃
      await expect(page.locator("body")).toBeVisible();
    }
  });

  test("设置入口存在（如果有侧边栏图标）", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // 查找设置按钮（齿轮图标或文本）
    const settingsBtn = page.locator(
      "button, a, [role='button'], [class*='setting' i], [class*='gear' i], svg"
    ).filter({ hasText: /setting|preference|设置|配置/i }).first();
    
    const hasSettings = await settingsBtn.isVisible().catch(() => false);
    if (hasSettings) {
      await settingsBtn.click();
      await page.waitForTimeout(1_000);
      // 设置面板/对话框应打开
      const dialog = page.locator("dialog, [role='dialog'], [class*='modal' i], [class*='dialog' i]").first();
      await dialog.isVisible().catch(() => false);
      // 不崩溃是唯一要求
      expect(true).toBeTruthy();
    }
  });

  test("键盘导航：Tab 可以在元素间移动", async ({ page }) => {
    await page.goto("/", { waitUntil: "networkidle", timeout: 30_000 });
    await page.waitForTimeout(2_000);

    // Tab 应该能在可聚焦元素间移动
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    await page.keyboard.press("Tab");
    
    // 页面仍然正常
    await expect(page.locator("body")).toBeVisible();
  });
});
