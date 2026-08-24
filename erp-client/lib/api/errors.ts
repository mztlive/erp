/** 错误分类：网络 / HTTP / 鉴权 / 解析 / 业务校验 / 未知。 */
export type ApiErrorKind =
    | "Network"
    | "Http"
    | "Auth"
    | "Parse"
    | "Validation"
    | "Unknown"

/** 统一错误结构，所有 API 调用失败最终都映射为该形状。 */
export interface ApiError {
    kind: ApiErrorKind
    message: string
    /** HTTP 状态码（仅有 HTTP 响应时存在）。 */
    status?: number
    /** 稳定错误码；后端未返回时保持为空。 */
    code?: string
    /** 字段级校验说明；键为接口字段名，值为可直接展示的业务提示。 */
    fieldErrors?: Readonly<Record<string, string>>
    /** 是否可以安全地直接重试原操作。 */
    retryable?: boolean
    /** 请求编号；用于联系支持人员定位服务端日志。 */
    requestId?: string
    /** 响应原始数据（信封对象或文本），便于适配层定位问题。 */
    responseData?: unknown
    /** 底层原始错误（fetch 失败原因 / 解析异常等）。 */
    cause?: unknown
}

type ApiErrorInput = Omit<ApiError, "message"> & { message: string }

/**
 * 运行时 API 异常。
 *
 * 保留既有 `ApiError` 结构合同，同时确保所有统一请求错误都能被标准
 * `Error` 分支捕获；页面不得再因结构化错误不是 `Error` 实例而丢失原因。
 */
export class ApiErrorException extends Error implements ApiError {
    readonly kind: ApiErrorKind
    readonly status?: number
    readonly code?: string
    readonly fieldErrors?: Readonly<Record<string, string>>
    readonly retryable?: boolean
    readonly requestId?: string
    readonly responseData?: unknown
    override readonly cause?: unknown

    constructor(input: ApiErrorInput) {
        super(input.message)
        this.name = "ApiError"
        this.kind = input.kind
        this.status = input.status
        this.code = input.code
        this.fieldErrors = input.fieldErrors
        this.retryable = input.retryable
        this.requestId = input.requestId
        this.responseData = input.responseData
        this.cause = input.cause
    }
}

/** 构造兼容 `Error` 与 `ApiError` 的统一异常。 */
export const createApiError = (input: ApiErrorInput): ApiErrorException =>
    new ApiErrorException(input)

/** 判断未知异常是否满足统一 API 错误合同。 */
const isApiError = (error: unknown): error is ApiError =>
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    "message" in error &&
    typeof error.message === "string"

type ErrorEnvelope = {
    status?: unknown
    errorMessage?: unknown
    code?: unknown
    fieldErrors?: unknown
    retryable?: unknown
    requestId?: unknown
    request_id?: unknown
    success?: unknown
}

const asEnvelope = (responseData: unknown): ErrorEnvelope | undefined =>
    typeof responseData === "object" && responseData !== null
        ? (responseData as ErrorEnvelope)
        : undefined

const nonEmptyString = (value: unknown): string | undefined =>
    typeof value === "string" && value.trim().length > 0
        ? value.trim()
        : undefined

const asFieldErrors = (
    value: unknown,
): Readonly<Record<string, string>> | undefined => {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return undefined
    }
    const entries = Object.entries(value).flatMap(([field, message]) => {
        const readable = nonEmptyString(message)
        return readable ? [[field, readable] as const] : []
    })
    return entries.length > 0 ? Object.fromEntries(entries) : undefined
}

const TECHNICAL_MESSAGE_PATTERN =
    /(?:\[object Object\]|(?:Type|Reference|Syntax|Network|Validation)Error|Validation error|\bat\s+\S+\s*\(|https?:\/\/|\b(?:GET|POST|PUT|PATCH|DELETE)\s+\/|\b(?!(?:SKU|ERP|PDF|CSV)\b)[A-Z][A-Z0-9_]{2,}\b|\b(?:id|payload|canonical|handler|blocker|view|status|dto|enum|rbac)\b|work_item|idempotency|lockVersion|stack trace|数据库|服务端|客户端|前端|后端|堆栈|唯一索引|内部错误|状态机接口|接口未交付|接口|事务|幂等|投影|水位|快照|指纹|锁版本|JSON|SQL|Mongo)/i

/** 判断原始原因能否直接给业务用户阅读，拦截英文和实现细节。 */
const userReadableMessage = (value: unknown): string | undefined => {
    const message = nonEmptyString(value)
    if (!message || !/[\u3400-\u9fff]/u.test(message)) return undefined
    if (TECHNICAL_MESSAGE_PATTERN.test(message)) return undefined
    return message
}

const envelopeDetails = (responseData: unknown) => {
    const envelope = asEnvelope(responseData)
    const isErrorEnvelope = envelope?.success === false
    return {
        backendMessage: isErrorEnvelope
            ? nonEmptyString(envelope?.errorMessage)
            : undefined,
        code: nonEmptyString(envelope?.code),
        fieldErrors: asFieldErrors(envelope?.fieldErrors),
        retryable:
            typeof envelope?.retryable === "boolean"
                ? envelope.retryable
                : undefined,
        requestId:
            nonEmptyString(envelope?.requestId) ??
            nonEmptyString(envelope?.request_id),
    }
}

/** 为旧版或非标准响应提供不泄露实现细节的兼容说明。 */
const fallbackMessage = (status: number): string => {
    if (status === 400 || status === 422) {
        return "提交内容不符合要求，请检查后重试。"
    }
    if (status === 403) {
        return "当前账号没有执行此操作的权限，请联系管理员或有权限的同事。"
    }
    if (status === 404) {
        return "没有找到所需资料，请刷新后重新选择。"
    }
    if (status === 409) {
        return "当前资料状态不允许继续操作，请刷新后核对。"
    }
    if (status === 429) return "请求过于频繁，请稍后重试。"
    if (status >= 500) {
        return "系统暂时无法完成操作，请稍后重试；如仍失败，请联系支持人员。"
    }
    return "请求未完成，请稍后重试。"
}

/** 兼容尚未返回 retryable 的旧版接口。 */
const defaultRetryable = (status: number): boolean =>
    status === 408 || status === 429 || status >= 500

/** 网络层失败（断网、连接拒绝、超时或取消）。 */
export const fromFetchError = (cause: unknown): ApiError =>
    createApiError({
        kind: "Network",
        message: "网络连接失败或请求超时，请检查网络后重试。",
        retryable: true,
        cause,
    })

/**
 * 将 HTTP 非 2xx 响应转换为统一异常。
 *
 * 403 的中间件原始消息可能只有 `Permission denied`，该技术文案不得直接
 * 展示给用户；业务 Handler 返回的具体中文权限原因仍会保留。
 */
export const fromHttpResponse = (
    status: number,
    responseData?: unknown,
    responseRequestId?: string,
): ApiError => {
    const { backendMessage, code, fieldErrors, retryable, requestId } =
        envelopeDetails(responseData)
    const message = backendMessage ?? fallbackMessage(status)

    return createApiError({
        kind: status === 400 || status === 422 ? "Validation" : "Http",
        message,
        status,
        code,
        fieldErrors,
        retryable: retryable ?? defaultRetryable(status),
        requestId: requestId ?? responseRequestId,
        responseData,
    })
}

/** 鉴权失败（HTTP 401 或业务信封 status 401）。 */
export const fromAuth = (
    status: number,
    responseData?: unknown,
    responseRequestId?: string,
): ApiError => {
    const { code, requestId } = envelopeDetails(responseData)
    return createApiError({
        kind: "Auth",
        message: "登录状态已失效，请重新登录。",
        status,
        code,
        retryable: false,
        requestId: requestId ?? responseRequestId,
        responseData,
    })
}

/** 响应体 JSON 解析失败。 */
export const fromParse = (cause: unknown, responseData?: unknown): ApiError =>
    createApiError({
        kind: "Parse",
        message: "系统返回的数据无法读取，请稍后重试。",
        retryable: true,
        responseData,
        cause,
    })

/** 面向业务界面的错误分类。 */
type ErrorPresentationKind =
    | "validation"
    | "business"
    | "permission"
    | "conflict"
    | "system"

/** 用户可见错误内容；不暴露堆栈、内部对象或第三方原始响应。 */
export interface ErrorPresentation {
    kind: ErrorPresentationKind
    title: string
    description: string
    code?: string
    fieldErrors?: Readonly<Record<string, string>>
    requestId?: string
    retryable: boolean
}

const presentationFromApiError = (
    error: ApiError,
    fallback: string,
): ErrorPresentation => {
    const status = error.status
    const description =
        userReadableMessage(error.message) ??
        (typeof status === "number" ? fallbackMessage(status) : fallback)
    const retryable =
        error.retryable ??
        (typeof status === "number" ? defaultRetryable(status) : false)
    if (status === 401 || error.kind === "Auth") {
        return {
            kind: "permission",
            title: "登录状态已失效",
            description: "请重新登录后继续操作。",
            code: error.code,
            fieldErrors: error.fieldErrors,
            requestId: error.requestId,
            retryable: false,
        }
    }
    if (status === 403) {
        return {
            kind: "permission",
            title: "权限不足",
            description,
            code: error.code,
            fieldErrors: error.fieldErrors,
            requestId: error.requestId,
            retryable: false,
        }
    }
    if (status === 409) {
        return {
            kind: "conflict",
            title: "操作暂不能继续",
            description,
            code: error.code,
            fieldErrors: error.fieldErrors,
            requestId: error.requestId,
            retryable,
        }
    }
    if (status === 400 || status === 422 || error.kind === "Validation") {
        return {
            kind: "validation",
            title: "提交内容需要调整",
            description,
            code: error.code,
            fieldErrors: error.fieldErrors,
            requestId: error.requestId,
            retryable,
        }
    }
    if (status === 404) {
        return {
            kind: "business",
            title: "资料不可用",
            description,
            code: error.code,
            fieldErrors: error.fieldErrors,
            requestId: error.requestId,
            retryable,
        }
    }
    if (status === 429) {
        return {
            kind: "system",
            title: "操作过于频繁",
            description,
            code: error.code,
            fieldErrors: error.fieldErrors,
            requestId: error.requestId,
            retryable,
        }
    }
    return {
        kind: "system",
        title:
            error.kind === "Network" ? "网络连接失败" : "系统暂时无法完成操作",
        description,
        code: error.code,
        fieldErrors: error.fieldErrors,
        requestId: error.requestId,
        retryable,
    }
}

/** 把任意异常转换成稳定、可执行的用户提示。 */
export const getErrorPresentation = (
    error: unknown,
    fallback = "操作未完成，请稍后重试。",
): ErrorPresentation => {
    if (isApiError(error)) return presentationFromApiError(error, fallback)
    if (error instanceof Error) {
        const description = userReadableMessage(error.message)
        if (!description) {
            return {
                kind: "system",
                title: "系统暂时无法完成操作",
                description: fallback,
                retryable: true,
            }
        }
        return {
            kind: "business",
            title: "操作未完成",
            description,
            retryable: false,
        }
    }
    const description = userReadableMessage(error)
    if (description) {
        return {
            kind: "business",
            title: "操作未完成",
            description,
            retryable: false,
        }
    }
    return {
        kind: "system",
        title: "系统暂时无法完成操作",
        description: fallback,
        retryable: true,
    }
}

/** 提取用户可读错误原因，供仍使用局部 Alert 的页面复用。 */
export const getErrorMessage = (error: unknown, fallback?: string): string =>
    getErrorPresentation(error, fallback).description
