import { apiGet, apiPost, apiPut } from "@/lib/api"
import {
    FINANCIAL_DRAFTS,
    receivedAtUnixSeconds,
    type FinancialDraft,
    type FinancialDraftFormValues,
    type FinancialDraftKind,
    type FinancialDraftAllocation,
} from "./types"

function draftPath(kind: FinancialDraftKind, id: string) {
    return `/admin/${FINANCIAL_DRAFTS[kind].resource}/${encodeURIComponent(id)}`
}

/** 读取原单草稿；来源与单号只展示，不提供修改命令。 */
export function fetchFinancialDraft(kind: FinancialDraftKind, id: string) {
    return apiGet<FinancialDraft>(
        `${draftPath(kind, id)}${kind === "customer_receipt" ? "/draft" : ""}`,
    )
}

/** 保存原草稿的合法字段，以服务端当前版本检查冲突。 */
export function saveFinancialDraft(input: {
    kind: FinancialDraftKind
    id: string
    version: number
    values: FinancialDraftFormValues
}) {
    const body =
        input.kind === "customer_receipt"
            ? {
                  version: input.version,
                  amount: input.values.amount.trim(),
                  received_at: receivedAtUnixSeconds(input.values.receivedAt),
                  bank_reference: input.values.bankReference.trim(),
              }
            : {
                  version: input.version,
                  amount: input.values.amount.trim(),
                  reason_text: input.values.reasonText.trim(),
              }
    return apiPut<FinancialDraft>(draftPath(input.kind, input.id), body)
}

/** 只提交已保存的原单及新版本，重新启动审批。 */
export function submitFinancialDraft(input: {
    kind: FinancialDraftKind
    id: string
    version: number
    idempotencyKey: string
    allocations?: FinancialDraftAllocation[]
}) {
    return apiPost<FinancialDraft>(
        `${draftPath(input.kind, input.id)}/submit`,
        {
            expected_version: input.version,
            idempotency_key: input.idempotencyKey,
            ...(input.kind === "customer_receipt"
                ? { allocations: input.allocations ?? [] }
                : {}),
        },
    )
}
