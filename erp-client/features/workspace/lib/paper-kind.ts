export type WorkspacePaperKind = "sales_order" | "purchase_order"

const SALES_PAPER_TYPES = new Set([
    "sales_order",
    "voucher_sales_order",
    "salesorder",
    "vouchersalesorder",
])

const PURCHASE_PAPER_TYPES = new Set(["purchase_order", "purchaseorder"])

function normalizeObjectType(businessObjectType: string): string {
    return businessObjectType
        .trim()
        .replace(/([a-z])([A-Z])/g, "$1_$2")
        .toLowerCase()
}

/**
 * 工作台能否用纸质件读当前业务对象。没有适配器的类型不得硬套甲乙方纸。
 */
export function workspacePaperKind(
    businessObjectType: string,
): WorkspacePaperKind | null {
    const kind = normalizeObjectType(businessObjectType)
    if (SALES_PAPER_TYPES.has(kind)) return "sales_order"
    if (PURCHASE_PAPER_TYPES.has(kind)) return "purchase_order"
    return null
}
