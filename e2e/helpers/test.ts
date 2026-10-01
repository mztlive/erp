import { test as baseTest, type Browser, type BrowserContextOptions } from "@playwright/test"

export * from "@playwright/test"

export type IsolatedApiRouting = {
    sourceOrigin: string
    targetOrigin: string
}

/** 只有隔离运行才转发共享前端构建中的 API 地址。 */
export function isolatedApiRouting(environment: NodeJS.ProcessEnv = process.env): IsolatedApiRouting | null {
    if (!environment.ERP_E2E_CONFIG_PATH && environment.ERP_E2E_ISOLATED !== "1") return null
    const sourceOrigin = new URL(environment.ERP_E2E_SOURCE_API_BASE ?? "http://127.0.0.1:10001").origin
    const targetOrigin = new URL(environment.API_BASE ?? sourceOrigin).origin
    if (sourceOrigin === targetOrigin) return null
    if (new URL(sourceOrigin).protocol !== new URL(targetOrigin).protocol) {
        throw new Error("隔离 E2E 的源 API 和目标 API 必须使用相同协议")
    }
    return { sourceOrigin, targetOrigin }
}

/** 所有新上下文在导航前安装真实 HTTP 转发；原地址匹配 mock，响应保留路径和查询参数。 */
export function installIsolatedApiRouting(browser: Browser, routing: IsolatedApiRouting): () => void {
    const originalNewContext = browser.newContext
    browser.newContext = async (options?: BrowserContextOptions) => {
        const context = await originalNewContext.call(browser, { ...options, serviceWorkers: "block" })
        try {
            await context.route(`${routing.sourceOrigin}/**`, async (route) => {
                const source = new URL(route.request().url())
                await route.continue({ url: `${routing.targetOrigin}${source.pathname}${source.search}` })
            })
        } catch (error) {
            await context.close()
            throw error
        }
        return context
    }
    return () => {
        browser.newContext = originalNewContext
    }
}

export const test = baseTest.extend({
    browser: [async ({ browser }, use) => {
        const routing = isolatedApiRouting()
        const restore = routing ? installIsolatedApiRouting(browser, routing) : undefined
        try {
            await use(browser)
        } finally {
            restore?.()
        }
    }, { scope: "worker" }],
})
