import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test"

import { resolveAccount, type LoginIdentity } from "./accounts"
import { headedContextOptions, maximizePageIfHeaded } from "./headed"

export type { LoginIdentity }

export type LoggedInSession = {
    context: BrowserContext
    page: Page
}

const LOGIN_TIMEOUT = 40_000

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
    await maximizePageIfHeaded(page)
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
    // 后端登录限流为每账号每 60 秒 5 次；多会话流程可能撞限，等待窗口滑过后重试。
    for (let attempt = 0; ; attempt += 1) {
        try {
            await page.waitForURL((url) => !url.pathname.startsWith("/login"), {
                timeout: LOGIN_TIMEOUT,
            })
            break
        } catch (error) {
            if (await loginError.isVisible().catch(() => false)) {
                const detail = (await loginError.textContent())?.trim() ?? "无法登录"
                if (detail.includes("频繁") && attempt < 2) {
                    await page.waitForTimeout(35_000)
                    await page.locator("#governance-auth-login-submit").click()
                    continue
                }
                throw new Error(`UI 登录失败 (${cred.account}): ${detail}`)
            }
            throw error
        }
    }

    const workspace = page.getByRole("heading", { name: "我的工作台" })
    try {
        await expect(workspace).toBeVisible({ timeout: LOGIN_TIMEOUT })
    } catch {
        await page.goto("/workspace")
        await expect(workspace).toBeVisible({ timeout: LOGIN_TIMEOUT })
    }
}

const sessionPool = new WeakMap<Browser, Map<string, LoggedInSession>>()

async function openWorkspace(page: Page): Promise<void> {
    if (!page.url().includes("/workspace")) {
        await page.goto("/workspace")
    }
    const workspace = page.getByRole("heading", { name: "我的工作台" })
    try {
        await expect(workspace).toBeVisible({ timeout: LOGIN_TIMEOUT })
    } catch {
        await page.goto("/workspace")
        await expect(workspace).toBeVisible({ timeout: LOGIN_TIMEOUT })
    }
}

/**
 * 按账号复用已登录的 BrowserContext。同一 Browser 内同岗位只登录一次；
 * 调用方 `context.close()` 只回到工作台，不销毁会话，避免切角色时重复登录和限流。
 */
export async function newLoggedInContext(
    browser: Browser,
    identity: LoginIdentity,
    password?: string,
): Promise<LoggedInSession> {
    const cred = resolveAccount(identity, password)
    let pool = sessionPool.get(browser)
    if (!pool) {
        pool = new Map()
        sessionPool.set(browser, pool)
    }
    const cached = pool.get(cred.account)
    if (cached && !cached.page.isClosed()) {
        await maximizePageIfHeaded(cached.page)
        if (/\/login(?:\?|$)/.test(cached.page.url())) {
            await loginViaUi(cached.page, cred)
        } else {
            await openWorkspace(cached.page)
        }
        return cached
    }

    const context = await browser.newContext(headedContextOptions())
    const page = await context.newPage()
    await maximizePageIfHeaded(page)
    await loginViaUi(page, cred)
    const session: LoggedInSession = { context, page }
    const realClose = context.close.bind(context)
    context.close = (async () => {
        if (!page.isClosed()) {
            await page.goto("/workspace").catch(() => undefined)
        }
    }) as BrowserContext["close"]
    context.on("close", () => {
        pool.delete(cred.account)
        context.close = realClose
    })
    pool.set(cred.account, session)
    return session
}
