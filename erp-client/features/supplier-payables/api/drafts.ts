/**
 * W12 供应商往来 · 草稿与结果查询请求（保存草稿、幂等结果解析）。
 * 共享状态见 api/shared。
 */

import {
    draftSnapshots,
    sessions,
    submitIdempotency,
    submitUnknownResolvers,
} from "@/features/supplier-payables/api/shared"
import type {
    FormalSubmitResult,
    SaveAllocationDraftInput,
} from "@/features/supplier-payables/types"

export async function saveAllocationDraft(
    input: SaveAllocationDraftInput,
): Promise<{ savedAt: string }> {
    draftSnapshots.set(input.draftSessionId, input.formSnapshot)
    const savedAt = new Date().toISOString()
    const s = sessions.get(input.draftSessionId)
    if (s) {
        sessions.set(input.draftSessionId, { ...s, draftSavedAt: savedAt })
    }
    return { savedAt }
}

export async function resolveUnknownResult(
    idempotencyKey: string,
): Promise<FormalSubmitResult | null> {
    const cached = submitIdempotency.get(idempotencyKey)
    if (cached?.status !== "unknown") return cached ?? null
    const resolve = submitUnknownResolvers.get(idempotencyKey)
    return resolve ? resolve() : cached
}
