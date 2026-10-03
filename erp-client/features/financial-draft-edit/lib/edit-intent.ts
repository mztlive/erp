import { compareDecimal, sumFixed } from "@/lib/fixed-decimal"
import {
    receivedAtUnixSeconds,
    type FinancialDraft,
    type FinancialDraftAllocation,
    type FinancialDraftFormValues,
    type FinancialDraftKind,
} from "../types"

export type FinancialSaveIntent = {
    stage: "save"
    version: number
    values: FinancialDraftFormValues
    shouldSubmit: boolean
    confirmedUnchanged?: boolean
    latest?: FinancialDraft
}

export type FinancialSavedIntent = {
    stage: "saved"
    version: number
    values: FinancialDraftFormValues
    shouldSubmit: boolean
}

export type FinancialSubmitIntent = {
    stage: "submit"
    version: number
    idempotencyKey: string
    allocations?: FinancialDraftAllocation[]
}

export type FinancialUnknownIntent =
    | FinancialSaveIntent
    | FinancialSavedIntent
    | FinancialSubmitIntent

/** 保存确认只比较本次 PUT 的合法字段，金额比较使用十进制定点等值。 */
export function matchesSavedFields(
    kind: FinancialDraftKind,
    current: FinancialDraft,
    values: FinancialDraftFormValues,
) {
    try {
        if (compareDecimal(current.amount, values.amount, 2) !== 0) return false
        return kind === "customer_receipt"
            ? current.received_at ===
                  receivedAtUnixSeconds(values.receivedAt) &&
                  (current.bank_reference ?? "").trim() ===
                      values.bankReference.trim()
            : (current.reason_text ?? "").trim() === values.reasonText.trim()
    } catch {
        return false
    }
}

/** 冻结原核销来源，仅允许修改这些来源对应的金额。 */
export function submitAllocations(values: FinancialDraftFormValues) {
    return values.allocations.map((line) => ({
        receivable_entry_id: line.receivableEntryId,
        allocated_amount: line.allocatedAmount.trim(),
    }))
}

/** 回款核销总额不得大于实际到账金额。 */
export function allocationTotalWithinAmount(values: FinancialDraftFormValues) {
    try {
        const total = sumFixed(
            values.allocations.map((line) => line.allocatedAmount),
            {
                maxScale: 2,
                outputScale: 2,
            },
        )
        return compareDecimal(total, values.amount, 2) <= 0
    } catch {
        return false
    }
}
