/** 开票申请 API；所有调用由 TanStack Query 管理。 */
import { apiGet, apiPost, type Page } from "@/lib/api"
import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
export type RequestStatus = "draft" | "in_approval" | "approved" | "completed"
export const requestStatusLabels: Record<RequestStatus, string> = {
    draft: "草稿",
    in_approval: "审批中",
    approved: "待开票",
    completed: "已开票",
}
export type RequestData = {
    amount: string
    invoice_title: string
    tax_number: string
    invoice_content: string
    reason: string
}
export type InvoiceRequest = {
    id: string
    version: number
    request_no: string
    sales_order_id: string
    sales_order_no: string
    receivable_account_id: string
    customer_id: string
    counterparty_party_id: string
    created_by: string
    created_by_name?: string | null
    created_at: number
    status: RequestStatus
    data: RequestData
    invoiced_amount: string
    work_item_id?: string | null
    approval?: DocumentApprovalViewDto | null
}
export type RequestAmounts = {
    receivable_account_id: string
    available_amount: string
    pending_amount: string
    approved_remaining_amount: string
    invoiced_amount: string
}
export type RequestQuery = {
    sales_order_id?: string
    customer_id?: string
    receivable_account_id?: string
    work_item_id?: string
    status?: RequestStatus
    q?: string
    page?: number
    page_size?: number
}
export type SubmitRequest = {
    receivable_account_id: string
    request_id?: string
    expected_version?: number
    data: RequestData
    idempotency_key: string
}
/** 同一条件分页读取申请列表及总数。 */
export const listRequests = (query: RequestQuery) =>
    apiGet<Page<InvoiceRequest>>("/admin/sales-invoice-requests", query)
/** 读取申请和实际审批进度。 */
export const getRequest = (id: string) =>
    apiGet<InvoiceRequest>(
        `/admin/sales-invoice-requests/${encodeURIComponent(id)}`,
    )
/** 读取指定应收的可申请金额。 */
export const getRequestAmounts = (id: string) =>
    apiGet<RequestAmounts>(
        `/admin/sales-invoice-requests/amounts/${encodeURIComponent(id)}`,
    )
/** 原子提交申请；重试必须保持相同载荷和命令键。 */
export const submitRequest = (input: SubmitRequest) =>
    apiPost<InvoiceRequest>("/admin/sales-invoice-requests/submit", input)
/** 撤回本人申请，释放额度。 */
export const cancelRequest = (input: {
    id: string
    expected_version: number
    reason: string
    idempotency_key: string
}) => {
    const { id, ...body } = input
    return apiPost<InvoiceRequest>(
        `/admin/sales-invoice-requests/${encodeURIComponent(id)}/cancel`,
        body,
    )
}
