import type { CompleteSupplierSupplyExceptionTaskInput } from "../types"

export type SupplyExceptionCompletionPayload = Omit<
    CompleteSupplierSupplyExceptionTaskInput,
    "idempotencyKey"
>

/** 规范化 W21 完成意图，并生成按任务版本与完整载荷隔离的账本槽位。 */
export const supplyExceptionCompletionIntent = (
    payload: SupplyExceptionCompletionPayload,
): {
    slot: string
    prefix: string
    payload: SupplyExceptionCompletionPayload
} => {
    const normalized = {
        ...payload,
        evidenceReference: payload.evidenceReference.trim(),
        comment: payload.comment.trim(),
    }
    return {
        slot: JSON.stringify(normalized),
        prefix: `w21:${normalized.workItemId}:${normalized.expectedTaskVersion}:complete`,
        payload: normalized,
    }
}
