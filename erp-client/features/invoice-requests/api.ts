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

/** 列表/详情现在返回扁平范围行：金额在行上，不再保证嵌套 `data`。 */
type InvoiceRequestWire = Partial<InvoiceRequest> & {
    amount?: string
    applicant_user_id?: string
}

function decimalText(value: unknown, fallback: string): string {
    return typeof value === "string" && /^-?\d+(?:\.\d{1,2})?$/.test(value.trim())
        ? value.trim()
        : fallback
}

function requestStatusOf(status: unknown): RequestStatus {
    return typeof status === "string" && status in requestStatusLabels
        ? (status as RequestStatus)
        : "draft"
}

/** 定义节点缺失时映射会抛错，审批区直接不展示。 */
function readableApproval(
    approval: InvoiceRequest["approval"],
): InvoiceRequest["approval"] {
    if (!approval || typeof approval !== "object") return null
    if (approval.definition && !Array.isArray(approval.definition.nodes))
        return null
    if (
        approval.recent_history != null &&
        !Array.isArray(approval.recent_history)
    ) {
        return null
    }
    return {
        ...approval,
        recent_history: approval.recent_history ?? [],
        allowed_actions: Array.isArray(approval.allowed_actions)
            ? approval.allowed_actions
            : [],
    }
}

/** 把范围行或旧详情补成页面可读的申请。缺字段留空，不在渲染期取值崩溃。 */
export function normalizeInvoiceRequest(row: InvoiceRequestWire): InvoiceRequest {
    const nested = row.data
    return {
        id: row.id ?? "",
        version: typeof row.version === "number" ? row.version : 0,
        request_no: row.request_no ?? "",
        sales_order_id: row.sales_order_id ?? "",
        sales_order_no: row.sales_order_no ?? "",
        receivable_account_id: row.receivable_account_id ?? "",
        customer_id: row.customer_id ?? "",
        counterparty_party_id: row.counterparty_party_id ?? "",
        created_by: row.created_by || row.applicant_user_id || "",
        created_by_name: row.created_by_name,
        created_at: typeof row.created_at === "number" ? row.created_at : 0,
        status: requestStatusOf(row.status),
        data: {
            amount: decimalText(nested?.amount ?? row.amount, "0.00"),
            invoice_title: nested?.invoice_title ?? "",
            tax_number: nested?.tax_number ?? "",
            invoice_content: nested?.invoice_content ?? "",
            reason: nested?.reason ?? "",
        },
        invoiced_amount: decimalText(row.invoiced_amount, "0.00"),
        work_item_id: row.work_item_id ?? null,
        approval: readableApproval(row.approval),
    }
}

/** 同一条件分页读取申请列表及总数。 */
export const listRequests = async (query: RequestQuery) => {
    const page = await apiGet<Page<InvoiceRequestWire>>(
        "/admin/sales-invoice-requests",
        query,
    )
    return {
        ...page,
        items: (page.items ?? []).map(normalizeInvoiceRequest),
    }
}
/** 读取申请和实际审批进度。范围详情是扁平行，读完再补齐嵌套资料。 */
export const getRequest = async (id: string) => {
    const row = await apiGet<InvoiceRequestWire>(
        `/admin/sales-invoice-requests/${encodeURIComponent(id)}`,
    )
    return normalizeInvoiceRequest(row ?? {})
}
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
