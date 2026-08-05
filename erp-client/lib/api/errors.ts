/**
 * 统一 API 错误模型（AGENTS.md 第 10 节错误处理约定的基础类型）。
 *
 * 本项目不引入 neverthrow：本文件只定义 ApiError 类型与工厂函数（返回错误对象），
 * 抛出时机由 client.ts 决定（queryFn / mutationFn 边界抛错，让 TanStack Query
 * 正确进入 error 状态）。
 */

/** 错误分类：网络 / HTTP / 鉴权 / 解析 / 业务校验 / 未知 */
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
  /** HTTP 状态码（仅 Http / Auth / Validation 等有实际状态码的场景）。 */
  status?: number
  /** 响应原始数据（信封对象或文本），便于上层定位问题。 */
  responseData?: unknown
  /** 底层原始错误（fetch 失败原因 / 解析异常等）。 */
  cause?: unknown
}

/**
 * 网络层失败（fetch 抛出）：断网、连接拒绝、超时、Abort 等。
 */
export const fromFetchError = (cause: unknown): ApiError => ({
  kind: "Network",
  message: "网络请求失败或连接超时",
  cause,
})

/**
 * HTTP 非 2xx 错误（401 除外，401 归类为 Auth）。
 */
export const fromHttpResponse = (
  status: number,
  responseData?: unknown
): ApiError => ({
  kind: "Http",
  message: `请求失败（HTTP ${status}）`,
  status,
  responseData,
})

/**
 * 鉴权失败（HTTP 401 或业务信封 status 401）。
 */
export const fromAuth = (status: number, responseData?: unknown): ApiError => ({
  kind: "Auth",
  message: "登录状态已失效，请重新登录",
  status,
  responseData,
})

/**
 * 响应体 JSON 解析失败。
 */
export const fromParse = (cause: unknown, responseData?: unknown): ApiError => ({
  kind: "Parse",
  message: "响应数据解析失败",
  responseData,
  cause,
})

/**
 * 未归类的兜底错误。
 */
export const fromUnknown = (cause: unknown): ApiError => ({
  kind: "Unknown",
  message: "发生未知错误",
  cause,
})
