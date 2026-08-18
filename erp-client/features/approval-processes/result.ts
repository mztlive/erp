import {
    createApiError,
    type ApiError,
    type ApiErrorException,
} from "@/lib/api/errors"

/** API 层成功/失败显式结果，对应现有 Result 合同。 */
export type Result<T, E> =
    | { readonly ok: true; readonly value: T }
    | { readonly ok: false; readonly error: E }

/** 异步 Result，供 api.ts 返回、Query 边界再 unwrap。 */
export type ResultAsync<T, E> = Promise<Result<T, E>>

/**
 * 构造成功结果。
 *
 * @param value 业务数据
 */
export const ok = <T>(value: T): Result<T, never> => ({ ok: true, value })

/**
 * 构造失败结果。
 *
 * @param error 统一 API 错误
 */
export const err = <E>(error: E): Result<never, E> => ({ ok: false, error })

/**
 * 在 Query / Mutation 边界解开 Result；失败时抛出以进入 React Query error 态。
 *
 * @param result API 层结果
 * @returns 成功值
 * @throws {ApiError} 失败分支
 */
export const unwrapResult = <T>(result: Result<T, ApiError>): T => {
    if (!result.ok) throw result.error
    return result.value
}

/**
 * 把未知异常规范为 ApiError，避免匹配后端 message 文本。
 *
 * @param cause 捕获到的异常
 */
export const toApiError = (cause: unknown): ApiError => {
    if (
        typeof cause === "object" &&
        cause !== null &&
        "kind" in cause &&
        "message" in cause &&
        typeof (cause as ApiError).message === "string"
    ) {
        return cause as ApiError
    }
    return createApiError({
        kind: "Unknown",
        message: "系统暂时无法完成操作，请稍后重试。",
        cause,
    })
}

/**
 * 把会抛错的 Promise 收成 ResultAsync。
 *
 * @param run 实际请求
 */
export const fromPromise = async <T>(
    run: () => Promise<T>,
): ResultAsync<T, ApiError> => {
    try {
        return ok(await run())
    } catch (cause) {
        return err(toApiError(cause))
    }
}

export type { ApiError, ApiErrorException }
