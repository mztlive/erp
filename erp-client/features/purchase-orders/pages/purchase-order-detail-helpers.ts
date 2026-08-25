export type PurchaseOrderDetailSectionId =
    | "overview"
    | "approval"
    | "fulfillment"
    | "payable"
    | "changes"
    | "audit"

export type PurchaseOrderDetailMode = "view" | "edit" | "review"

export const PURCHASE_ORDER_DETAIL_NAV: readonly {
    id: PurchaseOrderDetailSectionId
    label: string
}[] = [
    { id: "overview", label: "概览" },
    { id: "approval", label: "审批" },
    { id: "fulfillment", label: "履约" },
    { id: "payable", label: "应付与票款" },
    { id: "changes", label: "变更与异常" },
    { id: "audit", label: "审计" },
]

export function resolvePurchaseOrderDetailSection(
    section?: string,
): PurchaseOrderDetailSectionId {
    return (
        PURCHASE_ORDER_DETAIL_NAV.find((item) => item.id === section)?.id ??
        "overview"
    )
}

export function resolvePurchaseOrderDetailMode(
    mode?: string,
): PurchaseOrderDetailMode {
    if (mode === "edit" || mode === "review") return mode
    return "view"
}

/**
 * 采购单详情分区 URL。概览不带 `section`；切换分区时保留 mode / 任务等现有查询参数。
 */
export function purchaseOrderSectionHref(
    purchaseOrderId: string,
    section: PurchaseOrderDetailSectionId,
    currentSearch?: string | URLSearchParams,
): string {
    const params = new URLSearchParams(currentSearch)
    if (section === "overview") params.delete("section")
    else params.set("section", section)
    const qs = params.toString()
    const base = `/procurement/orders/${purchaseOrderId}`
    return qs ? `${base}?${qs}` : base
}
