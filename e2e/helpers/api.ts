import { APIRequestContext } from "@playwright/test"
import { ACCOUNTS, AccountKey } from "./accounts"

/** 后端 API 基址（与 erp-client/lib/api/client.ts 默认值一致）。 */
export const API_BASE = "http://127.0.0.1:10001"

/** 后端统一信封解包：{status, errorMessage, data, success}。 */
export async function api<T>(
    request: APIRequestContext,
    method: "GET" | "POST" | "PUT" | "DELETE",
    path: string,
    options: { token?: string; body?: unknown; query?: Record<string, unknown> } = {},
): Promise<T> {
    const headers: Record<string, string> = {}
    if (options.token) headers.Authorization = `Bearer ${options.token}`
    if (options.body !== undefined) headers["Content-Type"] = "application/json"
    const query = options.query
        ? `?${new URLSearchParams(
              Object.entries(options.query).map(([k, v]) => [k, String(v)]),
          ).toString()}`
        : ""
    const res = await request.fetch(`${API_BASE}${path}${query}`, {
        method,
        headers,
        data: options.body === undefined ? undefined : JSON.stringify(options.body),
    })
    const text = await res.text()
    const parsed = text ? JSON.parse(text) : null
    if (res.status() === 401 || (parsed && parsed.status === 401)) {
        throw new Error(`API ${method} ${path} 未授权: ${text}`)
    }
    if (!res.ok()) {
        throw new Error(`API ${method} ${path} HTTP ${res.status()}: ${text}`)
    }
    if (parsed && parsed.success === false) {
        throw new Error(`API ${method} ${path} 业务失败: ${parsed.errorMessage} (code=${parsed.code})`)
    }
    return (parsed?.data ?? parsed) as T
}

/** 账号密码登录（POST /login），返回 JWT。 */
export async function apiLogin(
    request: APIRequestContext,
    accountKey: AccountKey,
): Promise<string> {
    const acc = ACCOUNTS[accountKey]
    const result = await api<{ token: string }>(request, "POST", "/login", {
        body: { account: acc.account, password: acc.password, account_kind: "admin" },
    })
    if (!result?.token) throw new Error(`登录 ${acc.account} 未返回 token`)
    return result.token
}

/** 读取当前账号资料（userid/权限），用于断言登录态与权限。 */
export type AccountProfile = {
    userid: string
    account: string
    name: string
    subject: string
    role_ids: string[]
    permissions: string[]
}

export async function apiProfile(
    request: APIRequestContext,
    token: string,
): Promise<AccountProfile> {
    return api<AccountProfile>(request, "GET", "/account/profile", { token })
}
