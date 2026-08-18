import { versionText } from "@/lib/ui-text"

import type { ApiError } from "./result"

/** 按稳定错误码映射的用户文案。禁止匹配后端 message。 */
const ERROR_COPY: Record<string, string> = {
    APPROVAL_DEFINITION_VERSION_CONFLICT:
        "审批流程已被更新，请核对当前版本后重新确认。",
    APPROVAL_DEFINITION_INVALID: "审批流程未通过检查，请核对节点和审批人。",
    APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE:
        "当前没有可复制的已发布版本，请改用空白流程。",
    APPROVAL_DEFINITION_NOT_DRAFT: "只能修改草稿。已发布或已退役版本不可改写。",
    APPROVAL_PROCESS_NOT_CONFIGURED:
        "该单据类型必须配置审批流程后才能创建新单据。",
    APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT:
        "本次操作与原任务内容不一致，请重新发起。",
}

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
 * 按稳定 code 映射中文说明；未知错误展示关联编号。
 *
 * @param error 统一 API 错误或未知异常
 */
export const definitionErrorMessage = (error: unknown): string => {
    if (typeof error !== "object" || error === null) {
        return "操作未完成，请稍后重试。"
    }
    const apiError = error as ApiError
    if (apiError.code && ERROR_COPY[apiError.code]) {
        return ERROR_COPY[apiError.code]
    }
    if (apiError.status === 403) {
        return "当前账号没有执行此操作的权限，请联系管理员。"
    }
    if (apiError.status === 409) {
        return versionText.versionChangedRefresh
    }
    if (apiError.requestId) {
        return `系统暂时无法完成操作。错误编号 ${apiError.requestId}`
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
