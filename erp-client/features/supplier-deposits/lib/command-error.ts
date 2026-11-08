import { isApiError } from "@/lib/api/errors"

/**
 * 充值命令在断网、无法解析或 5xx 时不能证明服务端没有执行。
 * 非接口异常同样视为未知，避免把未收到的结果当成确定失败。
 */
export function isUncertainDepositCommandError(error: unknown): boolean {
    return (
        !isApiError(error) ||
        ["Network", "Parse", "Unknown"].includes(error.kind) ||
        (error.status ?? 0) >= 500
    )
}
