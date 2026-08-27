/** W12 供应商往来 · API 内部共享状态与工具（不属于公开导出面）。 */

import type {
    AllocationSessionView,
    FormalSubmitResult,
} from "@/features/supplier-payables/types"
import { classifyFormalCommandError } from "@/lib/formal-command"

export const LIST_PAGE_SIZE = 100

/** 核销草稿会话（页面内存态；回话时按 draftSessionId 复用） */
export const sessions = new Map<string, AllocationSessionView>()

/** 草稿表单快照（暂存，供恢复编辑） */
export const draftSnapshots = new Map<string, Record<string, unknown>>()

/** 提交幂等结果缓存（同 key 重复提交返回首次结果） */
export const submitIdempotency = new Map<string, FormalSubmitResult>()

/** 结果待确认时使用原业务载荷和原操作号重新核对。 */
export const submitUnknownResolvers = new Map<
    string,
    () => Promise<FormalSubmitResult>
>()

let sessionSeq = 200

export function nextSessionId(): string {
    return `alloc_sup_${++sessionSeq}`
}

/**
 * 结束上一笔核销尝试并生成全新的会话身份。
 *
 * 成功的分次付款不得复用上一笔会话或幂等结果，否则下一笔会被客户端缓存
 * 误判为上一笔的重复提交。
 */
export function beginFreshAllocationAttempt(
    previousDraftSessionId: string | undefined,
    previousIdempotencyKey: string | null,
): string {
    if (previousDraftSessionId) {
        sessions.delete(previousDraftSessionId)
        draftSnapshots.delete(previousDraftSessionId)
    }
    if (previousIdempotencyKey) {
        submitIdempotency.delete(previousIdempotencyKey)
        submitUnknownResolvers.delete(previousIdempotencyKey)
    }
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
