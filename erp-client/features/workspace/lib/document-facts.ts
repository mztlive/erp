import { apiGet } from "@/lib/api"
import type {
    BackendSalesOrderDetail,
    BackendSubmission,
    BackendWorkingCopy,
    BackendWorkingCopyLine,
} from "@/features/sales-orders/api/contracts"
import type {
    BackendPaymentReversal,
    BackendSupplierPayment,
} from "@/features/supplier-payables/api/mappers"

export type WorkspaceFactSection = Readonly<{
    label: string
    value: string
    numeric?: boolean
    objectId?: string
}>

export type WorkspaceFactLine = Readonly<{
    title: string
    quantity?: string
    dueLabel?: string
}>

export type WorkspaceDocumentFacts = Readonly<{
    counterparty?: string
    impact: string
    listSummary?: string
    sections: readonly WorkspaceFactSection[]
    lines: readonly WorkspaceFactLine[]
    moreCount: number
}>

const LINE_LIMIT = 3

const SALES_ORDER_TYPES = new Set([
    "sales_order",
    "voucher_sales_order",
    "salesorder",
])

const RECEIPT_TYPES = new Set(["customer_receipt", "customerreceipt"])

const PAYMENT_REVERSAL_TYPES = new Set(["payment_reversal", "paymentreversal"])

/**
 * 把工作台任务的业务对象类型收成小写稳定码。
 */
export function normalizeObjectType(businessObjectType: string): string {
    return businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
}

/**
 * 工作台任务是否应补拉单据事实。
 */
export function shouldLoadDocumentFacts(input: {
    businessObjectType: string
    hasSummary: boolean
}): boolean {
    if (input.hasSummary) return false
    const kind = normalizeObjectType(input.businessObjectType)
    return (
        SALES_ORDER_TYPES.has(kind) ||
        RECEIPT_TYPES.has(kind) ||
        PAYMENT_REVERSAL_TYPES.has(kind)
    )
}

/**
 * 按单据类型读取工作台只读事实。未知类型返回 null。
 */
export async function fetchWorkspaceDocumentFacts(input: {
    businessObjectType: string
    businessObjectId: string
}): Promise<WorkspaceDocumentFacts | null> {
    const id = input.businessObjectId.trim()
    if (!id) return null
    const kind = normalizeObjectType(input.businessObjectType)
    if (SALES_ORDER_TYPES.has(kind)) return loadSalesOrderFacts(id)
    if (RECEIPT_TYPES.has(kind)) return loadCustomerReceiptFacts(id)
    if (PAYMENT_REVERSAL_TYPES.has(kind)) {
        return loadPaymentReversalFacts(id)
    }
    return null
}

type CustomerReceiptDto = Readonly<{
    receipt_no: string
    amount: string
    received_at: number
    bank_reference?: string | null
    unallocated_amount?: string | null
    allocated_total?: string | null
    allocations?: readonly Readonly<{
        allocated_amount: string
        receivable_entry_id: string
    }>[]
}>

async function loadSalesOrderFacts(
    salesOrderId: string,
): Promise<WorkspaceDocumentFacts | null> {
    const detail = await apiGet<BackendSalesOrderDetail>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}`,
    )
    return salesOrderDocumentFacts(detail)
}

/**
 * 把销售单详情转成工作台只读事实。
 */
export function salesOrderDocumentFacts(
    detail: BackendSalesOrderDetail,
): WorkspaceDocumentFacts | null {
    const content = pickSalesContent(detail)
    if (!content) return null
    return salesContentToFacts(detail.business_type, content)
}

async function loadCustomerReceiptFacts(
    receiptId: string,
): Promise<WorkspaceDocumentFacts | null> {
    const receipt = await apiGet<CustomerReceiptDto>(
        `/admin/customer-receipts/${encodeURIComponent(receiptId)}`,
    )
    const amount = formatYuan(receipt.amount)
    const sections: WorkspaceFactSection[] = [
        { label: "含税金额", value: amount, numeric: true },
        { label: "到账日", value: formatDate(receipt.received_at) },
    ]
    pushSection(sections, "银行流水", receipt.bank_reference)
    if (receipt.unallocated_amount) {
        sections.push({
            label: "未分配",
            value: formatYuan(receipt.unallocated_amount),
            numeric: true,
        })
    }
    const lines = (receipt.allocations ?? [])
        .slice(0, LINE_LIMIT)
        .map((row): WorkspaceFactLine => ({
            title: "核销应收",
            quantity: formatYuan(row.allocated_amount),
        }))
    return {
        impact: "不审批则回款不能过账、不能核销应收",
        listSummary: joinSummary([amount, receipt.bank_reference ?? undefined]),
        sections,
        lines,
        moreCount: Math.max(0, (receipt.allocations?.length ?? 0) - LINE_LIMIT),
    }
}

async function loadPaymentReversalFacts(
    reversalId: string,
): Promise<WorkspaceDocumentFacts> {
    const reversal = await apiGet<BackendPaymentReversal>(
        `/admin/payment-reversals/${encodeURIComponent(reversalId)}`,
    )
    const payment = await apiGet<BackendSupplierPayment>(
        `/admin/supplier-payments/${encodeURIComponent(
            reversal.original_supplier_payment_id,
        )}`,
    )
    return paymentReversalDocumentFacts(reversal, payment)
}

/**
 * 把付款冲正与原付款裁剪成工作台只读事实。
 *
 * 待审批冲正只说明潜在影响，不修改付款金额与状态。
 */
export function paymentReversalDocumentFacts(
    reversal: BackendPaymentReversal,
    payment: BackendSupplierPayment,
): WorkspaceDocumentFacts {
    const reversalAmount = formatYuan(reversal.amount)
    const sections: WorkspaceFactSection[] = [
        { label: "冲正金额", value: reversalAmount, numeric: true },
        {
            label: "原付款单",
            value: payment.payment_no,
            objectId: payment.id,
        },
        {
            label: "原付款金额",
            value: formatYuan(payment.amount),
            numeric: true,
        },
        { label: "冲正原因", value: reversal.reason_text },
        { label: "冲正日期", value: formatDate(reversal.occurred_at) },
        { label: "付款日期", value: formatDate(payment.paid_at) },
    ]
    pushSection(sections, "供应商", payment.supplier_name)
    pushSection(sections, "银行流水", payment.bank_reference)
    const lines = payment.allocations
        .slice(0, LINE_LIMIT)
        .map((allocation): WorkspaceFactLine => ({
            title: allocation.source_document_no?.trim() || "原付款核销明细",
            quantity: formatYuan(allocation.allocated_amount),
        }))
    return {
        counterparty: emptyToUndefined(payment.supplier_name),
        impact: "审批通过前原付款保持不变；通过后系统追加冲正记录并回冲原付款。",
        listSummary: joinSummary([
            payment.supplier_name ?? undefined,
            reversalAmount,
            payment.payment_no,
        ]),
        sections,
        lines,
        moreCount: Math.max(0, payment.allocations.length - LINE_LIMIT),
    }
}

function pickSalesContent(
    detail: BackendSalesOrderDetail,
): BackendSubmission | BackendWorkingCopy | null {
    const inReview = [...detail.submissions]
        .filter((item) => item.status === "IN_REVIEW")
        .sort((a, b) => b.submission_no - a.submission_no)[0]
    if (inReview) return inReview
    const latest = [...detail.submissions].sort(
        (a, b) => b.submission_no - a.submission_no,
    )[0]
    return latest ?? detail.working_copy ?? null
}

function salesContentToFacts(
    businessType: string,
    content: BackendSubmission | BackendWorkingCopy,
): WorkspaceDocumentFacts {
    const voucher = businessType === "VOUCHER"
    const linesSource = content.lines ?? []
    const moreCount = Math.max(0, linesSource.length - LINE_LIMIT)
    const lines = linesSource.slice(0, LINE_LIMIT).map(mapLine)
    const sections: WorkspaceFactSection[] = []
    pushSection(sections, "客户", content.customer_name)
    pushSection(sections, "业务性质", voucher ? "卡券" : "实物及服务")
    pushSection(sections, "结算主体", content.settlement_party_name)
    pushSection(sections, "合同", content.contract_no)
    sections.push({
        label: "含税金额",
        value: formatYuan(content.gross_amount),
        numeric: true,
    })
    sections.push({
        label: "不含税金额",
        value: formatYuan(content.net_amount),
        numeric: true,
    })
    if (Number(content.tax_amount) > 0) {
        sections.push({
            label: "税额",
            value: formatYuan(content.tax_amount),
            numeric: true,
        })
    }
    pushSection(sections, "付款条件", content.payment_term_name)
    pushSection(sections, "项目", content.project_name)
    const first = lines[0]
    return {
        counterparty: emptyToUndefined(content.customer_name),
        impact: voucher
            ? "不审批则卡券销售不能生效"
            : "不审批则销售单不能生效、不能履约",
        listSummary: joinSummary([
            content.customer_name,
            formatYuan(content.gross_amount),
            content.payment_term_name ?? undefined,
            first
                ? `${first.title}${first.quantity ? ` ${first.quantity}` : ""}`
                : undefined,
        ]),
        sections,
        lines,
        moreCount,
    }
}

function mapLine(line: BackendWorkingCopyLine): WorkspaceFactLine {
    const title = [line.item_name_snapshot, line.spec_snapshot]
        .map((part) => part?.trim())
        .filter(Boolean)
        .join(" ")
    const amount = formatYuan(line.gross_amount)
    let quantity = amount
    if (line.card_count && line.card_count > 0) {
        quantity = `${line.card_count} 张 · ${amount}`
    } else if (line.quantity) {
        const unit = line.unit_snapshot || line.base_unit_code || ""
        quantity = `${line.quantity}${unit ? ` ${unit}` : ""} · ${amount}`
    }
    return {
        title: title || "未命名明细",
        quantity,
        dueLabel: line.fulfillment_due_at
            ? `${formatMonthDay(line.fulfillment_due_at)} 交`
            : undefined,
    }
}

function pushSection(
    sections: WorkspaceFactSection[],
    label: string,
    value?: string | null,
) {
    const text = value?.trim()
    if (!text) return
    sections.push({ label, value: text })
}

function joinSummary(parts: Array<string | undefined>): string {
    return parts
        .map((part) => part?.trim())
        .filter((part): part is string => Boolean(part))
        .join(" · ")
}

function emptyToUndefined(value?: string | null): string | undefined {
    const text = value?.trim()
    return text ? text : undefined
}

function formatYuan(raw: string): string {
    const value = Number(raw)
    if (!Number.isFinite(value)) return raw
    return `¥${value.toLocaleString("zh-CN", {
        maximumFractionDigits: 2,
    })}`
}

function formatDate(unixSecs: number): string {
    const date = new Date(unixSecs * 1000)
    if (Number.isNaN(date.getTime())) return "—"
    const year = date.getFullYear()
    const month = String(date.getMonth() + 1).padStart(2, "0")
    const day = String(date.getDate()).padStart(2, "0")
    return `${year}-${month}-${day}`
}

function formatMonthDay(unixSecs: number): string {
    const date = new Date(unixSecs * 1000)
    if (Number.isNaN(date.getTime())) return ""
    return `${date.getMonth() + 1}/${date.getDate()}`
}
