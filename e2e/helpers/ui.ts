import { expect, type Page } from "@playwright/test"

/** 流程断言默认超时，与 playwright.config expect.timeout 对齐。 */
export const UI_TIMEOUT = 20_000

/**
 * 等待 sonner/shadcn toast 标题出现，确认后关闭悬浮提示，
 * 避免提示遮挡后续按钮造成偶发点击失败。
 */
export async function expectToast(
    page: Page,
    title: string | RegExp,
): Promise<void> {
    const toast = page
        .locator('[data-slot="toast-title"]')
        .filter({ hasText: title })
    await expect(toast.first()).toBeVisible({ timeout: UI_TIMEOUT })
    await dismissToasts(page)
}

/**
 * 关闭当前全部可关闭的悬浮提示。
 */
export async function dismissToasts(page: Page): Promise<void> {
    for (let i = 0; i < 5; i += 1) {
        const dismiss = page
            .locator('[data-slot="toast"]')
            .getByRole("button", { name: "Dismiss" })
            .first()
        if (!(await dismiss.count())) return
        await dismiss.click({ timeout: 5_000 }).catch(() => undefined)
    }
}

/**
 * 确认已进入 W01 我的工作台。
 */
export async function expectWorkspace(page: Page): Promise<void> {
    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: UI_TIMEOUT,
    })
}
