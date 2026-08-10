/**
 * 登录态与会话管理：token 存取（localStorage）、401 回调注册。
 *
 * 纯客户端 SPA 场景（AGENTS.md 第 1 节），不做 SSR 环境判断。
 */

/** localStorage 中保存 token 的键名。 */
const TOKEN_STORAGE_KEY = "erp.token"

/** 已注册的 401 处理器集合（client.ts 检测到未授权时逐个通知）。 */
const unauthorizedHandlers = new Set<() => void>()

/**
 * 读取当前登录 token。
 *
 * @returns 未登录时返回 null。
 */
export const getToken = (): string | null =>
    localStorage.getItem(TOKEN_STORAGE_KEY)

/**
 * 保存登录 token。
 *
 * @param token 服务端下发的 JWT 字符串。
 */
export const setToken = (token: string): void => {
    localStorage.setItem(TOKEN_STORAGE_KEY, token)
}

/**
 * 清除登录 token（登出或 401 失效时调用）。
 */
export const clearToken = (): void => {
    localStorage.removeItem(TOKEN_STORAGE_KEY)
}

/**
 * 当前是否处于已登录状态（存在 token 即视为已登录）。
 */
export const isAuthenticated = (): boolean => Boolean(getToken())

/**
 * 注册 401 未授权回调（如跳转登录页）。
 *
 * @param handler 401 触发时执行的回调。
 * @returns 取消注册函数，调用后该回调不再被触发。
 */
export const onUnauthorized = (handler: () => void): (() => void) => {
    unauthorizedHandlers.add(handler)
    return () => {
        unauthorizedHandlers.delete(handler)
    }
}

/**
 * 通知所有已注册的 401 回调（由 client.ts 在检测到未授权时调用）。
 *
 * 会先清除本地 token，再逐个执行回调（跳转登录页、清空 Query 缓存等）。
 */
export const notifyUnauthorized = (): void => {
    clearToken()
    for (const handler of unauthorizedHandlers) {
        handler()
    }
}
