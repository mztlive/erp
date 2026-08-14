import type { ApiError } from "@/lib/api"
import { getErrorMessage } from "@/lib/api/errors"
import { classifyFormalCommandError } from "@/lib/formal-command"
import type { FormalActionResponse } from "@/features/purchase-orders/types"

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

export function formalActionFailure<T>(
    error: unknown,
    idempotencyKey: string,
): FormalActionResponse<T> {
    if (classifyFormalCommandError(error) === "unknown") {
        return {
            status: "unknown",
            message:
                "处理结果待确认。当前输入已保留，请稍后使用本次操作重试。",
            idempotencyKey,
        }
    }
    return {
        status: "failed",
        message: apiErrorMessage(error),
        code: apiErrorCode(error),
    }
}
