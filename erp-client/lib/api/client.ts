/**
 * fetch 封装基座：base URL、JWT 头、超时、ApiResponse 信封统一解包。
 *
 * 后端信封（backend/apps/web-api/src/core/response.rs）真实字段：
 * { status: number, errorMessage: string, data: T | null, success: boolean }
 * 成功判定：HTTP 2xx 且信封 success === true，业务数据取信封 data。
 *
 * 失败统一抛 ApiError（errors.ts）：
 * - fetch 抛出            -> Network
 * - HTTP/信封 401         -> Auth（并通知 session 清理）
 * - HTTP 非 2xx           -> Http
 * - 信封 success=false    -> Validation（业务校验失败，取 errorMessage）
 * - JSON 解析失败         -> Parse
 *
 * 所有调用点必须是 TanStack Query 的 queryFn / mutationFn（AGENTS.md 第 2 节）。
 */

import {
    createApiError,
    fromAuth,
    fromFetchError,
    fromHttpResponse,
    fromParse,
} from "@/lib/api/errors"
import { toQueryString } from "@/lib/api/paging"
import { getToken, notifyUnauthorized } from "@/lib/api/session"

/** 默认后端地址（可用 NEXT_PUBLIC_API_BASE_URL 覆盖）。 */
const DEFAULT_API_BASE_URL = "http://127.0.0.1:10001"

/** 默认请求超时（毫秒）。 */
const DEFAULT_TIMEOUT_MS = 15_000

/**
 * 读取后端 base URL（浏览器端读取环境变量）。
 */
export const getApiBaseUrl = (): string =>
    process.env.NEXT_PUBLIC_API_BASE_URL ?? DEFAULT_API_BASE_URL

/** 后端统一响应信封（与 response.rs 字段一一对应）。 */
interface ApiResponseEnvelope<T> {
    status: number
    errorMessage: string
    code?: string
    requestId?: string
    data: T | null
    success: boolean
}

/** apiFetch 请求选项。 */
export interface ApiRequestOptions {
    /** HTTP 方法，默认 GET。 */
    method?: string
    /** 额外请求头。 */
    headers?: Record<string, string>
    /** 请求体：对象会被 JSON 序列化并自动带 Content-Type；字符串原样透传。 */
    body?: unknown
    /** 外部取消信号（与内置超时叠加）。 */
    signal?: AbortSignal
    /** 超时毫秒数，默认 DEFAULT_TIMEOUT_MS。 */
    timeoutMs?: number
}

/**
 * 通用请求封装：拼接 base URL、附带 Authorization 头、超时控制、
 * 统一解包 ApiResponse 信封并返回业务数据。
 *
 * @param path 以 "/" 开头的接口路径。
 * @param options 请求选项。
 * @returns 信封解包后的业务数据（T）。
 * @throws {ApiError} 网络失败 / 非 2xx / 业务失败 / 解析失败统一抛 ApiError。
 */
const apiFetch = async <T>(
    path: string,
    options: ApiRequestOptions = {},
): Promise<T> => {
    const headers: Record<string, string> = { ...options.headers }
    const token = getToken()
    if (token) {
        headers.Authorization = `Bearer ${token}`
    }

    let body: string | undefined
    if (options.body !== undefined) {
        if (typeof options.body === "string") {
            body = options.body
        } else {
            body = JSON.stringify(options.body)
            headers["Content-Type"] = "application/json"
        }
    }

    const timeoutMs = options.timeoutMs ?? DEFAULT_TIMEOUT_MS
    const signal = options.signal
        ? AbortSignal.any([AbortSignal.timeout(timeoutMs), options.signal])
        : AbortSignal.timeout(timeoutMs)

    let res: Response
    try {
        res = await fetch(`${getApiBaseUrl()}${path}`, {
            method: options.method ?? "GET",
            headers,
            body,
            signal,
        })
    } catch (cause) {
        throw fromFetchError(cause)
    }

    const bodyText = await res.text()
    let parsed: unknown
    if (bodyText) {
        try {
            parsed = JSON.parse(bodyText) as unknown
        } catch (cause) {
            if (res.ok) throw fromParse(cause, bodyText)
        }
    }

    const envelope = parsed as Partial<ApiResponseEnvelope<T>> | undefined

    // 401：HTTP 状态或业务信封中的 status 均归类为 Auth，并通知 session 清理
    if (res.status === 401 || envelope?.status === 401) {
        notifyUnauthorized()
        throw fromAuth(401, parsed)
    }

    // HTTP 非 2xx（403 / 404 / 500 ...）
    if (!res.ok) {
        throw fromHttpResponse(res.status, parsed)
    }

    // 业务失败（信封 success=false，携带后端 errorMessage）
    if (envelope && envelope.success === false) {
        if (typeof envelope.status === "number" && envelope.status >= 400) {
            throw fromHttpResponse(envelope.status, parsed)
        }
        throw createApiError({
            kind: "Validation",
            message:
                envelope.errorMessage ||
                "请求未通过业务校验，请检查填写内容后重试。",
            status: envelope.status,
            code: envelope.code,
            requestId: envelope.requestId,
            responseData: envelope,
        })
    }

    // 成功：优先取信封 data，非信封形态（如纯 JSON 接口）直接返回原体
    return (envelope?.data ?? parsed) as T
}

/**
 * GET 请求，query 参数自动序列化为查询字符串。
 *
 * @param path 接口路径。
 * @param query 扁平查询参数（PageParams 或任意筛选字段）。
 */
export const apiGet = <T>(
    path: string,
    query?: Record<string, unknown>,
    options?: ApiRequestOptions,
): Promise<T> => {
    const target = query ? `${path}?${toQueryString(query)}` : path
    return apiFetch<T>(target, options)
}

/**
 * POST 请求，body 对象自动 JSON 序列化。
 */
export const apiPost = <T>(
    path: string,
    body?: unknown,
    options?: ApiRequestOptions,
): Promise<T> => apiFetch<T>(path, { ...options, method: "POST", body })

/**
 * PUT 请求，body 对象自动 JSON 序列化。
 */
export const apiPut = <T>(
    path: string,
    body?: unknown,
    options?: ApiRequestOptions,
): Promise<T> => apiFetch<T>(path, { ...options, method: "PUT", body })

/**
 * DELETE 请求。
 */
export const apiDelete = <T>(
    path: string,
    options?: ApiRequestOptions,
): Promise<T> => apiFetch<T>(path, { ...options, method: "DELETE" })
