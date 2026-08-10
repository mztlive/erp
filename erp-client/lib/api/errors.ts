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
  readonly requestId?: string
  readonly responseData?: unknown
  override readonly cause?: unknown

  constructor(input: ApiErrorInput) {
    super(input.message)
    this.name = "ApiError"
    this.kind = input.kind
    this.status = input.status
    this.code = input.code
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
  errorMessage?: unknown
  code?: unknown
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

const envelopeDetails = (responseData: unknown) => {
  const envelope = asEnvelope(responseData)
  return {
    backendMessage: nonEmptyString(envelope?.errorMessage),
    code: nonEmptyString(envelope?.code),
    requestId:
      nonEmptyString(envelope?.requestId) ??
      nonEmptyString(envelope?.request_id),
  }
}

/** 网络层失败（断网、连接拒绝、超时或取消）。 */
export const fromFetchError = (cause: unknown): ApiError =>
  createApiError({
    kind: "Network",
    message: "网络连接失败或请求超时，请检查网络后重试。",
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
  responseData?: unknown
): ApiError => {
  const { backendMessage, code, requestId } = envelopeDetails(responseData)
  const permissionMessage =
    !backendMessage || backendMessage.toLowerCase() === "permission denied"
      ? "当前账号没有执行此操作的权限，请联系管理员或有权限的同事。"
      : backendMessage
  const message =
    status === 403
      ? permissionMessage
      : backendMessage ||
        (status === 400 || status === 422
          ? "请求未通过业务校验，请检查填写内容。"
          : status === 404
            ? "请求的资料不存在或已被移除。"
            : status === 409
              ? "数据已被其他操作更新，请刷新后重试。"
              : status === 429
                ? "请求过于频繁，请稍后重试。"
                : `请求失败（HTTP ${status}）`)

  return createApiError({
    kind: status === 400 || status === 422 ? "Validation" : "Http",
    message,
    status,
    code,
    requestId,
    responseData,
  })
}

/** 鉴权失败（HTTP 401 或业务信封 status 401）。 */
export const fromAuth = (status: number, responseData?: unknown): ApiError => {
  const { code, requestId } = envelopeDetails(responseData)
  return createApiError({
    kind: "Auth",
    message: "登录状态已失效，请重新登录。",
    status,
    code,
    requestId,
    responseData,
  })
}

/** 响应体 JSON 解析失败。 */
export const fromParse = (cause: unknown, responseData?: unknown): ApiError =>
  createApiError({
    kind: "Parse",
    message: "系统返回的数据无法读取，请稍后重试。",
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
  requestId?: string
  retryable: boolean
}

const presentationFromApiError = (error: ApiError): ErrorPresentation => {
  const status = error.status
  if (status === 401 || error.kind === "Auth") {
    return {
      kind: "permission",
      title: "登录状态已失效",
      description: "请重新登录后继续操作。",
      code: error.code,
      requestId: error.requestId,
      retryable: false,
    }
  }
  if (status === 403) {
    return {
      kind: "permission",
      title: "权限不足",
      description: error.message,
      code: error.code,
      requestId: error.requestId,
      retryable: false,
    }
  }
  if (status === 409) {
    return {
      kind: "conflict",
      title: "数据已发生变化",
      description: error.message,
      code: error.code,
      requestId: error.requestId,
      retryable: true,
    }
  }
  if (status === 400 || status === 422 || error.kind === "Validation") {
    return {
      kind: "validation",
      title: "提交内容未通过检查",
      description: error.message,
      code: error.code,
      requestId: error.requestId,
      retryable: false,
    }
  }
  if (status === 404) {
    return {
      kind: "business",
      title: "资料不可用",
      description: error.message,
      code: error.code,
      requestId: error.requestId,
      retryable: false,
    }
  }
  if (status === 429) {
    return {
      kind: "system",
      title: "操作过于频繁",
      description: error.message,
      code: error.code,
      requestId: error.requestId,
      retryable: true,
    }
  }
  return {
    kind: "system",
    title: error.kind === "Network" ? "网络连接失败" : "系统暂时无法完成操作",
    description: error.message,
    code: error.code,
    requestId: error.requestId,
    retryable: true,
  }
}

/** 把任意异常转换成稳定、可执行的用户提示。 */
export const getErrorPresentation = (
  error: unknown,
  fallback = "操作未完成，请稍后重试。"
): ErrorPresentation => {
  if (isApiError(error)) return presentationFromApiError(error)
  if (error instanceof Error && error.message.trim().length > 0) {
    return {
      kind: "business",
      title: "操作未完成",
      description: error.message,
      retryable: false,
    }
  }
  if (typeof error === "string" && error.trim().length > 0) {
    return {
      kind: "business",
      title: "操作未完成",
      description: error.trim(),
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
