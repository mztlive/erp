/** W12 供应商往来 · 无状态 API 工具（不属于公开导出面）。 */

import { classifyFormalCommandError } from "@/lib/formal-command"

export const LIST_PAGE_SIZE = 100

export function nextSessionId(): string {
    return `alloc_sup_${crypto.randomUUID()}`
}

/**
 * 结束上一笔核销尝试并生成全新的会话身份。
 *
 * 成功的分次付款不得复用上一笔会话或幂等结果，否则下一笔会被客户端缓存
 * 误判为上一笔的重复提交。
 */
export function beginFreshAllocationAttempt(
    _previousDraftSessionId: string | undefined,
    _previousIdempotencyKey: string | null,
): string {
    return nextSessionId()
}

export function errorMessage(err: unknown, fallback: string): string {
    return err && typeof err === "object" && "message" in err
        ? String((err as { message: unknown }).message)
        : fallback
}

/** 判断正式命令是否处于无法证明成功或失败的结果未知状态。 */
export function isOutcomeUnknown(err: unknown): boolean {
    const backendMarkedUnknown =
        err !== null &&
        typeof err === "object" &&
        "code" in err &&
        (err as { code?: unknown }).code === "OUTCOME_UNKNOWN"
    return backendMarkedUnknown || classifyFormalCommandError(err) === "unknown"
}
