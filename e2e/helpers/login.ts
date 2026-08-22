import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test"
import { ACCOUNTS, AccountKey } from "./accounts"

/**
 * 通过登录页 UI 登录（真实走表单提交，覆盖认证流程）。
 * 登录成功后跳离 /login。
 */
export async function loginViaUi(page: Page, accountKey: AccountKey): Promise<void> {
    const acc = ACCOUNTS[accountKey]
    await page.goto("/login")
    await page.getByPlaceholder("请输入账号").fill(acc.account)
    await page.getByPlaceholder("请输入密码").fill(acc.password)
    await page.getByRole("button", { name: "登录" }).click()
    await expect(page).not.toHaveURL(/\/login/, { timeout: 20_000 })
    // 登录后落在工作台或 returnTo；等待侧栏出现表示会话就绪
    await expect(page.locator("aside, nav").first()).toBeVisible({ timeout: 20_000 }).catch(async () => {
        // 兼容无侧栏布局：至少等待页面非登录错误态
        await page.waitForLoadState("networkidle").catch(() => {})
    })
}

/**
 * 创建已登录指定账号的独立 browser context（用于流程中途切换账号）。
 * 每个账号独立 context，避免 localStorage token 互相覆盖。
 */
export async function newLoggedInContext(
    browser: Browser,
    accountKey: AccountKey,
): Promise<{ context: BrowserContext; page: Page }> {
    if (!browser) throw new Error("newLoggedInContext 需要 browser 实例")
    // 关闭固定 viewport，让独立账号窗口继承 Chromium 最大化后的可用尺寸。
    const context = await browser.newContext({ viewport: null })
    const page = await context.newPage()
    await loginViaUi(page, accountKey)
    return { context, page }
}
