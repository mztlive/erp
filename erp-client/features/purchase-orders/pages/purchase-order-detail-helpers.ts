export type PurchaseOrderDetailSectionId =
    | "overview"
    | "approval"
    | "fulfillment"
    | "payable"
    | "changes"

export type PurchaseOrderDetailMode = "view" | "edit"

export const PURCHASE_ORDER_DETAIL_NAV: readonly {
    id: PurchaseOrderDetailSectionId
    label: string
}[] = [
    { id: "overview", label: "概览" },
    { id: "approval", label: "审批" },
    { id: "fulfillment", label: "履约" },
    { id: "payable", label: "票款" },
    { id: "changes", label: "变更" },
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
    if (mode === "edit") return mode
    return "view"
}

export type PurchaseOrderBackContext = {
    href: string
    label: string
    sourceHint?: string
    hasReturnTo: boolean
}

/** 校验并解析来源返回路径；支持工作台、付款申请、销售单及任意安全站内路径。 */
export function resolvePurchaseOrderBackContext(
    searchParams: Pick<URLSearchParams, "get">,
): PurchaseOrderBackContext {
    const rawReturnTo = searchParams.get("returnTo")?.trim()
    const from = searchParams.get("from")?.trim()
    const safeReturnTo =
        rawReturnTo &&
        rawReturnTo.startsWith("/") &&
        !rawReturnTo.startsWith("//")
            ? rawReturnTo
            : null

    if (!safeReturnTo) {
        return {
            href: "/procurement/orders",
            label: "采购单列表",
            hasReturnTo: false,
        }
    }

    const returnPath = safeReturnTo.split("?")[0] ?? ""
    if (
        from === "workspace" &&
        (returnPath === "/workspace" ||
            returnPath === "/workspace/tasks" ||
            /^\/workspace\/(tasks|approvals|copies)\/[^/]+$/.test(returnPath))
    ) {
        return {
            href: safeReturnTo,
            label: "返回工作台",
            sourceHint: "来自工作台",
            hasReturnTo: true,
        }
    }
    if (returnPath.startsWith("/procurement/payment-requests")) {
        return {
            href: safeReturnTo,
            label: "返回付款申请",
            sourceHint: "来自付款申请",
            hasReturnTo: true,
        }
    }
    if (returnPath.startsWith("/sales/orders")) {
        return {
            href: safeReturnTo,
            label: "返回销售单",
            sourceHint: "来自销售单",
            hasReturnTo: true,
        }
    }
    if (returnPath.startsWith("/procurement/orders")) {
        return {
            href: safeReturnTo,
            label: "采购单列表",
            hasReturnTo: true,
        }
    }
    return {
        href: safeReturnTo,
        label: "返回上一页",
        sourceHint: "来自关联页面",
        hasReturnTo: true,
    }
}

/**
 * 采购单详情 URL 构造器：全程透传 `returnTo` / `from` / 任务等查询参数。
 */
export function purchaseOrderDetailHref(
    purchaseOrderId: string,
    currentSearch?: string | URLSearchParams,
    patch?: {
        section?: PurchaseOrderDetailSectionId | null
        mode?: PurchaseOrderDetailMode | null
    },
): string {
    const params = new URLSearchParams(currentSearch)
    if (patch && "section" in patch) {
        if (!patch.section || patch.section === "overview") {
            params.delete("section")
        } else {
            params.set("section", patch.section)
        }
    }
    if (patch && "mode" in patch) {
        if (!patch.mode || patch.mode === "view") {
            params.delete("mode")
        } else {
            params.set("mode", patch.mode)
        }
    }
    const qs = params.toString()
    const base = `/procurement/orders/${encodeURIComponent(purchaseOrderId)}`
    return qs ? `${base}?${qs}` : base
}

/**
 * 采购单详情分区 URL。概览不带 `section`；切换分区时保留 mode / returnTo / from / 任务等查询参数。
 */
export function purchaseOrderSectionHref(
    purchaseOrderId: string,
    section: PurchaseOrderDetailSectionId,
    currentSearch?: string | URLSearchParams,
): string {
    return purchaseOrderDetailHref(purchaseOrderId, currentSearch, { section })
}
