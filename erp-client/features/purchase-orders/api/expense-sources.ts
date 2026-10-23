import { apiGet } from "@/lib/api"

export type ExpenseSource = {
    id: string
    order_no: string
    customer_name?: string | null
    status_label: string
}
export type ExpenseSourceDetail = ExpenseSource & {
    lines: Array<{ id: string; name: string }>
}
export type ExpenseSourceQuery = {
    q: string
    page: number
    scopeVersion?: string
}
export type ExpenseSourceList = {
    items: ExpenseSource[]
    total: number
    page: number
    page_size: number
    scope_version: string
    empty_reason?: string | null
}
/** 仅费用来源范围内可读取的询价费用，不依赖已经结束的询价处理任务。 */
export type InquiryExpenseCandidate = {
    inquiry_id: string
    inquiry_no: string
    feedback_round: number
    fee_id: string
    allocation_id: string
    name: string
    tax_rate: string
    available_amount: string
    public_note: string
    unavailable_reason: string | null
    payee_supplier_id: string | null
    payment_term_code: string | null
}

export function fetchInquiryExpenseCandidates(salesOrderId: string) {
    return apiGet<{ items: InquiryExpenseCandidate[] }>(
        `/admin/purchase-orders/expense-sources/${encodeURIComponent(salesOrderId)}/inquiry-fees`,
    )
}
export function fetchExpenseSources(input: ExpenseSourceQuery) {
    const params = new URLSearchParams({ q: input.q, page: String(input.page) })
    if (input.scopeVersion) params.set("scope_version", input.scopeVersion)
    return apiGet<ExpenseSourceList>(
        `/admin/purchase-orders/expense-sources?${params}`,
    )
}
export function fetchExpenseSource(id: string) {
    return apiGet<ExpenseSourceDetail>(
        `/admin/purchase-orders/expense-sources/${encodeURIComponent(id)}`,
    )
}
