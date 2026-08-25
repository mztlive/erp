/**
 * fetch 封装基座：base URL、JWT 头、超时、ApiResponse 信封统一解包。
 *
 * 后端信封（backend/apps/web-api/src/core/response.rs）真实字段：
 * { status, errorMessage, code?, fieldErrors?, retryable?, data, success }
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
    fieldErrors?: Record<string, string>
    retryable?: boolean
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
    /** 请求体：普通对象会序列化为 JSON；字符串与 FormData 原样透传。 */
    body?: unknown
    /** 外部取消信号（与内置超时叠加）。 */
    signal?: AbortSignal
    /** 超时毫秒数，默认 DEFAULT_TIMEOUT_MS。 */
    timeoutMs?: number
    /** 浏览器 HTTP 缓存策略（透传 fetch cache）。 */
    cache?: RequestCache
}

/**
 * 组装并发送请求：拼接 base URL、附带 Authorization 头、超时控制。
 * 网络层失败统一映射为 Network 类 ApiError；响应状态由调用方分支处理。
 *
 * @param path 以 "/" 开头的接口路径。
 * @param options 请求选项。
 * @throws {ApiError} fetch 失败（断网、超时、取消）时抛出。
 */
const sendRequest = async (
    path: string,
    options: ApiRequestOptions,
): Promise<Response> => {
    const headers: Record<string, string> = { ...options.headers }
    const token = getToken()
    if (token) {
        headers.Authorization = `Bearer ${token}`
    }

    let body: BodyInit | undefined
    if (options.body !== undefined) {
        if (typeof options.body === "string") {
            body = options.body
        } else if (options.body instanceof FormData) {
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

    try {
        return await fetch(`${getApiBaseUrl()}${path}`, {
            method: options.method ?? "GET",
            headers,
            body,
            signal,
            cache: options.cache,
        })
    } catch (cause) {
        throw fromFetchError(cause)
    }
}

/** 错误响应体尽力解析为对象，供统一错误层提取后端 errorMessage 与错误码。 */
const readErrorBody = async (res: Response): Promise<unknown> => {
    try {
        const text = await res.text()
        return text ? JSON.parse(text) : undefined
    } catch {
        return undefined
    }
}

/**
 * 通用请求封装：统一解包 ApiResponse 信封并返回业务数据。
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
    const res = await sendRequest(path, options)

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
    const requestId = res.headers.get("X-Trace-Id") ?? undefined

    // 401：HTTP 状态或业务信封中的 status 均归类为 Auth，并通知 session 清理
    if (res.status === 401 || envelope?.status === 401) {
        notifyUnauthorized()
        throw fromAuth(401, parsed, requestId)
    }

    // HTTP 非 2xx（403 / 404 / 500 ...）
    if (!res.ok) {
        throw fromHttpResponse(res.status, parsed, requestId)
    }

    // 业务失败（信封 success=false，携带后端 errorMessage）
    if (envelope && envelope.success === false) {
        if (typeof envelope.status === "number" && envelope.status >= 400) {
            throw fromHttpResponse(envelope.status, parsed, requestId)
        }
        throw createApiError({
            kind: "Validation",
            message:
                envelope.errorMessage ||
                "请求未通过业务校验，请检查填写内容后重试。",
            status: envelope.status,
            code: envelope.code,
            fieldErrors: envelope.fieldErrors,
            retryable: envelope.retryable ?? false,
            requestId: envelope.requestId ?? requestId,
            responseData: envelope,
        })
    }

    // 成功：优先取信封 data，非信封形态（如纯 JSON 接口）直接返回原体
    return (envelope?.data ?? parsed) as T
}

/**
 * GET 请求并以 Blob 返回二进制内容（文件预览/下载等）。
 *
 * 失败路径与 apiFetch 完全一致：401 归 Auth 并通知 session 清理；
 * 非 2xx 归 Http/Validation；错误响应体仍按 JSON 信封解析，
 * 优先透传后端 errorMessage，仅在其缺失时使用统一兜底文案。
 *
 * @param path 接口路径。
 * @param options 请求选项。
 */
export const apiGetBlob = async (
    path: string,
    options: ApiRequestOptions = {},
): Promise<Blob> => {
    const res = await sendRequest(path, options)
    const requestId = res.headers.get("X-Trace-Id") ?? undefined

    if (res.status === 401) {
        notifyUnauthorized()
        throw fromAuth(401, await readErrorBody(res), requestId)
    }
    if (!res.ok) {
        throw fromHttpResponse(res.status, await readErrorBody(res), requestId)
    }
    return res.blob()
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
 * POST multipart 请求；浏览器负责生成包含 boundary 的 Content-Type。
 */
export const apiPostForm = <T>(
    path: string,
    body: FormData,
    options?: ApiRequestOptions,
): Promise<T> => apiFetch<T>(path, { ...options, method: "POST", body })

/**
 * PUT multipart 请求；浏览器负责生成包含 boundary 的 Content-Type。
 */
export const apiPutForm = <T>(
    path: string,
    body: FormData,
    options?: ApiRequestOptions,
): Promise<T> => apiFetch<T>(path, { ...options, method: "PUT", body })

/**
 * DELETE 请求。
 */
export const apiDelete = <T>(
    path: string,
    options?: ApiRequestOptions,
): Promise<T> => apiFetch<T>(path, { ...options, method: "DELETE" })
