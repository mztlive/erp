/** 开票申请 API；所有调用由 TanStack Query 管理。 */
import { apiGet, apiPost, type Page } from "@/lib/api"
import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import {
    enrichSubmittedInvoiceRequest,
    submittedInvoiceRequest,
} from "./pending-detail"
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
    can_submit: boolean
    unavailable_reason: string | null
    invoice_title: string
    tax_number: string
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

function asRecord(value: unknown): Record<string, unknown> {
    if (!value || typeof value !== "object" || Array.isArray(value)) return {}
    return value as Record<string, unknown>
}

function textOf(value: unknown): string {
    return typeof value === "string" ? value.trim() : ""
}

function decimalText(value: unknown, fallback: string): string {
    return typeof value === "string" &&
        /^-?\d+(?:\.\d{1,2})?$/.test(value.trim())
        ? value.trim()
        : fallback
}

function versionOf(value: unknown): number {
    if (typeof value !== "number" || !Number.isFinite(value) || value <= 0)
        return 0
    return value
}

/** 认不出的状态不要当成草稿，否则刚提交成功的申请会藏起撤回。 */
function knownRequestStatus(status: unknown): RequestStatus | undefined {
    return typeof status === "string" &&
        Object.hasOwn(requestStatusLabels, status)
        ? (status as RequestStatus)
        : undefined
}

function nodesAreReadable(
    nodes: unknown,
): nodes is readonly { key?: unknown }[] {
    return (
        Array.isArray(nodes) &&
        nodes.every((node) => node && typeof node === "object")
    )
}

/** 定义节点缺失时映射会抛错，审批区直接不展示。 */
function readableApproval(approval: unknown): InvoiceRequest["approval"] {
    if (!approval || typeof approval !== "object" || Array.isArray(approval))
        return null
    const view = approval as NonNullable<InvoiceRequest["approval"]>
    if (view.definition && !nodesAreReadable(view.definition.nodes)) return null
    if (view.recent_history != null && !nodesAreReadable(view.recent_history))
        return null
    return {
        ...view,
        recent_history: view.recent_history ?? [],
        allowed_actions: Array.isArray(view.allowed_actions)
            ? view.allowed_actions
            : [],
    }
}

/**
 * 把范围行或扁平详情补成页面可读的申请。
 * 范围行金额在行上；完整详情金额在 `data` 里。缺字段留空，不在渲染期崩溃。
 * 状态缺失时不默认成草稿，提交快照里的审批中状态要留下来。
 */
export function normalizeInvoiceRequest(row: unknown): InvoiceRequest {
    const wire = asRecord(row)
    const nested = asRecord(wire.data)
    const knownStatus = knownRequestStatus(wire.status)
    const normalized: InvoiceRequest = {
        id: textOf(wire.id),
        version: versionOf(wire.version),
        request_no: textOf(wire.request_no),
        sales_order_id: textOf(wire.sales_order_id),
        sales_order_no: textOf(wire.sales_order_no),
        receivable_account_id: textOf(wire.receivable_account_id),
        customer_id: textOf(wire.customer_id),
        counterparty_party_id: textOf(wire.counterparty_party_id),
        created_by: textOf(wire.created_by) || textOf(wire.applicant_user_id),
        created_by_name:
            typeof wire.created_by_name === "string" ||
            wire.created_by_name === null
                ? wire.created_by_name
                : undefined,
        created_at:
            typeof wire.created_at === "number" && wire.created_at > 0
                ? wire.created_at
                : 0,
        status: knownStatus ?? "draft",
        data: {
            amount: decimalText(nested.amount ?? wire.amount, "0.00"),
            invoice_title:
                textOf(nested.invoice_title) || textOf(wire.invoice_title),
            tax_number: textOf(nested.tax_number) || textOf(wire.tax_number),
            invoice_content:
                textOf(nested.invoice_content) || textOf(wire.invoice_content),
            reason: textOf(nested.reason) || textOf(wire.reason),
        },
        invoiced_amount: decimalText(wire.invoiced_amount, "0.00"),
        work_item_id: textOf(wire.work_item_id) || null,
        approval: readableApproval(wire.approval),
    }
    return enrichSubmittedInvoiceRequest(normalized, knownStatus !== undefined)
}

/** 同一条件分页读取申请列表及总数。 */
export const listRequests = async (query: RequestQuery) => {
    const page = await apiGet<Page<unknown>>(
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
    const normalized = normalizeInvoiceRequest(
        await apiGet<unknown>(
            `/admin/sales-invoice-requests/${encodeURIComponent(id)}`,
        ),
    )
    if (normalized.id) return normalized
    const saved = submittedInvoiceRequest(id)
    if (saved) return saved
    throw new Error("开票申请详情缺少编号")
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
