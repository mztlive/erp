/** W12 供应商往来 · API 内部共享状态与工具（不属于公开导出面）。 */

import type {
    AllocationSessionView,
    FormalSubmitResult,
} from "@/features/supplier-payables/types"

export const LIST_PAGE_SIZE = 100

/** 核销草稿会话（页面内存态；回话时按 draftSessionId 复用） */
export const sessions = new Map<string, AllocationSessionView>()

/** 草稿表单快照（暂存，供恢复编辑） */
export const draftSnapshots = new Map<string, Record<string, unknown>>()

/** 提交幂等结果缓存（同 key 重复提交返回首次结果） */
export const submitIdempotency = new Map<string, FormalSubmitResult>()

let sessionSeq = 200

export function nextSessionId(): string {
    return `alloc_sup_${++sessionSeq}`
}

export function errorMessage(err: unknown, fallback: string): string {
    return err && typeof err === "object" && "message" in err
        ? String((err as { message: unknown }).message)
        : fallback
}
