import type { WorkspacePaperKind } from "@/features/workspace/lib/paper-kind"

/** 简报里来源销售单的展示标签。 */
export const SOURCE_SALES_ORDER_LABEL = "来源销售单"

/** 付款冲正简报里原付款单的展示标签。 */
export const ORIGINAL_SUPPLIER_PAYMENT_LABEL = "原付款单"

export type SourceSalesOrderRef = Readonly<{
    orderNo: string
    objectId?: string
}>

export type LinkedDocumentSection = Readonly<{
    label: string
    value: string
    objectId?: string
}>

/**
 * 从简报键值段取出可上屏的来源销售单号，以及可选的跳转身份。
 */
export function findSourceSalesOrder(
    sections?: readonly LinkedDocumentSection[],
): SourceSalesOrderRef | null {
    const section = sections?.find(
        (item) => item.label === SOURCE_SALES_ORDER_LABEL,
    )
    if (!section) return null
    const orderNo = section.value.trim()
    if (!orderNo) return null
    const objectId = section.objectId?.trim()
    return { orderNo, objectId: objectId || undefined }
}

/**
 * 财务从工作台打开来源销售单时带回当前队列。
 */
export function sourceSalesOrderHref(
    salesOrderId: string,
    returnTo = "/workspace",
): string {
    const id = salesOrderId.trim()
    const params = new URLSearchParams({
        from: "workspace",
        returnTo,
    })
    return `/sales/orders/${encodeURIComponent(id)}?${params.toString()}`
}

/**
 * 简报关联单据能否用纸质件预览。未知标签不得硬套销售单纸。
 */
export function linkedDocumentPaperKind(
    label: string,
): WorkspacePaperKind | null {
    return label === SOURCE_SALES_ORDER_LABEL ? "sales_order" : null
}

/**
 * 简报关联单据的受控跳转地址。未知标签不得套销售单路径。
 */
export function linkedDocumentHref(
    label: string,
    objectId: string,
    returnTo = "/workspace",
): string | null {
    if (label === SOURCE_SALES_ORDER_LABEL) {
        return sourceSalesOrderHref(objectId, returnTo)
    }
    if (label === ORIGINAL_SUPPLIER_PAYMENT_LABEL) {
        const params = new URLSearchParams({
            view: "payment",
            detailId: objectId,
            previewKind: "payment",
        })
        return `/finance/supplier-accounts?${params.toString()}`
    }
    return null
}

/**
 * 把采购单对象中心的来源销售单补进简报段，供预览和跳转。
 */
export function withSourceSalesOrder(
    sections: readonly LinkedDocumentSection[],
    source: SourceSalesOrderRef | null,
): LinkedDocumentSection[] {
    if (!source) return [...sections]
    const next: LinkedDocumentSection = {
        label: SOURCE_SALES_ORDER_LABEL,
        value: source.orderNo,
        objectId: source.objectId,
    }
    const index = sections.findIndex(
        (section) => section.label === SOURCE_SALES_ORDER_LABEL,
    )
    if (index < 0) return [next, ...sections]
    return sections.map((section, sectionIndex) =>
        sectionIndex === index
            ? { ...section, value: next.value, objectId: next.objectId }
            : section,
    )
}
