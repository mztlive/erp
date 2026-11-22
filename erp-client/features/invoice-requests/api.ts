/** 开票申请 API；所有调用由 TanStack Query 管理。 */
import { apiGet, apiPost, apiPut, type Page } from "@/lib/api"
import { compactFixed, normalizeFixed } from "@/lib/fixed-decimal"
import type {
    InvoiceRequestData,
    InvoiceRequestLine,
} from "@/lib/invoice-request-data"
import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import {
    INVOICE_CONTENT_LIMIT,
    INVOICE_QUANTITY_SCALE,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceLineAmount,
    isPositiveInvoiceDecimal,
} from "./line-amounts"
import {
    enrichSubmittedInvoiceRequest,
    submittedInvoiceRequest,
} from "./pending-detail"
export type {
    InvoiceRequestData,
    InvoiceRequestLine,
} from "@/lib/invoice-request-data"

export type RequestStatus =
    | "draft"
    | "rejected"
    | "in_approval"
    | "approved"
    | "completed"
export const requestStatusLabels: Record<RequestStatus, string> = {
    draft: "草稿",
    rejected: "已驳回",
    in_approval: "审批中",
    approved: "待开票",
    completed: "已开票",
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
    data: InvoiceRequestData
    invoiced_amount: string
    work_item_id?: string | null
    approval?: DocumentApprovalViewDto | null
}
export type RequestAmounts = {
    can_submit: boolean
    unavailable_reason: string | null
    invoice_title: string
    tax_number: string
    seller_name?: string | null
    seller_tax_number?: string | null
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
    resubmission_reason?: string
    receivable_account_id: string
    request_id?: string
    expected_version?: number
    data: InvoiceRequestData
    idempotency_key: string
}

function asRecord(value: unknown): Record<string, unknown> {
    if (!value || typeof value !== "object" || Array.isArray(value)) return {}
    return value as Record<string, unknown>
}

function textOf(value: unknown): string {
    return typeof value === "string" ? value.trim() : ""
}

function optionalText(value: unknown): string | null | undefined {
    return value === null ? null : textOf(value) || undefined
}

function requestLines(value: unknown): InvoiceRequestLine[] | undefined {
    if (!Array.isArray(value)) return undefined
    return value.map((entry) => {
        const line = asRecord(entry)
        return {
            content: textOf(line.content),
            quantity: textOf(line.quantity),
            unit_price: textOf(line.unit_price),
        }
    })
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
            seller_name: optionalText(
                nested.seller_name === undefined
                    ? wire.seller_name
                    : nested.seller_name,
            ),
            seller_tax_number: optionalText(
                nested.seller_tax_number === undefined
                    ? wire.seller_tax_number
                    : nested.seller_tax_number,
            ),
            lines: requestLines(nested.lines) ?? requestLines(wire.lines),
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

/** 原单保存不启动审批；没有命令回执时不得以最新资料推断原保存成功。 */
export type UpdateRequest = {
    id: string
    expected_version: number
    data: InvoiceRequestData
}
export async function updateRequest(
    input: UpdateRequest,
): Promise<InvoiceRequest> {
    const { id, ...body } = input
    return apiPut<InvoiceRequest>(
        `/admin/sales-invoice-requests/${encodeURIComponent(id)}`,
        body,
    )
}

/** 核对保存后的服务端版本和状态，不使用提交快照补齐缺失字段。 */
export async function getRequestSaveState(
    id: string,
): Promise<Pick<InvoiceRequest, "id" | "version" | "status">> {
    const wire = asRecord(
        await apiGet<unknown>(
            `/admin/sales-invoice-requests/${encodeURIComponent(id)}`,
        ),
    )
    const status = knownRequestStatus(wire.status)
    if (
        wire.id !== id ||
        typeof wire.version !== "number" ||
        !Number.isSafeInteger(wire.version) ||
        wire.version < 1 ||
        !status
    ) {
        throw new Error("未取得完整的服务端申请版本和状态，请重新核对")
    }
    return { id, version: wire.version, status }
}

export type SalesOrderInvoiceCandidateLine = {
    id: string
    lineNo: number
    content: string
    quantity: string
    unitPrice: string
    unit?: string
    lineAmount: string | null
    isGift: boolean
    selectable: boolean
    disabledReason?: string
}

export type SalesOrderInvoiceLinesResult = {
    orderNo: string
    lines: SalesOrderInvoiceCandidateLine[]
}

const SNAPSHOT_IDENTITY_PATTERN = /^[0-9a-f]{24}(?:[0-9a-f]{8})?$/i

function cleanSnapshotLabel(value: unknown, skuId?: string): string {
    const text = textOf(value)
    if (!text) return ""
    if (skuId && text === skuId) return ""
    if (SNAPSHOT_IDENTITY_PATTERN.test(text)) return ""
    return text
}

function normalizeCandidateDecimal(raw: string, maxScale: number): string {
    const trimmed = raw.trim()
    const match = /^(\d+)(?:\.(\d+))?$/.exec(trimmed)
    if (!match) return ""
    const whole = match[1].replace(/^0+(?=\d)/, "")
    const fraction = match[2] ?? ""
    const canonical = fraction ? `${whole}.${fraction}` : whole
    if (fraction.length <= maxScale) {
        return compactFixed(canonical)
    }
    return compactFixed(
        normalizeFixed(canonical, {
            maxScale: fraction.length,
            outputScale: maxScale,
        }),
    )
}

function extractCommercialLines(
    wire: Record<string, unknown>,
): Record<string, unknown>[] {
    const currentRevisionId = textOf(wire.current_revision_id)
    const revisions = Array.isArray(wire.revisions) ? wire.revisions : []
    if (currentRevisionId) {
        const current = revisions
            .map(asRecord)
            .find((revision) => textOf(revision.id) === currentRevisionId)
        if (current && Array.isArray(current.commercial_lines)) {
            return current.commercial_lines.map(asRecord)
        }
    }
    const workingCopy = asRecord(wire.working_copy)
    if (Array.isArray(workingCopy.lines) && workingCopy.lines.length > 0) {
        return workingCopy.lines.map(asRecord)
    }
    const submissions = Array.isArray(wire.submissions)
        ? wire.submissions.map(asRecord)
        : []
    const latestSubmission = submissions.reduce<Record<string, unknown> | null>(
        (latest, entry) => {
            if (!Array.isArray(entry.lines) || entry.lines.length === 0) {
                return latest
            }
            if (!latest) return entry
            const currentNo =
                typeof entry.submission_no === "number"
                    ? entry.submission_no
                    : 0
            const latestNo =
                typeof latest.submission_no === "number"
                    ? latest.submission_no
                    : 0
            return currentNo >= latestNo ? entry : latest
        },
        null,
    )
    if (latestSubmission && Array.isArray(latestSubmission.lines)) {
        return latestSubmission.lines.map(asRecord)
    }
    return []
}

/** 读取销售单当前生效版本的商业明细，用于一键带入开票明细。 */
export async function getSalesOrderInvoiceLines(
    salesOrderId: string,
): Promise<SalesOrderInvoiceLinesResult> {
    const wire = asRecord(
        await apiGet<unknown>(
            `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
        ),
    )
    const orderNo = textOf(wire.order_no)
    const rawLines = extractCommercialLines(wire)
    const lines: SalesOrderInvoiceCandidateLine[] = rawLines.map(
        (line, index) => {
            const lineNo =
                typeof line.line_no === "number" &&
                Number.isSafeInteger(line.line_no) &&
                line.line_no > 0
                    ? line.line_no
                    : index + 1
            const id =
                textOf(line.sales_order_line_id) ||
                textOf(line.id) ||
                `line-${lineNo}`
            const skuId = textOf(line.sku_id) || undefined
            const name = textOf(line.item_name_snapshot)
            const spec = cleanSnapshotLabel(line.spec_snapshot, skuId)
            const rawContent =
                spec && name && !name.includes(spec)
                    ? `${name}（${spec}）`
                    : name || spec
            const content = rawContent.slice(0, INVOICE_CONTENT_LIMIT)
            const isVoucher = textOf(line.line_type) === "VOUCHER"
            const isGift = textOf(line.pricing_mode) === "GIFT"
            const cardCountText =
                typeof line.card_count === "number" &&
                Number.isSafeInteger(line.card_count) &&
                line.card_count > 0
                    ? String(line.card_count)
                    : ""
            const rawQuantity = isVoucher
                ? cardCountText || textOf(line.quantity)
                : textOf(line.quantity)
            const quantity = normalizeCandidateDecimal(
                rawQuantity,
                INVOICE_QUANTITY_SCALE,
            )
            const unitPrice = normalizeCandidateDecimal(
                textOf(line.unit_price_gross),
                INVOICE_UNIT_PRICE_SCALE,
            )
            const unit =
                cleanSnapshotLabel(line.unit_snapshot) ||
                cleanSnapshotLabel(line.base_unit_code) ||
                (isVoucher ? "张" : "") ||
                undefined
            const lineAmount = invoiceLineAmount({
                quantity,
                unit_price: unitPrice,
            })
            const hasPositiveAmount =
                !isGift &&
                Boolean(content) &&
                isPositiveInvoiceDecimal(quantity, INVOICE_QUANTITY_SCALE) &&
                isPositiveInvoiceDecimal(unitPrice, INVOICE_UNIT_PRICE_SCALE) &&
                lineAmount !== null &&
                lineAmount !== "0.00"
            return {
                id,
                lineNo,
                content,
                quantity,
                unitPrice,
                unit,
                lineAmount,
                isGift,
                selectable: hasPositiveAmount,
                disabledReason: isGift
                    ? "赠品行，不可开票"
                    : !hasPositiveAmount
                      ? "金额为零或缺少单价，不可直接带入"
                      : undefined,
            }
        },
    )
    return { orderNo, lines }
}
