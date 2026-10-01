import { expect, type Browser, type BrowserContext, type Page } from "@playwright/test"

import { resolveAccount, type LoginIdentity } from "./accounts"
import { rememberApiToken } from "./api"
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
    const submit = page.locator("#governance-auth-login-submit")
    // 在点击前订阅响应。失败无需先耗尽 URL 超时；429 按服务器窗口重试。
    for (let attempt = 0; ; attempt += 1) {
        const [response] = await Promise.all([
            page.waitForResponse(
                (candidate) =>
                    candidate.request().method() === "POST" &&
                    new URL(candidate.url()).pathname === "/login",
                { timeout: LOGIN_TIMEOUT },
            ),
            submit.click(),
        ])
        const result = await response.json().catch(() => null) as
            | { success?: boolean; errorMessage?: string; data?: { token?: string } }
            | null
        if (response.ok() && result?.success !== false) {
            if (result?.data?.token) rememberApiToken(cred, result.data.token)
            break
        }
        if (response.status() === 429 && attempt < 2) {
            const seconds = Number(response.headers()["retry-after"])
            const delay = Number.isFinite(seconds) && seconds > 0
                ? Math.ceil(seconds * 1_000) + 250
                : 35_000
            await page.waitForTimeout(delay)
            continue
        }
        throw new Error(
            `UI 登录失败 (${cred.account}, HTTP ${response.status()}): ${result?.errorMessage ?? response.statusText()}`,
        )
    }

    await page.waitForURL((url) => !url.pathname.startsWith("/login"), {
        timeout: LOGIN_TIMEOUT,
    })
    // returnTo 可能指向业务页。直接打开工作台，无需先等一个不会出现的标题。
    if (new URL(page.url()).pathname !== "/workspace") await page.goto("/workspace")
    const workspace = page.getByRole("heading", { name: "我的工作台" })
    await expect(workspace).toBeVisible({ timeout: LOGIN_TIMEOUT })
}

const sessionPool = new WeakMap<Browser, Map<string, LoggedInSession>>()

/**
 * 刚整页加载、之后本页没有发过写请求也没离开工作台的页面。
 * `openWorkspaceTask` 据此跳过再一次整页加载：切角色时工作台只加载一次。
 */
const freshWorkspacePages = new WeakSet<Page>()
const trackedPages = new WeakSet<Page>()

function trackWorkspaceFreshness(page: Page): void {
    if (trackedPages.has(page)) return
    trackedPages.add(page)
    page.on("request", (request) => {
        const type = request.resourceType()
        if ((type === "fetch" || type === "xhr") && !["GET", "HEAD", "OPTIONS"].includes(request.method())) {
            freshWorkspacePages.delete(page)
        }
    })
    page.on("framenavigated", (frame) => {
        if (frame !== page.mainFrame()) return
        if (new URL(frame.url()).pathname !== "/workspace") freshWorkspacePages.delete(page)
    })
}

function markWorkspaceFresh(page: Page): void {
    trackWorkspaceFreshness(page)
    freshWorkspacePages.add(page)
}

/**
 * 取走「工作台刚整页加载且未被写操作弄旧」标记。返回 true 时调用方可直接在当前工作台上找任务，
 * 不必再 goto；标记只能用一次。
 */
export function takeFreshWorkspace(page: Page): boolean {
    const fresh = freshWorkspacePages.has(page) && new URL(page.url()).pathname === "/workspace"
    freshWorkspacePages.delete(page)
    return fresh
}

async function openWorkspace(page: Page): Promise<void> {
    // 复用会话时页面可能已停在空工作台，必须硬导航才能重新拉待办。
    trackWorkspaceFreshness(page)
    await page.goto("/workspace")
    const workspace = page.getByRole("heading", { name: "我的工作台" })
    await expect(workspace).toBeVisible({ timeout: LOGIN_TIMEOUT })
    markWorkspaceFresh(page)
}

/**
 * 按账号复用已登录的 BrowserContext。同一 Browser 内同岗位只登录一次；
 * 调用方 `context.close()` 什么也不做，不销毁会话，避免切角色时重复登录和限流。
 * 复用会话时不导航（仍在登录页则补登录）；需要工作台的调用方用 `openLoggedInWorkspace`。
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
        // 其它角色可能已改了数据：上一次轮到本账号时的新鲜标记作废。
        freshWorkspacePages.delete(cached.page)
        await maximizePageIfHeaded(cached.page)
        if (/\/login(?:\?|$)/.test(cached.page.url())) {
            await loginViaUi(cached.page, cred)
            markWorkspaceFresh(cached.page)
        }
        return cached
    }

    const context = await browser.newContext(headedContextOptions())
    const page = await context.newPage()
    await maximizePageIfHeaded(page)
    trackWorkspaceFreshness(page)
    await loginViaUi(page, cred)
    // loginViaUi 以工作台标题可见收尾：刚登录的工作台就是最新的。
    markWorkspaceFresh(page)
    const session: LoggedInSession = { context, page }
    const realClose = context.close.bind(context)
    // 切角色时不导航：下一次 openLoggedInWorkspace 会整页加载工作台，这里再 goto 只是白加载一次。
    context.close = (async () => {
        freshWorkspacePages.delete(page)
    }) as BrowserContext["close"]
    context.on("close", () => {
        pool.delete(cred.account)
        context.close = realClose
    })
    pool.set(cred.account, session)
    return session
}

/**
 * 按登录名或角色键打开已登录工作台。基于 `newLoggedInContext` + `ACCOUNTS`，
 * 同一 Browser 内同账号只登录一次。
 *
 * spec 不要再复制 accountCred / asSession / openSession：
 *   import { openLoggedInWorkspace } from "../helpers/login"
 *   const { page, context } = await openLoggedInWorkspace(browser, "xiaoshou")
 *
 * 登录页 id：`#governance-auth-login-account` / `#governance-auth-login-password` /
 * `#governance-auth-login-submit`；成功后 heading「我的工作台」。
 */
export async function openLoggedInWorkspace(
    browser: Browser,
    loginName: LoginIdentity,
): Promise<LoggedInSession> {
    const session = await newLoggedInContext(browser, loginName)
    if (/\/login(?:\?|$)/.test(session.page.url())) {
        await loginViaUi(session.page, loginName)
        markWorkspaceFresh(session.page)
    }
    // 刚登录的页面已停在最新工作台；复用的会话整页加载一次，拉最新待办。
    if (!freshWorkspacePages.has(session.page)) {
        await openWorkspace(session.page)
    }
    return session
}
