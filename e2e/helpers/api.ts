import { resolveAccount, type LoginIdentity } from "./accounts"

export const API_BASE = process.env.API_BASE ?? "http://127.0.0.1:10001"

type Envelope<T> = {
    status?: number
    success?: boolean
    errorMessage?: string
    data?: T | null
}

/**
 * 以后台账号登录，返回已解包的 JWT 字符串。
 * 入参可以是登录名、角色键或凭据对象。
 */
export async function apiLogin(identity: LoginIdentity): Promise<string> {
    const cred = resolveAccount(identity)
    const data = await request<{ token?: string }>("POST", "/login", {
        body: {
            account: cred.account,
            password: cred.password,
            account_kind: "admin",
        },
    })
    const token = data?.token?.trim()
    if (!token) {
        throw new Error(`API 登录失败: ${cred.account}`)
    }
    return token
}

/**
 * GET 已认证接口，返回信封 data。query 为扁平对象，空值跳过。
 */
export async function apiGet<T>(
    token: string,
    path: string,
    query?: Record<string, unknown>,
): Promise<T> {
    const qs = toQueryString(query)
    const full = qs ? `${path}${path.includes("?") ? "&" : "?"}${qs}` : path
    return request<T>("GET", full, { token })
}

async function request<T>(
    method: string,
    path: string,
    options: { token?: string; body?: unknown } = {},
): Promise<T> {
    const headers: Record<string, string> = {}
    if (options.token) {
        headers.Authorization = `Bearer ${options.token}`
    }
    if (options.body !== undefined) {
        headers["Content-Type"] = "application/json"
    }

    let response: Response
    try {
        response = await fetch(`${API_BASE}${path}`, {
            method,
            headers,
            body:
                options.body === undefined
                    ? undefined
                    : JSON.stringify(options.body),
            signal: AbortSignal.timeout(15_000),
        })
    } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        throw new Error(`API ${method} ${path} 网络错误: ${message}`)
    }

    const text = await response.text()
    let parsed: Envelope<T> | null = null
    try {
        parsed = text ? (JSON.parse(text) as Envelope<T>) : null
    } catch {
        throw new Error(
            `API ${method} ${path} 返回非 JSON（HTTP ${response.status}）: ${text.slice(0, 300)}`,
        )
    }

    if (response.status === 401 || parsed?.status === 401) {
        throw new Error(`API ${method} ${path} 未授权`)
    }
    if (!response.ok || parsed?.success === false) {
        throw new Error(
            `API ${method} ${path} 失败（HTTP ${response.status}）: ${parsed?.errorMessage ?? text}`,
        )
    }
    if (parsed?.data === undefined || parsed.data === null) {
        throw new Error(`API ${method} ${path} 响应缺少 data`)
    }
    return parsed.data
}

function toQueryString(query?: Record<string, unknown>): string {
    if (!query) return ""
    const search = new URLSearchParams()
    for (const [key, value] of Object.entries(query)) {
        if (value === undefined || value === null || value === "") continue
        search.set(key, String(value))
    }
    return search.toString()
}
