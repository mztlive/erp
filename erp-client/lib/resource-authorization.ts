import { isApiError } from "@/lib/api/errors"

/** 仅表示正式来源的授权生成状态，不代表命令失败或应当重放命令。 */
export const isResourceAuthorizationPending = (error: unknown): boolean =>
    isApiError(error) && error.code === "RESOURCE_AUTHORIZATION_NOT_READY"
