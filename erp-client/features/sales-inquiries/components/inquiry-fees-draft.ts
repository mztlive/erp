import { type PurchaseExpenseCategory } from "@/features/purchase-orders/types"
import {
    compareDecimal,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import type {
    InquiryFee,
    InquiryFeeAllocation,
    ProcurementInquiry,
    SalesInquiry,
} from "../types"

export type FeeInquiry = SalesInquiry | ProcurementInquiry

export type FeeDraft = {
    action: "allocate" | "expense"
    fee_id: string
    sales_order_id: string
    allocation_amount: string
    expense_allocation_id: string
    expense_amount: string
    supplier_id: string
    supplier_name: string
    payment_term_code: string
    category: PurchaseExpenseCategory
}

export const EMPTY_DRAFT: FeeDraft = {
    action: "allocate",
    fee_id: "",
    sales_order_id: "",
    allocation_amount: "",
    expense_allocation_id: "",
    expense_amount: "",
    supplier_id: "",
    supplier_name: "",
    payment_term_code: "",
    category: "other",
}
export const DECIMAL_OPTIONS = { maxScale: 2, outputScale: 2 } as const

export function sumAmounts(values: readonly string[]): string {
    return sumFixed(values, DECIMAL_OPTIONS)
}

export function remaining(amount: string, used: readonly string[]): string {
    return subtractFixed(amount, sumAmounts(used), DECIMAL_OPTIONS)
}

export function positiveAmount(value: string): boolean {
    try {
        return (
            /^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/.test(value.trim()) &&
            compareDecimal(value.trim(), "0", 2) > 0
        )
    } catch {
        return false
    }
}

export function taxPercent(value: string): string {
    return multiplyFixed(value, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })
}

export function feeUnavailable(
    inquiry: FeeInquiry,
    fee: InquiryFee,
    round: number,
): string | undefined {
    if (inquiry.status === "closed") return "询价已关闭，不能继续办理费用"
    const status = inquiry.fee_statuses.find(
        (status) =>
            status.fee_id === fee.fee_id && status.feedback_round === round,
    )
    if (!status) return "费用办理资格尚未核对，请刷新后重试"
    if (status.unavailable_reason)
        return status.unavailable_reason.replace(/^业务逻辑错误[:：]\s*/, "")
    return undefined
}

export type FeeSummary = {
    fee: InquiryFee
    unallocated: string
    reserved: string
    handled: string
    available: string
    reason?: string
}
export type AllocationSummary = {
    allocation: InquiryFeeAllocation
    fee: InquiryFee
    balance: string
    reason?: string
}
export type FeeConfirmation = {
    kind: "allocate" | "expense"
    path: string
    body: Record<string, unknown>
    fee_name: string
    sales_order_no: string
    amount: string
    supplier_name?: string
    payment_term_code?: string
}
