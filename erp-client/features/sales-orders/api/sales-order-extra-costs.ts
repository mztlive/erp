/** 销售单附加费用：列表、财务登记与冲减（凭证随命令 multipart 提交）。 */

import { apiGet, apiPostForm } from "@/lib/api"

export type ExtraCostCategory =
    | "logistics"
    | "printing"
    | "storage"
    | "delivery"
    | "platform_tech"
    | "offline_service"
    | "rebate"
    | "other"

type ExtraCostReductionWire = {
    cost_entry_id: string
    description: string | null
    gross_amount: string
    occurred_at: number
    evidence_attachment_id: string | null
}

type SalesExtraCostWire = {
    source: "registration" | "purchase_expense"
    id: string
    category: ExtraCostCategory
    description: string
    gross_amount: string
    net_amount: string
    input_tax_rate: string
    status: "pending" | "occurred"
    occurred_at: number | null
    sales_order_line_id: string | null
    supplier_id: string | null
    purchase_order_id: string | null
    purchase_no: string | null
    evidence_attachment_id: string | null
    reduced_gross_amount: string
    remaining_gross_amount: string
    reductions: ExtraCostReductionWire[]
}

type SalesExtraCostListWire = {
    items: SalesExtraCostWire[]
    pending_gross_total: string
    occurred_gross_total: string
}

export type SalesExtraCostReduction = {
    costEntryId: string
    description: string
    grossAmount: string
    occurredAt: number
    evidenceAttachmentId: string | null
}

export type SalesExtraCost = {
    source: SalesExtraCostWire["source"]
    id: string
    category: ExtraCostCategory
    description: string
    grossAmount: string
    netAmount: string
    inputTaxRate: string
    status: SalesExtraCostWire["status"]
    occurredAt: number | null
    salesOrderLineId: string | null
    supplierId: string | null
    purchaseOrderId: string | null
    purchaseNo: string | null
    evidenceAttachmentId: string | null
    reducedGrossAmount: string
    remainingGrossAmount: string
    reductions: SalesExtraCostReduction[]
}

export type SalesExtraCostList = {
    items: SalesExtraCost[]
    pendingGrossTotal: string
    occurredGrossTotal: string
}

export type RegisterSalesExtraCostInput = {
    category: ExtraCostCategory
    description: string
    grossAmount: string
    inputTaxRate: string
    occurredOn: string
    supplierId?: string
    salesOrderLineId?: string
    evidence: File
    idempotencyKey: string
    inquirySource?: SalesExtraCostInquirySource
}

export type SalesExtraCostInquirySource = {
    inquiry_id: string
    feedback_round: number
    fee_id: string
    allocation_id: string
}

export type SalesInquiryFeeCandidate = SalesExtraCostInquirySource & {
    inquiry_no: string
    name: string
    tax_rate: string
    available_amount: string
    valid_until: number
    public_note: string
    unavailable_reason: string | null
}

export function fetchSalesInquiryFeeCandidates(salesOrderId: string) {
    return apiGet<{ items: SalesInquiryFeeCandidate[] }>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/inquiry-fees`,
    )
}

export type ReduceSalesExtraCostInput = {
    description: string
    grossAmount: string
    occurredOn: string
    evidence: File
    idempotencyKey: string
}

type ExtraCostCommandResultWire = { cost_entry_id: string; replayed: boolean }

/** 凭证在 multipart 中的临时引用名。 */
const EVIDENCE_REFERENCE = "pending-file:extra-cost-evidence"

function mapItem(item: SalesExtraCostWire): SalesExtraCost {
    return {
        source: item.source,
        id: item.id,
        category: item.category,
        description: item.description,
        grossAmount: item.gross_amount,
        netAmount: item.net_amount,
        inputTaxRate: item.input_tax_rate,
        status: item.status,
        occurredAt: item.occurred_at,
        salesOrderLineId: item.sales_order_line_id,
        supplierId: item.supplier_id,
        purchaseOrderId: item.purchase_order_id,
        purchaseNo: item.purchase_no,
        evidenceAttachmentId: item.evidence_attachment_id,
        reducedGrossAmount: item.reduced_gross_amount,
        remainingGrossAmount: item.remaining_gross_amount,
        reductions: item.reductions.map((reduction) => ({
            costEntryId: reduction.cost_entry_id,
            description: reduction.description ?? "",
            grossAmount: reduction.gross_amount,
            occurredAt: reduction.occurred_at,
            evidenceAttachmentId: reduction.evidence_attachment_id,
        })),
    }
}

export async function fetchSalesExtraCosts(
    salesOrderId: string,
): Promise<SalesExtraCostList> {
    const data = await apiGet<SalesExtraCostListWire>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/extra-costs`,
    )
    return {
        items: data.items.map(mapItem),
        pendingGrossTotal: data.pending_gross_total,
        occurredGrossTotal: data.occurred_gross_total,
    }
}

function commandForm(command: Record<string, unknown>, evidence: File) {
    const form = new FormData()
    form.append(
        "command",
        JSON.stringify({
            ...command,
            evidence_attachment_id: EVIDENCE_REFERENCE,
        }),
    )
    form.append(EVIDENCE_REFERENCE, evidence, evidence.name)
    return form
}

export function registerSalesExtraCost(
    salesOrderId: string,
    input: RegisterSalesExtraCostInput,
) {
    return apiPostForm<ExtraCostCommandResultWire>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/extra-costs`,
        commandForm(
            {
                category: input.category,
                description: input.description,
                gross_amount: input.grossAmount,
                input_tax_rate: input.inputTaxRate,
                occurred_on: input.occurredOn,
                supplier_id: input.supplierId || null,
                sales_order_line_id: input.salesOrderLineId || null,
                idempotency_key: input.idempotencyKey,
                inquiry_source: input.inquirySource ?? null,
            },
            input.evidence,
        ),
    )
}

export function reduceSalesExtraCost(
    salesOrderId: string,
    costEntryId: string,
    input: ReduceSalesExtraCostInput,
) {
    return apiPostForm<ExtraCostCommandResultWire>(
        `/admin/sales-orders/${encodeURIComponent(salesOrderId)}/extra-costs/${encodeURIComponent(costEntryId)}/reductions`,
        commandForm(
            {
                description: input.description,
                gross_amount: input.grossAmount,
                occurred_on: input.occurredOn,
                idempotency_key: input.idempotencyKey,
            },
            input.evidence,
        ),
    )
}
