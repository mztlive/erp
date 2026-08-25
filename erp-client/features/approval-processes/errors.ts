import type { ApiError } from "./result"

/** 判断是否为定义锁版本冲突。

/**
 * 判断是否为定义锁版本冲突。
 *
 * @param error 统一 API 错误
 */
export const isDefinitionVersionConflict = (error: unknown): boolean => {
    if (typeof error !== "object" || error === null) return false
    const apiError = error as ApiError
    return (
        apiError.status === 409 &&
        apiError.code === "APPROVAL_DEFINITION_VERSION_CONFLICT"
    )
}

/**
 * 透传后端错误文案；后端已按《审批流程错误目录》返回用户可读中文，
 * 前端不再按错误码自备翻译，仅为无稳定码的异常补充请求编号。
 *
 * @param error 统一 API 错误或未知异常
 */
export const definitionErrorMessage = (error: unknown): string => {
    if (typeof error !== "object" || error === null) {
        return "操作未完成，请稍后重试。"
    }
    const apiError = error as ApiError
    if (apiError.message) {
        return !apiError.code && apiError.requestId
            ? `${apiError.message}（请求编号 ${apiError.requestId}）`
            : apiError.message
    }
    return "系统暂时无法完成操作，请稍后重试。"
}

/**
 * 生成发布/退役使用的新操作标识。页面不得把该值展示给用户。
 *
 * @param prefix 动作前缀
 */
export const newCommandKey = (prefix: string): string => {
    const randomId = globalThis.crypto?.randomUUID?.()
    if (randomId) return `${prefix}:${randomId}`
    return `${prefix}:${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}
