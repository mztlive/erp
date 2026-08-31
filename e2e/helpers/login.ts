import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test"

import { resolveAccount, type LoginIdentity } from "./accounts"

export type { LoginIdentity }

export type LoggedInSession = {
    context: BrowserContext
    page: Page
}

const LOGIN_TIMEOUT = 20_000

/**
 * 用登录页账号密码登录当前 page。
 * 第二参可以是登录名、角色键或 { account, password }；第三参覆盖密码。
 */
export async function loginViaUi(
    page: Page,
    identity: LoginIdentity,
    password?: string,
): Promise<void> {
    const cred = resolveAccount(identity, password)
    if (!/\/login(?:\?|$)/.test(page.url())) {
        await page.goto("/login")
    }

    const accountInput = page.locator("#governance-auth-login-account")
    const alreadyIn =
        !(await accountInput.isVisible().catch(() => false)) &&
        !/\/login(?:\?|$)/.test(page.url())
    if (alreadyIn) {
        return
    }

    await expect(accountInput).toBeVisible({ timeout: LOGIN_TIMEOUT })
    await accountInput.fill(cred.account)
    await page.locator("#governance-auth-login-password").fill(cred.password)
    await page.locator("#governance-auth-login-submit").click()

    const loginError = page.getByRole("alert").filter({ hasText: "无法登录" })
    try {
        await page.waitForURL((url) => !url.pathname.startsWith("/login"), {
            timeout: LOGIN_TIMEOUT,
        })
    } catch (error) {
        if (await loginError.isVisible().catch(() => false)) {
            const detail = (await loginError.textContent())?.trim() ?? "无法登录"
            throw new Error(`UI 登录失败 (${cred.account}): ${detail}`)
        }
        throw error
    }

    await expect(page.getByRole("heading", { name: "我的工作台" })).toBeVisible({
        timeout: LOGIN_TIMEOUT,
    })
}

/**
 * 新开独立 BrowserContext 并完成 UI 登录。每个岗位必须用独立 context，避免 cookie/token 串号。
 */
export async function newLoggedInContext(
    browser: Browser,
    identity: LoginIdentity,
): Promise<LoggedInSession> {
    const context = await browser.newContext()
    const page = await context.newPage()
    await loginViaUi(page, identity)
    return { context, page }
}
