import type { ApiError } from "@/lib/api"

/** 判断未知异常是否为统一 API 错误。 */
export function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

/** 提取服务端稳定错误消息。 */
export function apiErrorMessage(error: ApiError): string {
    const data = error.responseData as { errorMessage?: string } | undefined
    return data?.errorMessage && data.errorMessage !== "OK"
        ? data.errorMessage
        : error.message
}
