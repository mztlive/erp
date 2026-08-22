import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test"
import { ACCOUNTS, type AccountKey } from "./accounts"

/** 前端登录态唯一存储键；必须与 erp-client/lib/api/session.ts 保持一致。 */
const TOKEN_STORAGE_KEY = "erp.token"

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
 * 创建单页面账号切换器。
 *
 * 合同：
 * - 每个账号首次使用时必须通过真实登录页登录；
 * - 后续切回账号时可以恢复首次登录取得的 token；
 * - 恢复 token 前必须先进入无登录态页面，恢复后必须整页导航；
 * - 每次切换完成后必须验证侧栏账号与单页面约束。
 */
export function createSinglePageAccountSwitcher(
    page: Page,
): (accountKey: AccountKey) => Promise<void> {
    const tokens = new Map<AccountKey, string>()
    let activeAccount: AccountKey | undefined

    return async (accountKey: AccountKey): Promise<void> => {
        if (activeAccount === accountKey) return

        // 先清除旧身份，再离开旧业务页面，禁止旧页面携带新账号 token 发起请求。
        if (page.url() !== "about:blank") {
            await page.evaluate(
                (storageKey) => localStorage.removeItem(storageKey),
                TOKEN_STORAGE_KEY,
            )
        }

        const cachedToken = tokens.get(accountKey)
        if (cachedToken) {
            await page.goto("/login")
            await page.evaluate(
                ({ storageKey, token }) => localStorage.setItem(storageKey, token),
                { storageKey: TOKEN_STORAGE_KEY, token: cachedToken },
            )
            // 整页导航销毁旧账号的 React Query 与权限菜单内存状态。
            await page.goto("/workspace")
        } else {
            await loginViaUi(page, accountKey)
            const token = await page.evaluate(
                (storageKey) => localStorage.getItem(storageKey),
                TOKEN_STORAGE_KEY,
            )
            if (!token) {
                throw new Error(`账号 ${ACCOUNTS[accountKey].account} 登录后未写入 token`)
            }
            tokens.set(accountKey, token)
        }

        await expect(page.getByRole("button", { name: "账号菜单" })).toContainText(
            ACCOUNTS[accountKey].account,
            { timeout: 20_000 },
        )
        expect(page.context().pages()).toHaveLength(1)
        activeAccount = accountKey
    }
}

/**
 * 创建已登录指定账号的独立 browser context。
 * 仅供权限/会话隔离探针使用；正式 flow-* 业务脚本必须使用单页面账号切换器。
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
