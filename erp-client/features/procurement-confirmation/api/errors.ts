/** 请求错误归类与用户可读错误文案（W07 适配层内部使用）。 */

import { getErrorMessage } from "@/lib/api/errors"
import type { ApiError } from "@/lib/api"

export function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

export function apiErrorMessage(error: unknown): string {
    if (!isApiError(error)) {
        return getErrorMessage(error, "请求失败")
    }
    const data = error.responseData as { errorMessage?: string } | undefined
    if (
        data &&
        typeof data.errorMessage === "string" &&
        data.errorMessage &&
        data.errorMessage !== "OK"
    ) {
        return data.errorMessage
    }
    return error.message
}

export function apiErrorCode(error: unknown): string {
    if (!isApiError(error)) return "REQUEST_FAILED"
    if (error.status === 409) return "CONFLICT"
    if (error.status === 404) return "NOT_FOUND"
    if (error.status === 403) return "FORBIDDEN"
    if (error.status === 422) return "UNPROCESSABLE"
    if (error.kind === "Validation") return "VALIDATION"
    return error.kind.toUpperCase()
}
