import { expect, type Page } from "@playwright/test"

/** 流程断言默认超时，与 playwright.config expect.timeout 对齐。 */
export const UI_TIMEOUT = 20_000

/**
 * 等待 sonner/shadcn toast 标题出现。
 */
export async function expectToast(
    page: Page,
    title: string | RegExp,
): Promise<void> {
    const toast = page
        .locator('[data-slot="toast-title"]')
        .filter({ hasText: title })
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT })
}

/**
 * 确认已进入 W01 我的工作台。
 */
export async function expectWorkspace(page: Page): Promise<void> {
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}
