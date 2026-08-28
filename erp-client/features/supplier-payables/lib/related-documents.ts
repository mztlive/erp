import type { PayableSourceType } from "@/features/supplier-payables/types"
import {
    MISSING_PURCHASE_ORDER_NO,
    MISSING_SETTLEMENT_NO,
    MISSING_SOURCE_DOCUMENT_NO,
} from "@/features/supplier-payables/lib/display-labels"

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
