/**
 * 履约作业表单的初始草稿。
 * TanStack Form 每次渲染都会把 hook 里的 defaultValues 写回 store；
 * 占位草稿必须与当前作业类型一致，不能固定成入库。
 */

import { cloneDraft } from "@/features/fulfillment-operations/lib/validation"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
} from "@/features/fulfillment-operations/types"

export const EMPTY_FULFILLMENT_DRAFT: FulfillmentDraft = {
    type: "RECEIPT",
    warehouseId: "",
    warehouseLabel: "",
    occurredAt: "",
    lines: [],
}

/**
 * 按当前作业给出表单身份和初始草稿。
 *
 * @param operation 当前履约作业；队列未就绪时为空。
 * @returns 表单 formId 与应对齐的草稿。
 */
export function fulfillmentDraftFormSeed(
    operation: FulfillmentOperation | undefined,
): { formId: string; draft: FulfillmentDraft } {
    if (!operation) {
        return {
            formId: "fulfillment-draft-empty",
            draft: EMPTY_FULFILLMENT_DRAFT,
        }
    }
    return {
        formId: `fulfillment-draft-${operation.operationId}`,
        draft: operation.draft,
    }
}

/**
 * 表单 store 与作业类型不一致时，改用作业自带草稿，避免画出入库表单。
 *
 * @param operation 当前履约作业；无作业时返回空。
 * @param formDraft 表单 store 中的草稿。
 * @returns 可展示、可校验的草稿。
 */
export function activeFulfillmentDraft(
    operation: FulfillmentOperation | undefined,
    formDraft: FulfillmentDraft,
): FulfillmentDraft | null {
    if (!operation) return null
    if (formDraft.type === operation.operationType) return formDraft
    return cloneDraft(operation.draft)
}
