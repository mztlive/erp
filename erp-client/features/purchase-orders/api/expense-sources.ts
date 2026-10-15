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
