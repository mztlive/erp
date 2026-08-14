export type PurchaseOrderDetailSectionId =
    | "overview"
    | "lines"
    | "fulfillment"
    | "payable"
    | "changes"
    | "audit"

export type PurchaseOrderDetailMode = "view" | "edit" | "review"

export type PurchaseOrderDetailNavItem = {
    id: PurchaseOrderDetailSectionId
    label: string
    href: string
}

export function resolvePurchaseOrderDetailSection(
    section?: string,
): PurchaseOrderDetailSectionId {
    if (
        section === "lines" ||
        section === "fulfillment" ||
        section === "payable" ||
        section === "changes" ||
        section === "audit"
    ) {
        return section
    }
    return "overview"
}

export function resolvePurchaseOrderDetailMode(
    mode?: string,
): PurchaseOrderDetailMode {
    if (mode === "edit" || mode === "review") return mode
    return "view"
}

export function buildPurchaseOrderDetailNavItems(
    baseHref: string,
    mode: PurchaseOrderDetailMode,
): PurchaseOrderDetailNavItem[] {
    return [
        { id: "overview", label: "概览", href: baseHref },
        {
            id: "lines",
            label: "明细与分配",
            href: `${baseHref}?section=lines${mode !== "view" ? `&mode=${mode}` : ""}`,
        },
        {
            id: "fulfillment",
            label: "履约",
            href: `${baseHref}?section=fulfillment${mode !== "view" ? `&mode=${mode}` : ""}`,
        },
        {
            id: "payable",
            label: "应付与票款",
            href: `${baseHref}?section=payable${mode !== "view" ? `&mode=${mode}` : ""}`,
        },
        {
            id: "changes",
            label: "变更与异常",
            href: `${baseHref}?section=changes${mode !== "view" ? `&mode=${mode}` : ""}`,
        },
        {
            id: "audit",
            label: "审计",
            href: `${baseHref}?section=audit${mode !== "view" ? `&mode=${mode}` : ""}`,
        },
    ]
}
