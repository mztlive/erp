import {
    SOURCE_TYPE_LABEL,
    type PayableSourceType,
    type PaymentAllocationLine,
    type PaymentRow,
} from "@/features/supplier-payables/types"
import {
    MISSING_PURCHASE_ORDER_NO,
    MISSING_SETTLEMENT_NO,
    MISSING_SOURCE_DOCUMENT_NO,
} from "@/features/supplier-payables/lib/display-labels"

/** 付款详情「关联单据」用的去重后导航目标；采购单与应付台账合并成一行。 */
export type PaymentRelatedDocumentRef = Readonly<{
    id: string
    kind: "payable" | "source" | "original-payment"
    documentType: string
    documentNumber: string
    payableAccountId?: string
    sourceHref?: string
    sourceType?: PayableSourceType
    amount: string
    statusLabel: string
    statusTone: "success" | "warning"
}>

/**
 * 应付来源单据对象中心地址。无来源身份时不生成链接。
 *
 * @param sourceType 应付来源类型。
 * @param sourceDocumentId 来源单据内部身份。
 */
export function sourceDocumentHref(
    sourceType: PayableSourceType,
    sourceDocumentId: string | null | undefined,
): string | undefined {
    const id = sourceDocumentId?.trim()
    if (!id) return undefined
    if (sourceType === "PURCHASE_ORDER") {
        return `/procurement/orders/${encodeURIComponent(id)}`
    }
    return `/supplier-api/settlements/${encodeURIComponent(id)}`
}

/**
 * 供应商往来应付预览地址。
 *
 * @param payableAccountId 应付子账主键。
 */
export function payablePreviewHref(payableAccountId: string): string {
    const params = new URLSearchParams({
        view: "payable",
        detailId: payableAccountId,
        previewKind: "payable",
    })
    return `/finance/supplier-accounts?${params.toString()}`
}

/**
 * 供应商往来付款预览地址。
 *
 * @param paymentId 付款主键。
 */
export function paymentPreviewHref(paymentId: string): string {
    const params = new URLSearchParams({
        view: "payment",
        detailId: paymentId,
        previewKind: "payment",
    })
    return `/finance/supplier-accounts?${params.toString()}`
}

/**
 * 供应商付款冲正预览地址。
 *
 * @param reversalId 付款冲正主键。
 */
export function paymentReversalPreviewHref(reversalId: string): string {
    const params = new URLSearchParams({
        view: "payment",
        detailId: reversalId,
        previewKind: "reversal",
    })
    return `/finance/supplier-accounts?${params.toString()}`
}

/**
 * 按来源类型返回缺失单号占位。
 *
 * @param sourceType 应付来源类型。
 */
export function missingSourceDocumentNo(
    sourceType: PayableSourceType | undefined,
): string {
    if (sourceType === "PURCHASE_ORDER") return MISSING_PURCHASE_ORDER_NO
    if (sourceType === "SUPPLIER_SETTLEMENT") return MISSING_SETTLEMENT_NO
    return MISSING_SOURCE_DOCUMENT_NO
}

/**
 * 来源单据「打开」按钮文案。结算单与采购单分开，避免都叫打开。
 *
 * @param sourceType 应付来源类型。
 */
export function sourceDocumentOpenLabel(
    sourceType: PayableSourceType | undefined,
): string {
    if (sourceType === "SUPPLIER_SETTLEMENT") return "打开结算单"
    return "打开采购单"
}

/**
 * 把付款核销行收成关联单据。同一应付只保留一行，采购单作为该应付的来源动作，
 * 不再并列一张看起来相同的采购单。
 *
 * @param row 付款事实；只需核销行、金额与原付款身份。
 */
export function paymentRelatedDocumentRefs(
    row: Pick<PaymentRow, "allocations" | "amount" | "reverseOfPaymentId">,
): readonly PaymentRelatedDocumentRef[] {
    const documents: PaymentRelatedDocumentRef[] = []
    const seen = new Set<string>()
    for (const allocation of row.allocations) {
        const ref = relatedDocumentRefFromAllocation(allocation)
        if (!ref || seen.has(ref.id)) continue
        seen.add(ref.id)
        documents.push(ref)
    }
    if (row.reverseOfPaymentId) {
        documents.push({
            id: `payment:${row.reverseOfPaymentId}`,
            kind: "original-payment",
            documentType: "原付款单",
            documentNumber: "查看原付款",
            amount: row.amount,
            statusLabel: "已冲正",
            statusTone: "warning",
        })
    }
    return documents
}

/**
 * 把一行核销投影成关联单据。应付身份优先；没有应付时退回来源单据。
 *
 * @param allocation 付款核销行。
 */
function relatedDocumentRefFromAllocation(
    allocation: PaymentAllocationLine,
): PaymentRelatedDocumentRef | undefined {
    const payableAccountId = allocation.payableAccountId.trim() || undefined
    if (payableAccountId) {
        return {
            id: `payable:${payableAccountId}`,
            kind: "payable",
            documentType: "应付台账",
            documentNumber: allocation.sourceDocumentNo,
            payableAccountId,
            sourceHref: allocation.sourceHref,
            sourceType: allocation.sourceType,
            amount: allocation.amount,
            statusLabel: "已核销",
            statusTone: "success",
        }
    }
    if (!allocation.sourceHref) return undefined
    return {
        id: `source:${allocation.sourceHref}`,
        kind: "source",
        documentType: SOURCE_TYPE_LABEL[allocation.sourceType],
        documentNumber: allocation.sourceDocumentNo,
        sourceHref: allocation.sourceHref,
        sourceType: allocation.sourceType,
        amount: allocation.amount,
        statusLabel: "已关联",
        statusTone: "success",
    }
}
